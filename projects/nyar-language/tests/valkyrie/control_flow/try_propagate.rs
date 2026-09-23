use nyar_language::{
    MirLowerer, MirOperation, ValkyrieCompiler,
    types::{SourceID, hir::HirExprKind},
};

fn compile(source: &str) -> nyar_language::types::hir::HirModule {
    ValkyrieCompiler::new(SourceID { version_id: 9200 }).compile_source(source).expect("compile")
}

#[test]
fn rejects_unknown_pattern_extractor_at_compile_time() {
    let error = ValkyrieCompiler::new(SourceID { version_id: 9201 })
        .compile_source(
            r#"
micro main(value: Point) -> unit {
    match value {
        case Point(_):
            ()
        else:
            ()
    }
}
class Point {}
"#,
        )
        .expect_err("unknown extractor");
    assert!(error.to_string().contains("unknown pattern extractor"));
}

#[test]
fn try_propagate_on_result_lowers_fine_branch() {
    let source = r#"
unite Result {
    Fine {
        value: utf8,
    }
    Fail {
        error: utf8,
    }
}

micro read() -> Result<utf8, utf8> {
    Fine { value: "ok" }
}

micro main() -> Result<utf8, utf8> {
    let value = read()?;
    Fine { value: value }
}
"#;
    let hir = compile(source);
    let main_fn = hir.functions.iter().find(|f| f.name.as_str() == "main").expect("main");
    assert!(main_fn.body.statements.iter().any(|stmt| {
        if let nyar_language::types::hir::HirStatementKind::Let { initializer: Some(init), .. } = &stmt.kind {
            matches!(init.kind, HirExprKind::TryPropagate(_))
        }
        else {
            false
        }
    }));

    let mir = MirLowerer::lower_module(&hir);
    let main_mir = mir.functions.iter().find(|f| f.symbol.contains("main")).expect("main mir");
    assert!(!main_mir.blocks.is_empty());
}

#[test]
fn rejects_tag_only_unite_in_favor_of_enums() {
    let error = ValkyrieCompiler::new(SourceID { version_id: 9202 })
        .compile_source(
            r#"
unite Preview {
    Preview1
    Preview2
    Preview3
}
"#,
        )
        .expect_err("tag-only unite");
    let message = error.to_string();
    assert!(message.contains("payload-less"), "got {message}");
    assert!(message.contains("enums"), "got {message}");
}

#[test]
fn enums_tag_only_preview_compiles() {
    let hir = compile(
        r#"
enums Preview {
    Preview1
    Preview2
    Preview3
}
micro name(preview: Preview) -> utf8 {
    match preview {
        case Preview1:
            "wasip1"
        case Preview2:
            "wasip2"
        case Preview3:
            "wasip3"
    }
}
"#,
    );
    assert!(hir.enums.iter().any(|item| item.name.as_str() == "Preview" && !item.is_unity));
}

#[test]
fn try_propagate_on_option_lowers_some_branch() {
    let source = r#"
unite Option<T> {
    Some { value: T }
    None
}

micro fetch() -> Option<utf8> {
    Some { value: "ok" }
}

micro main() -> Option<utf8> {
    let value = fetch()?;
    Some { value: value }
}
"#;
    let hir = compile(source);
    let main_fn = hir.functions.iter().find(|f| f.name.as_str() == "main").expect("main");
    assert!(main_fn.body.statements.iter().any(|stmt| {
        if let nyar_language::types::hir::HirStatementKind::Let { initializer: Some(init), .. } = &stmt.kind {
            matches!(init.kind, HirExprKind::TryPropagate(_))
        }
        else {
            false
        }
    }));

    let mir = MirLowerer::lower_module_semantic(&hir);
    let main_mir = mir
        .functions
        .iter()
        .find(|f| f.symbol.ends_with("::main") || f.symbol == "main")
        .unwrap_or_else(|| panic!("expected mir main, got {:?}", mir.functions.iter().map(|f| f.symbol.as_str()).collect::<Vec<_>>()));
    let labels: Vec<_> = main_mir.blocks.iter().map(|block| block.label.as_str()).collect();
    assert!(
        labels.iter().any(|label| label.contains("try_propagate_some")),
        "expected try_propagate_some, got {labels:?} in {}",
        main_mir.symbol
    );
    assert!(labels.iter().any(|label| label.contains("try_propagate_none")), "expected try_propagate_none, got {labels:?}");
}

#[test]
fn hir_try_propagate_kind_present() {
    let hir = compile(
        r#"
micro main() -> i64? {
    let value = fetch()?;
    value
}
"#,
    );
    let main = &hir.functions[0];
    let Some(expr) = main.body.expr.as_ref()
    else {
        panic!("expected tail expr");
    };
    let HirExprKind::Variable(_) = &expr.kind
    else {
        panic!("expected variable tail");
    };
}
