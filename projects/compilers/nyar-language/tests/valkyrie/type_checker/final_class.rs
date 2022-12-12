use nyar_language::{
    type_checker::*,
    types::{
        Identifier, NamePath, SourceID, SourceSpan,
        hir::{HirBlock, HirDocumentation, HirField, HirFunction, HirModule, HirParent, HirProperty, HirStruct, HirVisibility, ValkyrieType},
    },
};

fn create_final_class(name: &str) -> HirStruct {
    HirStruct {
        name: Identifier::new(name),
        namespace: vec![],
        doc: HirDocumentation::default(),
        generics: vec![],
        parents: vec![],
        fields: vec![HirField {
            name: Identifier::new("x"),
            doc: HirDocumentation::default(),
            ty: ValkyrieType::Integer64 { signed: true },
            visibility: HirVisibility::public(),
            is_mutable: false,
        }],
        methods: vec![],
        properties: vec![],
        visibility: HirVisibility::public(),
        is_value_type: false,
        is_abstract: false,
        is_sealed: false,
        is_final: true,
        is_open: false,
        abstract_methods: vec![],
        abstract_properties: vec![],
        derives: vec![],
    }
}

fn create_normal_class(name: &str, parents: Vec<HirParent>) -> HirStruct {
    HirStruct {
        name: Identifier::new(name),
        namespace: vec![],
        doc: HirDocumentation::default(),
        generics: vec![],
        parents,
        fields: vec![HirField {
            name: Identifier::new("x"),
            doc: HirDocumentation::default(),
            ty: ValkyrieType::Integer64 { signed: true },
            visibility: HirVisibility::public(),
            is_mutable: false,
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

fn create_class_with_final_method(name: &str, method_name: &str) -> HirStruct {
    HirStruct {
        name: Identifier::new(name),
        namespace: vec![],
        doc: HirDocumentation::default(),
        generics: vec![],
        parents: vec![],
        fields: vec![],
        methods: vec![HirFunction {
            name: Identifier::new(method_name),
            declaring_namespace: NamePath::default(),
            doc: HirDocumentation::default(),
            annotations: vec![],
            generics: vec![],
            params: vec![],
            return_type: ValkyrieType::Unit,
            body: HirBlock { statements: vec![], expr: None, span: SourceSpan::new(SourceID::default(), 0, 0) },
            span: SourceSpan::new(SourceID::default(), 0, 0),
            visibility: HirVisibility::public(),
            is_abstract: false,
            is_final: true,
            is_virtual: false,
            is_override: false,
        }],
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

fn create_class_with_final_property(name: &str, property_name: &str) -> HirStruct {
    HirStruct {
        name: Identifier::new(name),
        namespace: vec![],
        doc: HirDocumentation::default(),
        generics: vec![],
        parents: vec![],
        fields: vec![],
        methods: vec![],
        properties: vec![HirProperty {
            name: Identifier::new(property_name),
            doc: HirDocumentation::default(),
            ty: ValkyrieType::Integer64 { signed: true },
            getter: None,
            setter: None,
            is_readonly: true,
            visibility: HirVisibility::public(),
            is_abstract: false,
            is_final: true,
            is_static: false,
            is_virtual: false,
            is_override: false,
            is_lazy: false,
            lazy_backing_field: None,
        }],
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

fn create_class_with_override_method(name: &str, parents: Vec<HirParent>, method_name: &str) -> HirStruct {
    HirStruct {
        name: Identifier::new(name),
        namespace: vec![],
        doc: HirDocumentation::default(),
        generics: vec![],
        parents,
        fields: vec![],
        methods: vec![HirFunction {
            name: Identifier::new(method_name),
            declaring_namespace: NamePath::default(),
            doc: HirDocumentation::default(),
            annotations: vec![],
            generics: vec![],
            params: vec![],
            return_type: ValkyrieType::Unit,
            body: HirBlock { statements: vec![], expr: None, span: SourceSpan::new(SourceID::default(), 0, 0) },
            span: SourceSpan::new(SourceID::default(), 0, 0),
            visibility: HirVisibility::public(),
            is_abstract: false,
            is_final: false,
            is_virtual: false,
            is_override: false,
        }],
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

fn create_class_with_override_property(name: &str, parents: Vec<HirParent>, property_name: &str) -> HirStruct {
    HirStruct {
        name: Identifier::new(name),
        namespace: vec![],
        doc: HirDocumentation::default(),
        generics: vec![],
        parents,
        fields: vec![],
        methods: vec![],
        properties: vec![HirProperty {
            name: Identifier::new(property_name),
            doc: HirDocumentation::default(),
            ty: ValkyrieType::Integer64 { signed: true },
            getter: None,
            setter: None,
            is_readonly: true,
            visibility: HirVisibility::public(),
            is_abstract: false,
            is_final: false,
            is_static: false,
            is_virtual: false,
            is_override: false,
            is_lazy: false,
            lazy_backing_field: None,
        }],
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
fn test_final_class_inheritance_error() {
    let mut checker = FinalClassChecker::new();
    let final_class = create_final_class("ApiClient");
    let derived_class = create_normal_class(
        "CustomApiClient",
        vec![HirParent { name: NamePath::new(vec![Identifier::new("ApiClient")]), alias: None, generics: vec![], offset: None }],
    );

    let module = create_module(vec![final_class, derived_class]);

    let errors = checker.check_module(&module);
    let inheritance_errors: Vec<_> = errors.iter().filter(|e| matches!(e.kind, FinalClassErrorKind::FinalClassInheritance { .. })).collect();
    assert_eq!(inheritance_errors.len(), 1);
}

#[test]
fn test_normal_class_inheritance_ok() {
    let mut checker = FinalClassChecker::new();
    let base_class = create_normal_class("BaseController", vec![]);
    let derived_class = create_normal_class(
        "CustomController",
        vec![HirParent { name: NamePath::new(vec![Identifier::new("BaseController")]), alias: None, generics: vec![], offset: None }],
    );

    let module = create_module(vec![base_class, derived_class]);

    let errors = checker.check_module(&module);
    let inheritance_errors: Vec<_> = errors.iter().filter(|e| matches!(e.kind, FinalClassErrorKind::FinalClassInheritance { .. })).collect();
    assert!(inheritance_errors.is_empty());
}

#[test]
fn test_final_method_override_error() {
    let mut checker = FinalClassChecker::new();
    let base_class = create_class_with_final_method("BaseController", "handle_request");
    let derived_class = create_class_with_override_method(
        "CustomController",
        vec![HirParent { name: NamePath::new(vec![Identifier::new("BaseController")]), alias: None, generics: vec![], offset: None }],
        "handle_request",
    );

    let module = create_module(vec![base_class, derived_class]);

    let errors = checker.check_module(&module);
    let override_errors: Vec<_> = errors.iter().filter(|e| matches!(e.kind, FinalClassErrorKind::FinalMethodOverride { .. })).collect();
    assert_eq!(override_errors.len(), 1);
}

#[test]
fn test_normal_method_override_ok() {
    let mut checker = FinalClassChecker::new();
    let mut base_class = create_class_with_final_method("BaseController", "handle_request");
    base_class.methods[0].is_final = false;
    let derived_class = create_class_with_override_method(
        "CustomController",
        vec![HirParent { name: NamePath::new(vec![Identifier::new("BaseController")]), alias: None, generics: vec![], offset: None }],
        "handle_request",
    );

    let module = create_module(vec![base_class, derived_class]);

    let errors = checker.check_module(&module);
    let override_errors: Vec<_> = errors.iter().filter(|e| matches!(e.kind, FinalClassErrorKind::FinalMethodOverride { .. })).collect();
    assert!(override_errors.is_empty());
}

#[test]
fn test_final_property_override_error() {
    let mut checker = FinalClassChecker::new();
    let base_class = create_class_with_final_property("Entity", "entity_id");
    let derived_class = create_class_with_override_property(
        "CustomEntity",
        vec![HirParent { name: NamePath::new(vec![Identifier::new("Entity")]), alias: None, generics: vec![], offset: None }],
        "entity_id",
    );

    let module = create_module(vec![base_class, derived_class]);

    let errors = checker.check_module(&module);
    let override_errors: Vec<_> = errors.iter().filter(|e| matches!(e.kind, FinalClassErrorKind::FinalPropertyOverride { .. })).collect();
    assert_eq!(override_errors.len(), 1);
}

#[test]
fn test_normal_property_override_ok() {
    let mut checker = FinalClassChecker::new();
    let mut base_class = create_class_with_final_property("Entity", "entity_id");
    base_class.properties[0].is_final = false;
    let derived_class = create_class_with_override_property(
        "CustomEntity",
        vec![HirParent { name: NamePath::new(vec![Identifier::new("Entity")]), alias: None, generics: vec![], offset: None }],
        "entity_id",
    );

    let module = create_module(vec![base_class, derived_class]);

    let errors = checker.check_module(&module);
    let override_errors: Vec<_> = errors.iter().filter(|e| matches!(e.kind, FinalClassErrorKind::FinalPropertyOverride { .. })).collect();
    assert!(override_errors.is_empty());
}

#[test]
fn test_error_display() {
    let err = FinalClassError::final_class_inheritance(Identifier::new("CustomApiClient"), Identifier::new("ApiClient"), None);
    assert!(err.to_string().contains("CustomApiClient"));
    assert!(err.to_string().contains("ApiClient"));
    assert!(err.to_string().contains("不能继承 final 类"));

    let err = FinalClassError::final_method_override(
        Identifier::new("CustomController"),
        Identifier::new("handle_request"),
        Identifier::new("BaseController"),
        None,
    );
    assert!(err.to_string().contains("CustomController"));
    assert!(err.to_string().contains("handle_request"));
    assert!(err.to_string().contains("不能重写 final 方法 "));

    let err = FinalClassError::final_property_override(
        Identifier::new("CustomEntity"),
        Identifier::new("entity_id"),
        Identifier::new("Entity"),
        None,
    );
    assert!(err.to_string().contains("CustomEntity"));
    assert!(err.to_string().contains("entity_id"));
    assert!(err.to_string().contains("不能重写 final 属性 "));
}

#[test]
fn test_is_final_class() {
    let mut checker = FinalClassChecker::new();
    let final_class = create_final_class("ApiClient");
    let normal_class = create_normal_class("NormalClass", vec![]);

    let module = create_module(vec![final_class, normal_class]);

    checker.check_module(&module);

    assert!(checker.is_final_class(&Identifier::new("ApiClient")));
    assert!(!checker.is_final_class(&Identifier::new("NormalClass")));
    assert!(!checker.is_final_class(&Identifier::new("Unknown")));
}

#[test]
fn test_get_final_class_names() {
    let mut checker = FinalClassChecker::new();
    let final_class1 = create_final_class("ApiClient1");
    let final_class2 = create_final_class("ApiClient2");
    let normal_class = create_normal_class("NormalClass", vec![]);

    let module = create_module(vec![final_class1, final_class2, normal_class]);

    checker.check_module(&module);
    let names = checker.get_final_class_names();
    assert_eq!(names.len(), 2);
}

#[test]
fn test_get_final_methods() {
    let mut checker = FinalClassChecker::new();
    let base_class = create_class_with_final_method("BaseController", "handle_request");

    let module = create_module(vec![base_class]);

    checker.check_module(&module);

    let methods = checker.get_final_methods(&Identifier::new("BaseController"));
    assert!(methods.is_some());
    assert!(methods.unwrap().contains(&Identifier::new("handle_request")));
}

#[test]
fn test_get_final_properties() {
    let mut checker = FinalClassChecker::new();
    let base_class = create_class_with_final_property("Entity", "entity_id");

    let module = create_module(vec![base_class]);

    checker.check_module(&module);

    let properties = checker.get_final_properties(&Identifier::new("Entity"));
    assert!(properties.is_some());
    assert!(properties.unwrap().contains(&Identifier::new("entity_id")));
}

#[test]
fn test_checker_clear() {
    let mut checker = FinalClassChecker::new();
    let final_class = create_final_class("ApiClient");

    let module = create_module(vec![final_class]);

    checker.check_module(&module);
    assert!(!checker.class_map().is_empty());

    checker.clear();
    assert!(checker.class_map().is_empty());
    assert!(checker.inheritance_map().is_empty());
    assert!(checker.errors().is_empty());
}

#[test]
fn test_inherited_final_method() {
    let mut checker = FinalClassChecker::new();

    let base_class = create_class_with_final_method("Base", "method");

    let middle_class = create_normal_class(
        "Middle",
        vec![HirParent { name: NamePath::new(vec![Identifier::new("Base")]), alias: None, generics: vec![], offset: None }],
    );

    let derived_class = create_class_with_override_method(
        "Derived",
        vec![HirParent { name: NamePath::new(vec![Identifier::new("Middle")]), alias: None, generics: vec![], offset: None }],
        "method",
    );

    let module = create_module(vec![base_class, middle_class, derived_class]);

    let errors = checker.check_module(&module);
    let override_errors: Vec<_> = errors.iter().filter(|e| matches!(e.kind, FinalClassErrorKind::FinalMethodOverride { .. })).collect();
    assert_eq!(override_errors.len(), 1);
}
