use rusty_valkyrie::{
    types::hir::{GenericType, HirKind, ValkyrieType},
    *,
};

use rusty_valkyrie::hir::control_flow_validation::control_flow_value_type_compatible;

#[test]
fn rejects_unbound_names_as_builtin_or_generic_types() {
    for (name, builtin) in [
        ("bool", ValkyrieType::Boolean),
        ("usize", ValkyrieType::Integer32 { signed: true }),
        ("T", ValkyrieType::Utf8),
        ("Self", ValkyrieType::SelfType),
    ] {
        let named = ValkyrieType::Named(Identifier::new(name));
        assert!(!control_flow_value_type_compatible(&builtin, &named));
        assert!(!control_flow_value_type_compatible(&named, &builtin));
    }
    let generic = ValkyrieType::Generic(GenericType { binder: None, name: Identifier::new("T"), kind: HirKind::Type, bounds: Vec::new() });
    assert!(!control_flow_value_type_compatible(&generic, &ValkyrieType::Named(Identifier::new("T"))));
}

#[test]
fn rejects_named_array_and_generic_argument_wildcards() {
    let array = ValkyrieType::Array(Box::new(ValkyrieType::Utf8));
    let named = ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Array"))), vec![ValkyrieType::Utf8]);
    assert!(!control_flow_value_type_compatible(&array, &named));
    let named_owner = ValkyrieType::Named(Identifier::new("Box"));
    let applied = ValkyrieType::Apply(Box::new(named_owner.clone()), vec![ValkyrieType::Utf8]);
    assert!(!control_flow_value_type_compatible(&named_owner, &applied));
    let fake_generic = ValkyrieType::Apply(Box::new(named_owner), vec![ValkyrieType::Named(Identifier::new("T"))]);
    assert!(!control_flow_value_type_compatible(&fake_generic, &applied));
}

#[test]
fn keeps_array_text_encodings_distinct() {
    let utf8 = ValkyrieType::Array(Box::new(ValkyrieType::Utf8));
    let utf16 = ValkyrieType::Array(Box::new(ValkyrieType::Utf16));
    assert!(control_flow_value_type_compatible(&utf8, &utf8));
    assert!(!control_flow_value_type_compatible(&utf8, &utf16));
    assert!(!control_flow_value_type_compatible(&utf16, &utf8));
}
