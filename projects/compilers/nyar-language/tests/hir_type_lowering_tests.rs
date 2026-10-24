use nyar_language::{
    SourceID, ValkyrieCompiler,
    types::{Identifier as HirIdentifier, hir::ValkyrieType},
    valkyrie::{
        HirError,
        frontend::ast::{Identifier, NamePath, TypeExpression, type_nodes::TypeUnaryNode},
        hir::type_lowering::{canonical_builtin_type, lower_type_expression, validate_type_expression},
        types::hir::{HirModule, HirStruct},
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
            TupleTypeElement { name: Some(Identifier { name: "value".into(), span: oak_span(5) }), ty: path_type("u32"), span: oak_span(7) },
        ],
        span: oak_span(30),
    }));
    assert_eq!(
        lower_type_expression(&tuple).expect("named tuple lowers"),
        ValkyrieType::Tuple(vec![ValkyrieType::Named(HirIdentifier::new("usize")), ValkyrieType::Named(HirIdentifier::new("u32"))],)
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
    assert_eq!(lower_type_expression(&path).expect("std text path lowers"), ValkyrieType::Utf16);
}

#[test]
fn unknown_generic_apply_base_is_rejected_before_nominal_binding() {
    use nyar_language::valkyrie::frontend::ast::type_nodes::ApplyType;

    // `Option` 不在作用域内：不得退化成 `Named("Option")` 带进 canonical，必须结构化失败。
    let apply = TypeExpression::Apply(Box::new(ApplyType { base: path_type("Option"), arguments: vec![path_type("i32")], span: oak_span(0) }));
    let error = lower_type_expression(&apply).expect_err("未知泛型基底必须结构化失败");
    assert!(matches!(error, HirError::TypeNotFound { .. }), "error={error:?}");
}

#[test]
fn named_array_apply_is_not_structural_bracket_sugar() {
    use nyar_language::valkyrie::frontend::ast::type_nodes::ApplyType;
    let named = TypeExpression::Apply(Box::new(ApplyType { base: path_type("usize"), arguments: vec![path_type("i32")], span: oak_span(0) }));
    assert_eq!(
        lower_type_expression(&named).expect("named apply lowers"),
        ValkyrieType::Apply(Box::new(ValkyrieType::Named(HirIdentifier::new("usize"))), vec![ValkyrieType::Named(HirIdentifier::new("i32"))],)
    );
}

#[test]
fn dotted_module_type_path_is_rejected_before_canonical() {
    let path = TypeExpression::Namepath(Box::new(NamePath {
        parts: vec![
            Identifier { name: "leetcode".into(), span: oak_span(8) },
            Identifier { name: "two_sum".into(), span: oak_span(7) },
            Identifier { name: "Solution".into(), span: oak_span(8) },
        ],
        span: oak_span(25),
    }));
    // 多段限定名不得以 `QualifiedPath` 旁路进入 canonical：短名无法在作用域内绑定即结构化失败。
    let error = lower_type_expression(&path).expect_err("未绑定的限定类型名必须结构化失败");
    assert!(matches!(error, HirError::TypeNotFound { .. }), "error={error:?}");
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
    assert_eq!(lower_type_expression(&sugar).expect("bracket sugar lowers"), expected);
}

const NUMERIC_PRIMITIVE_NAMES: [&str; 16] =
    ["i8", "i16", "i32", "i64", "i128", "u8", "u16", "u32", "u64", "u128", "isize", "usize", "f32", "f64", "f128", "byte"];

fn compile(source: &str) -> Result<HirModule, HirError> {
    ValkyrieCompiler::new(SourceID::default()).compile_source(source)
}

fn find_struct<'a>(hir: &'a HirModule, name: &str) -> &'a HirStruct {
    hir.structs.iter().find(|item| item.name.as_str() == name).unwrap_or_else(|| panic!("缺少结构 `{name}`"))
}

fn declaration_of(hir: &HirModule, name: &str) -> nyar_types::ItemId {
    find_struct(hir, name).constructor_declaration.unwrap_or_else(|| panic!("结构 `{name}` 须已分配 constructor_declaration"))
}

/// 正例：数值名 lowering 为声明引用，不生产 carrier。
#[test]
fn numeric_primitive_names_lower_to_declaration_reference_not_carrier() {
    for name in NUMERIC_PRIMITIVE_NAMES {
        assert!(canonical_builtin_type(name).is_none(), "`{name}` 不是无声明身份的内建");
        assert_eq!(
            lower_type_expression(&path_type(name)).expect("数值名必须是待绑定的声明引用"),
            ValkyrieType::Named(HirIdentifier::new(name)),
            "`{name}` 必须是待绑定的声明引用，不得按名字生产低层 carrier"
        );
    }
}

/// 正例：可见的 `core/primitive` 声明把 `Named` 冻结成 `Nominal{ItemId}`。
#[test]
fn declared_numeric_primitive_binds_to_nominal_declaration() {
    let hir = compile(
        r#"
[primitive("core::primitive::i32")]
structure i32 { }

[primitive("core::primitive::usize")]
structure usize { }

micro add(lhs: i32, rhs: i32) -> i32 {
    return lhs
}

micro index(n: usize) -> usize {
    return n
}
"#,
    )
    .expect("已声明的数值 primitive 必须绑定为声明身份");

    let i32_declaration = declaration_of(&hir, "i32");
    let add = hir.functions.iter().find(|function| function.name.as_str() == "add").expect("缺少 `add`");
    for param in &add.params {
        assert_eq!(
            param.ty,
            ValkyrieType::Nominal { declaration: i32_declaration, arguments: vec![] },
            "形参类型必须是 `i32` 声明身份，不得是同名的低层 carrier"
        );
    }
    assert_eq!(add.return_type, ValkyrieType::Nominal { declaration: i32_declaration, arguments: vec![] });

    let usize_declaration = declaration_of(&hir, "usize");
    let index = hir.functions.iter().find(|function| function.name.as_str() == "index").expect("缺少 `index`");
    assert_eq!(index.params[0].ty, ValkyrieType::Nominal { declaration: usize_declaration, arguments: vec![] });
    assert_eq!(index.return_type, ValkyrieType::Nominal { declaration: usize_declaration, arguments: vec![] });
}

/// 反例：没有可见声明时不得凭名字获得 carrier；HIR 只保留待绑定引用，
/// 未绑定 `Named` 会由 canonical 以结构化诊断拒绝，而不是默认成 `Integer32`/`Float64`。
#[test]
fn undeclared_numeric_primitive_does_not_gain_carrier() {
    let hir = compile(
        r#"
micro probe(x: i32) -> i32 {
    return x
}
"#,
    )
    .expect("HIR 阶段只冻结已解析事实");

    let probe = hir.functions.iter().find(|function| function.name.as_str() == "probe").expect("缺少 `probe`");
    assert_eq!(probe.params[0].ty, ValkyrieType::Named(HirIdentifier::new("i32")));
    assert_eq!(probe.return_type, ValkyrieType::Named(HirIdentifier::new("i32")));
    assert_ne!(probe.params[0].ty, ValkyrieType::Integer32 { signed: true });
}
