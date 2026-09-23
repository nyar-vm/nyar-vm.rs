use nyar_language::types::{
    Identifier, NamePath,
    hir::{HirDocumentation, HirField, HirParent, HirStruct, HirVisibility, ValkyrieType},
};

fn parent(name: &str) -> HirParent {
    HirParent::new(NamePath::new(vec![Identifier::new(name)]))
}

fn aliased_parent(alias: &str, name: &str) -> HirParent {
    HirParent::with_alias(NamePath::new(vec![Identifier::new(name)]), Identifier::new(alias))
}

fn field(name: &str) -> HirField {
    HirField {
        name: Identifier::new(name),
        doc: HirDocumentation::default(),
        ty: ValkyrieType::Integer64 { signed: false },
        visibility: HirVisibility::public(),
        is_mutable: false,
    }
}

fn class_with_members(parents: Vec<HirParent>, fields: Vec<HirField>) -> HirStruct {
    HirStruct {
        name: Identifier::new("Child"),
        namespace: vec![],
        doc: HirDocumentation::default(),
        generics: vec![],
        parents,
        fields,
        methods: vec![],
        properties: vec![],
        visibility: HirVisibility::public(),
        is_value_type: false,
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
fn parent_slot_name_uses_alias_or_snake_case_type_name() {
    let class = class_with_members(vec![aliased_parent("primary", "Teacher"), parent("BaseWidget")], vec![]);

    let slots = class.parent_slot_names();

    assert_eq!(slots, vec![Identifier::new("primary"), Identifier::new("base_widget")]);
}

#[test]
fn duplicate_parent_slots_detect_alias_and_default_name_collisions() {
    let class = class_with_members(vec![aliased_parent("base_widget", "Teacher"), parent("BaseWidget")], vec![]);

    let duplicates = class.duplicate_parent_slots();

    assert_eq!(duplicates, vec![Identifier::new("base_widget")]);
}

#[test]
fn parent_slot_conflicts_with_owned_field_name() {
    let class = class_with_members(vec![parent("BaseWidget")], vec![field("base_widget"), field("id")]);

    let conflicts = class.parent_slot_field_conflicts();

    assert_eq!(conflicts, vec![Identifier::new("base_widget")]);
}

#[test]
fn parent_slot_conflict_helpers_ignore_distinct_slots() {
    let class = class_with_members(vec![aliased_parent("primary", "Teacher"), parent("StudentRecord")], vec![field("id")]);

    assert!(class.duplicate_parent_slots().is_empty());
    assert!(class.parent_slot_field_conflicts().is_empty());
}
