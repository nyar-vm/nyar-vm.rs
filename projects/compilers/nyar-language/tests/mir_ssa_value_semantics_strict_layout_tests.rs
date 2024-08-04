use nyar::NyarType;
use nyar_language::{
    mir::ssa::{FieldSpec, build_aggregate_layout, ensure_unite_tagged_layout},
    valkyrie::types::hir::types::ValkyrieType,
    *,
};
use std::collections::BTreeSet;

#[test]
fn unite_tagged_layout_uses_strict_concretize() {
    let mut plan = AggregateLayoutPlan::default();
    let declaration = nyar_types::ItemId::from_index(7).expect("test ItemId");
    let id = ensure_unite_tagged_layout(&mut plan, declaration, "Option");
    assert_eq!(plan.declaration_to_layout.get(&declaration).copied(), Some(id));
    assert!(plan.synthetic_layouts.is_empty(), "unite 布局不得进入合成布局表");
    let layout = plan.layouts.iter().find(|layout| layout.id == id).expect("layout");
    assert_eq!(layout.fields[0].ty, NyarType::Integer32 { signed: true });
    assert_eq!(layout.fields[1].ty, NyarType::Utf8);
}

#[test]
#[should_panic(expected = "无法严格 concretize")]
fn layout_panics_on_unsubstituted_self_field() {
    let _ = build_aggregate_layout(
        1,
        "Bad",
        String::new(),
        MirStorageKind::Value,
        vec![FieldSpec { name: "x".to_string(), ty: ValkyrieType::SelfType, size: 8, align: 8 }],
    );
}

#[test]
fn storage_kind_keys_value_by_declaration_not_named() {
    let point = nyar_types::ItemId::from_index(1).expect("test ItemId");
    let mut value_declarations = BTreeSet::new();
    value_declarations.insert(point);
    assert_eq!(storage_kind_for_type(&ValkyrieType::Tuple(vec![ValkyrieType::Boolean]), &value_declarations), MirStorageKind::Value);
    assert_eq!(
        storage_kind_for_type(
            &ValkyrieType::FixedArray { element: Box::new(ValkyrieType::Integer32 { signed: true }), length: 4 },
            &value_declarations
        ),
        MirStorageKind::Value
    );
    assert_eq!(
        storage_kind_for_type(&ValkyrieType::Array(Box::new(ValkyrieType::Integer32 { signed: true })), &value_declarations),
        MirStorageKind::Reference
    );
    assert_eq!(
        storage_kind_for_type(&ValkyrieType::Nominal { declaration: point, arguments: Vec::new() }, &value_declarations),
        MirStorageKind::Value
    );
    assert_eq!(
        storage_kind_for_type(
            &ValkyrieType::Nominal { declaration: nyar_types::ItemId::from_index(2).expect("test ItemId"), arguments: Vec::new() },
            &value_declarations
        ),
        MirStorageKind::Reference
    );
    // Named 短名不得再认亲为 value 存储。
    assert_eq!(
        storage_kind_for_type(&ValkyrieType::Named(nyar_language::types::Identifier::new("Point")), &value_declarations),
        MirStorageKind::Reference
    );
}
