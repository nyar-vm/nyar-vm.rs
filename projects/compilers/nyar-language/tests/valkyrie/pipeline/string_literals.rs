use nyar_language::{
    ValkyrieCompiler,
    types::{
        SourceID,
        hir::{HirExprKind, HirLiteral, HirStatementKind, HirStringSegment},
    },
};

#[test]
fn lowers_brace_interpolation_and_escaped_braces_into_hir_segments() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 901 });
    let module = compiler
        .compile_source(
            r#"micro main(name: utf8) {
    let message = "Hello, {name}!";
    let literal = "Template: \{name\}";
}
"#,
        )
        .unwrap();

    let HirStatementKind::Let { initializer: Some(message), .. } = &module.functions[0].body.statements[0].kind
    else {
        panic!("expected message let");
    };
    assert!(matches!(
        &message.kind,
        HirExprKind::Literal(HirLiteral::String(value))
            if matches!(
                value.segments.as_slice(),
                [
                    HirStringSegment::Text(prefix),
                    HirStringSegment::Interpolation { expr, is_fluent },
                    HirStringSegment::Text(suffix)
                ]
                if prefix == "Hello, "
                    && !is_fluent
                    && matches!(expr.kind, HirExprKind::Variable(ref identifier) if identifier.name.as_str() == "name")
                    && suffix == "!"
            )
    ));

    let HirStatementKind::Let { initializer: Some(literal), .. } = &module.functions[0].body.statements[1].kind
    else {
        panic!("expected literal let");
    };
    assert!(matches!(
        &literal.kind,
        HirExprKind::Literal(HirLiteral::String(value))
            if matches!(value.segments.as_slice(), [HirStringSegment::Text(text)] if text == "Template: {name}")
    ));
}

#[test]
fn lowers_triple_quoted_interpolation_into_hir_segments() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 901 });
    let module = compiler
        .compile_source(
            r#"micro main(name: utf8) {
    let message = """Hello, {name}!""";
}"#,
        )
        .unwrap();

    let HirStatementKind::Let { initializer: Some(message), .. } = &module.functions[0].body.statements[0].kind
    else {
        panic!("expected message let");
    };
    assert!(matches!(
        &message.kind,
        HirExprKind::Literal(HirLiteral::String(value))
            if matches!(
                value.segments.as_slice(),
                [
                    HirStringSegment::Text(prefix),
                    HirStringSegment::Interpolation { .. },
                    HirStringSegment::Text(suffix)
                ]
                if prefix == "Hello, "
                    && suffix == "!"
            )
    ));
}
