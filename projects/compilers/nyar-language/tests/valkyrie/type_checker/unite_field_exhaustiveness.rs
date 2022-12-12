use nyar_language::{SourceID, ValkyrieCompiler};

/// `match expr.kind` over a local unite with Object + Name + Extractor-style arms
/// must credit every variant (not only payload-less `Unit`).
#[test]
fn field_scrutinee_unite_object_arms_are_exhaustive() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4301 });
    compiler
        .compile_source(
            r#"
unite ExprKind {
    Unit
    IntegerLiteral { text: utf8 }
    FloatLiteral { text: utf8 }
    Name { path: utf8 }
}

structure Expr {
    kind: ExprKind
}

micro lower(expr: Expr) -> utf8 {
    match expr.kind {
        case Unit:
            "unit"
        case IntegerLiteral { text }:
            text
        case FloatLiteral { text }:
            text
        case Name { path }:
            path
    }
}
"#,
        )
        .expect("Object + Name unite arms on FieldAccess scrutinee must be exhaustive");
}

/// Incomplete Object coverage on `expr.kind` must still report missing variants.
#[test]
fn field_scrutinee_unite_reports_missing_object_variants() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4302 });
    let error = compiler
        .compile_source(
            r#"
unite ExprKind {
    Unit
    IntegerLiteral { text: utf8 }
    FloatLiteral { text: utf8 }
}

structure Expr {
    kind: ExprKind
}

micro lower(expr: Expr) -> utf8 {
    match expr.kind {
        case Unit:
            "unit"
    }
}
"#,
        )
        .expect_err("missing Object variants must be non-exhaustive");
    let message = error.to_string();
    assert!(message.contains("non exhaustive match"), "unexpected error: {message}");
    assert!(message.contains("IntegerLiteral") || message.contains("FloatLiteral"), "unexpected error: {message}");
}
