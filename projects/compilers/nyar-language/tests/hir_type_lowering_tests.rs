use nyar_language::{
    types::{Identifier as HirIdentifier, hir::ValkyrieType},
    valkyrie::{
        frontend::ast::{Identifier, NamePath, TypeExpression, type_nodes::TypeUnaryNode},
        hir::type_lowering::{canonical_builtin_type, lower_type_expression, validate_type_expression},
    },
};

use oak_core::Range;
use oak_valkyrie::lexer::ValkyrieTokenType;

fn oak_span(len: usize) -> Range<usize> {
    Range { start: 0, end: len }
}

fn path_type(name: &str) -> TypeExpression {
    TypeExpression::Namepath(Box::new(NamePath {
        parts: vec![Identifier { name: name.to_string(), span: oak_span(name.len()) }],
        span: oak_span(name.len()),
    }))
}

#[test]
fn named_tuple_type_lowers_to_positional_tuple() {
    use nyar_language::valkyrie::frontend::ast::type_nodes::{TupleType, TupleTypeElement};

    let tuple = TypeExpression::Tuple(Box::new(TupleType {
        elements: vec![
            TupleTypeElement {
                name: Some(Identifier { name: "ordinal".into(), span: oak_span(7) }),
                ty: path_type("usize"),
                span: oak_span(14),
            },
            TupleTypeElement { name: Some(Identifier { name: "value".into(), span: oak_span(5) }), ty: path_type("T"), span: oak_span(7) },
        ],
        span: oak_span(30),
    }));
    assert_eq!(
        lower_type_expression(&tuple),
        ValkyrieType::Tuple(vec![ValkyrieType::Named(HirIdentifier::new("usize")), ValkyrieType::Named(HirIdentifier::new("T"))],)
    );
}

#[test]
fn rejects_ambiguous_string_type_names() {
    for name in ["string", "str", "String"] {
        let error = validate_type_expression(&path_type(name)).expect_err("ambiguous string types must be rejected");
        let message = error.to_string();
        assert!(message.contains("ambiguous text type") || message.contains("forbidden"), "message={message}");
        assert!(message.contains("utf8") && message.contains("utf16") && message.contains("c_str"), "message={message}");
    }
}

#[test]
fn accepts_explicit_text_encodings() {
    for name in ["utf8", "utf16", "c_str"] {
        validate_type_expression(&path_type(name)).expect("explicit text encodings must be accepted");
    }
    assert_eq!(canonical_builtin_type("utf8"), Some(ValkyrieType::Utf8));
    assert_eq!(canonical_builtin_type("utf16"), Some(ValkyrieType::Utf16));
    assert_eq!(canonical_builtin_type("c_str"), Some(ValkyrieType::Named(HirIdentifier::new("c_str"))));
    assert!(canonical_builtin_type("string").is_none());
}

#[test]
fn std_text_utf16_path_lowers_to_utf16_builtin() {
    let path = TypeExpression::Namepath(Box::new(NamePath {
        parts: vec![
            Identifier { name: "std".into(), span: oak_span(3) },
            Identifier { name: "text".into(), span: oak_span(4) },
            Identifier { name: "utf16".into(), span: oak_span(5) },
        ],
        span: oak_span(14),
    }));
    assert_eq!(lower_type_expression(&path), ValkyrieType::Utf16);
}

#[test]
fn generic_apply_type_lowers_to_hir_apply() {
    use nyar_language::valkyrie::frontend::ast::type_nodes::ApplyType;

    let apply = TypeExpression::Apply(Box::new(ApplyType { base: path_type("Option"), arguments: vec![path_type("i32")], span: oak_span(0) }));
    let lowered = lower_type_expression(&apply);
    // 数值裸名不是内建：`i32` 落成 `Named`，声明身份由 `bind_declared_nominal_types` 冻结。
    assert_eq!(
        lowered,
        ValkyrieType::Apply(Box::new(ValkyrieType::Named(HirIdentifier::new("Option"))), vec![ValkyrieType::Named(HirIdentifier::new("i32"))],),
    );
}

#[test]
fn named_array_is_not_structural_bracket_sugar() {
    use nyar_language::valkyrie::frontend::ast::type_nodes::ApplyType;
    let named = TypeExpression::Apply(Box::new(ApplyType { base: path_type("Array"), arguments: vec![path_type("i32")], span: oak_span(0) }));
    assert_eq!(
        lower_type_expression(&named),
        ValkyrieType::Apply(Box::new(ValkyrieType::Named(HirIdentifier::new("Array"))), vec![ValkyrieType::Named(HirIdentifier::new("i32"))],)
    );
}

#[test]
fn dotted_module_type_path_keeps_full_name() {
    let path = TypeExpression::Namepath(Box::new(NamePath {
        parts: vec![
            Identifier { name: "leetcode".into(), span: oak_span(8) },
            Identifier { name: "two_sum".into(), span: oak_span(7) },
            Identifier { name: "Solution".into(), span: oak_span(8) },
        ],
        span: oak_span(25),
    }));
    assert_eq!(
        lower_type_expression(&path),
        ValkyrieType::QualifiedPath { namespace: vec!["leetcode".into(), "two_sum".into()], name: HirIdentifier::new("Solution") }
    );
}

#[test]
fn bracket_sugar_lowers_to_array() {
    let sugar = TypeExpression::Unary(Box::new(TypeUnaryNode {
        operator: ValkyrieTokenType::BracketL,
        base: path_type("i32"),
        length: None,
        span: oak_span(0),
    }));
    let expected = ValkyrieType::Array(Box::new(ValkyrieType::Named(HirIdentifier::new("i32"))));
    assert_eq!(lower_type_expression(&sugar), expected);
}
