use nyar_language::{
    derive::*,
    types::{
        Identifier, NamePath,
        hir::{HirDocumentation, HirField, HirImpl, HirModule, HirStruct, HirVisibility, ValkyrieType},
    },
};

fn create_test_struct(name: &str, derives: Vec<&str>) -> HirStruct {
    HirStruct {
        name: Identifier::new(name),
        namespace: vec![],
        doc: HirDocumentation::default(),
        generics: vec![],
        parents: vec![],
        fields: vec![
            HirField {
                name: Identifier::new("x"),
                doc: HirDocumentation::default(),
                ty: ValkyrieType::Integer32 { signed: true },
                visibility: HirVisibility::public(),
                is_mutable: false,
            },
            HirField {
                name: Identifier::new("y"),
                doc: HirDocumentation::default(),
                ty: ValkyrieType::Integer32 { signed: true },
                visibility: HirVisibility::public(),
                is_mutable: false,
            },
        ],
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
        derives: derives.iter().map(|s| NamePath::new(vec![Identifier::new(s)])).collect(),
    }
}

fn create_test_module() -> HirModule {
    HirModule {
        name: NamePath::default(),
        doc: HirDocumentation::default(),
        imports: vec![],
        warnings: Vec::new(),
        submodules: vec![],
        functions: vec![],
        structs: vec![create_test_struct("Point", vec!["Debug", "Clone"])],
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
fn test_injector_creation() {
    let injector = DeriveInjector::new();
    assert!(!injector.available_derives().is_empty());
    assert!(injector.errors().is_empty());
}

#[test]
fn test_inject_derives_empty_module() {
    let mut module = HirModule {
        name: NamePath::default(),
        doc: HirDocumentation::default(),
        imports: vec![],
        warnings: Vec::new(),
        submodules: vec![],
        functions: vec![],
        structs: vec![],
        enums: vec![],
        imported_enums: Vec::new(),
        flags: vec![],
        traits: vec![],
        impls: vec![],
        type_functions: vec![],
        type_families: vec![],
        widgets: vec![],
        singletons: vec![],
        statements: vec![],
        type_aliases: Vec::new(),
    };

    let mut injector = DeriveInjector::new();
    let result = injector.inject_derives(&mut module);

    assert!(result.is_ok());
    assert!(!result.has_impls());
    assert_eq!(result.stats.structs_processed, 0);
    assert_eq!(result.stats.structs_skipped, 0);
}

#[test]
fn test_inject_derives_success() {
    let mut module = create_test_module();

    let mut injector = DeriveInjector::new();
    let result = injector.inject_derives(&mut module);

    assert!(result.is_ok(), "应该没有错误");
    assert_eq!(result.impls.len(), 2, "应该生成 2 �?impl");
    assert_eq!(result.stats.structs_processed, 1);
    assert_eq!(result.stats.impls_generated, 2);
}

#[test]
fn test_inject_for_struct() {
    let struct_def = create_test_struct("Point", vec!["Debug"]);
    let mut injector = DeriveInjector::new();

    let result = injector.inject_for_struct(&struct_def, &[]);

    assert!(result.is_ok());
    assert_eq!(result.impls.len(), 1);
}

#[test]
fn test_conflict_detection() {
    let struct_def = create_test_struct("Point", vec!["Debug"]);

    let existing_impl = HirImpl {
        generics: vec![],
        where_constraints: vec![],
        target: ValkyrieType::Named(Identifier::new("Point")),
        trait_path: Some(NamePath::new(vec![Identifier::new("Debug")])),
        methods: vec![],
        associated_type_impls: vec![],
        associated_const_impls: vec![],
    };

    let mut injector = DeriveInjector::new();
    let result = injector.inject_for_struct(&struct_def, &[existing_impl]);

    assert!(result.has_errors());
    assert_eq!(result.errors.len(), 1);
    assert!(matches!(result.errors[0], DeriveError::Conflict { .. }));
}

#[test]
fn test_can_derive() {
    let struct_def = create_test_struct("Point", vec![]);
    let injector = DeriveInjector::new();

    assert!(injector.can_derive(&struct_def, "Debug", &[]).is_ok());
    assert!(injector.can_derive(&struct_def, "UnknownTrait", &[]).is_err());
}

#[test]
fn test_stats_tracking() {
    let mut module = HirModule {
        name: NamePath::default(),
        doc: HirDocumentation::default(),
        imports: vec![],
        warnings: Vec::new(),
        submodules: vec![],
        functions: vec![],
        structs: vec![create_test_struct("Point", vec!["Debug"]), create_test_struct("Vector", vec![])],
        enums: vec![],
        imported_enums: Vec::new(),
        flags: vec![],
        traits: vec![],
        impls: vec![],
        type_functions: vec![],
        type_families: vec![],
        widgets: vec![],
        singletons: vec![],
        statements: vec![],
        type_aliases: Vec::new(),
    };

    let mut injector = DeriveInjector::new();
    let result = injector.inject_derives(&mut module);

    assert_eq!(result.stats.structs_processed, 1);
    assert_eq!(result.stats.structs_skipped, 1);
    assert_eq!(result.stats.impls_generated, 1);
}

#[test]
fn test_injection_result_merge() {
    let mut result1 = InjectionResult::new();
    result1.impls.push(HirImpl {
        generics: vec![],
        where_constraints: vec![],
        target: ValkyrieType::Named(Identifier::new("A")),
        trait_path: None,
        methods: vec![],
        associated_type_impls: vec![],
        associated_const_impls: vec![],
    });
    result1.stats.structs_processed = 1;
    result1.stats.impls_generated = 1;

    let mut result2 = InjectionResult::new();
    result2.impls.push(HirImpl {
        generics: vec![],
        where_constraints: vec![],
        target: ValkyrieType::Named(Identifier::new("B")),
        trait_path: None,
        methods: vec![],
        associated_type_impls: vec![],
        associated_const_impls: vec![],
    });
    result2.stats.structs_processed = 2;
    result2.stats.impls_generated = 1;

    result1.merge(result2);

    assert_eq!(result1.impls.len(), 2);
    assert_eq!(result1.stats.structs_processed, 3);
    assert_eq!(result1.stats.impls_generated, 2);
}

#[test]
fn test_analyze_derive_usage() {
    let module = create_test_module();
    let injector = DeriveInjector::new();

    let analysis = injector.analyze_derive_usage(&module);

    assert_eq!(analysis.struct_count(), 1);
    assert_eq!(analysis.total_derive_requests(), 2);
}

#[test]
fn test_submodule_processing() {
    let submodule = create_test_module();
    let mut module = HirModule {
        name: NamePath::default(),
        doc: HirDocumentation::default(),
        imports: vec![],
        warnings: Vec::new(),
        submodules: vec![submodule],
        functions: vec![],
        structs: vec![create_test_struct("MainPoint", vec!["Debug"])],
        enums: vec![],
        imported_enums: Vec::new(),
        flags: vec![],
        traits: vec![],
        impls: vec![],
        type_functions: vec![],
        type_families: vec![],
        widgets: vec![],
        singletons: vec![],
        statements: vec![],
        type_aliases: Vec::new(),
    };

    let mut injector = DeriveInjector::new();
    let result = injector.inject_derives(&mut module);

    assert_eq!(result.stats.structs_processed, 2);
    assert_eq!(result.stats.impls_generated, 3);
}
