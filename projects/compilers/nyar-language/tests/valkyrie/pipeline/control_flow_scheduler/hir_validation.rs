use super::*;

fn empty_module() -> HirModule {
    HirModule {
        name: NamePath::new(vec![Identifier::new("demo")]),
        doc: HirDocumentation::default(),
        imports: Vec::new(),
        warnings: Vec::new(),
        submodules: Vec::new(),
        functions: Vec::new(),
        structs: Vec::new(),
        enums: Vec::new(),
        imported_enums: Vec::new(),
        flags: Vec::new(),
        traits: Vec::new(),
        impls: Vec::new(),
        type_functions: Vec::new(),
        type_families: Vec::new(),
        widgets: Vec::new(),
        singletons: Vec::new(),
        statements: Vec::new(),
        type_aliases: Vec::new(),
    }
}

fn empty_function(name: &str, return_type: ValkyrieType, body_expr: HirExpr) -> HirFunction {
    HirFunction {
        name: Identifier::new(name),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::new(),
        generics: Vec::new(),
        params: Vec::new(),
        return_type,
        body: HirBlock { statements: Vec::new(), expr: Some(Box::new(body_expr)), span: span() },
        span: span(),
        visibility: HirVisibility::default(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    }
}

#[test]
fn rejects_resume_inside_catch_guard_at_hir_validation() {
    let module = HirModule {
        name: NamePath::new(vec![Identifier::new("demo")]),
        doc: HirDocumentation::default(),
        imports: Vec::new(),
        warnings: Vec::new(),
        submodules: Vec::new(),
        functions: vec![HirFunction {
            name: Identifier::new("main"),
            declaring_namespace: NamePath::default(),
            doc: HirDocumentation::default(),
            annotations: Vec::new(),
            generics: Vec::new(),
            params: Vec::new(),
            return_type: ValkyrieType::Unit,
            body: HirBlock {
                statements: Vec::new(),
                expr: Some(Box::new(expr(HirExprKind::Catch {
                    expr: Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
                        name: Identifier::new("task"),
                        shadow_index: 0,
                        span: span(),
                    }))),
                    arms: vec![HirMatchArm {
                        pattern: HirPattern::Else,
                        guard: Some(Box::new(expr(HirExprKind::Resume(Box::new(expr(HirExprKind::Variable(
                            nyar_language::types::hir::HirIdentifier { name: Identifier::new("value"), shadow_index: 0, span: span() },
                        ))))))),
                        body: Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
                            name: Identifier::new("value"),
                            shadow_index: 0,
                            span: span(),
                        }))),
                    }],
                }))),
                span: span(),
            },
            span: span(),
            visibility: HirVisibility::default(),
            is_abstract: false,
            is_final: false,
            is_virtual: false,
            is_override: false,
        }],
        structs: Vec::new(),
        enums: Vec::new(),
        imported_enums: Vec::new(),
        flags: Vec::new(),
        traits: Vec::new(),
        impls: Vec::new(),
        type_functions: Vec::new(),
        type_families: Vec::new(),
        widgets: Vec::new(),
        singletons: Vec::new(),
        statements: Vec::new(),
        type_aliases: Vec::new(),
    };

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("catch arm body"));
    assert!(error.to_string().contains("resume"));
}

#[test]
fn validates_struct_methods_in_hir_module() {
    let mut module = empty_module();
    module.structs.push(nyar_language::types::hir::HirStruct {
        name: Identifier::new("Worker"),
        methods: vec![empty_function(
            "poll",
            ValkyrieType::Unit,
            expr(HirExprKind::Await(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Bool(true)))))),
        )],
        ..nyar_language::types::hir::HirStruct::new(Identifier::new("Worker"))
    });

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("await"));
    assert!(error.to_string().contains("Future"));
}

#[test]
fn validates_property_accessors_in_hir_module() {
    let mut module = empty_module();
    let getter = empty_function(
        "ready",
        ValkyrieType::Boolean,
        expr(HirExprKind::BlockOn(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Bool(true)))))),
    );
    module.structs.push(nyar_language::types::hir::HirStruct {
        name: Identifier::new("Service"),
        properties: vec![nyar_language::types::hir::HirProperty::new(Identifier::new("ready"), ValkyrieType::Boolean).with_getter(getter)],
        ..nyar_language::types::hir::HirStruct::new(Identifier::new("Service"))
    });

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("block"));
    assert!(error.to_string().contains("Future"));
}

#[test]
fn validates_trait_default_methods_in_hir_module() {
    let mut module = empty_module();
    module.traits.push(nyar_language::types::hir::HirTrait {
        name: Identifier::new("Runner"),
        doc: HirDocumentation::default(),
        generics: Vec::new(),
        methods: Vec::new(),
        associated_types: Vec::new(),
        associated_constants: Vec::new(),
        super_traits: Vec::new(),
        default_methods: vec![empty_function(
            "run",
            ValkyrieType::Unit,
            expr(HirExprKind::Catch {
                expr: Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Bool(true)))),
                arms: vec![HirMatchArm {
                    pattern: HirPattern::Else,
                    guard: Some(Box::new(expr(HirExprKind::Resume(Box::new(expr(HirExprKind::Literal(
                        nyar_language::types::hir::HirLiteral::Bool(true),
                    ))))))),
                    body: Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Unit))),
                }],
            }),
        )],
        visibility: HirVisibility::default(),
    });

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("catch arm body"));
    assert!(error.to_string().contains("resume"));
}

#[test]
fn validates_impl_methods_in_hir_module() {
    let mut module = empty_module();
    module.impls.push(nyar_language::types::hir::HirImpl {
        target: ValkyrieType::Named(Identifier::new("Counter")),
        methods: vec![empty_function(
            "next",
            ValkyrieType::Unit,
            expr(HirExprKind::Lambda {
                generics: Vec::new(),
                params: Vec::new(),
                return_type: ValkyrieType::Unit,
                body: Box::new(HirBlock {
                    statements: Vec::new(),
                    expr: Some(Box::new(expr(HirExprKind::Yield(Some(Box::new(expr(HirExprKind::Literal(
                        nyar_language::types::hir::HirLiteral::Integer64(1),
                    )))))))),
                    span: span(),
                }),
            }),
        )],
        ..nyar_language::types::hir::HirImpl::default()
    });

    ControlFlowScheduler::validate_hir_module(&module).expect("impl method lambda may contain generator control flow");
}

#[test]
fn rejects_block_inside_catch_guard_at_hir_validation() {
    let module = HirModule {
        name: NamePath::new(vec![Identifier::new("demo")]),
        doc: HirDocumentation::default(),
        imports: Vec::new(),
        warnings: Vec::new(),
        submodules: Vec::new(),
        functions: vec![HirFunction {
            name: Identifier::new("main"),
            declaring_namespace: NamePath::default(),
            doc: HirDocumentation::default(),
            annotations: Vec::new(),
            generics: Vec::new(),
            params: Vec::new(),
            return_type: ValkyrieType::Unit,
            body: HirBlock {
                statements: Vec::new(),
                expr: Some(Box::new(expr(HirExprKind::Catch {
                    expr: Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
                        name: Identifier::new("task"),
                        shadow_index: 0,
                        span: span(),
                    }))),
                    arms: vec![HirMatchArm {
                        pattern: HirPattern::Else,
                        guard: Some(Box::new(expr(HirExprKind::BlockOn(Box::new(expr(HirExprKind::Variable(
                            nyar_language::types::hir::HirIdentifier { name: Identifier::new("future"), shadow_index: 0, span: span() },
                        ))))))),
                        body: Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Unit))),
                    }],
                }))),
                span: span(),
            },
            span: span(),
            visibility: HirVisibility::default(),
            is_abstract: false,
            is_final: false,
            is_virtual: false,
            is_override: false,
        }],
        structs: Vec::new(),
        enums: Vec::new(),
        imported_enums: Vec::new(),
        flags: Vec::new(),
        traits: Vec::new(),
        impls: Vec::new(),
        type_functions: Vec::new(),
        type_families: Vec::new(),
        widgets: Vec::new(),
        singletons: Vec::new(),
        statements: Vec::new(),
        type_aliases: Vec::new(),
    };

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("guard"));
    assert!(error.to_string().contains("block"));
}

#[test]
fn rejects_break_expr_without_value_loop_context_at_hir_validation() {
    let module = HirModule {
        name: NamePath::new(vec![Identifier::new("demo")]),
        doc: HirDocumentation::default(),
        imports: Vec::new(),
        warnings: Vec::new(),
        submodules: Vec::new(),
        functions: vec![HirFunction {
            name: Identifier::new("main"),
            declaring_namespace: NamePath::default(),
            doc: HirDocumentation::default(),
            annotations: Vec::new(),
            generics: Vec::new(),
            params: Vec::new(),
            return_type: ValkyrieType::Unit,
            body: HirBlock {
                statements: vec![HirStatement {
                    kind: HirStatementKind::Expr(Box::new(expr(HirExprKind::Loop {
                        label: None,
                        pattern: None,
                        iterator: None,
                        condition: None,
                        body: Box::new(HirBlock {
                            statements: vec![HirStatement {
                                kind: HirStatementKind::Expr(Box::new(expr(HirExprKind::Break {
                                    label: None,
                                    expr: Some(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Integer64(1))))),
                                }))),
                                span: span(),
                            }],
                            expr: None,
                            span: span(),
                        }),
                    }))),
                    span: span(),
                }],
                expr: None,
                span: span(),
            },
            span: span(),
            visibility: HirVisibility::default(),
            is_abstract: false,
            is_final: false,
            is_virtual: false,
            is_override: false,
        }],
        structs: Vec::new(),
        enums: Vec::new(),
        imported_enums: Vec::new(),
        flags: Vec::new(),
        traits: Vec::new(),
        impls: Vec::new(),
        type_functions: Vec::new(),
        type_families: Vec::new(),
        widgets: Vec::new(),
        singletons: Vec::new(),
        statements: Vec::new(),
        type_aliases: Vec::new(),
    };

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("break expr"));
    assert!(error.to_string().contains("loop"));
}

#[test]
fn rejects_incompatible_break_expr_types_inside_same_value_loop() {
    let module = demo_module(expr(HirExprKind::Loop {
        label: None,
        pattern: None,
        iterator: None,
        condition: None,
        body: Box::new(HirBlock {
            statements: vec![
                HirStatement {
                    kind: HirStatementKind::Expr(Box::new(expr(HirExprKind::Break {
                        label: None,
                        expr: Some(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Integer64(1))))),
                    }))),
                    span: span(),
                },
                HirStatement {
                    kind: HirStatementKind::Expr(Box::new(expr(HirExprKind::Break {
                        label: None,
                        expr: Some(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Bool(true))))),
                    }))),
                    span: span(),
                },
            ],
            expr: None,
            span: span(),
        }),
    }));

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("break expr"));
    assert!(error.to_string().contains("i64"));
    assert!(error.to_string().contains("bool"));
}

#[test]
fn infers_block_and_if_types_for_break_expr_validation() {
    let module = demo_module(expr(HirExprKind::Loop {
        label: None,
        pattern: None,
        iterator: None,
        condition: None,
        body: Box::new(HirBlock {
            statements: vec![
                HirStatement {
                    kind: HirStatementKind::Expr(Box::new(expr(HirExprKind::Break {
                        label: None,
                        expr: Some(Box::new(expr(HirExprKind::Block(Box::new(HirBlock {
                            statements: Vec::new(),
                            expr: Some(Box::new(expr(HirExprKind::If {
                                condition: Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Bool(true)))),
                                then_branch: Box::new(HirBlock {
                                    statements: Vec::new(),
                                    expr: Some(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Integer64(1))))),
                                    span: span(),
                                }),
                                else_branch: Some(Box::new(HirBlock {
                                    statements: Vec::new(),
                                    expr: Some(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Integer64(2))))),
                                    span: span(),
                                })),
                            }))),
                            span: span(),
                        }))))),
                    }))),
                    span: span(),
                },
                HirStatement {
                    kind: HirStatementKind::Expr(Box::new(expr(HirExprKind::Break {
                        label: None,
                        expr: Some(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Bool(true))))),
                    }))),
                    span: span(),
                },
            ],
            expr: None,
            span: span(),
        }),
    }));

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("i64"));
    assert!(error.to_string().contains("bool"));
}

#[test]
fn infers_match_types_for_break_expr_validation() {
    let module = demo_module(expr(HirExprKind::Loop {
        label: None,
        pattern: None,
        iterator: None,
        condition: None,
        body: Box::new(HirBlock {
            statements: vec![
                HirStatement {
                    kind: HirStatementKind::Expr(Box::new(expr(HirExprKind::Break {
                        label: None,
                        expr: Some(Box::new(expr(HirExprKind::Match {
                            scrutinee: Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Bool(true)))),
                            arms: vec![
                                HirMatchArm {
                                    pattern: HirPattern::Literal(nyar_language::types::hir::HirLiteral::Bool(true)),
                                    guard: None,
                                    body: Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Integer64(1)))),
                                },
                                HirMatchArm {
                                    pattern: HirPattern::Else,
                                    guard: None,
                                    body: Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Integer64(2)))),
                                },
                            ],
                        }))),
                    }))),
                    span: span(),
                },
                HirStatement {
                    kind: HirStatementKind::Expr(Box::new(expr(HirExprKind::Break {
                        label: None,
                        expr: Some(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Float64(OrderedFloat(1.0)))))),
                    }))),
                    span: span(),
                },
            ],
            expr: None,
            span: span(),
        }),
    }));

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("i64"));
    assert!(error.to_string().contains("f64"));
}

#[test]
fn infers_let_bound_variable_type_for_break_expr_validation() {
    let module = demo_module(expr(HirExprKind::Loop {
        label: None,
        pattern: None,
        iterator: None,
        condition: None,
        body: Box::new(HirBlock {
            statements: vec![
                HirStatement {
                    kind: HirStatementKind::Let {
                        is_mutable: false,
                        pattern: HirPattern::Variable(nyar_language::types::hir::HirIdentifier {
                            name: Identifier::new("value"),
                            shadow_index: 0,
                            span: span(),
                        }),
                        initializer: Some(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Integer64(1))))),
                        ty: Some(ValkyrieType::Integer64 { signed: true }),
                    },
                    span: span(),
                },
                HirStatement {
                    kind: HirStatementKind::Expr(Box::new(expr(HirExprKind::Break {
                        label: None,
                        expr: Some(Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
                            name: Identifier::new("value"),
                            shadow_index: 0,
                            span: span(),
                        })))),
                    }))),
                    span: span(),
                },
                HirStatement {
                    kind: HirStatementKind::Expr(Box::new(expr(HirExprKind::Break {
                        label: None,
                        expr: Some(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Bool(true))))),
                    }))),
                    span: span(),
                },
            ],
            expr: None,
            span: span(),
        }),
    }));

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("i64"));
    assert!(error.to_string().contains("bool"));
}

#[test]
fn infers_await_and_block_result_types_for_break_expr_validation() {
    let future_bool = ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Future"))), vec![ValkyrieType::Boolean]);
    let promise_bool = ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Promise"))), vec![ValkyrieType::Boolean]);
    let module = demo_module(expr(HirExprKind::Loop {
        label: None,
        pattern: None,
        iterator: None,
        condition: None,
        body: Box::new(HirBlock {
            statements: vec![
                HirStatement {
                    kind: HirStatementKind::Let {
                        is_mutable: false,
                        pattern: HirPattern::Variable(nyar_language::types::hir::HirIdentifier {
                            name: Identifier::new("future"),
                            shadow_index: 0,
                            span: span(),
                        }),
                        initializer: Some(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Unit)))),
                        ty: Some(future_bool),
                    },
                    span: span(),
                },
                HirStatement {
                    kind: HirStatementKind::Let {
                        is_mutable: false,
                        pattern: HirPattern::Variable(nyar_language::types::hir::HirIdentifier {
                            name: Identifier::new("promise"),
                            shadow_index: 0,
                            span: span(),
                        }),
                        initializer: Some(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Unit)))),
                        ty: Some(promise_bool),
                    },
                    span: span(),
                },
                HirStatement {
                    kind: HirStatementKind::Expr(Box::new(expr(HirExprKind::Break {
                        label: None,
                        expr: Some(Box::new(expr(HirExprKind::Await(Box::new(expr(HirExprKind::Variable(
                            nyar_language::types::hir::HirIdentifier { name: Identifier::new("future"), shadow_index: 0, span: span() },
                        ))))))),
                    }))),
                    span: span(),
                },
                HirStatement {
                    kind: HirStatementKind::Expr(Box::new(expr(HirExprKind::Break {
                        label: None,
                        expr: Some(Box::new(expr(HirExprKind::BlockOn(Box::new(expr(HirExprKind::Variable(
                            nyar_language::types::hir::HirIdentifier { name: Identifier::new("promise"), shadow_index: 0, span: span() },
                        ))))))),
                    }))),
                    span: span(),
                },
                HirStatement {
                    kind: HirStatementKind::Expr(Box::new(expr(HirExprKind::Break {
                        label: None,
                        expr: Some(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Integer64(1))))),
                    }))),
                    span: span(),
                },
            ],
            expr: None,
            span: span(),
        }),
    }));

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("bool"));
    assert!(error.to_string().contains("i64"));
}

#[test]
fn rejects_await_on_non_future_operand_at_hir_validation() {
    let module = demo_module(expr(HirExprKind::Block(Box::new(HirBlock {
        statements: vec![HirStatement {
            kind: HirStatementKind::Let {
                is_mutable: false,
                pattern: HirPattern::Variable(nyar_language::types::hir::HirIdentifier {
                    name: Identifier::new("value"),
                    shadow_index: 0,
                    span: span(),
                }),
                initializer: Some(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Integer64(1))))),
                ty: Some(ValkyrieType::Integer64 { signed: true }),
            },
            span: span(),
        }],
        expr: Some(Box::new(expr(HirExprKind::Await(Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
            name: Identifier::new("value"),
            shadow_index: 0,
            span: span(),
        }))))))),
        span: span(),
    }))));

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("`await`"));
    assert!(error.to_string().contains("Future<T>"));
    assert!(error.to_string().contains("i64"));
}

#[test]
fn rejects_awake_on_non_future_operand_at_hir_validation() {
    let module = demo_module(expr(HirExprKind::Block(Box::new(HirBlock {
        statements: vec![HirStatement {
            kind: HirStatementKind::Let {
                is_mutable: false,
                pattern: HirPattern::Variable(nyar_language::types::hir::HirIdentifier {
                    name: Identifier::new("flag"),
                    shadow_index: 0,
                    span: span(),
                }),
                initializer: Some(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Bool(true))))),
                ty: Some(ValkyrieType::Boolean),
            },
            span: span(),
        }],
        expr: Some(Box::new(expr(HirExprKind::Awake(Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
            name: Identifier::new("flag"),
            shadow_index: 0,
            span: span(),
        }))))))),
        span: span(),
    }))));

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("`awake`"));
    assert!(error.to_string().contains("Future<T>"));
    assert!(error.to_string().contains("bool"));
}

#[test]
fn rejects_block_on_non_future_operand_at_hir_validation() {
    let module = demo_module(expr(HirExprKind::Block(Box::new(HirBlock {
        statements: vec![HirStatement {
            kind: HirStatementKind::Let {
                is_mutable: false,
                pattern: HirPattern::Variable(nyar_language::types::hir::HirIdentifier {
                    name: Identifier::new("text"),
                    shadow_index: 0,
                    span: span(),
                }),
                initializer: Some(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::String(
                    nyar_language::types::hir::HirStringLiteral {
                        prefix: None,
                        quote_count: 1,
                        segments: vec![nyar_language::types::hir::HirStringSegment::Text("hello".to_string())],
                    },
                ))))),
                ty: Some(ValkyrieType::Utf8),
            },
            span: span(),
        }],
        expr: Some(Box::new(expr(HirExprKind::BlockOn(Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
            name: Identifier::new("text"),
            shadow_index: 0,
            span: span(),
        }))))))),
        span: span(),
    }))));

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("`block`"));
    assert!(error.to_string().contains("Future<T>"));
    assert!(error.to_string().contains("utf8"));
}

#[test]
fn rejects_block_inside_lambda_body_without_blocking_context() {
    let future_bool = ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Future"))), vec![ValkyrieType::Boolean]);
    let module =
        demo_module(expr(HirExprKind::Lambda {
            generics: Vec::new(),
            params: vec![nyar_language::types::hir::HirParam {
                name: nyar_language::types::hir::HirIdentifier { name: Identifier::new("future"), shadow_index: 0, span: span() },
                ty: future_bool,
                ..Default::default()
            }],
            return_type: ValkyrieType::Boolean,
            body: Box::new(HirBlock {
                statements: Vec::new(),
                expr: Some(Box::new(expr(HirExprKind::BlockOn(Box::new(expr(HirExprKind::Variable(
                    nyar_language::types::hir::HirIdentifier { name: Identifier::new("future"), shadow_index: 0, span: span() },
                ))))))),
                span: span(),
            }),
        }));

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("`block`"));
    assert!(error.to_string().contains("允许阻塞"));
}

#[test]
fn accepts_yield_inside_lambda_body_as_first_class_control_flow() {
    let module = demo_module(expr(HirExprKind::Lambda {
        generics: Vec::new(),
        params: Vec::new(),
        return_type: ValkyrieType::Unit,
        body: Box::new(HirBlock {
            statements: Vec::new(),
            expr: Some(Box::new(expr(HirExprKind::Yield(Some(Box::new(expr(HirExprKind::Literal(
                nyar_language::types::hir::HirLiteral::Integer64(1),
            )))))))),
            span: span(),
        }),
    }));

    ControlFlowScheduler::validate_hir_module(&module).expect("lambda may contain yield");
}

#[test]
fn accepts_bare_yield_as_unit_payload_sugar() {
    let module = demo_module(expr(HirExprKind::Yield(None)));
    ControlFlowScheduler::validate_hir_module(&module).expect("bare yield should lower as yield unit");
}

#[test]
fn lowers_bare_yield_into_unit_payload_effect() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 207 });
    let mir = compiler
        .compile_source_to_mir(
            r#"micro main() {
    yield
    return
}
"#,
        )
        .unwrap();
    let effect_block = mir.functions[0]
        .blocks
        .iter()
        .find(|block| matches!(block.terminator, MirTerminator::YieldToRuntime { effect, .. } if effect == MirEffectKind::Yield))
        .expect("expected yield runtime effect");
    assert!(matches!(effect_block.terminator, MirTerminator::YieldToRuntime { payload: Some(MirOperand::Constant(MirConstant::Unit)), .. }));
}

#[test]
fn accepts_yield_from_inside_lambda_body_as_first_class_control_flow() {
    let module = demo_module(expr(HirExprKind::Lambda {
        generics: Vec::new(),
        params: Vec::new(),
        return_type: ValkyrieType::Unit,
        body: Box::new(HirBlock {
            statements: Vec::new(),
            expr: Some(Box::new(expr(HirExprKind::YieldFrom(Box::new(expr(HirExprKind::Variable(
                nyar_language::types::hir::HirIdentifier { name: Identifier::new("values"), shadow_index: 0, span: span() },
            ))))))),
            span: span(),
        }),
    }));

    ControlFlowScheduler::validate_hir_module(&module).expect("lambda may contain yield from");
}

#[test]
fn accepts_block_in_function_body_with_future_operand() {
    let future_bool = ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Future"))), vec![ValkyrieType::Boolean]);
    let module = demo_module(expr(HirExprKind::Block(Box::new(HirBlock {
        statements: vec![HirStatement {
            kind: HirStatementKind::Let {
                is_mutable: false,
                pattern: HirPattern::Variable(nyar_language::types::hir::HirIdentifier {
                    name: Identifier::new("future"),
                    shadow_index: 0,
                    span: span(),
                }),
                initializer: Some(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Unit)))),
                ty: Some(future_bool),
            },
            span: span(),
        }],
        expr: Some(Box::new(expr(HirExprKind::BlockOn(Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
            name: Identifier::new("future"),
            shadow_index: 0,
            span: span(),
        }))))))),
        span: span(),
    }))));
    ControlFlowScheduler::validate_hir_module(&module).expect("block on future in function body should be accepted");
}

#[test]
fn accepts_yield_in_function_body() {
    let module =
        demo_module(expr(HirExprKind::Yield(Some(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Integer64(1))))))));
    ControlFlowScheduler::validate_hir_module(&module).expect("yield in function body should be accepted");
}

#[test]
fn accepts_yield_from_in_function_body() {
    let module = demo_module(expr(HirExprKind::YieldFrom(Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
        name: Identifier::new("values"),
        shadow_index: 0,
        span: span(),
    }))))));
    ControlFlowScheduler::validate_hir_module(&module).expect("yield from in function body should be accepted");
}
#[test]
fn infers_awake_as_unit_for_break_expr_validation() {
    let future_bool = ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Future"))), vec![ValkyrieType::Boolean]);
    let module = demo_module(expr(HirExprKind::Loop {
        label: None,
        pattern: None,
        iterator: None,
        condition: None,
        body: Box::new(HirBlock {
            statements: vec![
                HirStatement {
                    kind: HirStatementKind::Let {
                        is_mutable: false,
                        pattern: HirPattern::Variable(nyar_language::types::hir::HirIdentifier {
                            name: Identifier::new("future"),
                            shadow_index: 0,
                            span: span(),
                        }),
                        initializer: Some(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Unit)))),
                        ty: Some(future_bool),
                    },
                    span: span(),
                },
                HirStatement {
                    kind: HirStatementKind::Expr(Box::new(expr(HirExprKind::Break {
                        label: None,
                        expr: Some(Box::new(expr(HirExprKind::Awake(Box::new(expr(HirExprKind::Variable(
                            nyar_language::types::hir::HirIdentifier { name: Identifier::new("future"), shadow_index: 0, span: span() },
                        ))))))),
                    }))),
                    span: span(),
                },
                HirStatement {
                    kind: HirStatementKind::Expr(Box::new(expr(HirExprKind::Break {
                        label: None,
                        expr: Some(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Integer64(1))))),
                    }))),
                    span: span(),
                },
            ],
            expr: None,
            span: span(),
        }),
    }));

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("unit"));
    assert!(error.to_string().contains("i64"));
}

#[test]
fn infers_yield_as_unit_for_break_expr_validation() {
    let module = demo_module(expr(HirExprKind::Loop {
        label: None,
        pattern: None,
        iterator: None,
        condition: None,
        body: Box::new(HirBlock {
            statements: vec![
                HirStatement {
                    kind: HirStatementKind::Expr(Box::new(expr(HirExprKind::Break {
                        label: None,
                        expr: Some(Box::new(expr(HirExprKind::Yield(Some(Box::new(expr(HirExprKind::Literal(
                            nyar_language::types::hir::HirLiteral::Integer64(1),
                        )))))))),
                    }))),
                    span: span(),
                },
                HirStatement {
                    kind: HirStatementKind::Expr(Box::new(expr(HirExprKind::Break {
                        label: None,
                        expr: Some(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Bool(true))))),
                    }))),
                    span: span(),
                },
            ],
            expr: None,
            span: span(),
        }),
    }));

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("unit"));
    assert!(error.to_string().contains("bool"));
}

#[test]
fn infers_yield_from_as_unit_for_break_expr_validation() {
    let module = demo_module(expr(HirExprKind::Loop {
        label: None,
        pattern: None,
        iterator: None,
        condition: None,
        body: Box::new(HirBlock {
            statements: vec![
                HirStatement {
                    kind: HirStatementKind::Expr(Box::new(expr(HirExprKind::Break {
                        label: None,
                        expr: Some(Box::new(expr(HirExprKind::YieldFrom(Box::new(expr(HirExprKind::Variable(
                            nyar_language::types::hir::HirIdentifier { name: Identifier::new("values"), shadow_index: 0, span: span() },
                        ))))))),
                    }))),
                    span: span(),
                },
                HirStatement {
                    kind: HirStatementKind::Expr(Box::new(expr(HirExprKind::Break {
                        label: None,
                        expr: Some(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Integer64(1))))),
                    }))),
                    span: span(),
                },
            ],
            expr: None,
            span: span(),
        }),
    }));

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("unit"));
    assert!(error.to_string().contains("i64"));
}

#[test]
fn rejects_incompatible_return_expr_type_at_hir_validation() {
    let module = HirModule {
        name: NamePath::new(vec![Identifier::new("demo")]),
        doc: HirDocumentation::default(),
        imports: Vec::new(),
        warnings: Vec::new(),
        submodules: Vec::new(),
        functions: vec![HirFunction {
            name: Identifier::new("main"),
            declaring_namespace: NamePath::default(),
            doc: HirDocumentation::default(),
            annotations: Vec::new(),
            generics: Vec::new(),
            params: Vec::new(),
            return_type: ValkyrieType::Integer64 { signed: true },
            body: HirBlock {
                statements: Vec::new(),
                expr: Some(Box::new(expr(HirExprKind::Return(Some(Box::new(expr(HirExprKind::Literal(
                    nyar_language::types::hir::HirLiteral::Bool(true),
                )))))))),
                span: span(),
            },
            span: span(),
            visibility: HirVisibility::default(),
            is_abstract: false,
            is_final: false,
            is_virtual: false,
            is_override: false,
        }],
        structs: Vec::new(),
        enums: Vec::new(),
        imported_enums: Vec::new(),
        flags: Vec::new(),
        traits: Vec::new(),
        impls: Vec::new(),
        type_functions: Vec::new(),
        type_families: Vec::new(),
        widgets: Vec::new(),
        singletons: Vec::new(),
        statements: Vec::new(),
        type_aliases: Vec::new(),
    };

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("return expr"));
    assert!(error.to_string().contains("bool"));
    assert!(error.to_string().contains("i64"));
}

#[test]
fn infers_typed_variable_for_return_expr_validation() {
    let module = HirModule {
        name: NamePath::new(vec![Identifier::new("demo")]),
        doc: HirDocumentation::default(),
        imports: Vec::new(),
        warnings: Vec::new(),
        submodules: Vec::new(),
        functions: vec![HirFunction {
            name: Identifier::new("main"),
            declaring_namespace: NamePath::default(),
            doc: HirDocumentation::default(),
            annotations: Vec::new(),
            generics: Vec::new(),
            params: Vec::new(),
            return_type: ValkyrieType::Boolean,
            body: HirBlock {
                statements: vec![HirStatement {
                    kind: HirStatementKind::Let {
                        is_mutable: false,
                        pattern: HirPattern::Variable(nyar_language::types::hir::HirIdentifier {
                            name: Identifier::new("value"),
                            shadow_index: 0,
                            span: span(),
                        }),
                        initializer: Some(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Integer64(1))))),
                        ty: Some(ValkyrieType::Integer64 { signed: true }),
                    },
                    span: span(),
                }],
                expr: Some(Box::new(expr(HirExprKind::Return(Some(Box::new(expr(HirExprKind::Variable(
                    nyar_language::types::hir::HirIdentifier { name: Identifier::new("value"), shadow_index: 0, span: span() },
                )))))))),
                span: span(),
            },
            span: span(),
            visibility: HirVisibility::default(),
            is_abstract: false,
            is_final: false,
            is_virtual: false,
            is_override: false,
        }],
        structs: Vec::new(),
        enums: Vec::new(),
        imported_enums: Vec::new(),
        flags: Vec::new(),
        traits: Vec::new(),
        impls: Vec::new(),
        type_functions: Vec::new(),
        type_families: Vec::new(),
        widgets: Vec::new(),
        singletons: Vec::new(),
        statements: Vec::new(),
        type_aliases: Vec::new(),
    };

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("return expr"));
    assert!(error.to_string().contains("i64"));
    assert!(error.to_string().contains("bool"));
}

#[test]
fn infers_await_result_type_for_return_expr_validation() {
    let future_bool = ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Future"))), vec![ValkyrieType::Boolean]);
    let module = HirModule {
        name: NamePath::new(vec![Identifier::new("demo")]),
        doc: HirDocumentation::default(),
        imports: Vec::new(),
        warnings: Vec::new(),
        submodules: Vec::new(),
        functions: vec![HirFunction {
            name: Identifier::new("main"),
            declaring_namespace: NamePath::default(),
            doc: HirDocumentation::default(),
            annotations: Vec::new(),
            generics: Vec::new(),
            params: Vec::new(),
            return_type: ValkyrieType::Integer64 { signed: true },
            body: HirBlock {
                statements: vec![HirStatement {
                    kind: HirStatementKind::Let {
                        is_mutable: false,
                        pattern: HirPattern::Variable(nyar_language::types::hir::HirIdentifier {
                            name: Identifier::new("future"),
                            shadow_index: 0,
                            span: span(),
                        }),
                        initializer: Some(Box::new(expr(HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Unit)))),
                        ty: Some(future_bool),
                    },
                    span: span(),
                }],
                expr: Some(Box::new(expr(HirExprKind::Return(Some(Box::new(expr(HirExprKind::Await(Box::new(expr(
                    HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
                        name: Identifier::new("future"),
                        shadow_index: 0,
                        span: span(),
                    }),
                )))))))))),
                span: span(),
            },
            span: span(),
            visibility: HirVisibility::default(),
            is_abstract: false,
            is_final: false,
            is_virtual: false,
            is_override: false,
        }],
        structs: Vec::new(),
        enums: Vec::new(),
        imported_enums: Vec::new(),
        flags: Vec::new(),
        traits: Vec::new(),
        impls: Vec::new(),
        type_functions: Vec::new(),
        type_families: Vec::new(),
        widgets: Vec::new(),
        singletons: Vec::new(),
        statements: Vec::new(),
        type_aliases: Vec::new(),
    };

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("return expr"));
    assert!(error.to_string().contains("bool"));
    assert!(error.to_string().contains("i64"));
}

#[test]
fn infers_match_arm_pattern_variable_for_return_expr_validation() {
    let module = HirModule {
        name: NamePath::new(vec![Identifier::new("demo")]),
        doc: HirDocumentation::default(),
        imports: Vec::new(),
        warnings: Vec::new(),
        submodules: Vec::new(),
        functions: vec![HirFunction {
            name: Identifier::new("main"),
            declaring_namespace: NamePath::default(),
            doc: HirDocumentation::default(),
            annotations: Vec::new(),
            generics: Vec::new(),
            params: vec![nyar_language::types::hir::HirParam {
                name: nyar_language::types::hir::HirIdentifier { name: Identifier::new("input"), shadow_index: 0, span: span() },
                ty: ValkyrieType::Integer64 { signed: true },
                ..Default::default()
            }],
            return_type: ValkyrieType::Boolean,
            body: HirBlock {
                statements: Vec::new(),
                expr: Some(Box::new(expr(HirExprKind::Return(Some(Box::new(expr(HirExprKind::Match {
                    scrutinee: Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
                        name: Identifier::new("input"),
                        shadow_index: 0,
                        span: span(),
                    }))),
                    arms: vec![HirMatchArm {
                        pattern: HirPattern::Variable(nyar_language::types::hir::HirIdentifier {
                            name: Identifier::new("captured"),
                            shadow_index: 0,
                            span: span(),
                        }),
                        guard: None,
                        body: Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
                            name: Identifier::new("captured"),
                            shadow_index: 0,
                            span: span(),
                        }))),
                    }],
                }))))))),
                span: span(),
            },
            span: span(),
            visibility: HirVisibility::default(),
            is_abstract: false,
            is_final: false,
            is_virtual: false,
            is_override: false,
        }],
        structs: Vec::new(),
        enums: Vec::new(),
        imported_enums: Vec::new(),
        flags: Vec::new(),
        traits: Vec::new(),
        impls: Vec::new(),
        type_functions: Vec::new(),
        type_families: Vec::new(),
        widgets: Vec::new(),
        singletons: Vec::new(),
        statements: Vec::new(),
        type_aliases: Vec::new(),
    };

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("return expr"));
    assert!(error.to_string().contains("bool"));
    assert!(error.to_string().contains("i64"));
}

#[test]
fn infers_nested_tuple_match_arm_pattern_variable_for_return_expr_validation() {
    let module = HirModule {
        name: NamePath::new(vec![Identifier::new("demo")]),
        doc: HirDocumentation::default(),
        imports: Vec::new(),
        warnings: Vec::new(),
        submodules: Vec::new(),
        functions: vec![HirFunction {
            name: Identifier::new("main"),
            declaring_namespace: NamePath::default(),
            doc: HirDocumentation::default(),
            annotations: Vec::new(),
            generics: Vec::new(),
            params: vec![nyar_language::types::hir::HirParam {
                name: nyar_language::types::hir::HirIdentifier { name: Identifier::new("input"), shadow_index: 0, span: span() },
                ty: ValkyrieType::Tuple(vec![
                    ValkyrieType::Integer64 { signed: true },
                    ValkyrieType::Tuple(vec![ValkyrieType::Boolean, ValkyrieType::Integer64 { signed: true }]),
                ]),
                ..Default::default()
            }],
            return_type: ValkyrieType::Integer64 { signed: true },
            body: HirBlock {
                statements: Vec::new(),
                expr: Some(Box::new(expr(HirExprKind::Return(Some(Box::new(expr(HirExprKind::Match {
                    scrutinee: Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
                        name: Identifier::new("input"),
                        shadow_index: 0,
                        span: span(),
                    }))),
                    arms: vec![HirMatchArm {
                        pattern: HirPattern::Tuple(vec![
                            HirPattern::Wildcard,
                            HirPattern::Tuple(vec![
                                HirPattern::Variable(nyar_language::types::hir::HirIdentifier {
                                    name: Identifier::new("captured"),
                                    shadow_index: 0,
                                    span: span(),
                                }),
                                HirPattern::Wildcard,
                            ]),
                        ]),
                        guard: None,
                        body: Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
                            name: Identifier::new("captured"),
                            shadow_index: 0,
                            span: span(),
                        }))),
                    }],
                }))))))),
                span: span(),
            },
            span: span(),
            visibility: HirVisibility::default(),
            is_abstract: false,
            is_final: false,
            is_virtual: false,
            is_override: false,
        }],
        structs: Vec::new(),
        enums: Vec::new(),
        imported_enums: Vec::new(),
        flags: Vec::new(),
        traits: Vec::new(),
        impls: Vec::new(),
        type_functions: Vec::new(),
        type_families: Vec::new(),
        widgets: Vec::new(),
        singletons: Vec::new(),
        statements: Vec::new(),
        type_aliases: Vec::new(),
    };

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("return expr"));
    assert!(error.to_string().contains("bool"));
    assert!(error.to_string().contains("i64"));
}

#[test]
fn rejects_case_fallthrough_when_next_arm_tries_to_reuse_previous_pattern_binding() {
    let module = HirModule {
        name: NamePath::new(vec![Identifier::new("demo")]),
        doc: HirDocumentation::default(),
        imports: Vec::new(),
        warnings: Vec::new(),
        submodules: Vec::new(),
        functions: vec![HirFunction {
            name: Identifier::new("main"),
            declaring_namespace: NamePath::default(),
            doc: HirDocumentation::default(),
            annotations: Vec::new(),
            generics: Vec::new(),
            params: vec![nyar_language::types::hir::HirParam {
                name: nyar_language::types::hir::HirIdentifier { name: Identifier::new("input"), shadow_index: 0, span: span() },
                ty: ValkyrieType::Integer64 { signed: true },
                ..Default::default()
            }],
            return_type: ValkyrieType::Unit,
            body: HirBlock {
                statements: Vec::new(),
                expr: Some(Box::new(expr(HirExprKind::Case {
                    scrutinee: Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
                        name: Identifier::new("input"),
                        shadow_index: 0,
                        span: span(),
                    }))),
                    arms: vec![
                        HirMatchArm {
                            pattern: HirPattern::Variable(nyar_language::types::hir::HirIdentifier {
                                name: Identifier::new("captured"),
                                shadow_index: 0,
                                span: span(),
                            }),
                            guard: None,
                            body: Box::new(expr(HirExprKind::Fallthrough)),
                        },
                        HirMatchArm {
                            pattern: HirPattern::Else,
                            guard: None,
                            body: Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
                                name: Identifier::new("captured"),
                                shadow_index: 0,
                                span: span(),
                            }))),
                        },
                    ],
                }))),
                span: span(),
            },
            span: span(),
            visibility: HirVisibility::default(),
            is_abstract: false,
            is_final: false,
            is_virtual: false,
            is_override: false,
        }],
        structs: Vec::new(),
        enums: Vec::new(),
        imported_enums: Vec::new(),
        flags: Vec::new(),
        traits: Vec::new(),
        impls: Vec::new(),
        type_functions: Vec::new(),
        type_families: Vec::new(),
        widgets: Vec::new(),
        singletons: Vec::new(),
        statements: Vec::new(),
        type_aliases: Vec::new(),
    };

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("fallthrough"));
    assert!(error.to_string().contains("captured"));
}

#[test]
fn rejects_empty_return_for_non_unit_function() {
    let module = HirModule {
        name: NamePath::new(vec![Identifier::new("demo")]),
        doc: HirDocumentation::default(),
        imports: Vec::new(),
        warnings: Vec::new(),
        submodules: Vec::new(),
        functions: vec![HirFunction {
            name: Identifier::new("main"),
            declaring_namespace: NamePath::default(),
            doc: HirDocumentation::default(),
            annotations: Vec::new(),
            generics: Vec::new(),
            params: Vec::new(),
            return_type: ValkyrieType::Boolean,
            body: HirBlock { statements: Vec::new(), expr: Some(Box::new(expr(HirExprKind::Return(None)))), span: span() },
            span: span(),
            visibility: HirVisibility::default(),
            is_abstract: false,
            is_final: false,
            is_virtual: false,
            is_override: false,
        }],
        structs: Vec::new(),
        enums: Vec::new(),
        imported_enums: Vec::new(),
        flags: Vec::new(),
        traits: Vec::new(),
        impls: Vec::new(),
        type_functions: Vec::new(),
        type_families: Vec::new(),
        widgets: Vec::new(),
        singletons: Vec::new(),
        statements: Vec::new(),
        type_aliases: Vec::new(),
    };

    let error = ControlFlowScheduler::validate_hir_module(&module).unwrap_err();
    assert!(error.to_string().contains("return"));
    assert!(error.to_string().contains("bool"));
}
