use nyar_language::{ValkyrieCompiler, types::SourceID};

#[test]
fn rejects_tuple_variant_declaration_in_unite() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 9501 });
    let error = compiler
        .compile_source(
            r#"
unite Option<T> { Some(T) None }
"#,
        )
        .expect_err("tuple variant declaration should be rejected");
    assert!(error.to_string().contains("not valid variant declaration syntax"));
    assert!(error.to_string().contains("Some { value: T }"));
}

#[test]
fn record_style_variant_declaration_compiles() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 9502 });
    let module = compiler
        .compile_source(
            r#"
unite Option<T> {
    Some { value: T }
    None
}

unite Result<T, E> {
    Fine { value: T }
    Fail { error: E }
}
"#,
        )
        .expect("record-style declarations should compile");

    let option = module.enums.iter().find(|item| item.name.as_str() == "Option").expect("Option");
    let some = option.variants.iter().find(|item| item.name.as_str() == "Some").expect("Some");
    assert_eq!(some.fields.len(), 1);
    assert_eq!(some.fields[0].name.as_str(), "value");

    let result = module.enums.iter().find(|item| item.name.as_str() == "Result").expect("Result");
    let fine = result.variants.iter().find(|item| item.name.as_str() == "Fine").expect("Fine");
    assert_eq!(fine.fields.len(), 1);
}

#[test]
fn variant_constructor_call_resolves_for_record_style_unite() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 9503 });
    let hir = compiler
        .compile_source(
            r#"
unite Option<T> {
    Some { value: T }
    None
}

micro main() -> Option<i64> {
    return Some(42);
}
"#,
        )
        .expect("constructor call should compile");

    use nyar_language::types::hir::{HirExprKind, HirStatementKind};
    let function = hir.functions.iter().find(|item| item.name.as_str() == "main").expect("main");
    let HirStatementKind::Expr(statement) = &function.body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Call { resolved: Some(resolved), .. } = &expression.kind
    else {
        panic!("expected resolved constructor call");
    };
    assert_eq!(resolved.symbol.to_string(), "Some");
}

#[test]
fn variant_extractor_pattern_resolves_for_record_style_unite() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 9504 });
    let hir = compiler
        .compile_source(
            r#"
unite Option<T> {
    Some { value: T }
    None
}

micro main(opt: Option<i64>) -> i64 {
    return match opt {
        case Some(x): x
        case None: 0
    };
}
"#,
        )
        .expect("extractor pattern should compile");

    use nyar_language::types::hir::{HirExprKind, HirPattern, HirStatementKind};
    let function = hir.functions.iter().find(|item| item.name.as_str() == "main").expect("main");
    let HirStatementKind::Expr(statement) = &function.body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Match { arms, .. } = &expression.kind
    else {
        panic!("expected match expression");
    };
    assert!(matches!(
        &arms[0].pattern,
        HirPattern::Extractor(nyar_language::types::hir::HirExtractorPattern::Constructor { name, .. })
            if name.to_string() == "Some"
    ));
}
