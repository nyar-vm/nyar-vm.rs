use nyar_language::{
    ValkyrieCompiler,
    type_checker::*,
    types::{
        Identifier, NamePath, SourceID, SourceSpan,
        hir::{
            HirBlock, HirDocumentation, HirExpr, HirExprKind, HirField, HirFunction, HirIdentifier, HirLiteral, HirModule, HirParam,
            HirStatement, HirStatementKind, HirStruct, HirVisibility, ValkyrieType,
        },
    },
    validation::validate_semantic_module,
};

fn create_value_type_struct(name: &str, parents: Vec<nyar_language::types::hir::HirParent>) -> HirStruct {
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
        is_value_type: true,
        is_abstract: false,
        is_sealed: false,
        is_final: false,
        is_open: false,
        abstract_methods: vec![],
        abstract_properties: vec![],
        derives: vec![],
    }
}

fn create_reference_type_struct(name: &str, parents: Vec<nyar_language::types::hir::HirParent>) -> HirStruct {
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

#[test]
fn test_value_type_no_inheritance() {
    let mut checker = ValueTypeChecker::new();
    let struct_def = create_value_type_struct("Point", vec![]);
    assert!(!struct_def.is_value_type && struct_def.parents.is_empty() || struct_def.is_value_type);

    let module = HirModule {
        name: nyar_language::types::NamePath::new(vec![Identifier::new("test")]),
        doc: HirDocumentation::default(),
        imports: vec![],
        warnings: Vec::new(),
        submodules: vec![],
        functions: vec![],
        structs: vec![struct_def],
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
    };

    let errors = checker.check_module(&module);
    let inheritance_errors: Vec<_> = errors.iter().filter(|e| matches!(e.kind, ValueTypeErrorKind::ValueTypeInheritance { .. })).collect();
    assert!(inheritance_errors.is_empty());
}

#[test]
fn test_value_type_with_inheritance_error() {
    let mut checker = ValueTypeChecker::new();
    let parent = nyar_language::types::hir::HirParent {
        name: nyar_language::types::NamePath::new(vec![Identifier::new("BaseClass")]),
        alias: None,
        generics: vec![],
        offset: None,
    };
    let struct_def = create_value_type_struct("Point", vec![parent]);

    let module = HirModule {
        name: nyar_language::types::NamePath::new(vec![Identifier::new("test")]),
        doc: HirDocumentation::default(),
        imports: vec![],
        warnings: Vec::new(),
        submodules: vec![],
        functions: vec![],
        structs: vec![struct_def],
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

    let errors = checker.check_module(&module);
    let inheritance_errors: Vec<_> = errors.iter().filter(|e| matches!(e.kind, ValueTypeErrorKind::ValueTypeInheritance { .. })).collect();
    assert_eq!(inheritance_errors.len(), 1);
}

#[test]
fn test_reference_type_with_inheritance_ok() {
    let mut checker = ValueTypeChecker::new();
    let parent = nyar_language::types::hir::HirParent {
        name: nyar_language::types::NamePath::new(vec![Identifier::new("BaseClass")]),
        alias: None,
        generics: vec![],
        offset: None,
    };
    let struct_def = create_reference_type_struct("MyClass", vec![parent]);

    let module = HirModule {
        name: nyar_language::types::NamePath::new(vec![Identifier::new("test")]),
        doc: HirDocumentation::default(),
        imports: vec![],
        warnings: Vec::new(),
        submodules: vec![],
        functions: vec![],
        structs: vec![struct_def],
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

    let errors = checker.check_module(&module);
    let inheritance_errors: Vec<_> = errors.iter().filter(|e| matches!(e.kind, ValueTypeErrorKind::ValueTypeInheritance { .. })).collect();
    assert!(inheritance_errors.is_empty());
}

#[test]
fn test_value_type_error_display() {
    let err = ValueTypeError::value_type_inheritance(Identifier::new("Point"), Identifier::new("Base"), None);
    assert!(err.to_string().contains("Point"));
    assert!(err.to_string().contains("Base"));

    let err = ValueTypeError::value_type_field_mutation(Identifier::new("Point"), Identifier::new("x"), None);
    assert!(err.to_string().contains("Point"));
    assert!(err.to_string().contains("x"));
}

#[test]
fn test_copy_semantics_validator() {
    let mut validator = CopySemanticsValidator::new();
    let struct_def = create_value_type_struct("Point", vec![]);
    validator.register_value_type(&struct_def);

    let ty = ValkyrieType::Named(Identifier::new("Point"));
    assert_eq!(validator.validate_assignment(&ty), AssignmentSemantics::Copy);
    assert_eq!(validator.validate_parameter_passing(&ty), ParameterSemantics::Copy);
    assert_eq!(validator.validate_return(&ty), ReturnSemantics::Copy);

    let ref_ty = ValkyrieType::Named(Identifier::new("MyClass"));
    assert_eq!(validator.validate_assignment(&ref_ty), AssignmentSemantics::Reference);
}

#[test]
fn test_is_value_type() {
    let mut checker = ValueTypeChecker::new();
    let struct_def = create_value_type_struct("Point", vec![]);

    let module = HirModule {
        name: nyar_language::types::NamePath::new(vec![Identifier::new("test")]),
        doc: HirDocumentation::default(),
        imports: vec![],
        warnings: Vec::new(),
        submodules: vec![],
        functions: vec![],
        structs: vec![struct_def],
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

    checker.check_module(&module);

    assert!(checker.is_value_type(&ValkyrieType::Named(Identifier::new("Point"))));
    assert!(!checker.is_value_type(&ValkyrieType::Named(Identifier::new("Unknown"))));
    assert!(!checker.is_value_type(&ValkyrieType::Integer64 { signed: true }));
}

#[test]
fn test_get_value_type_names() {
    let mut checker = ValueTypeChecker::new();
    let struct_def1 = create_value_type_struct("Point", vec![]);
    let struct_def2 = create_value_type_struct("Vector", vec![]);

    let module = HirModule {
        name: nyar_language::types::NamePath::new(vec![Identifier::new("test")]),
        doc: HirDocumentation::default(),
        imports: vec![],
        warnings: Vec::new(),
        submodules: vec![],
        functions: vec![],
        structs: vec![struct_def1, struct_def2],
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

    checker.check_module(&module);
    let names = checker.get_value_type_names();
    assert_eq!(names.len(), 2);
}

#[test]
fn test_checker_clear() {
    let mut checker = ValueTypeChecker::new();
    let struct_def = create_value_type_struct("Point", vec![]);

    let module = HirModule {
        name: nyar_language::types::NamePath::new(vec![Identifier::new("test")]),
        doc: HirDocumentation::default(),
        imports: vec![],
        warnings: Vec::new(),
        submodules: vec![],
        functions: vec![],
        structs: vec![struct_def],
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

    checker.check_module(&module);
    assert!(!checker.value_types().is_empty());

    checker.clear();
    assert!(checker.value_types().is_empty());
    assert!(checker.errors().is_empty());
}

fn test_span() -> SourceSpan {
    SourceSpan::new(SourceID::default(), 0, 0)
}

fn test_expr(kind: HirExprKind) -> HirExpr {
    HirExpr { kind, span: test_span() }
}

fn test_block(statements: Vec<HirStatement>, expr: Option<HirExpr>) -> HirBlock {
    HirBlock { statements, expr: expr.map(Box::new), span: test_span() }
}

fn self_identifier() -> HirIdentifier {
    HirIdentifier { name: Identifier::new("self"), shadow_index: 0, span: test_span() }
}

fn self_field_assignment(field: &str) -> HirExpr {
    test_expr(HirExprKind::StoreField {
        object: Box::new(test_expr(HirExprKind::Variable(self_identifier()))),
        field: Identifier::new(field),
        value: Box::new(test_expr(HirExprKind::Literal(HirLiteral::Integer64(1)))),
    })
}

fn make_value_type_struct_with_method(name: &str, method: HirFunction) -> HirStruct {
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
        methods: vec![method],
        properties: vec![],
        visibility: HirVisibility::public(),
        is_value_type: true,
        is_abstract: false,
        is_sealed: false,
        is_final: false,
        is_open: false,
        abstract_methods: vec![],
        abstract_properties: vec![],
        derives: vec![],
    }
}

fn make_test_module(struct_def: HirStruct) -> HirModule {
    HirModule {
        name: NamePath::new(vec![Identifier::new("test")]),
        doc: HirDocumentation::default(),
        imports: vec![],
        warnings: vec![],
        submodules: vec![],
        functions: vec![],
        structs: vec![struct_def],
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
        type_aliases: vec![],
    }
}

fn make_self_method(body: HirBlock) -> HirFunction {
    HirFunction {
        name: Identifier::new("mutate"),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: vec![],
        generics: vec![],
        params: vec![HirParam { name: self_identifier(), ty: ValkyrieType::Named(Identifier::new("Point")), ..Default::default() }],
        return_type: ValkyrieType::Unit,
        body,
        span: test_span(),
        visibility: HirVisibility::public(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    }
}

#[test]
fn test_value_type_field_mutation_rejected() {
    let method = make_self_method(test_block(
        vec![HirStatement { kind: HirStatementKind::Expr(Box::new(self_field_assignment("x"))), span: test_span() }],
        None,
    ));
    let struct_def = make_value_type_struct_with_method("Point", method);
    let module = make_test_module(struct_def);

    let mut checker = ValueTypeChecker::new();
    let errors = checker.check_module(&module);
    let mutation_errors: Vec<_> =
        errors.iter().filter(|error| matches!(error.kind, ValueTypeErrorKind::ValueTypeFieldMutation { .. })).collect();
    assert_eq!(mutation_errors.len(), 1, "expected one ValueTypeFieldMutation error, got {errors:?}");

    let result = validate_semantic_module(&module);
    assert!(result.is_err(), "validate_semantic_module should reject value type field mutation");
    let message = result.unwrap_err().to_string();
    assert!(message.contains("Point"), "error message should mention class name: {message}");
    assert!(message.contains("x"), "error message should mention field name: {message}");
}

#[test]
fn test_value_type_field_mutation_in_nested_block() {
    let nested_block =
        test_block(vec![HirStatement { kind: HirStatementKind::Expr(Box::new(self_field_assignment("x"))), span: test_span() }], None);
    let if_expr = test_expr(HirExprKind::If {
        condition: Box::new(test_expr(HirExprKind::Literal(HirLiteral::Bool(true)))),
        then_branch: Box::new(nested_block),
        else_branch: None,
    });
    let method = make_self_method(test_block(vec![HirStatement { kind: HirStatementKind::Expr(Box::new(if_expr)), span: test_span() }], None));
    let struct_def = make_value_type_struct_with_method("Point", method);
    let module = make_test_module(struct_def);

    let mut checker = ValueTypeChecker::new();
    let errors = checker.check_module(&module);
    let mutation_errors: Vec<_> =
        errors.iter().filter(|error| matches!(error.kind, ValueTypeErrorKind::ValueTypeFieldMutation { .. })).collect();
    assert_eq!(mutation_errors.len(), 1, "nested block scan should find the field mutation, got {errors:?}");

    let result = validate_semantic_module(&module);
    assert!(result.is_err(), "validate_semantic_module should reject value type field mutation in nested block");
    let message = result.unwrap_err().to_string();
    assert!(message.contains("Point"), "error message should mention class name: {message}");
    assert!(message.contains("x"), "error message should mention field name: {message}");
}

#[test]
fn test_copy_semantics_validator_in_pipeline() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9600 })
        .compile_source(
            r#"
structure Point {
    x: f64,
    y: f64,
}

micro main() {
    let a = Point { x: 1.0, y: 2.0 };
    let b = a;
}
"#,
        )
        .expect("compile should succeed with CopySemanticsValidator wired into the pipeline");

    let mut validator = CopySemanticsValidator::new();
    for class in &hir.structs {
        validator.register_value_type(class);
    }
    let point_ty = ValkyrieType::Named(Identifier::new("Point"));
    assert_eq!(validator.validate_assignment(&point_ty), AssignmentSemantics::Copy);
    assert_eq!(validator.validate_parameter_passing(&point_ty), ParameterSemantics::Copy);
    assert_eq!(validator.validate_return(&point_ty), ReturnSemantics::Copy);
}

#[test]
fn test_copy_semantics_validator_rejects_violation() {
    let bad_struct = HirStruct {
        name: Identifier::new("BadPoint"),
        namespace: vec![],
        doc: HirDocumentation::default(),
        generics: vec![],
        parents: vec![],
        fields: vec![HirField {
            name: Identifier::new("items"),
            doc: HirDocumentation::default(),
            ty: ValkyrieType::Array(Box::new(ValkyrieType::Integer32 { signed: true })),
            visibility: HirVisibility::public(),
            is_mutable: false,
        }],
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
        derives: vec![],
    };
    let function = HirFunction {
        name: Identifier::new("take_bad"),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: vec![],
        generics: vec![],
        params: vec![HirParam {
            name: HirIdentifier { name: Identifier::new("p"), shadow_index: 0, span: test_span() },
            ty: ValkyrieType::Named(Identifier::new("BadPoint")),
            ..Default::default()
        }],
        return_type: ValkyrieType::Unit,
        body: test_block(vec![], None),
        span: test_span(),
        visibility: HirVisibility::public(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    };
    let module = HirModule {
        name: NamePath::new(vec![Identifier::new("test")]),
        doc: HirDocumentation::default(),
        imports: vec![],
        warnings: vec![],
        submodules: vec![],
        functions: vec![function],
        structs: vec![bad_struct],
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
        type_aliases: vec![],
    };

    let result = validate_semantic_module(&module);
    assert!(result.is_err(), "validate_semantic_module should reject value type with reference field on parameter/return path");
    let message = result.unwrap_err().to_string();
    assert!(
        message.contains("copy") || message.contains("parameter") || message.contains("return"),
        "error message should mention copy/parameter/return: {message}"
    );
}

#[test]
fn test_validate_semantic_module_collects_multiple_errors() {
    let parent = nyar_language::types::hir::HirParent {
        name: nyar_language::types::NamePath::new(vec![Identifier::new("BaseClass")]),
        alias: None,
        generics: vec![],
        offset: None,
    };
    let mutation_method = make_self_method(test_block(
        vec![HirStatement { kind: HirStatementKind::Expr(Box::new(self_field_assignment("x"))), span: test_span() }],
        None,
    ));
    let struct_def = HirStruct {
        name: Identifier::new("Point"),
        namespace: vec![],
        doc: HirDocumentation::default(),
        generics: vec![],
        parents: vec![parent],
        fields: vec![HirField {
            name: Identifier::new("x"),
            doc: HirDocumentation::default(),
            ty: ValkyrieType::Integer64 { signed: true },
            visibility: HirVisibility::public(),
            is_mutable: false,
        }],
        methods: vec![mutation_method],
        properties: vec![],
        visibility: HirVisibility::public(),
        is_value_type: true,
        is_abstract: false,
        is_sealed: false,
        is_final: false,
        is_open: false,
        abstract_methods: vec![],
        abstract_properties: vec![],
        derives: vec![],
    };
    let module = make_test_module(struct_def);

    let result = validate_semantic_module(&module);
    assert!(result.is_err(), "validate_semantic_module should reject value type with both inheritance and field mutation");
    let message = result.unwrap_err().to_string();
    assert!(
        message.contains("继承") || message.to_lowercase().contains("inheritance"),
        "aggregated error should mention inheritance: {message}"
    );
    assert!(message.contains("修改") || message.to_lowercase().contains("mutation"), "aggregated error should mention mutation: {message}");
}
