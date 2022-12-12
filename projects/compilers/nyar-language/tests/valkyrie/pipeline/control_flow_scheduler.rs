use nyar_language::{
    ControlFlowScheduler, MirLowerer, ValkyrieCompiler,
    lir::{LirEffectKind, LirOperand, LirTerminator},
    mir::{MirConstant, MirEffectKind, MirOperand, MirTerminator, MirValueRef},
    types::{
        Identifier, NamePath, SourceID, SourceSpan,
        hir::{
            HirBlock, HirDocumentation, HirExpr, HirExprKind, HirFunction, HirMatchArm, HirModule, HirPattern, HirStatement, HirStatementKind,
            HirVisibility, ValkyrieType,
        },
    },
};
use ordered_float::OrderedFloat;

#[path = "control_flow_scheduler/hir_validation.rs"]
mod hir_validation;
#[path = "control_flow_scheduler/pipeline_consistency.rs"]
mod pipeline_consistency;

fn span() -> SourceSpan {
    SourceSpan::new(SourceID::default(), 0, 0)
}

fn expr(kind: HirExprKind) -> HirExpr {
    HirExpr { kind, span: span() }
}

fn semantic_mir(compiler: &ValkyrieCompiler, source: &str) -> Result<nyar_language::mir::MirModule, std_data::text::valkyrie::ParseError> {
    let hir = compiler.compile_source(source)?;
    Ok(MirLowerer::lower_module_semantic(&hir))
}

fn semantic_lir(compiler: &ValkyrieCompiler, source: &str) -> Result<nyar_language::lir::LirModule, std_data::text::valkyrie::ParseError> {
    use nyar_language::lir::LirLowerer;
    let hir = compiler.compile_source(source)?;
    let mir = MirLowerer::lower_module_semantic(&hir);
    Ok(LirLowerer::lower_mir_module(&hir, &mir))
}

fn validate_legacy_lir(module: &nyar_language::lir::LirModule) -> Result<(), std_data::text::valkyrie::ParseError> {
    nyar_language::lir::validation::validate_module(module)
}

fn validate_legacy_pipeline(
    hir: &HirModule,
    mir: &nyar_language::mir::MirModule,
    lir: &nyar_language::lir::LirModule,
) -> Result<(), std_data::text::valkyrie::ParseError> {
    ControlFlowScheduler::validate_hir_module(hir)?;
    ControlFlowScheduler::validate_mir_module(mir)?;
    compare_mir_lir_modules(mir, lir)?;
    validate_legacy_lir(lir)?;
    Ok(())
}

fn compare_mir_lir_modules(
    mir: &nyar_language::mir::MirModule,
    lir: &nyar_language::lir::LirModule,
) -> Result<(), std_data::text::valkyrie::ParseError> {
    for mir_function in &mir.functions {
        let Some(lir_function) = lir.functions.iter().find(|function| function.symbol == mir_function.symbol)
        else {
            return Err(std_data::text::valkyrie::ParseError::invalid(format!(
                "控制流调度校验失败：`MIR / LIR` 函数 `{}` 在 `LIR` 中缺失",
                mir_function.symbol
            )));
        };
        compare_suspend_points(&mir_function.symbol, &mir_function.suspend_points, &lir_function.suspend_points)?;
        compare_frame_layouts(&mir_function.symbol, &mir_function.frame_layouts, &lir_function.frame_layouts)?;
        compare_continuations(&mir_function.symbol, &mir_function.continuations, &lir_function.continuations)?;
        compare_case_chains(&mir_function.symbol, &mir_function.case_chains, &lir_function.case_chains)?;
        compare_state_machines(&mir_function.symbol, mir_function.suspend_plan.as_ref(), lir_function.state_machine.as_ref())?;
    }
    Ok(())
}

fn compare_suspend_points(
    function_name: &str,
    mir_suspend_points: &[nyar_language::mir::ssa::MirSuspendPoint],
    lir_suspend_points: &[nyar_language::lir::LirSuspendPoint],
) -> Result<(), std_data::text::valkyrie::ParseError> {
    if mir_suspend_points.len() != lir_suspend_points.len() {
        return Err(std_data::text::valkyrie::ParseError::invalid(format!(
            "控制流调度校验失败：`MIR / LIR` 函数 `{function_name}` 的 suspend 点数量不一致"
        )));
    }
    for (index, (mir_suspend_point, lir_suspend_point)) in mir_suspend_points.iter().zip(lir_suspend_points.iter()).enumerate() {
        if mir_suspend_point.state_id != lir_suspend_point.state_id
            || mir_suspend_point.effect != lower_effect_kind_to_mir(lir_suspend_point.effect)
            || mir_suspend_point.suspend_block != lir_suspend_point.suspend_block
            || mir_suspend_point.resume_target != lir_suspend_point.resume_target
            || mir_suspend_point.resume_parameter_count != lir_suspend_point.resume_parameter_count
            || mir_suspend_point.payload_type != lir_suspend_point.payload_type
            || mir_suspend_point.spill_candidates != lir_suspend_point.spill_candidates
            || mir_suspend_point.continuation_index != lir_suspend_point.continuation_index
        {
            return Err(std_data::text::valkyrie::ParseError::invalid(format!(
                "控制流调度校验失败：`MIR / LIR` 函数 `{function_name}` 的第 {} 个 suspend 点元数据不一致",
                index + 1
            )));
        }
    }
    Ok(())
}

fn compare_frame_layouts(
    function_name: &str,
    mir_frame_layouts: &[nyar_language::mir::ssa::MirFrameLayout],
    lir_frame_layouts: &[nyar_language::lir::LirFrameLayout],
) -> Result<(), std_data::text::valkyrie::ParseError> {
    if mir_frame_layouts.len() != lir_frame_layouts.len() {
        return Err(std_data::text::valkyrie::ParseError::invalid(format!(
            "控制流调度校验失败：`MIR / LIR` 函数 `{function_name}` 的 frame layout 数量不一致"
        )));
    }
    for (index, (mir_layout, lir_layout)) in mir_frame_layouts.iter().zip(lir_frame_layouts.iter()).enumerate() {
        if mir_layout.state_id != lir_layout.state_id
            || mir_layout.effect != lower_effect_kind_to_mir(lir_layout.effect)
            || mir_layout.resume_target != lir_layout.resume_target
            || mir_layout.slots.len() != lir_layout.slots.len()
        {
            return Err(std_data::text::valkyrie::ParseError::invalid(format!(
                "控制流调度校验失败：`MIR / LIR` 函数 `{function_name}` 的第 {} 个 frame layout 元数据不一致",
                index + 1
            )));
        }
        for (slot_index, (mir_slot, lir_slot)) in mir_layout.slots.iter().zip(lir_layout.slots.iter()).enumerate() {
            if mir_slot.slot_index != lir_slot.slot_index || mir_slot.value != lir_slot.value || mir_slot.value_type != lir_slot.value_type {
                return Err(std_data::text::valkyrie::ParseError::invalid(format!(
                    "控制流调度校验失败：`MIR / LIR` 函数 `{function_name}` 的第 {} 个 frame layout 的第 {} 个槽位不一致",
                    index + 1,
                    slot_index + 1
                )));
            }
        }
    }
    Ok(())
}

fn compare_continuations(
    function_name: &str,
    mir_continuations: &[nyar_language::mir::ssa::MirContinuation],
    lir_continuations: &[nyar_language::lir::LirContinuation],
) -> Result<(), std_data::text::valkyrie::ParseError> {
    if mir_continuations.len() != lir_continuations.len() {
        return Err(std_data::text::valkyrie::ParseError::invalid(format!(
            "控制流调度校验失败：`MIR / LIR` 函数 `{function_name}` 的 continuation 数量不一致"
        )));
    }
    for (index, (mir_continuation, lir_continuation)) in mir_continuations.iter().zip(lir_continuations.iter()).enumerate() {
        if mir_continuation.dispatch_block != lir_continuation.dispatch_block
            || mir_continuation.resume_target != lir_continuation.resume_target
            || mir_continuation.resume_parameter != lir_continuation.resume_parameter
            || mir_continuation.handler_exit != lir_continuation.handler_exit
        {
            return Err(std_data::text::valkyrie::ParseError::invalid(format!(
                "控制流调度校验失败：`MIR / LIR` 函数 `{function_name}` 的第 {} 个 continuation 元数据不一致",
                index + 1
            )));
        }
        if mir_continuation.resume_parameter_type != lir_continuation.resume_parameter_type {
            return Err(std_data::text::valkyrie::ParseError::invalid(format!(
                "控制流调度校验失败：`MIR / LIR` 函数 `{function_name}` 的第 {} 个 continuation 恢复类型为 `{}`，但 `LIR` 记录为 `{}`",
                index + 1,
                display_optional_type(mir_continuation.resume_parameter_type.as_ref()),
                display_optional_type(lir_continuation.resume_parameter_type.as_ref())
            )));
        }
    }
    Ok(())
}

fn compare_case_chains(
    function_name: &str,
    mir_case_chains: &[nyar_language::mir::ssa::MirCaseChain],
    lir_case_chains: &[nyar_language::lir::LirCaseChain],
) -> Result<(), std_data::text::valkyrie::ParseError> {
    if mir_case_chains.len() != lir_case_chains.len() {
        return Err(std_data::text::valkyrie::ParseError::invalid(format!(
            "控制流调度校验失败：`MIR / LIR` 函数 `{function_name}` 的 case chain 数量不一致"
        )));
    }
    for (index, (mir_chain, lir_chain)) in mir_case_chains.iter().zip(lir_case_chains.iter()).enumerate() {
        if mir_chain.dispatch_block != lir_chain.dispatch_block
            || mir_chain.first_arm != lir_chain.first_arm
            || mir_chain.no_match_block != lir_chain.no_match_block
            || mir_chain.exit_block != lir_chain.exit_block
            || mir_chain.produce_value != lir_chain.produce_value
            || mir_chain.arms.len() != lir_chain.arms.len()
        {
            return Err(std_data::text::valkyrie::ParseError::invalid(format!(
                "控制流调度校验失败：`MIR / LIR` 函数 `{function_name}` 的第 {} 个 case chain 元数据不一致",
                index + 1
            )));
        }
        for (arm_index, (mir_arm, lir_arm)) in mir_chain.arms.iter().zip(lir_chain.arms.iter()).enumerate() {
            if mir_arm.entry_block != lir_arm.entry_block
                || mir_arm.check_block != lir_arm.check_block
                || mir_arm.guard_block != lir_arm.guard_block
                || mir_arm.body_block != lir_arm.body_block
                || mir_arm.next_arm_target != lir_arm.next_arm_target
                || mir_arm.exit_target != lir_arm.exit_target
                || mir_arm.fallthrough_target != lir_arm.fallthrough_target
            {
                return Err(std_data::text::valkyrie::ParseError::invalid(format!(
                    "控制流调度校验失败：`MIR / LIR` 函数 `{function_name}` 的第 {} 个 case chain 的第 {} 个 arm 不一致",
                    index + 1,
                    arm_index + 1
                )));
            }
        }
    }
    Ok(())
}

fn compare_state_machines(
    function_name: &str,
    mir_state_machine: Option<&nyar_language::mir::continuation_runtime::SuspendLoweringPlan>,
    lir_state_machine: Option<&nyar_language::lir::LirStateMachineDescriptor>,
) -> Result<(), std_data::text::valkyrie::ParseError> {
    match (mir_state_machine, lir_state_machine) {
        (None, None) => Ok(()),
        (Some(_), None) | (None, Some(_)) => Err(std_data::text::valkyrie::ParseError::invalid(format!(
            "控制流调度校验失败：`MIR / LIR` 函数 `{function_name}` 的状态机描述存在性不一致"
        ))),
        (Some(mir_descriptor), Some(lir_descriptor)) => {
            if mir_descriptor.function_symbol != lir_descriptor.function_symbol
                || mir_descriptor.entry_block != lir_descriptor.entry_block
                || mir_descriptor.handler_dispatch_blocks != lir_descriptor.handler_dispatch_blocks
            {
                return Err(std_data::text::valkyrie::ParseError::invalid(format!(
                    "控制流调度校验失败：`MIR / LIR` 函数 `{function_name}` 的状态机描述不一致"
                )));
            }
            if mir_descriptor.states.len() != lir_descriptor.states.len() {
                return Err(std_data::text::valkyrie::ParseError::invalid(format!(
                    "控制流调度校验失败：`MIR / LIR` 函数 `{function_name}` 的状态机状态数量不一致"
                )));
            }
            for (index, (mir_state, lir_state)) in mir_descriptor.states.iter().zip(lir_descriptor.states.iter()).enumerate() {
                if mir_state.state_id != lir_state.state_id
                    || mir_state.effect != lower_effect_kind_to_mir(lir_state.effect)
                    || mir_state.suspend_block != lir_state.suspend_block
                    || mir_state.resume_target != lir_state.resume_target
                    || mir_state.resume_parameter_count != lir_state.resume_parameter_count
                    || mir_state.resume_parameter_type != lir_state.resume_parameter_type
                    || mir_state.payload_type != lir_state.payload_type
                    || mir_state.spill_slots != lir_state.spill_slots
                    || mir_state.frame_carrier != lir_state.frame_carrier
                    || mir_state.continuation_index != lir_state.continuation_index
                {
                    return Err(std_data::text::valkyrie::ParseError::invalid(format!(
                        "控制流调度校验失败：`MIR / LIR` 函数 `{function_name}` 的第 {} 个状态机状态不一致",
                        index + 1
                    )));
                }
            }
            Ok(())
        }
    }
}

fn lower_effect_kind_to_mir(effect: LirEffectKind) -> MirEffectKind {
    match effect {
        LirEffectKind::Raise => MirEffectKind::Raise,
        LirEffectKind::Yield => MirEffectKind::Yield,
        LirEffectKind::DelegateYield => MirEffectKind::DelegateYield,
        LirEffectKind::Await => MirEffectKind::Await,
        LirEffectKind::AsyncSpawn => MirEffectKind::AsyncSpawn,
        LirEffectKind::AsyncBlock => MirEffectKind::AsyncBlock,
    }
}

fn display_optional_type(ty: Option<&ValkyrieType>) -> String {
    ty.map(display_type).unwrap_or_else(|| "<unknown>".to_string())
}

fn display_type(ty: &ValkyrieType) -> String {
    match ty {
        ValkyrieType::Void => "void".to_string(),
        ValkyrieType::Unit => "unit".to_string(),
        ValkyrieType::Boolean => "bool".to_string(),
        ValkyrieType::Named(name) => name.to_string(),
        ValkyrieType::Apply(base, arguments) => {
            format!("{}<{}>", display_type(base), arguments.iter().map(display_type).collect::<Vec<_>>().join(", "))
        }
        other => format!("{other:?}"),
    }
}

fn demo_module(body_expr: HirExpr) -> HirModule {
    HirModule {
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
            body: HirBlock { statements: Vec::new(), expr: Some(Box::new(body_expr)), span: span() },
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
    }
}
