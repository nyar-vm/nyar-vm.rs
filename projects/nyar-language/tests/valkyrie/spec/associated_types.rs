use nyar_language::{
    types::{
        Identifier, NamePath, SourceID, SourceSpan,
        hir::{
            HirAssociatedType, HirAssociatedTypeImpl, HirBlock, HirDocumentation, HirFunction, HirIdentifier, HirImpl, HirParam, HirStruct,
            HirTrait, HirVisibility, HirWhereConstraint, ValkyrieType,
        },
    },
    valkyrie::{
        row::{RowMethodSignature, RowRequirement, RowRequirementError, RowRequirementItem},
        trait_system::{TraitSatisfactionError, resolve_associated_type},
    },
};

fn int32_type() -> ValkyrieType {
    ValkyrieType::Integer32 { signed: true }
}

#[test]
fn associated_types_belong_only_to_named_traits() {
    let row_error = RowRequirement::try_from_items(vec![
        RowRequirementItem::Method(RowMethodSignature::new("next", vec![], int32_type())),
        RowRequirementItem::AssociatedType(Identifier::new("Item")),
    ])
    .unwrap_err();

    assert_eq!(row_error, RowRequirementError::UnsupportedAssociatedType { name: Identifier::new("Item") });

    let iterator_trait = iterator_trait();
    let counter = struct_with_methods("Counter", vec![method("next", vec![], int32_type())]);
    let explicit_impl = trait_impl("Counter", "Iterator", vec![HirAssociatedTypeImpl::new(Identifier::new("Item"), int32_type(), span())]);

    let resolved = resolve_associated_type(&counter, &iterator_trait, &[explicit_impl], &Identifier::new("Item")).unwrap();
    assert_eq!(resolved, int32_type());
}

#[test]
fn associated_type_solution_must_be_unique() {
    let iterator_trait = iterator_trait();
    let counter = struct_with_methods("Counter", vec![method("next", vec![], int32_type())]);
    let explicit_impl = trait_impl("Counter", "Iterator", vec![HirAssociatedTypeImpl::new(Identifier::new("Item"), int32_type(), span())]);

    let resolved = resolve_associated_type(&counter, &iterator_trait, &[explicit_impl], &Identifier::new("Item")).unwrap();
    assert_eq!(resolved, int32_type());
}

#[test]
fn ambiguous_associated_type_inference_fails() {
    let iterator_trait = iterator_trait();
    let counter = struct_with_methods("Counter", vec![method("next", vec![], int32_type())]);
    let first_impl = trait_impl("Counter", "Iterator", vec![HirAssociatedTypeImpl::new(Identifier::new("Item"), int32_type(), span())]);
    let second_impl = trait_impl("Counter", "Iterator", vec![HirAssociatedTypeImpl::new(Identifier::new("Item"), ValkyrieType::Utf8, span())]);

    let error = resolve_associated_type(&counter, &iterator_trait, &[first_impl, second_impl], &Identifier::new("Item")).unwrap_err();

    assert_eq!(
        error,
        TraitSatisfactionError::AmbiguousExplicitImpls {
            trait_path: NamePath::new(vec![Identifier::new("Iterator")]),
            target: ValkyrieType::Named(Identifier::new("Counter")),
        }
    );
}

#[test]
fn more_specific_where_impl_wins_for_associated_type_resolution() {
    let iterator_trait = iterator_trait();
    let counter = struct_with_methods("Counter", vec![method("next", vec![], int32_type())]);
    let general_impl = HirImpl {
        where_constraints: vec![where_constraint(ValkyrieType::Named(Identifier::new("T")), vec!["Clone"])],
        ..trait_impl("Counter", "Iterator", vec![HirAssociatedTypeImpl::new(Identifier::new("Item"), int32_type(), span())])
    };
    let specific_impl = HirImpl {
        where_constraints: vec![where_constraint(ValkyrieType::Named(Identifier::new("T")), vec!["Clone", "Debug"])],
        ..trait_impl("Counter", "Iterator", vec![HirAssociatedTypeImpl::new(Identifier::new("Item"), ValkyrieType::Utf8, span())])
    };

    let resolved = resolve_associated_type(&counter, &iterator_trait, &[general_impl, specific_impl], &Identifier::new("Item")).unwrap();
    assert_eq!(resolved, ValkyrieType::Utf8);
}

#[test]
fn incomparable_where_impls_keep_associated_type_resolution_ambiguous() {
    let iterator_trait = iterator_trait();
    let counter = struct_with_methods("Counter", vec![method("next", vec![], int32_type())]);
    let first_impl = HirImpl {
        where_constraints: vec![where_constraint(ValkyrieType::Named(Identifier::new("T")), vec!["Clone"])],
        ..trait_impl("Counter", "Iterator", vec![HirAssociatedTypeImpl::new(Identifier::new("Item"), int32_type(), span())])
    };
    let second_impl = HirImpl {
        where_constraints: vec![where_constraint(ValkyrieType::Named(Identifier::new("T")), vec!["Debug"])],
        ..trait_impl("Counter", "Iterator", vec![HirAssociatedTypeImpl::new(Identifier::new("Item"), ValkyrieType::Utf8, span())])
    };

    let error = resolve_associated_type(&counter, &iterator_trait, &[first_impl, second_impl], &Identifier::new("Item")).unwrap_err();

    assert_eq!(
        error,
        TraitSatisfactionError::AmbiguousExplicitImpls {
            trait_path: NamePath::new(vec![Identifier::new("Iterator")]),
            target: ValkyrieType::Named(Identifier::new("Counter")),
        }
    );
}

#[test]
fn missing_associated_type_binding_fails() {
    let iterator_trait = iterator_trait();
    let counter = struct_with_methods("Counter", vec![method("next", vec![], int32_type())]);
    let explicit_impl = trait_impl("Counter", "Iterator", vec![]);

    let error = resolve_associated_type(&counter, &iterator_trait, &[explicit_impl], &Identifier::new("Item")).unwrap_err();

    assert_eq!(
        error,
        TraitSatisfactionError::MissingAssociatedTypeBinding {
            trait_path: NamePath::new(vec![Identifier::new("Iterator")]),
            name: Identifier::new("Item"),
        }
    );
}

fn iterator_trait() -> HirTrait {
    HirTrait {
        name: Identifier::new("Iterator"),
        doc: HirDocumentation::default(),
        generics: vec![],
        methods: vec![method("next", vec![], int32_type())],
        associated_types: vec![HirAssociatedType::new(Identifier::new("Item"), span())],
        associated_constants: vec![],
        super_traits: vec![],
        default_methods: vec![],
        visibility: HirVisibility::public(),
    }
}

fn struct_with_methods(name: &str, methods: Vec<HirFunction>) -> HirStruct {
    HirStruct {
        name: Identifier::new(name),
        namespace: vec![],
        doc: HirDocumentation::default(),
        generics: vec![],
        parents: vec![],
        fields: vec![],
        methods,
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

fn trait_impl(target: &str, trait_name: &str, associated_type_impls: Vec<HirAssociatedTypeImpl>) -> HirImpl {
    HirImpl {
        generics: vec![],
        where_constraints: vec![],
        target: ValkyrieType::Named(Identifier::new(target)),
        trait_path: Some(NamePath::new(vec![Identifier::new(trait_name)])),
        methods: vec![],
        associated_type_impls,
        associated_const_impls: vec![],
    }
}

fn where_constraint(target: ValkyrieType, bounds: Vec<&str>) -> HirWhereConstraint {
    HirWhereConstraint { target, bounds: bounds.into_iter().map(|name| NamePath::new(vec![Identifier::new(name)])).collect(), span: span() }
}

fn method(name: &str, params: Vec<ValkyrieType>, return_type: ValkyrieType) -> HirFunction {
    HirFunction {
        name: Identifier::new(name),
        declaring_namespace: NamePath::default(),
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
        visibility: HirVisibility::public(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    }
}

fn span() -> SourceSpan {
    SourceSpan::new(SourceID::default(), 0, 0)
}
