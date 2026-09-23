use nyar_language::{
    derive::*,
    types::{
        Identifier, NamePath,
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
fn test_eq_derive_name() {
    let derive = EqDerive::new();
    assert_eq!(derive.name(), "Eq");
}

#[test]
fn test_can_derive_simple_struct() {
    let derive = EqDerive::new();
    let target =
        create_test_struct("Point", vec![("x", ValkyrieType::Integer32 { signed: true }), ("y", ValkyrieType::Integer32 { signed: true })]);
    assert!(derive.can_derive(&target).is_ok());
}

#[test]
fn test_cannot_derive_abstract_class() {
    let derive = EqDerive::new();
    let mut target = create_test_struct("AbstractPoint", vec![]);
    target.is_abstract = true;
    assert!(derive.can_derive(&target).is_err());
}

#[test]
fn test_derive_generates_impl() {
    let derive = EqDerive::new();
    let target =
        create_test_struct("Point", vec![("x", ValkyrieType::Integer32 { signed: true }), ("y", ValkyrieType::Integer32 { signed: true })]);
    let result = derive.derive(&target);
    assert!(result.is_ok());

    let impl_blocks = result.unwrap();
    assert_eq!(impl_blocks.len(), 2);
    assert_eq!(impl_blocks[0].methods.len(), 1);
    assert_eq!(impl_blocks[0].methods[0].name.as_str(), "eq");
    assert_eq!(impl_blocks[0].trait_path, Some(NamePath::new(vec![Identifier::new("PartialEq")])));
    assert_eq!(impl_blocks[1].trait_path, Some(NamePath::new(vec![Identifier::new("Eq")])));
    assert!(impl_blocks[1].methods.is_empty());
}

#[test]
fn test_derive_empty_struct() {
    let derive = EqDerive::new();
    let target = create_test_struct("Empty", vec![]);
    let result = derive.derive(&target);
    assert!(result.is_ok());
    let impl_blocks = result.unwrap();
    assert_eq!(impl_blocks.len(), 2);
}

#[test]
fn test_generate_eq_impl() {
    let target =
        create_test_struct("Point", vec![("x", ValkyrieType::Integer32 { signed: true }), ("y", ValkyrieType::Integer32 { signed: true })]);
    let impl_block = generate_eq_impl(&target);
    assert_eq!(impl_block.trait_path, Some(NamePath::new(vec![Identifier::new("Eq")])));
    assert!(impl_block.methods.is_empty());
}
