use rusty_valkyrie::{
    NamePath,
    types::{
        Identifier,
        hir::{HirDependencySemanticExport, HirEnum, HirField, HirModule, HirVariant, HirVisibility},
    },
    {
        hir::nominal::{NominalModuleView, UniteCoverageError, UniteDefinitionError, UniteLayout, lower_unite, validate_unite_definition},
        types::hir::{
            HirDocumentation,
            types::{GenericType, HirKind, ValkyrieType},
        },
    },
};

fn option_unite() -> HirEnum {
    let mut enum_def = HirEnum::new_unity(Identifier::new("Option"));
    enum_def.visibility = HirVisibility::public();
    enum_def.variants = vec![
        HirVariant {
            name: Identifier::new("Some"),
            doc: HirDocumentation::default(),
            fields: vec![HirField {
                id: None,
                name: Identifier::new("value"),
                doc: HirDocumentation::default(),
                ty: ValkyrieType::Integer32 { signed: true },
                visibility: HirVisibility::public(),
                is_mutable: false,
            }],
            result_type: None,
            discriminator: None,
            ..Default::default()
        },
        HirVariant {
            name: Identifier::new("None"),
            doc: HirDocumentation::default(),
            fields: vec![],
            result_type: None,
            discriminator: None,
            ..Default::default()
        },
    ];
    enum_def
}

fn gadt_unite() -> HirEnum {
    let mut enum_def = HirEnum::new_unity(Identifier::new("Expr"));
    enum_def.visibility = HirVisibility::public();
    enum_def.generics = vec![GenericType { binder: None, name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] }];
    enum_def.variants = vec![
        HirVariant {
            name: Identifier::new("Literal"),
            doc: HirDocumentation::default(),
            fields: vec![HirField {
                id: None,
                name: Identifier::new("value"),
                doc: HirDocumentation::default(),
                ty: ValkyrieType::Float64,
                visibility: HirVisibility::public(),
                is_mutable: false,
            }],
            result_type: Some(ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Expr"))), vec![ValkyrieType::Float64])),
            discriminator: None,
            ..Default::default()
        },
        HirVariant {
            name: Identifier::new("If"),
            doc: HirDocumentation::default(),
            fields: vec![
                HirField {
                    id: None,
                    name: Identifier::new("condition"),
                    doc: HirDocumentation::default(),
                    ty: ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Expr"))), vec![ValkyrieType::Boolean]),
                    visibility: HirVisibility::public(),
                    is_mutable: false,
                },
                HirField {
                    id: None,
                    name: Identifier::new("then_branch"),
                    doc: HirDocumentation::default(),
                    ty: ValkyrieType::Apply(
                        Box::new(ValkyrieType::Named(Identifier::new("Expr"))),
                        vec![ValkyrieType::Generic(GenericType {
                            binder: None,
                            name: Identifier::new("T"),
                            kind: HirKind::Type,
                            bounds: vec![],
                        })],
                    ),
                    visibility: HirVisibility::public(),
                    is_mutable: false,
                },
            ],
            result_type: Some(ValkyrieType::Apply(
                Box::new(ValkyrieType::Named(Identifier::new("Expr"))),
                vec![ValkyrieType::Generic(GenericType { binder: None, name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] })],
            )),
            discriminator: None,
            ..Default::default()
        },
    ];
    enum_def
}

fn module_with_imported_unite(enum_def: HirEnum) -> HirModule {
    HirModule {
        name: NamePath::new(vec![Identifier::new("consumer")]),
        dependency_key: "consumer".into(),
        source_path: "consumer.v".into(),
        doc: HirDocumentation::default(),
        imports: vec![],
        warnings: Vec::new(),
        submodules: vec![],
        functions: vec![],
        structs: vec![],
        enums: vec![],
        imported_enums: Vec::new(),
        imported_semantic_exports: vec![HirDependencySemanticExport {
            module: NamePath::new(vec![Identifier::new("dep")]),
            functions: Vec::new(),
            structs: Vec::new(),
            enums: vec![enum_def],
            traits: Vec::new(),
            type_aliases: Vec::new(),
            impls: Vec::new(),
        }],
        flags: vec![],
        traits: vec![],
        impls: vec![],
        type_functions: vec![],
        type_families: vec![],
        widgets: vec![],
        type_aliases: Vec::new(),
        singletons: vec![],
        statements: vec![],
    }
}

#[test]
fn gadt_variant_result_type_refines_parent_generics() {
    let lowered = lower_unite(&gadt_unite(), UniteLayout::Tagged);

    let literal = &lowered.variants[0];
    assert!(literal.generics.is_empty());
    assert_eq!(literal.parents[0].generics, vec![ValkyrieType::Float64]);

    let branch = &lowered.variants[1];
    assert_eq!(branch.generics, vec![GenericType { binder: None, name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] }]);
    assert_eq!(
        branch.parents[0].generics,
        vec![ValkyrieType::Generic(GenericType { binder: None, name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] })]
    );
}

#[test]
fn unite_exhaustiveness_is_independent_of_runtime_layout() {
    let option = option_unite();
    let tagged = lower_unite(&option, UniteLayout::Tagged);
    let untagged = lower_unite(&option, UniteLayout::Untagged);
    let complete = vec![Identifier::new("Some"), Identifier::new("None")];
    let incomplete = vec![Identifier::new("Some")];

    assert!(tagged.is_exhaustive_over(&complete));
    assert!(untagged.is_exhaustive_over(&complete));
    assert!(!tagged.is_exhaustive_over(&incomplete));
    assert!(!untagged.is_exhaustive_over(&incomplete));
}

#[test]
fn unite_coverage_rejects_missing_variants() {
    let lowered = lower_unite(&option_unite(), UniteLayout::Untagged);
    let error = lowered.check_exhaustiveness(&[Identifier::new("Some")]).unwrap_err();

    assert_eq!(error, UniteCoverageError::MissingVariants { names: vec![Identifier::new("None")] });
}

#[test]
fn unite_definition_rejects_variant_result_type_outside_family() {
    let mut expr = gadt_unite();
    expr.variants[0].result_type = Some(ValkyrieType::Named(Identifier::new("Other")));

    let error = validate_unite_definition(&expr).unwrap_err();

    assert_eq!(error, UniteDefinitionError::InvalidVariantResultType { variant: Identifier::new("Literal") });
}

#[test]
fn imported_semantic_export_unite_visible_in_nominal_module_view() {
    let option = option_unite();
    let view = NominalModuleView::from_module(&module_with_imported_unite(option));

    assert!(view.matches_nominal_parameter(&Identifier::new("Some"), &Identifier::new("Option")).unwrap());
    assert!(view.matches_nominal_parameter(&Identifier::new("None"), &Identifier::new("Option")).unwrap());
    let lowered = view.lower_unite(&Identifier::new("Option"), UniteLayout::Tagged).expect("imported Option");
    assert_eq!(lowered.variant_names(), vec![Identifier::new("Some"), Identifier::new("None")]);
}
