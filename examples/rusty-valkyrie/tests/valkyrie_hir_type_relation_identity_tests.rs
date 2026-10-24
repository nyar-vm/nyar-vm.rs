use rusty_valkyrie::{
    {
        hir::type_relation::{ParameterMatchResult, TypeRelationContext},
        types::hir::{HirModule, types::ValkyrieType},
    },
    *,
};

#[test]
fn unresolved_primitive_and_array_spellings_do_not_create_type_equivalence() {
    let context = TypeRelationContext::from_module(&HirModule::default());
    for (spelling, actual) in [
        ("f64", ValkyrieType::Float64),
        ("core::primitive::f64", ValkyrieType::Float64),
        ("Utf8Text", ValkyrieType::Utf8),
        ("Array", ValkyrieType::Array(Box::new(ValkyrieType::Float64))),
    ] {
        let unresolved = ValkyrieType::Named(Identifier::new(spelling));
        assert!(matches!(context.match_parameter(&actual, &unresolved), ParameterMatchResult::NoMatch { .. }));
        assert!(matches!(context.match_parameter(&unresolved, &actual), ParameterMatchResult::NoMatch { .. }));
        assert_eq!(context.match_parameter(&actual, &actual), ParameterMatchResult::NominalExact);
    }
}

#[test]
fn unresolved_name_cannot_match_a_bound_nominal_declaration() {
    let hir = rusty_valkyrie::ValkyrieCompiler::default().compile_source("structure Record { value: i32 }").expect("结构声明解析");
    let context = TypeRelationContext::from_module(&hir);
    let item = hir.structs.first().expect("结构声明");
    let nominal = ValkyrieType::Nominal { declaration: item.constructor_declaration.expect("冻结声明身份"), arguments: vec![] };
    let unresolved = ValkyrieType::Named(item.name.clone());
    assert!(matches!(context.match_parameter(&nominal, &unresolved), ParameterMatchResult::NoMatch { .. }));
    assert!(matches!(context.match_parameter(&unresolved, &nominal), ParameterMatchResult::NoMatch { .. }));
    assert_eq!(context.match_parameter(&nominal, &nominal), ParameterMatchResult::NominalExact);
}
