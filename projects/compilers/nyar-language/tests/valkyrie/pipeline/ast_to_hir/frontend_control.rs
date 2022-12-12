use super::*;

#[test]
fn compiler_facade_parses_and_lowers_return_statement() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 3 });
    let module = compiler
        .compile_source(
            r#"micro main() {
    return;
}
"#,
        )
        .unwrap();
    let statement = &module.functions[0].body.statements[0];
    let HirStatementKind::Expr(expression) = &statement.kind
    else {
        panic!("expected expression statement");
    };
    assert!(matches!(expression.kind, HirExprKind::Return(_)));
}

#[test]
fn compiler_facade_lowers_yield_statement_into_hir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 79 });
    let module = compiler
        .compile_source(
            r#"micro main() {
    yield 1;
}
"#,
        )
        .unwrap();
    let statement = &module.functions[0].body.statements[0];
    let HirStatementKind::Expr(expression) = &statement.kind
    else {
        panic!("expected expression statement");
    };
    assert!(matches!(
        expression.kind,
        HirExprKind::Yield(Some(ref value)) if matches!(value.kind, HirExprKind::Literal(_))
    ));
}

#[test]
fn compiler_facade_lowers_yield_from_statement_into_hir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 80 });
    let module = compiler
        .compile_source(
            r#"micro main() {
    yield from values;
}
"#,
        )
        .unwrap();
    let statement = &module.functions[0].body.statements[0];
    let HirStatementKind::Expr(expression) = &statement.kind
    else {
        panic!("expected expression statement");
    };
    assert!(matches!(
        expression.kind,
        HirExprKind::YieldFrom(ref value)
            if matches!(value.kind, HirExprKind::Variable(ref identifier) if identifier.name.as_str() == "values")
    ));
}

#[test]
fn compiler_facade_lowers_raise_statement_into_hir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 94 });
    let module = compiler
        .compile_source(
            r#"micro main() {
    raise problem
}
"#,
        )
        .unwrap();
    let expression = module.functions[0].body.expr.as_ref().expect("expected final expression");
    assert!(matches!(
        expression.kind,
        HirExprKind::Raise(ref value)
            if matches!(value.kind, HirExprKind::Variable(ref identifier) if identifier.name.as_str() == "problem")
    ));
}

#[test]
fn compiler_facade_lowers_resume_statement_into_hir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 95 });
    let module = compiler
        .compile_source(
            r#"micro main() {
    catch task {
        case Yielded(next_value):
            resume next_value
    }
}"#,
        )
        .unwrap();
    let expression = module.functions[0].body.expr.as_ref().expect("expected final expression");
    let HirExprKind::Catch { arms, .. } = &expression.kind
    else {
        panic!("expected catch expression");
    };
    let HirExprKind::Block(body) = &arms[0].body.kind
    else {
        panic!("expected catch arm body block");
    };
    let Some(body_expr) = block_expr_or_single_statement(body)
    else {
        panic!("expected resume expression inside catch arm body");
    };
    assert!(matches!(
        body_expr.kind,
        HirExprKind::Resume(ref value)
            if matches!(value.kind, HirExprKind::Variable(ref identifier) if identifier.name.as_str() == "next_value")
    ));
}

#[test]
fn lowers_range_pattern_into_hir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 1001 });
    let module = compiler
        .compile_source(
            r#"micro main() -> bool {
    return match value {
        case 1..=10:
            true
        else:
            false
    };
}"#,
        )
        .unwrap();
    let HirStatementKind::Expr(statement) = &module.functions[0].body.statements[0].kind
    else {
        panic!("expected expression statement")
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression")
    };
    let HirExprKind::Match { arms, .. } = &expression.kind
    else {
        panic!("expected match expression")
    };
    assert!(matches!(arms[0].pattern, HirPattern::Range { .. }));
}

#[test]
fn lowers_array_extractor_pattern_into_hir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 1002 });
    let module = compiler
        .compile_source(
            r#"micro main() -> bool {
    return match items {
        case []:
            true
        else:
            false
    };
}"#,
        )
        .unwrap();
    let HirStatementKind::Expr(statement) = &module.functions[0].body.statements[0].kind
    else {
        panic!("expected expression statement")
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression")
    };
    let HirExprKind::Match { arms, .. } = &expression.kind
    else {
        panic!("expected match expression")
    };
    assert!(matches!(arms[0].pattern, HirPattern::Extractor(nyar_language::types::hir::HirExtractorPattern::Array { .. })));
}
#[test]
fn compiler_facade_parses_fallthrough_as_expression_statement_and_rejects_in_validation() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 97 });
    let error = compiler
        .compile_source(
            r#"micro main() {
    fallthrough
}
"#,
        )
        .unwrap_err();
    assert!(error.to_string().contains("fallthrough"));
}

#[test]
fn compiler_facade_rejects_fallthrough_in_value_match_expression() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 971 });
    let error = compiler
        .compile_source_to_mir(
            r#"micro main() -> bool {
    return match value {
        case Flag():
            fallthrough
        else:
            true
    };
}
"#,
        )
        .unwrap_err();
    assert!(error.to_string().contains("fallthrough"));
}

#[test]
fn compiler_facade_lowers_statement_match_fallthrough_into_jump_chain() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 972 });
    let hir = compiler
        .compile_source(
            r#"micro main() {
    match value {
        case Flag():
            fallthrough
        else:
            return
    };
    return
}
"#,
        )
        .unwrap();
    let HirStatementKind::Expr(case_expr) = &hir.functions[0].body.statements[0].kind
    else {
        panic!("expected expression statement");
    };
    assert!(matches!(case_expr.kind, HirExprKind::Case { .. }));

    let mir = compiler
        .compile_source_to_mir(
            r#"micro main() {
    match value {
        case Flag():
            fallthrough
        else:
            return
    };
    return
}
"#,
        )
        .unwrap();
    let first_fallthrough_block = mir.functions[0]
        .blocks
        .iter()
        .find(|block| matches!(block.terminator, nyar_language::MirTerminator::Jump { .. }) && block.label.starts_with("case_arm_0"))
        .expect("expected first arm block");
    let target = match first_fallthrough_block.terminator {
        nyar_language::MirTerminator::Jump { target, .. } => target,
        _ => unreachable!(),
    };
    let target_block = mir.functions[0].blocks.iter().find(|block| block.id == target).expect("expected jump target block");
    assert!(target_block.label.starts_with("case_arm_1"));

    let lir = compile_source_to_legacy_lir(
        &compiler,
            r#"micro main() {
    match value {
        case Flag():
            fallthrough
        else:
            return
    };
    return
}
"#,
    )
    .unwrap();
    let first_lir_fallthrough_block = lir.functions[0]
        .blocks
        .iter()
        .find(|block| matches!(block.terminator, LirTerminator::Jump { .. }) && block.label.starts_with("case_arm_0"))
        .expect("expected first lir arm block");
    let target = match first_lir_fallthrough_block.terminator {
        LirTerminator::Jump { target, .. } => target,
        _ => unreachable!(),
    };
    let target_block = lir.functions[0].blocks.iter().find(|block| block.id == target).expect("expected lir jump target block");
    assert!(target_block.label.starts_with("case_arm_1"));
}

#[test]
fn compiler_facade_lowers_literal_variable_and_or_match_patterns_into_hir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 973 });
    let hir = compiler
        .compile_source(
            r#"micro main(value: i64) -> bool {
    return match value {
        case 1 | 2:
            true
        case n if n > 0:
            false
        case _:
            false
    };
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(statement) = &hir.functions[0].body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Match { arms, .. } = &expression.kind
    else {
        panic!("expected match expression");
    };

    assert!(matches!(
        &arms[0].pattern,
        nyar_language::types::hir::HirPattern::Or(patterns)
            if matches!(patterns.as_slice(),
                [
                    nyar_language::types::hir::HirPattern::Literal(HirLiteral::Integer64(1)),
                    nyar_language::types::hir::HirPattern::Literal(HirLiteral::Integer64(2))
                ])
    ));
    assert!(matches!(
        &arms[1].pattern,
        nyar_language::types::hir::HirPattern::Variable(identifier) if identifier.name.as_str() == "n"
    ));
    assert!(arms[1].guard.is_some());
    assert!(matches!(&arms[2].pattern, nyar_language::types::hir::HirPattern::Wildcard));
}

#[test]
fn compiler_facade_lowers_catch_expression_into_hir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 96 });
    let module = compiler
        .compile_source(
            r#"micro main() {
    catch task {
        case Yielded(value):
            resume value
        else:
            raise fallback
    }
}
"#,
        )
        .unwrap();
    let expression = module.functions[0].body.expr.as_ref().expect("expected final expression");
    let HirExprKind::Catch { expr, arms } = &expression.kind
    else {
        panic!("expected catch expression");
    };
    assert!(matches!(expr.kind, HirExprKind::Variable(ref identifier) if identifier.name.as_str() == "task"));
    assert_eq!(arms.len(), 2);
    assert!(matches!(
        arms[0].pattern,
        HirPattern::Extractor(nyar_language::types::hir::HirExtractorPattern::Constructor { ref name, ref fields, .. })
            if name.to_string() == "Yielded"
                && fields.len() == 1
                && matches!(&fields[0], HirPattern::Variable(identifier) if identifier.name.as_str() == "value")
    ));
    let HirExprKind::Block(first_body) = &arms[0].body.kind
    else {
        panic!("expected first catch arm body block");
    };
    assert!(matches!(block_expr_or_single_statement(first_body), Some(HirExpr { kind: HirExprKind::Resume(_), .. })));
    assert!(matches!(arms[1].pattern, HirPattern::Else));
    let HirExprKind::Block(second_body) = &arms[1].body.kind
    else {
        panic!("expected else catch arm body block");
    };
    assert!(matches!(block_expr_or_single_statement(second_body), Some(HirExpr { kind: HirExprKind::Raise(_), .. })));
}

#[test]
fn compiler_facade_lowers_named_object_pattern_into_hir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 112 });
    let module = compiler
        .compile_source(
            r#"micro main() {
    catch task {
        case Yielded { next_value }:
            resume next_value
        else:
            raise fallback
    }
}"#,
        )
        .unwrap();
    let expression = module.functions[0].body.expr.as_ref().expect("expected final expression");
    let HirExprKind::Catch { arms, .. } = &expression.kind
    else {
        panic!("expected catch expression");
    };
    assert!(matches!(
        &arms[0].pattern,
        HirPattern::Object { name: Some(name), fields, .. }
            if name.to_string() == "Yielded"
                && fields.len() == 1
                && matches!(&fields[0], (field, HirPattern::Variable(identifier))
                    if field.as_str() == "next_value" && identifier.name.as_str() == "next_value")
    ));
}

#[test]
fn compiler_facade_lowers_anonymous_object_pattern_into_hir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 113 });
    let module = compiler
        .compile_source(
            r#"micro main() {
    catch task {
        case { foo, bar }:
            resume foo
        else:
            raise fallback
    }
}"#,
        )
        .unwrap();
    let expression = module.functions[0].body.expr.as_ref().expect("expected final expression");
    let HirExprKind::Catch { arms, .. } = &expression.kind
    else {
        panic!("expected catch expression");
    };
    assert!(matches!(
        &arms[0].pattern,
        HirPattern::Object { name: None, fields, .. }
            if fields.len() == 2
                && matches!(&fields[0], (field, HirPattern::Variable(identifier))
                    if field.as_str() == "foo" && identifier.name.as_str() == "foo")
                && matches!(&fields[1], (field, HirPattern::Variable(identifier))
                    if field.as_str() == "bar" && identifier.name.as_str() == "bar")
    ));
}

#[test]
fn lowers_uncaught_raise_into_mir_perform_effect() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 104 });
    let mir = compiler
        .compile_source_to_mir(
            r#"micro main() {
    raise problem
    return
}"#,
        )
        .unwrap();

    assert!(mir.functions[0].blocks.iter().any(|block| {
        matches!(
            &block.terminator,
            nyar_language::MirTerminator::PerformEffect { effect, payload: Some(_), .. }
                if *effect == MirEffectKind::Raise
        )
    }));
}

#[test]
fn lowers_catch_resume_into_mir_handler_region() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 105 });
    let mir = compiler
        .compile_source_to_mir(
            r#"micro main() {
    let value = catch raise task {
        case Yielded(next_value):
            resume next_value
        else:
            raise fallback
    }
    return value
}"#,
        )
        .unwrap();

    let dispatch_block = mir.functions[0].blocks.iter().find(|block| block.label == "catch_dispatch").expect("expected catch dispatch block");
    assert_eq!(dispatch_block.parameters.len(), 1);

    let resume_block = mir.functions[0].blocks.iter().find(|block| block.label == "catch_resume").expect("expected catch resume block");
    assert_eq!(resume_block.parameters.len(), 1);

    let exit_block = mir.functions[0].blocks.iter().find(|block| block.label == "catch_exit").expect("expected catch exit block");
    assert_eq!(exit_block.parameters.len(), 1);

    assert!(mir.functions[0].blocks.iter().any(|block| {
        matches!(
            &block.terminator,
            nyar_language::MirTerminator::Jump { target, arguments }
                if *target == resume_block.id && arguments.len() == 1
        )
    }));
}

#[test]
fn lowers_catch_guard_into_mir_arm_branch() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 107 });
    let mir = compiler
        .compile_source_to_mir(
            r#"micro main() {
    let value = catch raise task {
        case Yielded(next_value) if false:
            resume next_value
        else:
            raise fallback
    }
    return value
}"#,
        )
        .unwrap();

    let guard_block = mir.functions[0].blocks.iter().find(|block| block.label == "catch_arm_0").expect("expected first catch arm block");
    let match_block = mir.functions[0]
        .blocks
        .iter()
        .find(|block| block.label == "catch_arm_0_match")
        .unwrap_or_else(|| panic!("expected first catch arm match block, got blocks: {:?}", mir.functions[0].blocks));
    let body_block =
        mir.functions[0].blocks.iter().find(|block| block.label == "catch_arm_0_body").expect("expected guarded catch arm body block");

    assert!(
        guard_block.instructions.iter().any(|instruction| matches!(instruction.kind, nyar_language::MirInstructionKind::PatternMatch { .. }))
    );

    assert!(matches!(
        &guard_block.terminator,
        nyar_language::MirTerminator::Branch { then_target, .. } if *then_target == match_block.id
    ));

    assert!(matches!(
        &match_block.terminator,
        nyar_language::MirTerminator::Branch { then_target, .. } if *then_target == body_block.id
    ));
}

#[test]
fn rethrows_unmatched_catch_effect_into_outer_raise_path() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 111 });
    let mir = compiler
        .compile_source_to_mir(
            r#"micro main() {
    catch raise task {
        case Yielded(next_value):
            resume next_value
    }
    return
}"#,
        )
        .unwrap();

    let no_match_block = mir.functions[0].blocks.iter().find(|block| block.label == "catch_no_match").expect("expected catch no-match block");
    assert!(matches!(
        &no_match_block.terminator,
        nyar_language::MirTerminator::PerformEffect { effect, payload: Some(_), .. }
            if *effect == MirEffectKind::Raise
    ));

    let raise_resume_block =
        mir.functions[0].blocks.iter().find(|block| block.label == "raise_resume").expect("expected propagated raise resume block");
    let catch_exit_block = mir.functions[0].blocks.iter().find(|block| block.label == "catch_exit").expect("expected catch exit block");
    assert!(matches!(
        &raise_resume_block.terminator,
        nyar_language::MirTerminator::Jump { target, arguments }
            if *target == catch_exit_block.id && arguments.len() == 1
    ));
}

#[test]
fn preserves_raise_effect_into_lir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 106 });
    let lir = compile_source_to_legacy_lir(
        &compiler,
            r#"micro main() {
    raise problem
    return
}"#,
    )
    .unwrap();

    assert!(lir.functions[0].blocks.iter().any(|block| {
        matches!(
            &block.terminator,
            LirTerminator::PerformEffect { effect, payload: Some(_), .. }
                if *effect == LirEffectKind::Raise
        )
    }));
}

#[test]
fn preserves_catch_pattern_match_into_lir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 108 });
    let lir = compile_source_to_legacy_lir(
        &compiler,
            r#"micro main() {
    let value = catch raise task {
        case Yielded(next_value) if false:
            resume next_value
        else:
            raise fallback
    }
    return value
}"#,
    )
    .unwrap();

    let guard_block = lir.functions[0].blocks.iter().find(|block| block.label == "catch_arm_0").expect("expected first catch arm block");

    assert!(
        guard_block
            .operations
            .iter()
            .any(|operation| matches!(operation.kind, nyar_language::lir::LirOperationKind::PatternMatch { .. }))
    );
}

#[test]
fn compiler_facade_lowers_labeled_loop_control_into_hir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 99 });
    let module = compiler
        .compile_source(
            r#"micro main() {
    'outer: while true {
        while true {
            continue 'outer
        }
    }
}"#,
        )
        .unwrap();
    let expression = block_expr_or_single_statement(&module.functions[0].body).expect("expected outer loop expression");
    let HirExprKind::Loop { label: Some(label), body, .. } = &expression.kind
    else {
        panic!("expected labeled loop");
    };
    assert_eq!(label.as_str(), "outer");
    let inner_loop = block_expr_or_single_statement(body).expect("expected inner loop expression");
    let HirExprKind::Loop { body: inner_body, .. } = &inner_loop.kind
    else {
        panic!("expected inner loop");
    };
    let continue_expr = block_expr_or_single_statement(inner_body).expect("expected continue expression");
    assert!(matches!(
        continue_expr.kind,
        HirExprKind::Continue { label: Some(ref label) } if label.as_str() == "outer"
    ));
}

#[test]
fn rejects_continue_with_unknown_label() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 100 });
    let err = compiler
        .compile_source(
            r#"micro main() {
    while true {
        continue 'missing
    }
}"#,
        )
        .unwrap_err();
    assert!(err.to_string().contains("missing"));
    assert!(err.to_string().contains("label"));
}

#[test]
fn rejects_resume_outside_catch_arm() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 101 });
    let err = compiler
        .compile_source(
            r#"micro main() {
    resume value
}"#,
        )
        .unwrap_err();
    assert!(err.to_string().contains("resume"));
    assert!(err.to_string().contains("catch"));
}

#[test]
fn rejects_break_expr_in_statement_loop() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 110 });
    let err = compiler
        .compile_source(
            r#"micro main() {
    loop {
        break 1
    }
}"#,
        )
        .unwrap_err();
    assert!(err.to_string().contains("break expr"));
    assert!(err.to_string().contains("loop"));
}

#[test]
fn suspend_runtime_payload_includes_continuation_artifacts() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 200 });
    let hir = compiler
        .compile_source(
            r#"micro main() {
    catch raise true {
        else:
            resume true
    }
}"#,
        )
        .expect("hir");
    use nyar_language::build_first_class_suspend_payload;
    use nyar_language::valkyrie::frontend_contract::NyarPlanningContract;
    let symbol = hir.program_facts().functions[0].symbol.clone();
    let payload = build_first_class_suspend_payload(&hir, &[symbol]);
    let artifact = &payload.functions[0];
    assert_eq!(artifact.continuations.len(), 1);
    assert_eq!(artifact.continuations[0].resume_block_label, "catch_resume");
}
