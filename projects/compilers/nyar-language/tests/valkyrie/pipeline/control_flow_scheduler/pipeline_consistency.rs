use super::*;

#[test]
fn rejects_mir_yield_resume_block_without_parameter() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 201 });
    let mut mir = semantic_mir(
        &compiler,
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
            MirTerminator::PerformEffect { effect, resume_target, .. } if *effect == MirEffectKind::Yield => Some(*resume_target),
            _ => None,
        })
        .expect("expected yield perform effect");
    let resume_block = mir.functions[0].blocks.iter_mut().find(|block| block.id == resume_target).expect("expected yield resume block");
    resume_block.parameters.clear();

    let error = ControlFlowScheduler::validate_mir_module(&mir).unwrap_err();
    assert!(error.to_string().contains("`MIR`"));
    assert!(error.to_string().contains("effect"));
}

#[test]
fn rejects_lir_awake_resume_block_with_unexpected_parameter() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 202 });
    let mut lir = semantic_lir(
        &compiler,
        r#"micro main() {
    future.awake
    return
}
"#,
    )
    .unwrap();
    let resume_target = lir.functions[0]
        .blocks
        .iter()
        .find_map(|block| match &block.terminator {
            LirTerminator::PerformEffect { effect, resume_target, .. } if matches!(effect, LirEffectKind::AsyncSpawn) => Some(*resume_target),
            _ => None,
        })
        .expect("expected awake perform effect");
    let resume_block = lir.functions[0].blocks.iter_mut().find(|block| block.id == resume_target).expect("expected awake resume block");
    resume_block.parameters.push(MirValueRef(999));

    let error = validate_legacy_lir(&lir).unwrap_err();
    assert!(error.to_string().contains("`LIR`"));
    assert!(error.to_string().contains("effect"));
}

#[test]
fn rejects_pipeline_when_effect_resume_parameter_shape_drifts() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 203 });
    let source = r#"micro main() {
    future.await
    return
}
"#;
    let hir = compiler.compile_source(source).unwrap();
    let mir = MirLowerer::lower_module_semantic(&hir);
    let mut lir = semantic_lir(&compiler, source).unwrap();
    let resume_target = mir.functions[0]
        .blocks
        .iter()
        .find_map(|block| match &block.terminator {
            MirTerminator::PerformEffect { effect, resume_target, .. } if *effect == MirEffectKind::Await => Some(*resume_target),
            _ => None,
        })
        .expect("expected await perform effect");
    let resume_block = lir.functions[0].blocks.iter_mut().find(|block| block.id == resume_target).expect("expected await resume block");
    resume_block.parameters.clear();

    let error = validate_legacy_pipeline(&hir, &mir, &lir).unwrap_err();
    assert!(error.to_string().contains("effect"));
}

#[test]
fn rejects_mir_yield_resume_block_with_non_unit_parameter_type() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 203 });
    let mut mir = semantic_mir(
        &compiler,
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
            MirTerminator::PerformEffect { effect: MirEffectKind::Yield, resume_target, .. } => Some(*resume_target),
            _ => None,
        })
        .expect("expected yield perform effect");
    let resume_parameter = *mir.functions[0]
        .blocks
        .iter()
        .find(|block| block.id == resume_target)
        .and_then(|block| block.parameters.first())
        .expect("expected yield resume parameter");
    mir.functions[0].value_types.insert(resume_parameter, ValkyrieType::Boolean);

    let error = ControlFlowScheduler::validate_mir_module(&mir).unwrap_err();
    assert!(error.to_string().contains("`MIR`"));
    assert!(error.to_string().contains("unit"));
}

#[test]
fn rejects_mir_await_perform_effect_without_payload() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 204 });
    let mut mir = semantic_mir(
        &compiler,
        r#"micro main() {
    future.await
    return
}
"#,
    )
    .unwrap();
    let effect_block = mir.functions[0]
        .blocks
        .iter_mut()
        .find(|block| matches!(block.terminator, MirTerminator::PerformEffect { effect, .. } if effect == MirEffectKind::Await))
        .expect("expected await perform effect");
    if let MirTerminator::PerformEffect { payload, .. } = &mut effect_block.terminator {
        *payload = None;
    }

    let error = ControlFlowScheduler::validate_mir_module(&mir).unwrap_err();
    assert!(error.to_string().contains("`MIR`"));
    assert!(error.to_string().contains("effect payload"));
}

#[test]
fn rejects_lir_block_perform_effect_without_payload() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 205 });
    let mut lir = semantic_lir(
        &compiler,
        r#"micro main() {
    future.block
    return
}
"#,
    )
    .unwrap();
    let effect_block = lir.functions[0]
        .blocks
        .iter_mut()
        .find(|block| matches!(block.terminator, LirTerminator::PerformEffect { effect, .. } if matches!(effect, LirEffectKind::AsyncBlock)))
        .expect("expected block perform effect");
    if let LirTerminator::PerformEffect { payload, .. } = &mut effect_block.terminator {
        *payload = None;
    }

    let error = validate_legacy_lir(&lir).unwrap_err();
    assert!(error.to_string().contains("`LIR`"));
    assert!(error.to_string().contains("effect payload"));
}

#[test]
fn rejects_lir_block_resume_block_with_wrong_parameter_type() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 211 });
    let mut lir = semantic_lir(
        &compiler,
        r#"micro main() {
    yield 1
    return
}
"#,
    )
    .unwrap();
    let resume_target = lir.functions[0]
        .blocks
        .iter()
        .find_map(|block| match &block.terminator {
            LirTerminator::PerformEffect { effect: LirEffectKind::Yield, resume_target, .. } => Some(*resume_target),
            _ => None,
        })
        .expect("expected yield perform effect");
    let resume_parameter = *lir.functions[0]
        .blocks
        .iter()
        .find(|block| block.id == resume_target)
        .and_then(|block| block.parameters.first())
        .expect("expected yield resume parameter");
    lir.functions[0].value_types.insert(resume_parameter, ValkyrieType::Boolean);

    let error = validate_legacy_lir(&lir).unwrap_err();
    assert!(error.to_string().contains("`LIR`"));
    assert!(error.to_string().contains("bool"));
    assert!(error.to_string().contains("unit"));
}

#[test]
fn rejects_pipeline_when_effect_payload_shape_drifts() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 206 });
    let source = r#"micro main() {
    future.await
    return
}
"#;
    let hir = compiler.compile_source(source).unwrap();
    let mir = MirLowerer::lower_module_semantic(&hir);
    let mut lir = semantic_lir(&compiler, source).unwrap();
    let effect_block = lir.functions[0]
        .blocks
        .iter_mut()
        .find(|block| matches!(block.terminator, LirTerminator::PerformEffect { effect, .. } if matches!(effect, LirEffectKind::Await)))
        .expect("expected await perform effect");
    if let LirTerminator::PerformEffect { payload, .. } = &mut effect_block.terminator {
        *payload = None;
    }

    let error = validate_legacy_pipeline(&hir, &mir, &lir).unwrap_err();
    assert!(error.to_string().contains("effect payload"));
}

#[test]
fn rejects_mir_await_perform_effect_with_non_future_payload_type() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 208 });
    let mut mir = semantic_mir(
        &compiler,
        r#"micro main() {
    future.await
    return
}
"#,
    )
    .unwrap();
    let effect_block = mir.functions[0]
        .blocks
        .iter_mut()
        .find(|block| matches!(block.terminator, MirTerminator::PerformEffect { effect, .. } if effect == MirEffectKind::Await))
        .expect("expected await perform effect");
    if let MirTerminator::PerformEffect { payload, .. } = &mut effect_block.terminator {
        *payload = Some(MirOperand::Constant(MirConstant::Bool(true)));
    }

    let error = ControlFlowScheduler::validate_mir_module(&mir).unwrap_err();
    assert!(error.to_string().contains("`MIR`"));
    assert!(error.to_string().contains("`await`"));
    assert!(error.to_string().contains("Future<T>` / `Promise<T>`"));
}

#[test]
fn rejects_lir_awake_perform_effect_with_non_future_payload_type() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 209 });
    let mut lir = semantic_lir(
        &compiler,
        r#"micro main() {
    future.awake
    return
}
"#,
    )
    .unwrap();
    let effect_block = lir.functions[0]
        .blocks
        .iter_mut()
        .find(|block| matches!(block.terminator, LirTerminator::PerformEffect { effect, .. } if matches!(effect, LirEffectKind::AsyncSpawn)))
        .expect("expected awake perform effect");
    if let LirTerminator::PerformEffect { payload, .. } = &mut effect_block.terminator {
        *payload = Some(LirOperand::Constant(MirConstant::Bool(true)));
    }

    let error = validate_legacy_lir(&lir).unwrap_err();
    assert!(error.to_string().contains("`LIR`"));
    assert!(error.to_string().contains("`awake`"));
    assert!(error.to_string().contains("Future<T>` / `Promise<T>`"));
}

#[test]
fn rejects_pipeline_when_effect_payload_static_type_drifts() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 210 });
    let source = r#"micro main() {
    future.await
    return
}
"#;
    let hir = compiler.compile_source(source).unwrap();
    let mir = MirLowerer::lower_module_semantic(&hir);
    let mut lir = semantic_lir(&compiler, source).unwrap();
    let effect_block = lir.functions[0]
        .blocks
        .iter_mut()
        .find(|block| matches!(block.terminator, LirTerminator::PerformEffect { effect, .. } if matches!(effect, LirEffectKind::Await)))
        .expect("expected await perform effect");
    if let LirTerminator::PerformEffect { payload, .. } = &mut effect_block.terminator {
        *payload = Some(LirOperand::Constant(MirConstant::Bool(true)));
    }

    let error = validate_legacy_pipeline(&hir, &mir, &lir).unwrap_err();
    assert!(error.to_string().contains("`LIR`"));
    assert!(error.to_string().contains("`await`"));
    assert!(error.to_string().contains("Future<T>` / `Promise<T>`"));
}

#[test]
fn rejects_pipeline_when_effect_resume_static_type_drifts() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 212 });
    let source = r#"micro main() {
    yield 1
    return
}
"#;
    let hir = compiler.compile_source(source).unwrap();
    let mir = MirLowerer::lower_module_semantic(&hir);
    let mut lir = semantic_lir(&compiler, source).unwrap();
    let resume_target = mir.functions[0]
        .blocks
        .iter()
        .find_map(|block| match &block.terminator {
            MirTerminator::PerformEffect { effect: MirEffectKind::Yield, resume_target, .. } => Some(*resume_target),
            _ => None,
        })
        .expect("expected yield perform effect");
    let resume_parameter = *lir.functions[0]
        .blocks
        .iter()
        .find(|block| block.id == resume_target)
        .and_then(|block| block.parameters.first())
        .expect("expected yield resume parameter");
    lir.functions[0].value_types.insert(resume_parameter, ValkyrieType::Boolean);

    let error = validate_legacy_pipeline(&hir, &mir, &lir).unwrap_err();
    assert!(error.to_string().contains("`LIR`"));
    assert!(error.to_string().contains("unit"));
}

#[test]
fn rejects_pipeline_when_catch_resume_parameter_type_drifts() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 213 });
    let source = r#"micro main() {
    catch raise true {
        else:
            resume true
    }
    return
}
"#;
    let hir = compiler.compile_source(source).unwrap();
    let mir = MirLowerer::lower_module_semantic(&hir);
    let mut lir = semantic_lir(&compiler, source).unwrap();
    let resume_parameter = *lir.functions[0]
        .blocks
        .iter()
        .find(|block| block.label == "catch_resume")
        .and_then(|block| block.parameters.first())
        .expect("expected catch resume parameter");
    lir.functions[0].value_types.insert(resume_parameter, ValkyrieType::Unit);

    let error = validate_legacy_pipeline(&hir, &mir, &lir).unwrap_err();
    assert!(error.to_string().contains("catch_resume"));
    assert!(error.to_string().contains("bool"));
    assert!(error.to_string().contains("unit"));
}

#[test]
fn rejects_mir_jump_argument_type_drift_to_catch_resume_parameter() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 214 });
    let mut mir = semantic_mir(
        &compiler,
        r#"micro main() {
    let flag: bool = true
    catch raise true {
        else:
            resume flag
    }
    return
}
"#,
    )
    .unwrap();
    let resume_target =
        mir.functions[0].blocks.iter().find(|block| block.label == "catch_resume").map(|block| block.id).expect("expected catch resume block");
    let jump_argument = mir.functions[0]
        .blocks
        .iter()
        .find_map(|block| match &block.terminator {
            MirTerminator::Jump { target, arguments } if *target == resume_target => match arguments.first() {
                Some(MirOperand::Value(value)) => Some(*value),
                _ => None,
            },
            _ => None,
        })
        .expect("expected jump into catch resume block");
    mir.functions[0].value_types.insert(jump_argument, ValkyrieType::Unit);

    let error = ControlFlowScheduler::validate_mir_module(&mir).unwrap_err();
    assert!(error.to_string().contains("Jump"));
    assert!(error.to_string().contains("catch_resume"));
    assert!(error.to_string().contains("unit"));
    assert!(error.to_string().contains("bool"));
}

#[test]
fn rejects_lir_jump_argument_type_drift_to_catch_resume_parameter() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 215 });
    let mut lir = semantic_lir(
        &compiler,
        r#"micro main() {
    let flag: bool = true
    catch raise true {
        else:
            resume flag
    }
    return
}
"#,
    )
    .unwrap();
    let resume_target =
        lir.functions[0].blocks.iter().find(|block| block.label == "catch_resume").map(|block| block.id).expect("expected catch resume block");
    let jump_argument = lir.functions[0]
        .blocks
        .iter()
        .find_map(|block| match &block.terminator {
            LirTerminator::Jump { target, arguments } if *target == resume_target => match arguments.first() {
                Some(LirOperand::Value(value)) => Some(*value),
                _ => None,
            },
            _ => None,
        })
        .expect("expected jump into catch resume block");
    lir.functions[0].value_types.insert(jump_argument, ValkyrieType::Unit);

    let error = validate_legacy_lir(&lir).unwrap_err();
    assert!(error.to_string().contains("Jump"));
    assert!(error.to_string().contains("catch_resume"));
    assert!(error.to_string().contains("unit"));
    assert!(error.to_string().contains("bool"));
}

#[test]
fn rejects_pipeline_when_jump_argument_type_drifts() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 216 });
    let source = r#"micro main() {
    let flag: bool = true
    catch raise true {
        else:
            resume flag
    }
    return
}
"#;
    let hir = compiler.compile_source(source).unwrap();
    let mir = MirLowerer::lower_module_semantic(&hir);
    let mut lir = semantic_lir(&compiler, source).unwrap();
    let resume_target =
        lir.functions[0].blocks.iter().find(|block| block.label == "catch_resume").map(|block| block.id).expect("expected catch resume block");
    let jump_argument = lir.functions[0]
        .blocks
        .iter()
        .find_map(|block| match &block.terminator {
            LirTerminator::Jump { target, arguments } if *target == resume_target => match arguments.first() {
                Some(LirOperand::Value(value)) => Some(*value),
                _ => None,
            },
            _ => None,
        })
        .expect("expected jump into catch resume block");
    lir.functions[0].value_types.insert(jump_argument, ValkyrieType::Unit);

    let error = validate_legacy_pipeline(&hir, &mir, &lir).unwrap_err();
    assert!(error.to_string().contains("Jump"));
    assert!(error.to_string().contains("catch_arm_0"));
    assert!(error.to_string().contains("bool"));
    assert!(error.to_string().contains("unit"));
}

#[test]
fn rejects_mir_continuation_when_resume_parameter_leaves_target_block() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 217 });
    let mut mir = semantic_mir(
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
    mir.functions[0].continuations[0].resume_parameter = MirValueRef(999);

    let error = ControlFlowScheduler::validate_mir_module(&mir).unwrap_err();
    assert!(error.to_string().contains("continuation"));
    assert!(error.to_string().contains("block"));
}

#[test]
fn rejects_pipeline_when_continuation_resume_type_drifts() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 218 });
    let source = r#"micro main() {
    catch raise true {
        else:
            resume true
    }
    return
}
"#;
    let hir = compiler.compile_source(source).unwrap();
    let mir = MirLowerer::lower_module_semantic(&hir);
    let mut lir = semantic_lir(&compiler, source).unwrap();
    lir.functions[0].continuations[0].resume_parameter_type = Some(ValkyrieType::Unit);

    let error = validate_legacy_pipeline(&hir, &mir, &lir).unwrap_err();
    assert!(error.to_string().contains("continuation"));
    assert!(error.to_string().contains("`MIR / LIR`"));
    assert!(error.to_string().contains("bool"));
    assert!(error.to_string().contains("unit"));
}

#[test]
fn rejects_mir_suspend_point_when_resume_parameter_count_drifts() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 219 });
    let mut mir = semantic_mir(
        &compiler,
        r#"micro main() {
    let future: Future<bool> = ()
    future.await
    return
}
"#,
    )
    .unwrap();
    mir.functions[0].suspend_points[0].resume_parameter_count = 0;

    let error = ControlFlowScheduler::validate_mir_module(&mir).unwrap_err();
    assert!(error.to_string().contains("suspend"));
    assert!(error.to_string().contains("0"));
}

#[test]
fn rejects_pipeline_when_suspend_point_payload_type_drifts() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 220 });
    let source = r#"micro main() {
    let future: Future<bool> = ()
    future.await
    return
}
"#;
    let hir = compiler.compile_source(source).unwrap();
    let mir = MirLowerer::lower_module_semantic(&hir);
    let mut lir = semantic_lir(&compiler, source).unwrap();
    lir.functions[0].suspend_points[0].payload_type = Some(ValkyrieType::Unit);

    let error = validate_legacy_pipeline(&hir, &mir, &lir).unwrap_err();
    assert!(error.to_string().contains("suspend"));
    assert!(error.to_string().contains("`MIR / LIR`"));
}

#[test]
fn rejects_pipeline_when_suspend_point_spill_candidates_drift() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 221 });
    let source = r#"micro main() {
    let future: Future<bool> = ()
    let kept: bool = true
    future.await
    let sink: bool = kept
    return
}
"#;
    let hir = compiler.compile_source(source).unwrap();
    let mir = MirLowerer::lower_module_semantic(&hir);
    let mut lir = semantic_lir(&compiler, source).unwrap();
    lir.functions[0].suspend_points[0].spill_candidates.push(MirValueRef(999));

    let error = validate_legacy_pipeline(&hir, &mir, &lir).unwrap_err();
    assert!(error.to_string().contains("suspend"));
    assert!(error.to_string().contains("`MIR / LIR`"));
}

#[test]
fn rejects_pipeline_when_frame_layout_slots_drift() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 222 });
    let source = r#"micro main() {
    let future: Future<bool> = ()
    let kept: bool = true
    future.await
    let sink: bool = kept
    return
}
"#;
    let hir = compiler.compile_source(source).unwrap();
    let mir = MirLowerer::lower_module_semantic(&hir);
    let mut lir = semantic_lir(&compiler, source).unwrap();
    lir.functions[0].frame_layouts[0].slots.push(nyar_language::lir::LirFrameSlot {
        slot_index: 0,
        value: MirValueRef(999),
        value_type: Some(ValkyrieType::Boolean),
    });

    let error = validate_legacy_pipeline(&hir, &mir, &lir).unwrap_err();
    assert!(error.to_string().contains("frame layout"));
}

#[test]
fn rejects_lir_runtime_frame_when_slots_drift_from_frame_layout() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 223 });
    let mut lir = semantic_lir(
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
    lir.functions[0].runtime_frames[0].slots.push(nyar_language::lir::LirRuntimeSlot {
        field_name: "slot_0".to_string(),
        slot_index: 0,
        value: MirValueRef(999),
        value_type: Some(ValkyrieType::Boolean),
    });

    let error = validate_legacy_lir(&lir).unwrap_err();
    assert!(error.to_string().contains("runtime frame"));
}

#[test]
fn rejects_lir_runtime_continuation_when_resume_binding_drifts() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 224 });
    let mut lir = semantic_lir(
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
    lir.functions[0].runtime_continuations[0].resume_parameter = MirValueRef(999);

    let error = validate_legacy_lir(&lir).unwrap_err();
    assert!(error.to_string().contains("runtime continuation"));
}

#[test]
fn preserves_case_chain_metadata_across_pipeline() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 225 });
    let source = r#"micro main() {
    case value {
        case 0:
            fallthrough
        case 1 if value > 0:
            return
        else:
            return
    };
    return
}
"#;

    let mir = semantic_mir(&compiler, source).unwrap();
    let lir = semantic_lir(&compiler, source).unwrap();

    let mir_chain = &mir.functions[0].case_chains[0];
    assert!(!mir_chain.produce_value);
    assert_eq!(mir_chain.arms.len(), 3);
    assert_eq!(mir_chain.first_arm, mir_chain.arms[0].entry_block);
    assert_eq!(mir_chain.arms[0].fallthrough_target, Some(mir_chain.arms[1].entry_block));
    assert!(mir_chain.arms[1].guard_block.is_some());
    assert_eq!(mir_chain.arms[2].fallthrough_target, None);

    let lir_chain = &lir.functions[0].case_chains[0];
    assert_eq!(lir_chain.dispatch_block, mir_chain.dispatch_block);
    assert_eq!(lir_chain.first_arm, mir_chain.first_arm);
    assert_eq!(lir_chain.no_match_block, mir_chain.no_match_block);
    assert_eq!(lir_chain.exit_block, mir_chain.exit_block);
    assert_eq!(lir_chain.produce_value, mir_chain.produce_value);
    assert_eq!(lir_chain.arms.len(), mir_chain.arms.len());
    assert_eq!(lir_chain.arms[0].fallthrough_target, mir_chain.arms[0].fallthrough_target);
    assert_eq!(lir_chain.arms[1].guard_block, mir_chain.arms[1].guard_block);
    assert_eq!(lir_chain.arms[2].fallthrough_target, mir_chain.arms[2].fallthrough_target);
}

#[test]
fn preserves_nested_loop_control_flow_across_pipeline() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 226 });
    let source = r#"micro nested(limit: i64): i64 {
    let outer: i64 = 0
    let total: i64 = 0
    while outer < limit {
        let inner: i64 = 0
        while inner < 5 {
            inner = inner + 1
            if inner == 2 {
                continue
            }
            if outer == 2 {
                if inner == 4 {
                    break
                }
            }
            total = total + outer + inner
        }
        outer = outer + 1
    }
    return total
}

[main]
micro main(): i64 {
    if nested(4) != 63 {
        return 1
    }
    return 0
}
"#;
    let hir = compiler.compile_source(source).unwrap();
    let mir = semantic_mir(&compiler, source).unwrap();
    let lir = semantic_lir(&compiler, source).unwrap();

    validate_legacy_pipeline(&hir, &mir, &lir).expect("nested loop control flow should remain pipeline-consistent");
    assert!(mir.functions.iter().any(|function| {
        function.blocks.iter().any(|block| matches!(&block.terminator, MirTerminator::Jump { .. } | MirTerminator::Branch { .. }))
    }));
}

#[test]
fn preserves_nested_early_return_across_pipeline() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 227 });
    let source = r#"micro find_target(limit: i64): i64 {
    let outer: i64 = 0
    while outer < limit {
        let inner: i64 = 0
        while inner < 4 {
            if outer == 1 {
                if inner == 2 {
                    return outer * 10 + inner
                }
            }
            inner = inner + 1
        }
        outer = outer + 1
    }
    return 0
}

[main]
micro main(): i64 {
    if find_target(3) != 12 {
        return 1
    }
    return 0
}
"#;
    let hir = compiler.compile_source(source).unwrap();
    let mir = semantic_mir(&compiler, source).unwrap();
    let lir = semantic_lir(&compiler, source).unwrap();

    validate_legacy_pipeline(&hir, &mir, &lir).expect("nested early return should remain pipeline-consistent");
    assert!(
        mir.functions
            .iter()
            .any(|function| { function.blocks.iter().any(|block| matches!(&block.terminator, MirTerminator::Return { value: _ })) })
    );
}

#[test]
fn preserves_mixed_complex_control_flow_across_pipeline() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 228 });
    let source = r#"micro search(limit: i64): i64 {
    let outer: i64 = 0
    loop {
        if outer >= limit {
            break
        }

        let inner: i64 = 0
        while inner < 6 {
            inner = inner + 1
            if inner == 2 {
                continue
            }
            if outer == 1 {
                if inner == 5 {
                    break
                }
            }
            if outer == 2 {
                if inner == 4 {
                    return outer * 10 + inner
                }
            }
        }

        outer = outer + 1
    }

    return -1
}

[main]
micro main(): i64 {
    if search(5) != 24 {
        return 1
    }
    if search(2) != -1 {
        return 2
    }
    return 0
}
"#;
    let hir = compiler.compile_source(source).unwrap();
    let mir = semantic_mir(&compiler, source).unwrap();
    let lir = semantic_lir(&compiler, source).unwrap();

    validate_legacy_pipeline(&hir, &mir, &lir).expect("mixed complex control flow should remain pipeline-consistent");
    assert!(mir.functions.iter().any(|function| function.blocks.iter().any(|block| {
        matches!(&block.terminator, MirTerminator::Jump { .. } | MirTerminator::Branch { .. } | MirTerminator::Return { value: _ })
    })));
}

#[test]
fn preserves_loop_scope_control_flow_across_pipeline() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 229 });
    let source = r#"micro scoped(limit: i64): i64 {
    let outer: i64 = 0
    let total: i64 = 0
    while outer < limit {
        let inner: i64 = 0
        while inner < 5 {
            inner = inner + 1
            if inner == 1 {
                continue
            }
            if inner == 4 {
                break
            }
            total = total + outer * 10 + inner
        }
        total = total + 100 + outer
        outer = outer + 1
    }
    return total
}

[main]
micro main(): i64 {
    if scoped(3) != 378 {
        return 1
    }
    return 0
}
"#;
    let hir = compiler.compile_source(source).unwrap();
    let mir = semantic_mir(&compiler, source).unwrap();
    let lir = semantic_lir(&compiler, source).unwrap();

    validate_legacy_pipeline(&hir, &mir, &lir).expect("loop scope control flow should remain pipeline-consistent");
    assert!(mir.functions.iter().any(|function| function.blocks.len() > 1));
}

#[test]
fn preserves_array_and_control_flow_combo_across_pipeline() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 230 });
    let source = r#"micro classify(value: i64): i64 {
    if value == 0 {
        return 5
    }
    if value == 2 {
        return 20
    }
    return value + 1
}

micro accumulate(limit: i64): i64 {
    let mut values: [i64] = [0, 0, 0, 0, 0]
    let index: i64 = 0
    loop {
        if index >= limit {
            break
        }
        if index == 1 {
            values[index] = 50
            index = index + 1
            continue
        }
        if index == 4 {
            break
        }
        values[index] = classify(index)
        index = index + 1
    }

    let cursor: i64 = 0
    let total: i64 = 0
    while cursor < 5 {
        if cursor == 3 {
            cursor = cursor + 1
            continue
        }
        total = total + values[cursor]
        cursor = cursor + 1
    }
    return total
}

[main]
micro main(): i64 {
    if accumulate(5) != 75 {
        return 1
    }
    if accumulate(2) != 55 {
        return 2
    }
    return 0
}
"#;
    let hir = compiler.compile_source(source).unwrap();
    let mir = semantic_mir(&compiler, source).unwrap();
    let lir = semantic_lir(&compiler, source).unwrap();

    validate_legacy_pipeline(&hir, &mir, &lir).expect("array and control flow combo should remain pipeline-consistent");
    assert!(mir.functions.iter().any(|function| function.blocks.iter().any(|block| {
        matches!(&block.terminator, MirTerminator::Jump { .. } | MirTerminator::Branch { .. } | MirTerminator::Return { value: _ })
    })));
}
