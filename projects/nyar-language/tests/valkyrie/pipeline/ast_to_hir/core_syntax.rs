use nyar_language::{
    ValkyrieCompiler,
    types::{
        SourceID, SourceSpan,
        hir::{HirExprKind, HirVariadicKind, ValkyrieType},
    },
    valkyrie::hir::bind_construct_fields,
};

fn compile(source: &str) -> nyar_language::types::hir::HirModule {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 9001 });
    compiler.compile_source(source).expect("compile")
}

fn test_span() -> SourceSpan {
    SourceSpan::new(SourceID::default(), 0, 0)
}

#[test]
fn core_syntax_default_parameters() {
    let hir = compile(
        r#"
micro greet(name: utf8 = "world") {
    name
}
"#,
    );
    assert!(hir.functions[0].params[0].default.is_some());
}

#[test]
fn core_syntax_variadic_parameters() {
    let hir = compile(
        r#"
micro sum(..items: i64) {
    items
}
"#,
    );
    assert_eq!(hir.functions[0].params[0].variadic, HirVariadicKind::PositionalRest);

    let hir = compile(
        r#"
micro merge(a, <, >, ...pairs) {
    pairs
}
"#,
    );
    assert_eq!(hir.functions[0].params[1].variadic, HirVariadicKind::KeywordRest);
}

#[test]
fn core_syntax_construct_field_binding() {
    use nyar_language::types::{Identifier, hir::HirField};
    let hir = compile(
        r#"
class Point {
    x: i64;
    y: i64;
}
"#,
    );
    let span = test_span();
    let fields: Vec<HirField> = hir.structs[0].fields.clone();
    let lit = |n: i64| HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Integer64(n));
    let args = vec![
        nyar_language::types::hir::HirExpr {
            kind: HirExprKind::FieldInit {
                name: Identifier::new("y"),
                value: Box::new(nyar_language::types::hir::HirExpr { kind: lit(2), span: test_span() }),
            },
            span: span.clone(),
        },
        nyar_language::types::hir::HirExpr {
            kind: HirExprKind::FieldInit {
                name: Identifier::new("x"),
                value: Box::new(nyar_language::types::hir::HirExpr { kind: lit(1), span: test_span() }),
            },
            span,
        },
    ];
    let ordered = bind_construct_fields(&fields, &args).expect("bind");
    assert!(matches!(ordered[0].kind, HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Integer64(1))));
}

#[test]
fn core_syntax_union_types() {
    let hir = compile(
        r#"
micro pick(value: i64 | utf8) {
    value
}
"#,
    );
    assert!(matches!(
        &hir.functions[0].params[0].ty,
        ValkyrieType::Union(items) if items.len() == 2
    ));
}

#[test]
fn core_syntax_intersection_types() {
    let hir = compile(
        r#"
micro display(value: Display & Clone) {
    value
}
"#,
    );
    assert!(matches!(
        &hir.functions[0].params[0].ty,
        ValkyrieType::Intersection(items) if items.len() == 2
    ));
}

#[test]
fn core_syntax_type_aliases() {
    let hir = compile(
        r#"
type UserId = i64;

micro id() -> UserId {
    return 1;
}
"#,
    );
    assert_eq!(hir.type_aliases.len(), 1);
    assert_eq!(hir.type_aliases[0].name.as_str(), "UserId");
    assert!(hir.type_aliases[0].generics.is_empty());
    assert!(matches!(hir.type_aliases[0].target, ValkyrieType::Integer64 { signed: true }));
}

#[test]
fn core_syntax_generic_type_alias_to_result() {
    let hir = compile(
        r#"
structure Diag {
    message: utf8
}

type BinResult<T> = Result<T, Diag>;

micro read() -> BinResult<i32> {
    return Fine(1);
}

micro wrap_fail(message: utf8) -> BinResult<i32> {
    return Fail(Diag { message: message });
}
"#,
    );
    assert_eq!(hir.type_aliases.len(), 1);
    assert_eq!(hir.type_aliases[0].name.as_str(), "BinResult");
    assert_eq!(hir.type_aliases[0].generics.len(), 1);
    assert_eq!(hir.type_aliases[0].generics[0].as_str(), "T");
    assert!(matches!(
        &hir.type_aliases[0].target,
        ValkyrieType::Apply(base, args)
            if matches!(base.as_ref(), ValkyrieType::Named(name) if name.as_str() == "Result")
                && args.len() == 2
    ));
    assert!(matches!(
        &hir.functions[0].return_type,
        ValkyrieType::Apply(base, args)
            if matches!(base.as_ref(), ValkyrieType::Named(name) if name.as_str() == "Result")
                && args.len() == 2
                && matches!(&args[0], ValkyrieType::Integer32 { signed: true })
                && matches!(&args[1], ValkyrieType::Named(name) if name.as_str() == "Diag")
    ));
    assert!(matches!(
        &hir.functions[1].return_type,
        ValkyrieType::Apply(base, args)
            if matches!(base.as_ref(), ValkyrieType::Named(name) if name.as_str() == "Result")
                && args.len() == 2
    ));
    // Alias expands to Result — no parallel Fine/Fail unite is required.
    assert!(hir.enums.iter().all(|e| e.name.as_str() != "BinResult"));
}

#[test]
fn core_syntax_rejects_duplicate_type_alias_name() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 9002 });
    let err = compiler
        .compile_source(
            r#"
type Alias<T> = Result<T, utf8>;
type Alias<T> = Result<T, i32>;
"#,
        )
        .expect_err("duplicate type alias should fail");
    let message = format!("{err}");
    assert!(message.contains("duplicate") || message.contains("Alias"), "unexpected error: {message}");
}

#[test]
fn core_syntax_rejects_duplicate_unite_name() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 9003 });
    let err = compiler
        .compile_source(
            r#"
unite Dup<T> {
    Fine { value: T }
    Fail { error: utf8 }
}
unite Dup<T> {
    Fine { value: T }
    Fail { error: utf8 }
}
"#,
        )
        .expect_err("duplicate unite should fail");
    let message = format!("{err}");
    assert!(message.contains("duplicate") || message.contains("Dup"), "unexpected error: {message}");
}
