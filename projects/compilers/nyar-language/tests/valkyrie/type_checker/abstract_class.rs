use nyar_language::{
    type_checker::*,
    types::{
        Identifier, NamePath, SourceID, SourceSpan,
        hir::{HirDocumentation, HirExpr, HirExprKind, HirField, HirFunction, HirModule, HirParent, HirStruct, HirVisibility},
    },
};

fn create_abstract_class(name: &str, abstract_methods: Vec<Identifier>) -> HirStruct {
    HirStruct {
        name: Identifier::new(name),
        namespace: vec![],
        doc: HirDocumentation::default(),
        generics: vec![],
        parents: vec![],
        fields: vec![HirField {
            name: Identifier::new("x"),
            doc: HirDocumentation::default(),
            ty: nyar_language::types::hir::ValkyrieType::Integer64 { signed: true },
            visibility: HirVisibility::public(),
            is_mutable: false,
        }],
        methods: vec![],
        properties: vec![],
        visibility: HirVisibility::public(),
        is_value_type: false,
        is_abstract: true,
        is_sealed: false,
        is_final: false,
        is_open: false,
        abstract_methods,
        abstract_properties: vec![],
        derives: vec![],
    }
}

fn create_concrete_class(name: &str, parents: Vec<HirParent>, methods: Vec<Identifier>) -> HirStruct {
    let hir_methods: Vec<HirFunction> = methods
        .into_iter()
        .map(|m| HirFunction {
            name: m,
            declaring_namespace: NamePath::default(),
            doc: HirDocumentation::default(),
            annotations: vec![],
            generics: vec![],
            params: vec![],
            return_type: nyar_language::types::hir::ValkyrieType::Unit,
            body: nyar_language::types::hir::HirBlock { statements: vec![], expr: None, span: SourceSpan::new(SourceID::default(), 0, 0) },
            span: SourceSpan::new(SourceID::default(), 0, 0),
            visibility: HirVisibility::public(),
            is_abstract: false,
            is_final: false,
            is_virtual: false,
            is_override: false,
        })
        .collect();

    HirStruct {
        name: Identifier::new(name),
        namespace: vec![],
        doc: HirDocumentation::default(),
        generics: vec![],
        parents,
        fields: vec![HirField {
            name: Identifier::new("x"),
            doc: HirDocumentation::default(),
            ty: nyar_language::types::hir::ValkyrieType::Integer64 { signed: true },
            visibility: HirVisibility::public(),
            is_mutable: false,
        }],
        methods: hir_methods,
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

fn create_module(structs: Vec<HirStruct>) -> HirModule {
    HirModule {
        name: NamePath::new(vec![Identifier::new("test")]),
        doc: HirDocumentation::default(),
        imports: vec![],
        warnings: Vec::new(),
        submodules: vec![],
        functions: vec![],
        structs,
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
fn test_abstract_class_instantiation_error() {
    let mut checker = AbstractClassChecker::new();
    let abstract_class = create_abstract_class("Base", vec![Identifier::new("do_something")]);

    let module = create_module(vec![abstract_class]);

    let errors = checker.check_module(&module);
    let instantiation_errors: Vec<_> =
        errors.iter().filter(|e| matches!(e.kind, AbstractClassErrorKind::AbstractClassInstantiation { .. })).collect();
    assert!(instantiation_errors.is_empty());
}

#[test]
fn test_abstract_method_not_implemented() {
    let mut checker = AbstractClassChecker::new();
    let abstract_class = create_abstract_class("Base", vec![Identifier::new("do_something")]);
    let concrete_class = create_concrete_class(
        "Derived",
        vec![HirParent { name: NamePath::new(vec![Identifier::new("Base")]), alias: None, generics: vec![], offset: None }],
        vec![],
    );

    let module = create_module(vec![abstract_class, concrete_class]);

    let errors = checker.check_module(&module);
    let impl_errors: Vec<_> = errors.iter().filter(|e| matches!(e.kind, AbstractClassErrorKind::AbstractMethodNotImplemented { .. })).collect();
    assert_eq!(impl_errors.len(), 1);
}

#[test]
fn test_abstract_method_implemented() {
    let mut checker = AbstractClassChecker::new();
    let abstract_class = create_abstract_class("Base", vec![Identifier::new("do_something")]);
    let concrete_class = create_concrete_class(
        "Derived",
        vec![HirParent { name: NamePath::new(vec![Identifier::new("Base")]), alias: None, generics: vec![], offset: None }],
        vec![Identifier::new("do_something")],
    );

    let module = create_module(vec![abstract_class, concrete_class]);

    let errors = checker.check_module(&module);
    let impl_errors: Vec<_> = errors.iter().filter(|e| matches!(e.kind, AbstractClassErrorKind::AbstractMethodNotImplemented { .. })).collect();
    assert!(impl_errors.is_empty());
}

#[test]
fn test_abstract_method_with_body_error() {
    let mut checker = AbstractClassChecker::new();
    let mut abstract_class = create_abstract_class("Base", vec![Identifier::new("do_something")]);
    abstract_class.methods = vec![HirFunction {
        name: Identifier::new("do_something"),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: vec![],
        generics: vec![],
        params: vec![],
        return_type: nyar_language::types::hir::ValkyrieType::Unit,
        body: nyar_language::types::hir::HirBlock {
            statements: vec![],
            expr: Some(Box::new(HirExpr {
                kind: HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Unit),
                span: SourceSpan::new(SourceID::default(), 0, 0),
            })),
            span: SourceSpan::new(SourceID::default(), 0, 0),
        },
        span: SourceSpan::new(SourceID::default(), 0, 0),
        visibility: HirVisibility::public(),
        is_abstract: true,
        is_final: false,
        is_virtual: false,
        is_override: false,
    }];

    let module = create_module(vec![abstract_class]);

    let errors = checker.check_module(&module);
    let body_errors: Vec<_> = errors.iter().filter(|e| matches!(e.kind, AbstractClassErrorKind::AbstractMethodWithBody { .. })).collect();
    assert_eq!(body_errors.len(), 1);
}

#[test]
fn test_error_display() {
    let err = AbstractClassError::abstract_class_instantiation(Identifier::new("Base"), None);
    assert!(err.to_string().contains("Base"));
    assert!(err.to_string().contains("不能实例化抽象类"));

    let err = AbstractClassError::abstract_method_not_implemented(
        Identifier::new("Derived"),
        Identifier::new("do_something"),
        Identifier::new("Base"),
        None,
    );
    assert!(err.to_string().contains("Derived"));
    assert!(err.to_string().contains("do_something"));
    assert!(err.to_string().contains("Base"));

    let err = AbstractClassError::abstract_method_with_body(Identifier::new("Base"), Identifier::new("method"), None);
    assert!(err.to_string().contains("Base"));
    assert!(err.to_string().contains("method"));
}

#[test]
fn test_is_abstract_class() {
    let mut checker = AbstractClassChecker::new();
    let abstract_class = create_abstract_class("Base", vec![]);

    let module = create_module(vec![abstract_class]);

    checker.check_module(&module);

    assert!(checker.is_abstract_class(&Identifier::new("Base")));
    assert!(!checker.is_abstract_class(&Identifier::new("Unknown")));
}

#[test]
fn test_get_abstract_class_names() {
    let mut checker = AbstractClassChecker::new();
    let abstract_class1 = create_abstract_class("Base1", vec![]);
    let abstract_class2 = create_abstract_class("Base2", vec![]);

    let module = create_module(vec![abstract_class1, abstract_class2]);

    checker.check_module(&module);
    let names = checker.get_abstract_class_names();
    assert_eq!(names.len(), 2);
}

#[test]
fn test_get_abstract_methods() {
    let mut checker = AbstractClassChecker::new();
    let abstract_class = create_abstract_class("Base", vec![Identifier::new("method1"), Identifier::new("method2")]);

    let module = create_module(vec![abstract_class]);

    checker.check_module(&module);

    let methods = checker.get_abstract_methods(&Identifier::new("Base"));
    assert!(methods.is_some());
    assert_eq!(methods.unwrap().len(), 2);
}

#[test]
fn test_checker_clear() {
    let mut checker = AbstractClassChecker::new();
    let abstract_class = create_abstract_class("Base", vec![]);

    let module = create_module(vec![abstract_class]);

    checker.check_module(&module);
    assert!(!checker.abstract_classes().is_empty());

    checker.clear();
    assert!(checker.abstract_classes().is_empty());
    assert!(checker.inheritance_map().is_empty());
    assert!(checker.class_methods().is_empty());
    assert!(checker.errors().is_empty());
}
