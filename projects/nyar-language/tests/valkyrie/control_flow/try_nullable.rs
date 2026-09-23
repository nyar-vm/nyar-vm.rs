use nyar_language::{
    MirLowerer, MirOperation, MirTerminator, ValkyrieCompiler,
    types::{
        SourceID,
        hir::{HirExprKind, HirStatementKind},
    },
};

fn compile(source: &str) -> nyar_language::types::hir::HirModule {
    ValkyrieCompiler::new(SourceID { version_id: 9300 }).compile_source(source).expect("compile")
}

fn compile_err(source: &str) -> String {
    ValkyrieCompiler::new(SourceID { version_id: 9301 }).compile_source(source).expect_err("expected compile error").to_string()
}

#[test]
fn rejects_try_propagate_on_non_nullable_operand() {
    let error = compile_err(
        r#"
micro main() -> i64? {
    let value = plain()?;
    value
}
micro plain() -> i64 {
    1
}
"#,
    );
    assert!(error.contains("cannot be applied"));
}

#[test]
fn rejects_try_propagate_outside_nullable_context() {
    let error = compile_err(
        r#"
micro main() -> i64 {
    let value = fetch()?;
    value
}
micro fetch() -> i64? {
    1
}
"#,
    );
    assert!(error.contains("nullable") || error.contains("try"));
}

#[test]
fn infers_try_propagate_as_payload_type() {
    let hir = compile(
        r#"
micro fetch() -> i64? {
    1
}
micro main() -> i64? {
    let value = fetch()?;
    value
}
"#,
    );
    let main = hir.functions.iter().find(|f| f.name.as_str() == "main").expect("main");
    assert!(matches!(main.return_type, nyar_language::types::hir::ValkyrieType::Union(_)));
}

#[test]
fn mir_try_propagate_narrows_ok_branch() {
    let hir = compile(
        r#"
micro fetch() -> i64? {
    1
}
micro main() -> i64? {
    let x = fetch()?;
    x
}
"#,
    );
    let mir = MirLowerer::lower_module_semantic(&hir);
    let function = mir.functions.iter().find(|f| f.symbol.ends_with("::main") || f.symbol == "main").expect("mir main");
    let labels: Vec<_> = function.blocks.iter().map(|b| b.label.clone()).collect();
    assert!(
        function.blocks.iter().any(|block| {
            block.instructions.iter().any(|ins| matches!(ins.kind, MirOperation::Call { .. })) && block.label.contains("try_propagate_ok"),
        }),
        "expected try_propagate_ok call block, got {labels:?}"
    );
    assert!(function.blocks.iter().any(|block| {
        block.instructions.iter().any(|ins| {
            matches!(
                &ins.kind,
                MirOperation::Call { callee, .. }
                    if matches!(
                        callee,
                        nyar_language::valkyrie::mir::ssa::MirOperand::Symbol(path)
                            if path.to_string().contains("unwrap_null")
                    )
            )
        })
    }));
}

#[test]
fn try_scope_allows_inner_question_mark() {
    let hir = compile(
        r#"
micro fetch() -> i64? {
    1
}
micro main() -> i64 {
    let result = try? {
        let value = fetch()?;
        value
    };
    result
}
"#,
    );
    let main = hir.functions.iter().find(|f| f.name.as_str() == "main").expect("main");
    let HirStatementKind::Let { initializer, .. } = &main.body.statements[0].kind
    else {
        panic!("expected let try scope binding");
    };
    let Some(initializer) = initializer
    else {
        panic!("expected try scope initializer");
    };
    assert!(matches!(initializer.kind, HirExprKind::TryScope { is_optional: true, .. }));

    let mir = MirLowerer::lower_module_semantic(&hir);
    let function = mir.functions.iter().find(|f| f.symbol.ends_with("::main") || f.symbol == "main").expect("mir main");
    let labels: Vec<_> = function.blocks.iter().map(|b| b.label.clone()).collect();
    assert!(function.blocks.iter().any(|block| block.label == "try_exit"), "expected try_exit block, got {labels:?}");
    assert!(
        function
            .blocks
            .iter()
            .any(|block| { matches!(&block.terminator, MirTerminator::Jump { .. }) && block.label.contains("try_propagate_early_exit") })
    );
}

#[test]
fn allows_return_literal_in_nullable_function() {
    compile(
        r#"
micro maybe_value(flag: bool) -> i64? {
    if flag {
        return 42
    }
    return null
}
"#,
    );
}

#[test]
fn compiles_feature_matrix_test_bundle() {
    use std::path::PathBuf;
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../valkyrie.v/examples/feature-matrix/test");
    let mut combined = String::new();
    for name in ["async_effect.v", "benchmark.v", "enums_flags.v", "mezzo_macro.v", "nullable.v"] {
        let path = base.join(name);
        if path.is_file() {
            combined.push_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("read {}", path.display())));
            combined.push('\n');
        }
    }
    ValkyrieCompiler::new(SourceID { version_id: 9302 })
        .compile_source(&combined)
        .unwrap_or_else(|error| panic!("feature-matrix bundle compile failed: {error}"));
}

#[test]
fn compiles_feature_matrix_effect_catch_compile_only() {
    use std::path::PathBuf;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../valkyrie.v/examples/feature-matrix/test/compile_only/effect_catch.v");
    let source = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("read {}", path.display()));
    ValkyrieCompiler::new(SourceID { version_id: 9304 })
        .compile_source(&source)
        .unwrap_or_else(|error| panic!("effect catch compile-only failed: {error}"));
}

#[test]
fn compiles_feature_matrix_test_bundle_with_await() {
    use std::path::PathBuf;
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../valkyrie.v/examples/feature-matrix/test");
    let path = base.join("async_effect.v");
    let mut source = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("read {}", path.display()));
    source.push_str(
        r#"
micro async_sample() {
    let future: Future<i32> = ReadyFuture { done: false }
    future.await
}
"#,
    );
    ValkyrieCompiler::new(SourceID { version_id: 9303 })
        .compile_source(&source)
        .unwrap_or_else(|error| panic!("async await surface compile failed: {error}"));
}
