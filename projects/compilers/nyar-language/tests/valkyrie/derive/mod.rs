mod clone;
mod debug;
mod eq;
mod hash;
mod injection;

use nyar_language::{
    derive::*,
    types::{
        Identifier, NamePath,
        hir::{HirDocumentation, HirField, HirImpl, HirStruct, HirVisibility, ValkyrieType},
    },
};

fn create_test_struct(name: &str, fields: Vec<(&str, ValkyrieType)>) -> HirStruct {
    HirStruct {
        name: Identifier::new(name),
        namespace: vec![],
        doc: HirDocumentation::default(),
        generics: vec![],
        parents: vec![],
        fields: fields
            .into_iter()
            .map(|(name, ty)| HirField {
                name: Identifier::new(name),
                doc: HirDocumentation::default(),
                ty,
                visibility: HirVisibility::public(),
                is_mutable: false,
            })
            .collect(),
        methods: vec![],
        properties: vec![],
        visibility: HirVisibility::public(),
        is_value_type: true,
        is_abstract: false,
        is_sealed: false,
        is_final: false,
        is_open: false,
        abstract_methods: vec![],
        abstract_properties: vec![],
        derives: vec![],
    }
}

#[test]
fn test_registry_find() {
    let registry = create_builtin_registry();
    assert!(registry.find("Debug").is_some());
    assert!(registry.find("Clone").is_some());
    assert!(registry.find("Eq").is_some());
    assert!(registry.find("Hash").is_some());
    assert!(registry.find("NonExistent").is_none());
}

#[test]
fn test_available_derives() {
    let registry = create_builtin_registry();
    let derives = registry.available_derives();
    assert!(derives.contains(&"Debug"));
    assert!(derives.contains(&"Clone"));
    assert!(derives.contains(&"Eq"));
    assert!(derives.contains(&"Hash"));
}

#[test]
fn test_error_display() {
    let error = DeriveError::field_missing_trait(Identifier::new("x"), "i32", "Clone");
    let msg = format!("{}", error);
    assert!(msg.contains("字段 `x: i32`"));
    assert!(msg.contains("未实现 `Clone` trait"));
}

#[test]
fn test_conflict_error() {
    let error = DeriveError::conflict(Identifier::new("Point"), "Debug", "第 10 行");
    let msg = format!("{}", error);
    assert!(msg.contains("类型 `Point`"));
    assert!(msg.contains("已在第 10 行手动实现 `Debug`"));
}

#[test]
fn test_unknown_trait_error() {
    let error = DeriveError::unknown_trait("Serialize", vec!["Debug".into(), "Clone".into()]);
    let msg = format!("{}", error);
    assert!(msg.contains("未知的派生 trait `Serialize`"));
    assert!(msg.contains("Debug, Clone"));
}

#[test]
fn test_derive_result() {
    let mut result = DeriveResult::new();
    assert!(result.is_ok());
    assert!(!result.has_errors());

    result.add_error(DeriveError::unknown_trait("Test", vec![]));
    assert!(!result.is_ok());
    assert!(result.has_errors());
}

#[test]
fn test_derive_trait_success() {
    let registry = create_builtin_registry();
    let target =
        create_test_struct("Point", vec![("x", ValkyrieType::Integer32 { signed: true }), ("y", ValkyrieType::Integer32 { signed: true })]);

    let trait_path = NamePath::new(vec![Identifier::new("Debug")]);
    let result = registry.derive_trait(&target, &trait_path, &[]);

    assert!(result.is_ok());
    let impl_blocks = result.unwrap();
    assert_eq!(impl_blocks.len(), 1);
    assert_eq!(impl_blocks[0].methods.len(), 1);
    assert_eq!(impl_blocks[0].methods[0].name.as_str(), "format");
}

#[test]
fn test_derive_trait_conflict() {
    let registry = create_builtin_registry();
    let target =
        create_test_struct("Point", vec![("x", ValkyrieType::Integer32 { signed: true }), ("y", ValkyrieType::Integer32 { signed: true })]);

    let trait_path = NamePath::new(vec![Identifier::new("Debug")]);

    let existing_impl = HirImpl {
        generics: vec![],
        where_constraints: vec![],
        target: ValkyrieType::Named(Identifier::new("Point")),
        trait_path: Some(trait_path.clone()),
        methods: vec![],
        associated_type_impls: vec![],
        associated_const_impls: vec![],
    };

    let result = registry.derive_trait(&target, &trait_path, &[existing_impl]);
    assert!(result.is_err());

    let error = result.unwrap_err();
    assert!(matches!(error, DeriveError::Conflict { .. }));
}

#[test]
fn test_derive_all() {
    let registry = create_builtin_registry();
    let target =
        create_test_struct("Point", vec![("x", ValkyrieType::Integer32 { signed: true }), ("y", ValkyrieType::Integer32 { signed: true })]);

    let traits = vec![NamePath::new(vec![Identifier::new("Debug")]), NamePath::new(vec![Identifier::new("Clone")])];

    let result = registry.derive_all(&target, &traits, &[]);
    assert!(result.is_ok());
    assert_eq!(result.impls.len(), 2);
}

#[test]
fn test_derive_eq_generates_two_impls() {
    let registry = create_builtin_registry();
    let target =
        create_test_struct("Point", vec![("x", ValkyrieType::Integer32 { signed: true }), ("y", ValkyrieType::Integer32 { signed: true })]);

    let traits = vec![NamePath::new(vec![Identifier::new("Eq")])];

    let result = registry.derive_all(&target, &traits, &[]);
    assert!(result.is_ok());
    assert_eq!(result.impls.len(), 2);

    let trait_names: Vec<String> = result.impls.iter().filter_map(|i| i.trait_path.as_ref().map(|p| p.to_string())).collect();
    assert!(trait_names.contains(&"PartialEq".to_string()));
    assert!(trait_names.contains(&"Eq".to_string()));
}

#[test]
fn test_derive_all_with_unknown_trait() {
    let registry = create_builtin_registry();
    let target = create_test_struct("Point", vec![("x", ValkyrieType::Integer32 { signed: true })]);

    let traits = vec![NamePath::new(vec![Identifier::new("Debug")]), NamePath::new(vec![Identifier::new("UnknownTrait")])];

    let result = registry.derive_all(&target, &traits, &[]);
    assert!(result.has_errors());
    assert_eq!(result.impls.len(), 1);
    assert_eq!(result.errors.len(), 1);
}

#[test]
fn test_can_derive() {
    let registry = create_builtin_registry();
    let target = create_test_struct("Point", vec![("x", ValkyrieType::Integer32 { signed: true })]);

    assert!(registry.can_derive(&target, "Debug").is_ok());
    assert!(registry.can_derive(&target, "UnknownTrait").is_err());
}
