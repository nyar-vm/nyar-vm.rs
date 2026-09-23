use nyar_language::{
    types::{
        Identifier, SourceID, SourceSpan,
        hir::{
            HirBlock, HirDocumentation, HirField, HirFunction, HirIdentifier, HirParam, HirProperty, HirStruct, HirVisibility, ValkyrieType,
        },
    },
    valkyrie::row::{RowMethodSignature, RowRequirement, RowRequirementError, RowRequirementItem},
};

#[test]
fn row_syntax_is_not_a_named_trait_entity() {
    let first = RowRequirement::from_methods(vec![method_sig("g", vec![], ValkyrieType::Unit)]);
    let second = RowRequirement::from_methods(vec![method_sig("g", vec![], ValkyrieType::Unit)]);

    assert_eq!(first, second);
    assert_eq!(first.methods().len(), 1);
}

#[test]
fn row_satisfaction_is_method_based() {
    let requirement = RowRequirement::from_methods(vec![method_sig("size", vec![], int32())]);
    let field_only = struct_with_field("HasFieldOnly", "size", int32());
    let method_based = struct_with_methods("HasMethod", vec![method("size", vec![], int32())]);

    assert!(!requirement.is_satisfied_by(&field_only));
    assert!(requirement.is_satisfied_by(&method_based));
}

#[test]
fn public_field_exposes_getter_and_setter_rows() {
    let requirement =
        RowRequirement::from_methods(vec![method_sig("get_size", vec![], int32()), method_sig("set_size", vec![int32()], ValkyrieType::Unit)]);
    let candidate = struct_with_field("HasPublicField", "size", int32());

    assert!(requirement.is_satisfied_by(&candidate));
}

#[test]
fn readonly_public_field_exposes_only_getter_row() {
    let getter_requirement = RowRequirement::from_methods(vec![method_sig("get_id", vec![], int64())]);
    let setter_requirement = RowRequirement::from_methods(vec![method_sig("set_id", vec![int64()], ValkyrieType::Unit)]);
    let candidate = struct_with_field_with_visibility("Entity", "id", int64(), HirVisibility::public(), false);

    assert!(getter_requirement.is_satisfied_by(&candidate));
    assert!(!setter_requirement.is_satisfied_by(&candidate));
}

#[test]
fn private_field_does_not_expose_row_methods() {
    let requirement = RowRequirement::from_methods(vec![method_sig("get_secret", vec![], ValkyrieType::Utf8)]);
    let candidate = struct_with_field_with_visibility("Vault", "secret", ValkyrieType::Utf8, HirVisibility::private(), false);

    assert!(!requirement.is_satisfied_by(&candidate));
}

#[test]
fn private_method_does_not_participate_in_row_validation() {
    let requirement = RowRequirement::from_methods(vec![method_sig("secret", vec![], ValkyrieType::Unit)]);
    let candidate = struct_with_methods_and_visibility(
        "Vault",
        vec![method_with_visibility("secret", vec![], ValkyrieType::Unit, HirVisibility::private())],
    );

    assert!(!requirement.is_satisfied_by(&candidate));
}

#[test]
fn public_property_exposes_accessor_rows() {
    let requirement =
        RowRequirement::from_methods(vec![method_sig("area", vec![], int32()), method_sig("set_area", vec![int32()], ValkyrieType::Unit)]);
    let candidate = struct_with_properties("Rect", vec![property_with_visibility("area", int32(), HirVisibility::public(), false)]);

    assert!(requirement.is_satisfied_by(&candidate));
}

#[test]
fn readonly_public_property_exposes_only_getter_row() {
    let getter_requirement = RowRequirement::from_methods(vec![method_sig("area", vec![], int32())]);
    let setter_requirement = RowRequirement::from_methods(vec![method_sig("set_area", vec![int32()], ValkyrieType::Unit)]);
    let candidate = struct_with_properties("Rect", vec![property_with_visibility("area", int32(), HirVisibility::public(), true)]);

    assert!(getter_requirement.is_satisfied_by(&candidate));
    assert!(!setter_requirement.is_satisfied_by(&candidate));
}

#[test]
fn private_property_does_not_participate_in_row_validation() {
    let requirement = RowRequirement::from_methods(vec![method_sig("area", vec![], int32())]);
    let candidate = struct_with_properties("Rect", vec![property_with_visibility("area", int32(), HirVisibility::private(), true)]);

    assert!(!requirement.is_satisfied_by(&candidate));
}

#[test]
fn mixed_visibility_members_only_contribute_public_rows() {
    let requirement = RowRequirement::from_methods(vec![
        method_sig("ping", vec![], ValkyrieType::Unit),
        method_sig("get_size", vec![], int32()),
        method_sig("set_size", vec![int32()], ValkyrieType::Unit),
        method_sig("area", vec![], int32()),
        method_sig("set_area", vec![int32()], ValkyrieType::Unit),
    ]);
    let candidate = struct_with_members(
        "Surface",
        vec![
            HirField {
                name: Identifier::new("size"),
                doc: HirDocumentation::default(),
                ty: int32(),
                visibility: HirVisibility::public(),
                is_mutable: true,
            },
            HirField {
                name: Identifier::new("secret"),
                doc: HirDocumentation::default(),
                ty: ValkyrieType::Utf8,
                visibility: HirVisibility::private(),
                is_mutable: false,
            },
        ],
        vec![
            method_with_visibility("ping", vec![], ValkyrieType::Unit, HirVisibility::public()),
            method_with_visibility("hidden", vec![], ValkyrieType::Unit, HirVisibility::private()),
        ],
        vec![
            property_with_visibility("area", int32(), HirVisibility::public(), false),
            property_with_visibility("code", ValkyrieType::Utf8, HirVisibility::private(), false),
        ],
    );

    assert!(requirement.is_satisfied_by(&candidate));
    assert!(!RowRequirement::from_methods(vec![method_sig("get_secret", vec![], ValkyrieType::Utf8)]).is_satisfied_by(&candidate));
    assert!(!RowRequirement::from_methods(vec![method_sig("hidden", vec![], ValkyrieType::Unit)]).is_satisfied_by(&candidate));
    assert!(!RowRequirement::from_methods(vec![method_sig("code", vec![], ValkyrieType::Utf8)]).is_satisfied_by(&candidate));
}

#[test]
fn row_rejects_missing_methods() {
    let requirement =
        RowRequirement::from_methods(vec![method_sig("read", vec![], ValkyrieType::Utf8), method_sig("close", vec![], ValkyrieType::Unit)]);
    let candidate = struct_with_methods("Reader", vec![method("read", vec![], ValkyrieType::Utf8)]);

    let errors = requirement.check_struct(&candidate).unwrap_err();

    assert_eq!(errors, vec![RowRequirementError::MissingMethod { name: Identifier::new("close") }]);
}

#[test]
fn row_rejects_signature_mismatch() {
    let requirement = RowRequirement::from_methods(vec![method_sig("write", vec![ValkyrieType::Utf8], ValkyrieType::Unit)]);
    let candidate = struct_with_methods("Writer", vec![method("write", vec![int32()], ValkyrieType::Unit)]);

    let errors = requirement.check_struct(&candidate).unwrap_err();

    assert_eq!(errors.len(), 1);
    assert!(matches!(
        &errors[0],
        RowRequirementError::SignatureMismatch { name, expected, actual }
        if name == &Identifier::new("write")
            && expected == &method_sig("write", vec![ValkyrieType::Utf8], ValkyrieType::Unit)
            && actual == &method_sig("write", vec![int32()], ValkyrieType::Unit)
    ));
}

#[test]
fn row_rejects_associated_types() {
    let error = RowRequirement::try_from_items(vec![
        RowRequirementItem::Method(method_sig("next", vec![], ValkyrieType::Named(Identifier::new("Option")))),
        RowRequirementItem::AssociatedType(Identifier::new("Item")),
    ])
    .unwrap_err();

    assert_eq!(error, RowRequirementError::UnsupportedAssociatedType { name: Identifier::new("Item") });
}

#[test]
fn row_rejects_duplicate_method_requirements() {
    let requirement =
        RowRequirement::from_methods(vec![method_sig("read", vec![], ValkyrieType::Utf8), method_sig("read", vec![], ValkyrieType::Utf8)]);
    let candidate = struct_with_methods("Reader", vec![method("read", vec![], ValkyrieType::Utf8)]);

    let errors = requirement.check_struct(&candidate).unwrap_err();

    assert_eq!(errors, vec![RowRequirementError::DuplicateMethodRequirement { name: Identifier::new("read") }]);
}

#[test]
fn row_rejects_ambiguous_candidate_methods() {
    let requirement = RowRequirement::from_methods(vec![method_sig("write", vec![ValkyrieType::Utf8], ValkyrieType::Unit)]);
    let candidate = struct_with_methods(
        "OverloadedWriter",
        vec![method("write", vec![ValkyrieType::Utf8], ValkyrieType::Unit), method("write", vec![int32()], ValkyrieType::Unit)],
    );

    let errors = requirement.check_struct(&candidate).unwrap_err();

    assert_eq!(errors, vec![RowRequirementError::AmbiguousCandidateMethod { name: Identifier::new("write") }]);
}

fn method_sig(name: &str, params: Vec<ValkyrieType>, return_type: ValkyrieType) -> RowMethodSignature {
    RowMethodSignature::new(name, params, return_type)
}

fn struct_with_field(name: &str, field_name: &str, field_type: ValkyrieType) -> HirStruct {
    struct_with_field_with_visibility(name, field_name, field_type, HirVisibility::public(), true)
}

fn struct_with_field_with_visibility(
    name: &str,
    field_name: &str,
    field_type: ValkyrieType,
    visibility: HirVisibility,
    is_mutable: bool,
) -> HirStruct {
    HirStruct {
        name: Identifier::new(name),
        namespace: vec![],
        doc: HirDocumentation::default(),
        generics: vec![],
        parents: vec![],
        fields: vec![HirField { name: Identifier::new(field_name), doc: HirDocumentation::default(), ty: field_type, visibility, is_mutable }],
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

fn struct_with_methods(name: &str, methods: Vec<HirFunction>) -> HirStruct {
    struct_with_members(name, vec![], methods, vec![])
}

fn struct_with_methods_and_visibility(name: &str, methods: Vec<HirFunction>) -> HirStruct {
    struct_with_members(name, vec![], methods, vec![])
}

fn struct_with_properties(name: &str, properties: Vec<HirProperty>) -> HirStruct {
    struct_with_members(name, vec![], vec![], properties)
}

fn struct_with_members(name: &str, fields: Vec<HirField>, methods: Vec<HirFunction>, properties: Vec<HirProperty>) -> HirStruct {
    HirStruct {
        name: Identifier::new(name),
        namespace: vec![],
        doc: HirDocumentation::default(),
        generics: vec![],
        parents: vec![],
        fields,
        methods,
        properties,
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

fn method(name: &str, params: Vec<ValkyrieType>, return_type: ValkyrieType) -> HirFunction {
    method_with_visibility(name, params, return_type, HirVisibility::public())
}

fn method_with_visibility(name: &str, params: Vec<ValkyrieType>, return_type: ValkyrieType, visibility: HirVisibility) -> HirFunction {
    HirFunction {
        name: Identifier::new(name),
        declaring_namespace: nyar_language::types::NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: vec![],
        generics: vec![],
        params: params
            .into_iter()
            .enumerate()
            .map(|(index, ty)| HirParam {
                name: HirIdentifier { name: Identifier::new(&format!("arg{index}")), shadow_index: 0, span: span() },
                ty,
                ..Default::default()
            })
            .collect(),
        return_type,
        body: HirBlock { statements: vec![], expr: None, span: span() },
        span: span(),
        visibility,
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    }
}

fn property_with_visibility(name: &str, ty: ValkyrieType, visibility: HirVisibility, is_readonly: bool) -> HirProperty {
    HirProperty {
        name: Identifier::new(name),
        doc: HirDocumentation::default(),
        ty: ty.clone(),
        getter: Some(method_with_visibility(name, vec![], ty.clone(), visibility)),
        setter: (!is_readonly).then(|| method_with_visibility(&format!("set_{name}"), vec![ty], ValkyrieType::Unit, visibility)),
        is_readonly,
        visibility,
        is_abstract: false,
        is_final: false,
        is_static: false,
        is_virtual: false,
        is_override: false,
        is_lazy: false,
        lazy_backing_field: None,
    }
}

fn span() -> SourceSpan {
    SourceSpan::new(SourceID::default(), 0, 0)
}

fn int32() -> ValkyrieType {
    ValkyrieType::Integer32 { signed: true }
}

fn int64() -> ValkyrieType {
    ValkyrieType::Integer64 { signed: true }
}
