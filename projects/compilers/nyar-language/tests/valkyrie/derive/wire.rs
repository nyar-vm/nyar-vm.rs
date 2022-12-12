use nyar_language::{
    derive::{WireInjector, WIREABLE_TRAIT, apply_hir_injections},
    types::{
        Identifier, NamePath,
        hir::{
            HirBlock, HirDocumentation, HirExprKind, HirField, HirFunction, HirImpl, HirModule, HirStatementKind, HirStruct, HirVisibility,
            ValkyrieType,
        },
    },
};

fn create_wire_struct(name: &str, wire_field: (&str, &str)) -> HirStruct {
    HirStruct {
        name: Identifier::new(name),
        namespace: vec![],
        doc: HirDocumentation::default(),
        generics: vec![],
        parents: vec![],
        fields: vec![HirField {
            name: Identifier::new(wire_field.0),
            doc: HirDocumentation::default(),
            ty: ValkyrieType::Named(Identifier::new(wire_field.1)),
            visibility: HirVisibility::public(),
            is_mutable: false,
            is_wire: true,
        }],
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

fn empty_module_with_struct(strukt: HirStruct) -> HirModule {
    HirModule {
        name: NamePath::default(),
        doc: HirDocumentation::default(),
        imports: vec![],
        warnings: Vec::new(),
        submodules: vec![],
        functions: vec![],
        structs: vec![strukt],
        enums: vec![],
        imported_enums: Vec::new(),
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
fn test_wire_injector_generates_apply_wire() {
    let mut module = empty_module_with_struct(create_wire_struct("OrderSystem", ("store", "OrderStore")));

    let mut injector = WireInjector::new();
    let result = injector.inject_wire_impls(&mut module);

    assert!(result.is_ok());
    assert_eq!(result.impls.len(), 1);
    assert_eq!(module.impls.len(), 1);

    let generated = &module.impls[0];
    assert_eq!(generated.trait_path.as_ref().map(|path| path.to_string()).as_deref(), Some(WIREABLE_TRAIT));
    assert_eq!(generated.methods.len(), 1);
    assert_eq!(generated.methods[0].name.as_str(), "apply_wire");
    assert_eq!(generated.methods[0].params.len(), 2);

    let body = &generated.methods[0].body;
    assert_eq!(body.statements.len(), 1);
    match &body.statements[0].kind {
        HirStatementKind::Expr(expr) => match &expr.kind {
            HirExprKind::StoreField { field, .. } => assert_eq!(field.as_str(), "store"),
            other => panic!("expected StoreField, got {other:?}"),
        },
        other => panic!("expected expr statement, got {other:?}"),
    }
}

#[test]
fn test_wire_injector_skips_without_wire_fields() {
    let mut strukt = create_wire_struct("PlainSystem", ("store", "OrderStore"));
    strukt.fields[0].is_wire = false;
    let mut module = empty_module_with_struct(strukt);

    let mut injector = WireInjector::new();
    let result = injector.inject_wire_impls(&mut module);

    assert!(result.is_ok());
    assert!(result.impls.is_empty());
    assert_eq!(result.stats.structs_skipped, 1);
}

#[test]
fn test_wire_injector_skips_manual_impl() {
    let mut module = empty_module_with_struct(create_wire_struct("OrderSystem", ("store", "OrderStore")));
    module.impls.push(HirImpl {
        generics: vec![],
        where_constraints: vec![],
        target: ValkyrieType::Named(Identifier::new("OrderSystem")),
        trait_path: Some(NamePath::new(vec![Identifier::new(WIREABLE_TRAIT)])),
        methods: vec![HirFunction {
            name: Identifier::new("apply_wire"),
            doc: HirDocumentation::default(),
            annotations: vec![],
            generics: vec![],
            params: vec![],
            return_type: ValkyrieType::Unit,
            body: HirBlock {
                statements: vec![],
                expr: None,
                span: nyar_language::types::SourceSpan::new(nyar_language::types::SourceID::default(), 0, 0),
            },
            span: nyar_language::types::SourceSpan::new(nyar_language::types::SourceID::default(), 0, 0),
            visibility: HirVisibility::public(),
            is_abstract: false,
            is_final: false,
        export_spec: None,
        }],
        associated_type_impls: vec![],
        associated_const_impls: vec![],
    });

    let mut injector = WireInjector::new();
    let result = injector.inject_wire_impls(&mut module);

    assert!(result.is_ok());
    assert_eq!(module.impls.len(), 1);
    assert!(result.impls.is_empty());
}

#[test]
fn test_apply_hir_injections_idempotent() {
    let mut module = empty_module_with_struct(create_wire_struct("OrderSystem", ("store", "OrderStore")));

    apply_hir_injections(&mut module);
    let count_after_first = module.impls.len();

    apply_hir_injections(&mut module);
    assert_eq!(module.impls.len(), count_after_first);
}
