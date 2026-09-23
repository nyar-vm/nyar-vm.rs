use super::*;

#[test]
fn lowers_let_binding_and_final_expression() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 9 });
    let module = compiler
        .compile_source(
            r#"micro main() -> i64 {
    let value: i64 = 42;
    value
}
"#,
        )
        .unwrap();
    assert_eq!(module.functions[0].body.statements.len(), 1);
    assert!(matches!(module.functions[0].body.statements[0].kind, HirStatementKind::Let { .. }));
    assert!(matches!(module.functions[0].body.expr.as_ref().map(|expr| &expr.kind), Some(HirExprKind::Variable(_))));
}

#[test]
fn lowers_tuple_pattern_let_into_hir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 71 });
    let module = compiler
        .compile_source(
            r#"micro main() -> i64 {
    let (x, y) = (1, 2);
    return x + y;
}
"#,
        )
        .unwrap();

    let HirStatementKind::Let { pattern, initializer, .. } = &module.functions[0].body.statements[0].kind
    else {
        panic!("expected let statement");
    };

    assert!(matches!(
        pattern,
        HirPattern::Tuple(items)
            if items.len() == 2
                && matches!(&items[0], HirPattern::Variable(identifier) if identifier.name.as_str() == "x")
                && matches!(&items[1], HirPattern::Variable(identifier) if identifier.name.as_str() == "y")
    ));
    assert!(matches!(
        initializer.as_deref().map(|expr| &expr.kind),
        Some(HirExprKind::Call { callee, args, .. })
            if matches!(callee.kind, HirExprKind::Path(ref path) if path.to_string() == "tuple") && args.len() == 2
    ));
}

#[test]
fn lowers_loop_in_tuple_pattern_into_hir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 73 });
    let module = compiler
        .compile_source(
            r#"micro main() -> i64 {
    loop (x, y) in [(1, 2)] {
        return x + y;
    }
    return 0;
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(expression) = &module.functions[0].body.statements[0].kind
    else {
        panic!("expected loop expression statement");
    };

    assert!(matches!(
        expression.kind,
        HirExprKind::Loop { ref pattern, ref iterator, ref condition, .. }
            if condition.is_none()
                && iterator.is_some()
                && matches!(pattern, Some(HirPattern::Tuple(items))
                    if items.len() == 2
                        && matches!(&items[0], HirPattern::Variable(identifier) if identifier.name.as_str() == "x")
                        && matches!(&items[1], HirPattern::Variable(identifier) if identifier.name.as_str() == "y"))
    ));
}

#[test]
fn lowers_only_explicit_builtin_type_names() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 81 });
    let module = compiler
        .compile_source(
            r#"micro main(a: utf8, b: utf16) -> void {
    let value: int32 = 0;
    return;
}
"#,
        )
        .unwrap();
    let function = &module.functions[0];
    assert_eq!(function.params[0].ty, ValkyrieType::Utf8);
    assert_eq!(function.params[1].ty, ValkyrieType::Utf16);
    assert_eq!(function.return_type, ValkyrieType::Void);

    let HirStatementKind::Let { ty: Some(local_ty), .. } = &function.body.statements[0].kind
    else {
        panic!("expected typed let statement");
    };
    assert_eq!(local_ty, &ValkyrieType::Named(nyar_language::types::Identifier::new("int32")));
}

#[test]
fn lowers_tuple_pattern_bindings_into_mir_values() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 75 });
    let mir = compiler
        .compile_source_to_mir(
            r#"micro main() -> i64 {
    let (x, y) = (1, 2);
    return x + y;
}
"#,
        )
        .unwrap();

    assert!(
        mir.functions[0].values.iter().any(|value| matches!(&value.origin, nyar_language::MirValueOrigin::LetBinding { name } if name == "x"))
    );
    assert!(
        mir.functions[0].values.iter().any(|value| matches!(&value.origin, nyar_language::MirValueOrigin::LetBinding { name } if name == "y"))
    );
}

#[test]
fn lowers_non_literal_nested_tuple_pattern_bindings_into_mir_values() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 76 });
    let mir = compiler
        .compile_source_to_mir(
            r#"micro main() -> i64 {
    let pair = ((1, 2), 3);
    let ((x, _), y) = pair;
    return x + y;
}
"#,
        )
        .unwrap();

    assert!(
        mir.functions[0].values.iter().any(|value| matches!(&value.origin, nyar_language::MirValueOrigin::LetBinding { name } if name == "x"))
    );
    assert!(
        mir.functions[0].values.iter().any(|value| matches!(&value.origin, nyar_language::MirValueOrigin::LetBinding { name } if name == "y"))
    );
    assert!(
        !mir.functions[0].values.iter().any(|value| matches!(&value.origin, nyar_language::MirValueOrigin::LetBinding { name } if name == "_"))
    );
    assert!(!mir.functions[0]
        .blocks[0]
        .instructions
        .iter()
        .any(|instruction| matches!(&instruction.kind, MirInstructionKind::Call { callee: nyar_language::MirOperand::Symbol(path), .. } if path.to_string().starts_with("tuple_get_"))));
}

#[test]
fn statically_unrolls_loop_in_tuple_pattern_for_literal_iterables() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 77 });
    let mir = compiler
        .compile_source_to_mir(
            r#"micro main() -> i64 {
    loop (x, y) in [(1, 2)] {
        return x + y;
    }
    return 0;
}
"#,
        )
        .unwrap();

    assert!(
        mir.functions[0].values.iter().any(|value| matches!(&value.origin, nyar_language::MirValueOrigin::LetBinding { name } if name == "x"))
    );
    assert!(
        mir.functions[0].values.iter().any(|value| matches!(&value.origin, nyar_language::MirValueOrigin::LetBinding { name } if name == "y"))
    );
    assert!(mir.functions[0].blocks[0].instructions.iter().any(|instruction| {
        matches!(
            &instruction.kind,
            MirInstructionKind::Call {                callee: nyar_language::MirOperand::Symbol(path),
                ..,
} if path.to_string() == "infix +"
        )
    }));
}

#[test]
fn statically_unrolls_loop_in_tuple_pattern_for_named_literal_iterables() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 78 });
    let mir = compiler
        .compile_source_to_mir(
            r#"micro main() -> i64 {
    let pairs = [((1, 2), 3)];
    loop ((x, _), y) in pairs {
        return x + y;
    }
    return 0;
}
"#,
        )
        .unwrap();

    assert!(
        mir.functions[0].values.iter().any(|value| matches!(&value.origin, nyar_language::MirValueOrigin::LetBinding { name } if name == "x"))
    );
    assert!(
        mir.functions[0].values.iter().any(|value| matches!(&value.origin, nyar_language::MirValueOrigin::LetBinding { name } if name == "y"))
    );
    assert!(
        !mir.functions[0].values.iter().any(|value| matches!(&value.origin, nyar_language::MirValueOrigin::LetBinding { name } if name == "_"))
    );
}

#[test]
fn lowers_break_expr_into_loop_exit_block_parameter_in_mir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 84 });
    let mir = compiler
        .compile_source_to_mir(
            r#"micro main() -> i64 {
    let value: i64 = loop {
        break 7
    }
    return value
}
"#,
        )
        .unwrap();

    let exit_block = mir.functions[0].blocks.iter().find(|block| !block.parameters.is_empty()).expect("expected loop exit block parameter");
    assert_eq!(exit_block.parameters.len(), 1);
    assert!(mir.functions[0].blocks.iter().any(|block| {
        matches!(
            &block.terminator,
            nyar_language::MirTerminator::Jump { target, arguments }
                if *target == exit_block.id && arguments.len() == 1
        )
    }));
}

#[test]
fn preserves_break_expr_jump_arguments_into_lir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 85 });
    let lir = compile_source_to_legacy_lir(
        &compiler,
            r#"micro main() -> i64 {
    let value: i64 = loop {
        break 7
    }
    return value
}
"#,
    )
    .unwrap();

    let exit_block = lir.functions[0].blocks.iter().find(|block| !block.parameters.is_empty()).expect("expected loop exit block parameter");
    assert_eq!(exit_block.parameters.len(), 1);
    assert!(lir.functions[0].blocks.iter().any(|block| {
        matches!(
            &block.terminator,
            LirTerminator::Jump { target, arguments }
                if *target == exit_block.id && arguments.len() == 1
        )
    }));
}

#[test]
fn lowers_continue_into_loop_header_block_parameter_in_mir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 97 });
    let mir = compiler
        .compile_source_to_mir(
            r#"micro main() {
    let seed: i64 = 0
    while true {
        let seed: i64 = 1
        continue
    }
}
"#,
        )
        .unwrap();

    let header_block = mir.functions[0].blocks.iter().find(|block| block.label == "loop_header").expect("expected loop header");
    assert_eq!(header_block.parameters.len(), 1);
    assert!(mir.functions[0].blocks.iter().any(|block| {
        matches!(
            &block.terminator,
            nyar_language::MirTerminator::Jump { target, arguments }
                if *target == header_block.id && arguments.len() == 1
        )
    }));
}

#[test]
fn preserves_continue_jump_arguments_into_lir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 98 });
    let lir = compile_source_to_legacy_lir(
        &compiler,
            r#"micro main() {
    let seed: i64 = 0
    while true {
        let seed: i64 = 1
        continue
    }
}
"#,
    )
    .unwrap();

    let header_block = lir.functions[0].blocks.iter().find(|block| block.label == "loop_header").expect("expected loop header");
    assert_eq!(header_block.parameters.len(), 1);
    assert!(lir.functions[0].blocks.iter().any(|block| {
        matches!(
            &block.terminator,
            LirTerminator::Jump { target, arguments }
                if *target == header_block.id && arguments.len() == 1
        )
    }));
}

#[test]
fn lowers_continue_label_into_outer_loop_header_in_mir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 102 });
    let mir = compiler
        .compile_source_to_mir(
            r#"micro main() {
    let seed: i64 = 0
    'outer: while true {
        let seed: i64 = 1
        while true {
            continue 'outer
        }
    }
}"#,
        )
        .unwrap();

    let outer_header = mir.functions[0]
        .blocks
        .iter()
        .find(|block| block.label == "loop_header" && block.parameters.len() == 1)
        .expect("expected outer loop header");
    assert!(mir.functions[0].blocks.iter().any(|block| {
        matches!(
            &block.terminator,
            nyar_language::MirTerminator::Jump { target, arguments }
                if *target == outer_header.id && arguments.len() == 1
        )
    }));
}

#[test]
fn lowers_break_label_expr_into_outer_loop_exit_in_mir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 103 });
    let mir = compiler
        .compile_source_to_mir(
            r#"micro main() -> i64 {
    let value: i64 = 'outer: loop {
        while true {
            break 'outer 7
        }
    }
    return value
}"#,
        )
        .unwrap();

    let outer_exit = mir.functions[0]
        .blocks
        .iter()
        .find(|block| block.label == "loop_exit" && block.parameters.len() == 1)
        .unwrap_or_else(|| panic!("expected outer loop exit, got blocks: {:?}", mir.functions[0].blocks));
    assert!(mir.functions[0].blocks.iter().any(|block| {
        matches!(
            &block.terminator,
            nyar_language::MirTerminator::Jump { target, arguments }
                if *target == outer_exit.id && arguments.len() == 1
        )
    }));
}

#[test]
fn lowers_yield_statement_into_mir_perform_effect() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 86 });
    let mir = compiler
        .compile_source_to_mir(
            r#"micro main() {
    yield 1
    return
}
"#,
        )
        .unwrap();

    let resume_target = mir.functions[0]
        .blocks
        .iter()
        .find_map(|block| match &block.terminator {
            nyar_language::MirTerminator::PerformEffect { effect, payload: Some(_), resume_target } if *effect == MirEffectKind::Yield => {
                Some(*resume_target)
            }
            _ => None,
        })
        .expect("expected yield perform effect");
    assert!(mir.functions[0].blocks.iter().any(|block| {
        matches!(
            &block.terminator,
            nyar_language::MirTerminator::PerformEffect { effect, payload: Some(_), .. }
                if *effect == MirEffectKind::Yield
        )
    }));
    let resume_block = mir.functions[0].blocks.iter().find(|block| block.id == resume_target).expect("expected yield resume block");
    assert_eq!(resume_block.parameters.len(), 1);
}

#[test]
fn lowers_yield_from_statement_into_lir_perform_effect() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 87 });
    let lir = compile_source_to_legacy_lir(
        &compiler,
            r#"micro main() {
    yield from values
    return
}
"#,
    )
    .unwrap();

    assert!(lir.functions[0].blocks.iter().any(|block| {
        matches!(
            &block.terminator,
            LirTerminator::PerformEffect { effect, payload: Some(_), .. }
                if *effect == LirEffectKind::DelegateYield
        )
    }));
}

#[test]
fn lowers_yield_from_statement_into_mir_resume_block_parameter() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 93 });
    let mir = compiler
        .compile_source_to_mir(
            r#"micro main() {
    yield from values
    return
}
"#,
        )
        .unwrap();
    let resume_target = mir.functions[0]
        .blocks
        .iter()
        .find_map(|block| match &block.terminator {
            nyar_language::MirTerminator::PerformEffect { effect, payload: Some(_), resume_target }
                if *effect == MirEffectKind::DelegateYield =>
            {
                Some(*resume_target)
            }
            _ => None,
        })
        .expect("expected yield from perform effect");
    let resume_block = mir.functions[0].blocks.iter().find(|block| block.id == resume_target).expect("expected yield from resume block");
    assert_eq!(resume_block.parameters.len(), 1);
}

#[test]
fn lowers_await_member_access_into_hir_control_flow() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 88 });
    let module = compiler
        .compile_source(
            r#"micro main() {
    future.await
}
"#,
        )
        .unwrap();
    let expression = module.functions[0].body.expr.as_ref().expect("expected final expression");
    assert!(matches!(
        expression.kind,
        HirExprKind::Await(ref value)
            if matches!(value.kind, HirExprKind::Variable(ref identifier) if identifier.name.as_str() == "future")
    ));
}

#[test]
fn lowers_await_member_access_into_mir_perform_effect() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 89 });
    let mir = compiler
        .compile_source_to_mir(
            r#"micro main() {
    future.await
    return
}
"#,
        )
        .unwrap();

    let resume_target = mir.functions[0]
        .blocks
        .iter()
        .find_map(|block| match &block.terminator {
            nyar_language::MirTerminator::PerformEffect { effect, payload: Some(_), resume_target } if *effect == MirEffectKind::Await => {
                Some(*resume_target)
            }
            _ => None,
        })
        .expect("expected await perform effect");
    assert!(mir.functions[0].blocks.iter().any(|block| {
        matches!(
            &block.terminator,
            nyar_language::MirTerminator::PerformEffect { effect, payload: Some(_), .. }
                if *effect == MirEffectKind::Await
        )
    }));
    let resume_block = mir.functions[0].blocks.iter().find(|block| block.id == resume_target).expect("expected await resume block");
    assert_eq!(resume_block.parameters.len(), 1);
    let resume_value = resume_block.parameters[0];
    assert!(mir.functions[0].values.iter().any(
        |value| matches!(value.origin, MirValueOrigin::BlockParameter { block, .. } if block == resume_block.id && value.id == resume_value)
    ));
}

#[test]
fn lowers_block_member_access_into_lir_perform_effect() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 90 });
    let lir = compile_source_to_legacy_lir(
        &compiler,
            r#"micro main() {
    future.block
    return
}
"#,
    )
    .unwrap();

    assert!(lir.functions[0].blocks.iter().any(|block| {
        matches!(
            &block.terminator,
            LirTerminator::PerformEffect { effect, payload: Some(_), .. }
                if *effect == LirEffectKind::AsyncBlock
        )
    }));
}

#[test]
fn lowers_awake_member_access_into_lir_perform_effect() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 91 });
    let lir = compile_source_to_legacy_lir(
        &compiler,
            r#"micro main() {
    future.awake
    return
}
"#,
    )
    .unwrap();

    assert!(lir.functions[0].blocks.iter().any(|block| {
        matches!(
            &block.terminator,
            LirTerminator::PerformEffect { effect, payload: Some(_), .. }
                if *effect == LirEffectKind::AsyncSpawn
        )
    }));
}

#[test]
fn lowers_frame_layouts_into_clr_runtime_frames() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 190 });
    let lir = compile_source_to_legacy_lir(
        &compiler,
            r#"micro main() {
    let future: Future<bool> = ()
    let kept: bool = true
    future.await
    let sink: bool = kept
    return
}
"#,
    )
    .unwrap();

    let runtime_frame = lir.functions[0].runtime_frames.first().expect("expected runtime frame");
    assert!(runtime_frame.carrier.contains("$clr_state_"));
    assert_eq!(runtime_frame.state_id, lir.functions[0].frame_layouts[0].state_id);
    assert_eq!(runtime_frame.resume_target, lir.functions[0].frame_layouts[0].resume_target);
    assert_eq!(runtime_frame.slots.len(), lir.functions[0].frame_layouts[0].slots.len());
    for (slot, layout_slot) in runtime_frame.slots.iter().zip(lir.functions[0].frame_layouts[0].slots.iter()) {
        assert_eq!(slot.field_name, format!("slot_{}", layout_slot.slot_index));
    }
}

#[test]
fn lowers_catch_resume_into_clr_runtime_continuation() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 191 });
    let lir = compile_source_to_legacy_lir(
        &compiler,
            r#"micro main() {
    catch raise true {
        else:
            resume true
    }
    return
}
"#,
    )
    .unwrap();

    let runtime_continuation = lir.functions[0].runtime_continuations.first().expect("expected runtime continuation");
    assert!(runtime_continuation.carrier.contains("$continuation_"));
    assert_eq!(runtime_continuation.resume_parameter_field, "resume_value");
    assert_eq!(runtime_continuation.resume_target, lir.functions[0].continuations[0].resume_target);
    assert_eq!(runtime_continuation.resume_parameter, lir.functions[0].continuations[0].resume_parameter);
    assert_eq!(runtime_continuation.resume_parameter_type, lir.functions[0].continuations[0].resume_parameter_type);
}

#[test]
fn lowers_block_member_access_into_mir_resume_block_parameter() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 92 });
    let mir = compiler
        .compile_source_to_mir(
            r#"micro main() {
    let value = future.block
    return
}
"#,
        )
        .unwrap();
    let resume_target = mir.functions[0]
        .blocks
        .iter()
        .find_map(|block| match &block.terminator {
            nyar_language::MirTerminator::PerformEffect { effect, payload: Some(_), resume_target } if *effect == MirEffectKind::AsyncBlock => {
                Some(*resume_target)
            }
            _ => None,
        })
        .expect("expected block perform effect");
    let resume_block = mir.functions[0].blocks.iter().find(|block| block.id == resume_target).expect("expected block resume block");
    assert_eq!(resume_block.parameters.len(), 1);
}
