use nyar_language::{
    type_checker::*,
    types::{
        Identifier, NamePath, SourceID, SourceSpan,
        hir::{HirBlock, HirDocumentation, HirField, HirFunction, HirModule, HirParent, HirProperty, HirStruct, HirVisibility, ValkyrieType},
    },
};

fn create_property_class(name: &str, properties: Vec<HirProperty>, parents: Vec<HirParent>, is_abstract: bool) -> HirStruct {
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
        properties,
        visibility: HirVisibility::public(),
        is_value_type: false,
        is_abstract,
        is_sealed: false,
        is_final: false,
        is_open: false,
        abstract_methods: vec![],
        abstract_properties: vec![],
        derives: vec![],
    }
}

fn create_getter(name: &str, ty: ValkyrieType, is_abstract: bool) -> HirFunction {
    HirFunction {
        name: Identifier::new(name),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: vec![],
        generics: vec![],
        params: vec![],
        return_type: ty,
        body: HirBlock { statements: vec![], expr: None, span: SourceSpan::new(SourceID::default(), 0, 0) },
        span: SourceSpan::new(SourceID::default(), 0, 0),
        visibility: HirVisibility::public(),
        is_abstract,
        is_final: false,
        is_virtual: false,
        is_override: false,
    }
}

fn create_property(
    name: &str,
    ty: ValkyrieType,
    is_static: bool,
    is_virtual: bool,
    is_override: bool,
    is_abstract: bool,
    is_lazy: bool,
    is_final: bool,
) -> HirProperty {
    HirProperty {
        name: Identifier::new(name),
        doc: HirDocumentation::default(),
        ty: ty.clone(),
        getter: Some(create_getter(name, ty, is_abstract)),
        setter: None,
        is_readonly: true,
        visibility: HirVisibility::public(),
        is_abstract,
        is_final,
        is_static,
        is_virtual,
        is_override,
        is_lazy,
        lazy_backing_field: if is_lazy { Some(Identifier::new(&format!("_lazy_{}", name))) } else { None },
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
fn test_virtual_static_conflict() {
    let mut checker = PropertyChecker::new();

    let prop = create_property("pi", ValkyrieType::Float64, true, true, false, false, false, false);
    let class = create_property_class("MathConstants", vec![prop], vec![], false);
    let module = create_module(vec![class]);

    let errors = checker.check_module(&module);
    assert!(!errors.is_empty());

    let has_conflict = errors.iter().any(|e| matches!(e.kind, PropertyErrorKind::VirtualStaticConflict { .. }));
    assert!(has_conflict);
}

#[test]
fn test_valid_static_property() {
    let mut checker = PropertyChecker::new();

    let prop = create_property("pi", ValkyrieType::Float64, true, false, false, false, false, false);
    let class = create_property_class("MathConstants", vec![prop], vec![], false);
    let module = create_module(vec![class]);

    let errors = checker.check_module(&module);
    assert!(errors.is_empty());
}

#[test]
fn test_lazy_property_with_setter() {
    let mut checker = PropertyChecker::new();

    let mut prop = create_property("cached_value", ValkyrieType::Integer64 { signed: true }, false, false, false, false, true, false);
    prop.setter = Some(create_getter("cached_value", ValkyrieType::Integer64 { signed: true }, false));
    prop.is_readonly = false;

    let class = create_property_class("Container", vec![prop], vec![], false);
    let module = create_module(vec![class]);

    let errors = checker.check_module(&module);
    let has_lazy_setter_error = errors.iter().any(|e| matches!(e.kind, PropertyErrorKind::LazyPropertyWithSetter { .. }));
    assert!(has_lazy_setter_error);
}

#[test]
fn test_valid_lazy_property() {
    let mut checker = PropertyChecker::new();

    let prop = create_property("cached_value", ValkyrieType::Integer64 { signed: true }, false, false, false, false, true, false);
    let class = create_property_class("Container", vec![prop], vec![], false);
    let module = create_module(vec![class]);

    let errors = checker.check_module(&module);
    let lazy_errors: Vec<_> = errors.iter().filter(|e| matches!(e.kind, PropertyErrorKind::LazyPropertyWithSetter { .. })).collect();
    assert!(lazy_errors.is_empty());
}

#[test]
fn test_override_without_parent() {
    let mut checker = PropertyChecker::new();

    let prop = create_property("area", ValkyrieType::Float64, false, false, true, false, false, false);
    let class = create_property_class("Circle", vec![prop], vec![], false);
    let module = create_module(vec![class]);

    let errors = checker.check_module(&module);
    let has_invalid_override = errors.iter().any(|e| matches!(e.kind, PropertyErrorKind::InvalidOverride { .. }));
    assert!(has_invalid_override);
}

#[test]
fn test_abstract_property_in_concrete_class() {
    let mut checker = PropertyChecker::new();

    let prop = create_property("id", ValkyrieType::Integer64 { signed: true }, false, false, false, true, false, false);
    let class = create_property_class("Entity", vec![prop], vec![], false);
    let module = create_module(vec![class]);

    let errors = checker.check_module(&module);
    let has_abstract_error = errors.iter().any(|e| matches!(e.kind, PropertyErrorKind::AbstractPropertyWithBody { .. }));
    assert!(has_abstract_error);
}

#[test]
fn test_valid_abstract_property() {
    let mut checker = PropertyChecker::new();

    let prop = create_property("id", ValkyrieType::Integer64 { signed: true }, false, false, false, true, false, false);
    let class = create_property_class("Entity", vec![prop], vec![], true);
    let module = create_module(vec![class]);

    let errors = checker.check_module(&module);
    let abstract_errors: Vec<_> = errors.iter().filter(|e| matches!(e.kind, PropertyErrorKind::AbstractPropertyWithBody { .. })).collect();
    assert!(abstract_errors.is_empty());
}

#[test]
fn test_property_error_display() {
    let err = PropertyError::virtual_static_conflict(Identifier::new("pi"), None);
    assert!(err.to_string().contains("pi"));
    assert!(err.to_string().contains("virtual"));
    assert!(err.to_string().contains("static"));

    let err = PropertyError::static_with_self(Identifier::new("counter"), None);
    assert!(err.to_string().contains("counter"));
    assert!(err.to_string().contains("self"));

    let err = PropertyError::lazy_with_setter(Identifier::new("cached"), None);
    assert!(err.to_string().contains("cached"));
    assert!(err.to_string().contains("setter"));
}

#[test]
fn test_setter_validation_analyzer() {
    let mut analyzer = SetterValidationAnalyzer::new();

    let prop = create_property("value", ValkyrieType::Integer64 { signed: true }, false, false, false, false, false, false);
    let conditions = analyzer.analyze(&prop);

    assert!(!conditions.is_empty());
}

#[test]
fn test_panic_code_generation() {
    let panic_code = SetterValidationAnalyzer::generate_panic_code("items", "value >= 0");
    assert!(panic_code.contains("items"));
    assert!(panic_code.contains("value >= 0"));
    assert!(panic_code.contains("panic"));
}
