use nyar_language::{
    ValkyrieCompiler,
    types::{
        SourceID,
        hir::{HirExprKind, HirImportBinding, HirStatementKind},
    },
};

fn compile(source: &str) -> nyar_language::types::hir::HirModule {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 9100 });
    compiler.compile_source(source).expect("compile")
}

#[test]
fn syntax_sugar_single_line_micro() {
    let hir = compile(
        r#"
micro double(x: i64) -> i64 = x + x
"#,
    );
    assert!(matches!(hir.functions[0].body.expr.as_ref().map(|expr| &expr.kind), Some(HirExprKind::Call { .. })));
}

#[test]
fn syntax_sugar_using_alias() {
    let hir = compile(
        r#"
using std.collections as col;
using std.data.{Map as HashMap, Set};
"#,
    );
    assert_eq!(hir.imports.len(), 2);
    assert_eq!(hir.imports[0].alias.as_ref().map(|name| name.as_str()), Some("col"));
    assert_eq!(hir.imports[1].bindings.len(), 2);
    assert_eq!(
        hir.imports[1].bindings[0],
        HirImportBinding { name: nyar_language::types::Identifier::new("Map"), alias: Some(nyar_language::types::Identifier::new("HashMap")) }
    );
}

#[test]
fn syntax_sugar_try_propagate() {
    let hir = compile(
        r#"
micro main() -> i64? {
    let value = fetch()?;
    value
}
"#,
    );
    let HirStatementKind::Let { initializer, .. } = &hir.functions[0].body.statements[0].kind
    else {
        panic!("expected let expression statement");
    };
    let Some(initializer) = initializer
    else {
        panic!("expected let initializer");
    };
    assert!(matches!(initializer.kind, HirExprKind::TryPropagate(_)));
}

#[test]
fn syntax_sugar_try_scope_metadata() {
    let hir = compile(
        r#"
micro main() -> i64? {
    try? {
        fetch()?
    }
}
micro fetch() -> i64? {
    1
}
"#,
    );
    let tail = hir.functions[0].body.expr.as_ref().expect("tail");
    let HirExprKind::TryScope { is_optional, is_forced, result_type, .. } = &tail.kind
    else {
        panic!("expected try scope");
    };
    assert!(*is_optional);
    assert!(!*is_forced);
    assert!(result_type.is_none());
}

#[test]
fn nullable_suffix_flattens_nested_question_marks() {
    use nyar_language::types::hir::ValkyrieType;
    let hir = compile(
        r#"
micro f(x: i64??) -> i64?? {
    x
}
"#,
    );
    let param_ty = &hir.functions[0].params[0].ty;
    let ValkyrieType::Union(items) = param_ty
    else {
        panic!("expected nullable union");
    };
    assert_eq!(items.len(), 2);
    assert!(items.iter().any(|item| matches!(item, ValkyrieType::Named(name) if name.as_str() == "null")));
    assert!(items.iter().any(|item| matches!(item, ValkyrieType::Integer64 { signed: true })));
}

#[test]
fn syntax_sugar_if_expression_value() {
    let hir = compile(
        r#"
micro pick(flag: bool) -> i64 {
    let value = if flag { 1 } else { 0 };
    value
}
"#,
    );
    let HirStatementKind::Let { initializer, .. } = &hir.functions[0].body.statements[0].kind
    else {
        panic!("expected let statement");
    };
    let Some(initializer) = initializer
    else {
        unreachable!()
    };
    assert!(matches!(initializer.kind, HirExprKind::If { .. }));
}

#[test]
fn syntax_sugar_derive_injects_impls() {
    let hir = compile(
        r#"
[derive(Clone, Debug, Eq)]
class Point {
    x: i64;
    y: i64;
}
"#,
    );
    assert_eq!(hir.structs[0].derives.len(), 3);
    assert!(hir.impls.iter().any(|impl_block| impl_block.trait_path.as_ref().is_some_and(|path| path.to_string() == "Clone")));
}

#[test]
fn syntax_sugar_implicit_self_warning() {
    let hir = compile(
        r#"
class Point {
    micro area() -> i64 {
        1
    }
}
"#,
    );
    assert!(hir.warnings.iter().any(|warning| warning.code == "W_IMPLICIT_SELF"));
}
