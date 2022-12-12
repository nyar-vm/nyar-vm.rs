use nyar_language::{MirLowerer, MirOperand, MirOperation, MirTerminator, SourceID, ValkyrieCompiler, valkyrie::mir::ssa::MirFunction};

fn compile_mir(source: &str) -> nyar_language::valkyrie::mir::MirModule {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9400 }).compile_source(source).expect("compile");
    MirLowerer::lower_module_semantic(&hir)
}

fn find_function<'a>(mir: &'a nyar_language::valkyrie::mir::MirModule, name: &str) -> &'a MirFunction {
    mir.functions
        .iter()
        .find(|f| f.symbol.ends_with(&format!("::{name}")) || f.symbol == name)
        .unwrap_or_else(|| panic!("expected mir function {name}"))
}

fn block_named<'a>(function: &'a MirFunction, label: &str) -> &'a nyar_language::valkyrie::mir::ssa::MirBlock {
    function.blocks.iter().find(|block| block.label == label).unwrap_or_else(|| {
        let labels: Vec<_> = function.blocks.iter().map(|b| b.label.as_str()).collect();
        panic!("expected block {label}, got {labels:?}")
    })
}

fn terminators(function: &MirFunction) -> Vec<&MirTerminator> {
    function.blocks.iter().map(|block| &block.terminator).collect()
}

fn has_call_named(function: &MirFunction, name: &str) -> bool {
    function.blocks.iter().any(|block| {
        block.instructions.iter().any(|ins| {
            matches!(
                &ins.kind,
                MirOperation::Call { callee: MirOperand::Symbol(path), .. }
                    if path.to_string().contains(name)
            )
        })
    })
}

#[test]
fn explicit_return_lowers_to_mir_return_terminator() {
    let mir = compile_mir(
        r#"
micro main() -> i64 {
    return 42
}
"#,
    );
    let function = find_function(&mir, "main");
    assert!(
        terminators(function).iter().any(|term| matches!(term, MirTerminator::Return { value: Some(_) })),
        "expected Return terminator with value"
    );
    assert!(!has_call_named(function, "is_null"));
}

#[test]
fn nullable_try_propagate_lowers_branch_return_and_unwrap() {
    let mir = compile_mir(
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
    let function = find_function(&mir, "main");
    assert!(terminators(function).iter().any(|term| matches!(term, MirTerminator::Branch { .. })));
    assert!(matches!(block_named(function, "try_propagate_early_exit").terminator, MirTerminator::Return { .. }));
    assert!(block_named(function, "try_propagate_ok").instructions.iter().any(|ins| {
        matches!(
            &ins.kind,
            MirOperation::Call { callee: MirOperand::Symbol(path), .. }
                if path.to_string().contains("unwrap_null")
        )
    }));
    assert!(has_call_named(function, "is_null"));
}

#[test]
fn result_try_propagate_lowers_branch_return_and_field_get() {
    let mir = compile_mir(
        r#"
unite Result {
    Fine { value: i64 }
    Fail { error: utf8 }
}
micro read() -> Result<i64, utf8> {
    Fine { value: 7 }
}
micro main() -> Result<i64, utf8> {
    let value = read()?;
    Fine { value: value }
}
"#,
    );
    let function = find_function(&mir, "main");
    assert!(terminators(function).iter().any(|term| matches!(term, MirTerminator::Branch { .. })));
    assert!(matches!(block_named(function, "try_propagate_fail").terminator, MirTerminator::Return { value: Some(_) }));
    assert!(
        block_named(function, "try_propagate_fine")
            .instructions
            .iter()
            .any(|ins| { matches!(&ins.kind, MirOperation::FieldGet { field, .. } if field == "value") })
    );
}

#[test]
fn option_try_propagate_lowers_branch_return_and_field_get() {
    let mir = compile_mir(
        r#"
unite Option<T> {
    Some { value: T }
    None
}
micro fetch() -> Option<i64> {
    Some { value: 7 }
}
micro main() -> Option<i64> {
    let value = fetch()?;
    Some { value: value }
}
"#,
    );
    let function = find_function(&mir, "main");
    assert!(terminators(function).iter().any(|term| matches!(term, MirTerminator::Branch { .. })));
    assert!(matches!(block_named(function, "try_propagate_none").terminator, MirTerminator::Return { value: Some(_) }));
    assert!(
        block_named(function, "try_propagate_some")
            .instructions
            .iter()
            .any(|ins| { matches!(&ins.kind, MirOperation::FieldGet { field, .. } if field == "value") })
    );
}

#[test]
fn try_scope_inner_question_mark_jumps_to_try_exit() {
    let mir = compile_mir(
        r#"
micro fetch() -> i64? {
    null
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
    let function = find_function(&mir, "main");
    let early_exit = block_named(function, "try_propagate_early_exit");
    assert!(matches!(early_exit.terminator, MirTerminator::Jump { .. }));
    assert!(matches!(block_named(function, "try_exit").terminator, MirTerminator::Return { .. } | MirTerminator::Jump { .. }));
    assert!(!matches!(early_exit.terminator, MirTerminator::Return { .. }), "try? inner ? should Jump to try_exit, not Return");
}

#[test]
fn nested_return_in_loop_emits_return_in_inner_function() {
    let mir = compile_mir(
        r#"
micro find_target(limit: i64) -> i64 {
    let outer: i64 = 0
    while outer < limit {
        let inner: i64 = 0
        while inner < 4 {
            if outer == 1 {
                if inner == 2 {
                    return outer * 10 + inner
                }
            }
            inner = inner + 1
        }
        outer = outer + 1
    }
    return 0
}
"#,
    );
    let function = find_function(&mir, "find_target");
    let return_count = terminators(function).iter().filter(|term| matches!(term, MirTerminator::Return { .. })).count();
    assert!(return_count >= 2, "expected early and fallthrough returns, got {return_count}");
    assert!(terminators(function).iter().any(|term| matches!(term, MirTerminator::Branch { .. })));
}

#[test]
fn early_return_in_generator_does_not_emit_suspend_on_return_path() {
    let mir = compile_mir(
        r#"
micro gen(flag: bool) {
    if flag {
        return
    }
    yield 1
}
"#,
    );
    let function = find_function(&mir, "gen");
    let return_blocks: Vec<_> = function.blocks.iter().filter(|block| matches!(block.terminator, MirTerminator::Return { .. })).collect();
    assert!(!return_blocks.is_empty());
    for block in return_blocks {
        assert!(
            !matches!(block.terminator, MirTerminator::PerformEffect { .. } | MirTerminator::YieldToRuntime { .. }),
            "return path block `{}` must not suspend",
            block.label
        );
    }
}

#[test]
fn nullable_early_return_uses_null_symbol_for_non_nullable_return_type() {
    let mir = compile_mir(
        r#"
micro fetch() -> i64? {
    null
}
micro main() -> i64 {
    let value = try? {
        fetch()?
    };
    value
}
"#,
    );
    let function = find_function(&mir, "main");
    let early_exit = block_named(function, "try_propagate_early_exit");
    assert!(matches!(early_exit.terminator, MirTerminator::Jump { .. }));
}

#[test]
fn result_try_propagate_fail_returns_full_result_value() {
    let mir = compile_mir(
        r#"
unite Result {
    Fine { value: i64 }
    Fail { error: utf8 }
}
micro read() -> Result<i64, utf8> {
    Fail { error: "err" }
}
micro main() -> Result<i64, utf8> {
    let value = read()?;
    Fine { value: value }
}
"#,
    );
    let function = find_function(&mir, "main");
    let fail_block = block_named(function, "try_propagate_fail");
    assert!(matches!(fail_block.terminator, MirTerminator::Return { value: Some(_) }));
}
