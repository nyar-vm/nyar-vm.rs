use nyar_language::{
    derive::*,
    types::{
        Identifier,
        hir::{HirDocumentation, HirField, HirVisibility, ValkyrieType},
    },
};

fn create_test_struct(name: &str, fields: Vec<(&str, ValkyrieType)>) -> nyar_language::types::hir::HirStruct {
    nyar_language::types::hir::HirStruct {
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
fn test_hash_derive_name() {
    let derive = HashDerive::new();
    assert_eq!(derive.name(), "Hash");
}

#[test]
fn test_can_derive_simple_struct() {
    let derive = HashDerive::new();
    let target =
        create_test_struct("Point", vec![("x", ValkyrieType::Integer32 { signed: true }), ("y", ValkyrieType::Integer32 { signed: true })]);
    assert!(derive.can_derive(&target).is_ok());
}

#[test]
fn test_cannot_derive_abstract_class() {
    let derive = HashDerive::new();
    let mut target = create_test_struct("AbstractPoint", vec![]);
    target.is_abstract = true;
    assert!(derive.can_derive(&target).is_err());
}

#[test]
fn test_derive_generates_impl() {
    let derive = HashDerive::new();
    let target =
        create_test_struct("Point", vec![("x", ValkyrieType::Integer32 { signed: true }), ("y", ValkyrieType::Integer32 { signed: true })]);
    let result = derive.derive(&target);
    assert!(result.is_ok());

    let impl_blocks = result.unwrap();
    assert_eq!(impl_blocks.len(), 1);
    assert_eq!(impl_blocks[0].methods.len(), 1);
    assert_eq!(impl_blocks[0].methods[0].name.as_str(), "hash");
}

#[test]
fn test_derive_empty_struct() {
    let derive = HashDerive::new();
    let target = create_test_struct("Empty", vec![]);
    let result = derive.derive(&target);
    assert!(result.is_ok());
}

#[test]
fn test_hash_with_string_field() {
    let derive = HashDerive::new();
    let target = create_test_struct("Person", vec![("name", ValkyrieType::Utf8), ("age", ValkyrieType::Integer32 { signed: true })]);
    assert!(derive.can_derive(&target).is_ok());
}
