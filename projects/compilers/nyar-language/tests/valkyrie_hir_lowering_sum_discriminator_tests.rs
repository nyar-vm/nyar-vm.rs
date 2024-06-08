use nyar_language::{
    CompilerSourceFile, CompilerSourceGroup, SourceID, ValkyrieCompiler,
    types::{Identifier, NamePath, SourceSpan},
    valkyrie::{
        hir::lowering::{DEFAULT_COMPILE_ARCH, validate_enum_discriminators, validate_expr_call_contracts},
        mir::MirLowerer,
        types::hir::{HirDependencySemanticExport, HirEnum, HirExpr, HirExprKind, HirLiteral, HirStatementKind, HirVariant},
    },
};

fn test_span() -> SourceSpan {
    SourceSpan::new(SourceID::default(), 0, 0)
}

#[test]
fn sum_layouts_are_not_synthesized_without_source_declarations() {
    let output = ValkyrieCompiler::default()
        .compile_source_to_program("[main] micro entry() -> i32 { return 23 }")
        .expect("普通源码必须完成正式 Compiler 成功边界");
    let semantic_mir = nyar_language::valkyrie::mir::MirLowerer::lower_module_semantic(
        &ValkyrieCompiler::default().compile_source("[main] micro entry() -> i32 { return 23 }").expect("test source"),
    );
    assert!(semantic_mir.sum_types.is_empty());
    assert!(output.canonical().linked.variants.is_empty());
}

#[test]
fn familiar_sum_names_preserve_the_declared_variants() {
    let module = ValkyrieCompiler::default()
        .compile_source("enums Option { Declared = 7 } enums Result { Actual = 11 }")
        .expect("sum 声明必须来自当前源码");
    let sums = MirLowerer::lower_module_semantic(&module).sum_types;
    assert_eq!(sums.len(), 2);
    for (name, variant, tag) in [("Option", "Declared", 7), ("Result", "Actual", 11)] {
        let sum = sums.iter().find(|sum| sum.name == name).expect("声明 owner");
        assert_eq!(sum.variants.len(), 1);
        assert_eq!(sum.variants[0].name, variant);
        assert_eq!(sum.variants[0].tag, tag);
        assert!(sum.variants[0].payload_type().is_none());
    }
}

#[test]
fn unresolved_call_contract_is_rejected_before_mir() {
    let expression = HirExpr {
        kind: HirExprKind::Call {
            callee: Box::new(HirExpr { kind: HirExprKind::Path(NamePath::new(vec![Identifier::new("missing")])), span: test_span() }),
            args: Vec::new(),
            resolved: None,
        },
        span: test_span(),
    };
    let error = validate_expr_call_contracts(&expression, "contract_test").expect_err("unresolved call must fail");
    assert!(error.to_string().contains("SMIR003"), "{error}");
}

#[test]
fn frontend_does_not_filter_copy_contract_errors() {
    let compiler = ValkyrieCompiler::new(SourceID::default());
    let actual = compiler.compile_source("structure Holder { items: [i32] }").expect_err("frontend must propagate copy error");
    assert!(actual.to_string().contains("copy discipline violation"));
}

#[test]
fn singleton_unresolved_call_is_rejected_before_mir_from_source() {
    let compiler = ValkyrieCompiler::new(SourceID::default());
    let error = compiler
        .compile_source("singleton Counter { micro invalid(self) -> i32 { return missing() } }")
        .expect_err("singleton methods must obey the same resolved call contract");
    assert!(error.to_string().contains("SMIR003"), "{error}");
}

#[test]
fn non_callable_local_is_rejected_from_source() {
    let compiler = ValkyrieCompiler::new(SourceID::default());
    let error = compiler
        .compile_source("micro invalid(value: i32) -> i32 { return value() }")
        .expect_err("scalar local must not acquire an inferred callable signature");
    assert!(error.to_string().contains("SMIR003"), "{error}");
}

#[test]
fn unique_callee_with_wrong_argument_type_is_rejected_from_source() {
    let compiler = ValkyrieCompiler::new(SourceID::default());
    let error = compiler
        .compile_source("micro take(value: i32) -> i32 { return value } micro invalid(value: utf8) -> i32 { return take(value) }")
        .expect_err("unique name and arity must not override signature matching");
    assert!(error.to_string().contains("SMIR003"), "{error}");
}

#[test]
fn callable_local_preserves_declared_signature_from_source() {
    let compiler = ValkyrieCompiler::new(SourceID::default());
    let module = compiler
        .compile_source("micro invoke(callback: micro(i32) -> i32, value: i32) -> i32 { return callback(value) }")
        .expect("declared function value must resolve");
    let function = module.functions.iter().find(|function| function.name.as_str() == "invoke").expect("invoke");
    let return_expr = match function.body.statements.first().map(|statement| &statement.kind) {
        Some(HirStatementKind::Expr(statement)) => statement.as_ref(),
        _ => function.body.expr.as_deref().expect("expected return in statements or block tail"),
    };
    let HirExprKind::Return(Some(expression)) = &return_expr.kind
    else {
        panic!("expected return value")
    };
    let HirExprKind::Call { resolved: Some(contract), .. } = &expression.kind
    else {
        panic!("expected resolved callback")
    };
    assert_eq!(contract.parameter_types, vec![function.params[1].ty.clone()]);
    assert_eq!(contract.return_type, function.return_type);
}

#[test]
fn array_push_missing_element_type_is_rejected_from_source() {
    let compiler = ValkyrieCompiler::new(SourceID::default());
    let error = compiler
        .compile_source("micro invalid(values: [i32]) -> [i32] { push(values, missing) return values }")
        .expect_err("missing element facts must not become AutoType intrinsic arguments");
    assert!(error.to_string().contains("SMIR003"), "{error}");
}

#[test]
fn array_push_nominal_receiver_mismatch_is_rejected_from_source() {
    let compiler = ValkyrieCompiler::new(SourceID::default());
    let error = compiler
        .compile_source("class Owner {} imply Owner { micro invalid(self, values: [utf8]) -> [utf8] { push(values, self) return values } }")
        .expect_err("nominal self must not be treated as an unknown utf8 element");
    assert!(error.to_string().contains("SMIR003"), "{error}");
}

#[test]
fn unite_rejects_duplicate_explicit_tags() {
    let compiler = ValkyrieCompiler::new(SourceID::default());
    let error = compiler
        .compile_source(
            r#"
unite Choice {
    [tag(0)]
    A { x: i64 }
    [tag(0)]
    B { y: i64 }
}
"#,
        )
        .expect_err("duplicate unite tag");
    assert!(error.to_string().contains("duplicate discriminator"), "{error}");
}

#[test]
fn unite_rejects_implicit_tag_collision_with_explicit_tag() {
    let compiler = ValkyrieCompiler::new(SourceID::default());
    let error = compiler
        .compile_source(
            r#"
unite Choice {
    A { x: i64 }
    [tag(0)]
    B { y: i64 }
}
"#,
        )
        .expect_err("implicit tag collision");
    assert!(error.to_string().contains("duplicate discriminator"), "{error}");
}

#[test]
fn named_union_is_not_yet_lowered() {
    let compiler = ValkyrieCompiler::new(SourceID::default());
    let error = compiler
        .compile_source(
            r#"union Limb {
    small: i64
    words: i64
}"#,
        )
        .expect_err("named union must be rejected");
    assert!(error.to_string().contains("not yet lowered"), "{error}");
}

#[test]
fn unite_rejects_non_integer_tag_literal() {
    let compiler = ValkyrieCompiler::new(SourceID::default());
    let error = compiler
        .compile_source(
            r#"
unite Choice {
    [tag(foo)]
    A { x: i64 }
}
"#,
        )
        .expect_err("invalid unite tag literal");
    assert!(error.to_string().contains("non-negative integer literal"), "{error}");
}

#[test]
fn unite_default_tags_follow_declaration_order() {
    let compiler = ValkyrieCompiler::new(SourceID::default());
    let module = compiler
        .compile_source(
            r#"
unite Choice {
    A { x: i64 }
    B { y: i64 }
}
"#,
        )
        .expect("compile unite with default tags");
    let sum_types = MirLowerer::lower_module_semantic(&module).sum_types;
    let choice = sum_types.iter().find(|layout| layout.name == "Choice").expect("Choice layout");
    assert_eq!(choice.variants[0].tag, 0);
    assert_eq!(choice.variants[1].tag, 1);
}

#[test]
fn enums_rejects_duplicate_explicit_discriminators() {
    let compiler = ValkyrieCompiler::new(SourceID::default());
    let error = compiler
        .compile_source(
            r#"
enums Status {
    Active = 0
    Paused = 0
}
"#,
        )
        .expect_err("duplicate enums discriminator");
    assert!(error.to_string().contains("duplicate discriminator"), "{error}");
}

#[test]
fn enums_auto_increment_follows_last_explicit_discriminator() {
    let compiler = ValkyrieCompiler::new(SourceID::default());
    let module = compiler
        .compile_source(
            r#"
enums Status {
    Active = 2
    Inactive
}
"#,
        )
        .expect("compile enums with gap auto-increment");
    let sum_types = MirLowerer::lower_module_semantic(&module).sum_types;
    let status = sum_types.iter().find(|layout| layout.name == "Status").expect("Status layout");
    assert_eq!(status.variants[0].tag, 2);
    assert_eq!(status.variants[1].tag, 3);
}

#[test]
fn enums_rejects_negative_discriminator_literal() {
    let compiler = ValkyrieCompiler::new(SourceID::default());
    let error = compiler
        .compile_source(
            r#"
enums Status {
    Active = -1
}
"#,
        )
        .expect_err("negative enums discriminator");
    assert!(error.to_string().contains("non-negative integer literal"), "{error}");
}

#[test]
fn imported_semantic_export_enums_contribute_sum_layout_tags() {
    let groups = vec![
        CompilerSourceGroup {
            dependency_key: "dep".into(),
            name: "dep".into(),
            sources: vec![CompilerSourceFile {
                path: "dep.v".into(),
                source: r#"
enums Status {
    Active = 2
    Inactive
}
"#
                .into(),
            }],
            direct_dependencies: Vec::new(),
        },
        CompilerSourceGroup {
            dependency_key: "app".into(),
            name: "app".into(),
            sources: vec![CompilerSourceFile { path: "app.v".into(), source: "micro main(current: Status) -> i32 { return 0 }".into() }],
            direct_dependencies: vec!["dep".into()],
        },
    ];
    let modules = ValkyrieCompiler::default().resolve_source_groups(&groups, DEFAULT_COMPILE_ARCH).expect("consumer with imported enums");
    let sum_types = MirLowerer::lower_module_semantic(&modules[1]).sum_types;
    let status = sum_types.iter().find(|layout| layout.name == "Status").expect("Status layout");
    assert_eq!(status.variants[0].tag, 2);
    assert_eq!(status.variants[1].tag, 3);
}

#[test]
fn rejects_duplicate_discriminators_in_imported_semantic_export_enums() {
    let compiler = ValkyrieCompiler::new(SourceID::default());
    let duplicate_tag = |value: i64| HirExpr { kind: HirExprKind::Literal(HirLiteral::Integer64(value)), span: test_span() };
    let bad_enum = HirEnum {
        declaration: None,
        name: Identifier::new("Status"),
        doc: Default::default(),
        generics: Vec::new(),
        variants: vec![
            HirVariant {
                name: Identifier::new("Active"),
                doc: Default::default(),
                fields: Vec::new(),
                result_type: None,
                discriminator: Some(duplicate_tag(0)),
                ..Default::default()
            },
            HirVariant {
                name: Identifier::new("Paused"),
                doc: Default::default(),
                fields: Vec::new(),
                result_type: None,
                discriminator: Some(duplicate_tag(0)),
                ..Default::default()
            },
        ],
        visibility: Default::default(),
        is_unity: false,
    };
    let mut module = compiler.compile_source("micro main() { return }").expect("consumer shell");
    module.imported_semantic_exports.push(HirDependencySemanticExport {
        module: NamePath::new(vec![Identifier::new("dep")]),
        functions: Vec::new(),
        structs: Vec::new(),
        enums: vec![bad_enum],
        traits: Vec::new(),
        type_aliases: Vec::new(),
        impls: Vec::new(),
    });
    let error = compiler.validate_hir_semantic_contract(&module).expect_err("imported duplicate discriminator");
    assert!(error.to_string().contains("duplicate discriminator"), "{error}");
}

#[test]
fn imported_semantic_export_unite_contributes_sum_layout_tags() {
    let groups = vec![
        CompilerSourceGroup {
            dependency_key: "dep".into(),
            name: "dep".into(),
            sources: vec![CompilerSourceFile {
                path: "dep.v".into(),
                source: r#"
unite Choice {
    [tag(2)]
    A { x: i64 }
    B { y: i64 }
}
"#
                .into(),
            }],
            direct_dependencies: Vec::new(),
        },
        CompilerSourceGroup {
            dependency_key: "app".into(),
            name: "app".into(),
            sources: vec![CompilerSourceFile { path: "app.v".into(), source: "micro main(choice: Choice) -> i32 { return 0 }".into() }],
            direct_dependencies: vec!["dep".into()],
        },
    ];
    let modules = ValkyrieCompiler::default().resolve_source_groups(&groups, DEFAULT_COMPILE_ARCH).expect("consumer with imported unite");
    let sum_types = MirLowerer::lower_module_semantic(&modules[1]).sum_types;
    let choice = sum_types.iter().find(|layout| layout.name == "Choice").expect("Choice layout");
    assert!(choice.is_unite);
    assert_eq!(choice.variants[0].tag, 2);
    assert_eq!(choice.variants[1].tag, 3);
}

#[test]
fn rejects_duplicate_unite_and_enums_names() {
    let compiler = ValkyrieCompiler::new(SourceID::default());
    let error = compiler
        .compile_source(
            r#"
unite Kind {
    A { x: i64 }
}

enums Kind {
    A
    B
}
"#,
        )
        .expect_err("duplicate unite and enums name");
    let message = error.to_string();
    assert!(message.contains("duplicate definition"), "{error}");
    assert!(message.contains("unite") && message.contains("enums"), "{error}");
}

#[test]
fn rejects_duplicate_unite_and_union_names() {
    let compiler = ValkyrieCompiler::new(SourceID::default());
    let error = compiler
        .compile_source(
            r#"
unite Limb {
    Small { value: i64 }
    Large { value: i64 }
}

union Limb {
    small: i64
    words: i64
}
"#,
        )
        .expect_err("duplicate sum type name");
    let message = error.to_string();
    assert!(message.contains("duplicate definition"), "{error}");
    assert!(message.contains("unite") && message.contains("union"), "{error}");
}
