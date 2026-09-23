use std::collections::{BTreeMap, BTreeSet};

use crate::types::hir::ValkyrieType;
use std_data::text::valkyrie::ParseError;

use super::{
    LirBlock, LirContinuation, LirEffectKind, LirFrameLayout, LirFunction, LirModule, LirOperand, LirRuntimeContinuation, LirRuntimeFrame,
    LirTerminator,
};
use crate::mir::{MirBlockRef, MirConstant, MirValueRef};

pub fn validate_module(module: &LirModule) -> Result<(), ParseError> {
    for function in &module.functions {
        validate_function(function)?;
    }
    Ok(())
}

fn validate_function(function: &LirFunction) -> Result<(), ParseError> {
    let blocks: BTreeMap<_, _> = function.blocks.iter().map(|block| (block.id, block.parameters.len())).collect();
    let block_map: BTreeMap<_, _> = function.blocks.iter().map(|block| (block.id, block)).collect();
    if !blocks.contains_key(&function.entry) {
        return Err(ParseError::invalid(format!("控制流调度校验失败：函数 `{}` 的 entry block 不存在", function.symbol)));
    }

    let reachable_blocks = collect_reachable_blocks(function);
    for block in &function.blocks {
        if !reachable_blocks.contains(&block.id) {
            continue;
        }
        match &block.terminator {
            LirTerminator::Jump { target, arguments } => validate_jump_target(function, block, *target, arguments, &blocks, &block_map)?,
            LirTerminator::Branch { then_target, else_target, .. } => {
                ensure_target_exists(&function.symbol, block.label.as_str(), *then_target, &blocks)?;
                ensure_target_exists(&function.symbol, block.label.as_str(), *else_target, &blocks)?;
            }
            LirTerminator::PerformEffect { effect, payload, resume_target, .. } => {
                validate_effect_payload(&function.symbol, block.label.as_str(), *effect, payload.is_some())?;
                validate_effect_resume_target(&function.symbol, block.label.as_str(), *effect, *resume_target, &blocks)?;
                if let Some(payload_type) = payload.as_ref().and_then(|payload| infer_operand_static_type(function, payload)) {
                    validate_effect_payload_static_type(&function.symbol, block.label.as_str(), *effect, &payload_type)?;
                }
                if let (Some(expected_type), Some(resume_block)) = (
                    infer_effect_resume_static_type(
                        *effect,
                        payload.as_ref().and_then(|payload| infer_operand_static_type(function, payload)).as_ref(),
                        carrier_type_for_block(function, *resume_target),
                    ),
                    block_map.get(resume_target),
                ) {
                    validate_resume_block_parameter_static_type(
                        &function.symbol,
                        block.label.as_str(),
                        function,
                        *resume_block,
                        &expected_type,
                    )?;
                }
            }
            LirTerminator::StateDispatch { cases, default_target, .. } => {
                ensure_target_exists(&function.symbol, block.label.as_str(), *default_target, &blocks)?;
                for (_, target) in cases {
                    ensure_target_exists(&function.symbol, block.label.as_str(), *target, &blocks)?;
                }
            }
            LirTerminator::YieldToRuntime { effect, payload, .. } => {
                validate_effect_payload(&function.symbol, block.label.as_str(), *effect, payload.is_some())?;
                if let Some(payload_type) = payload.as_ref().and_then(|payload| infer_operand_static_type(function, payload)) {
                    validate_effect_payload_static_type(&function.symbol, block.label.as_str(), *effect, &payload_type)?;
                }
            }
            LirTerminator::Return { .. } | LirTerminator::Unreachable => {}
        }
    }
    validate_suspend_points(function, &block_map)?;
    for (index, layout) in function.frame_layouts.iter().enumerate() {
        validate_frame_layout(&function.symbol, function, index, layout)?;
    }
    validate_continuations(function, &block_map)?;
    validate_case_chains(function, &block_map)?;
    for (index, runtime_frame) in function.runtime_frames.iter().enumerate() {
        validate_runtime_frame(&function.symbol, function, index, runtime_frame)?;
    }
    for (index, runtime_continuation) in function.runtime_continuations.iter().enumerate() {
        validate_runtime_continuation(&function.symbol, function, index, runtime_continuation)?;
    }
    Ok(())
}

fn validate_suspend_points(function: &LirFunction, block_map: &BTreeMap<MirBlockRef, &LirBlock>) -> Result<(), ParseError> {
    for (index, suspend_point) in function.suspend_points.iter().enumerate() {
        validate_suspend_target(&function.symbol, index, suspend_point, block_map)?;
    }
    Ok(())
}

fn validate_frame_layout(function_name: &str, function: &LirFunction, index: usize, layout: &LirFrameLayout) -> Result<(), ParseError> {
    let Some(suspend_point) = function.suspend_points.iter().find(|suspend_point| suspend_point.state_id == layout.state_id)
    else {
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：函数 `{function_name}` 的第 {} 个 `LIR` frame layout 找不到对应的 suspend 点",
            index + 1
        )));
    };
    if suspend_point.resume_target != layout.resume_target {
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：函数 `{function_name}` 的第 {} 个 `LIR` frame layout 恢复目标与 suspend 点不一致",
            index + 1
        )));
    }
    if suspend_point.spill_candidates.len() != layout.slots.len() {
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：函数 `{function_name}` 的第 {} 个 `LIR` frame layout 槽位数量与 suspend spill 集不一致",
            index + 1
        )));
    }
    for (slot_index, (expected_value, slot)) in suspend_point.spill_candidates.iter().zip(layout.slots.iter()).enumerate() {
        if slot.slot_index != slot_index || slot.value != *expected_value {
            return Err(ParseError::invalid(format!(
                "控制流调度校验失败：函数 `{function_name}` 的第 {} 个 `LIR` frame layout 的第 {} 个槽位未对齐 suspend spill 顺序",
                index + 1,
                slot_index + 1
            )));
        }
        if let (Some(expected_type), Some(actual_type)) = (function.value_types.get(&slot.value), slot.value_type.as_ref()) {
            if expected_type != actual_type {
                return Err(ParseError::invalid(format!(
                    "控制流调度校验失败：函数 `{function_name}` 的第 {} 个 `LIR` frame layout 的第 {} 个槽位类型为 `{}`，但 SSA 值类型为 `{}`",
                    index + 1,
                    slot_index + 1,
                    display_type(actual_type),
                    display_type(expected_type)
                )));
            }
        }
    }
    Ok(())
}

fn validate_runtime_frame(
    function_name: &str,
    function: &LirFunction,
    index: usize,
    runtime_frame: &LirRuntimeFrame,
) -> Result<(), ParseError> {
    let Some(layout) = function.frame_layouts.iter().find(|layout| layout.state_id == runtime_frame.state_id)
    else {
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：函数 `{function_name}` 的第 {} 个 `LIR` runtime frame 找不到对应的 frame layout",
            index + 1
        )));
    };
    if runtime_frame.resume_target != layout.resume_target {
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：函数 `{function_name}` 的第 {} 个 `LIR` runtime frame 恢复目标与 frame layout 不一致",
            index + 1
        )));
    }
    if runtime_frame.slots.len() != layout.slots.len() {
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：函数 `{function_name}` 的第 {} 个 `LIR` runtime frame 槽位数量与 frame layout 不一致",
            index + 1
        )));
    }
    for (slot_index, (layout_slot, runtime_slot)) in layout.slots.iter().zip(runtime_frame.slots.iter()).enumerate() {
        if runtime_slot.slot_index != layout_slot.slot_index
            || runtime_slot.value != layout_slot.value
            || runtime_slot.value_type != layout_slot.value_type
        {
            return Err(ParseError::invalid(format!(
                "控制流调度校验失败：函数 `{function_name}` 的第 {} 个 `LIR` runtime frame 的第 {} 个槽位未对齐 frame layout",
                index + 1,
                slot_index + 1
            )));
        }
    }
    Ok(())
}

fn validate_continuations(function: &LirFunction, block_map: &BTreeMap<MirBlockRef, &LirBlock>) -> Result<(), ParseError> {
    for (index, continuation) in function.continuations.iter().enumerate() {
        validate_continuation_target(
            &function.symbol,
            index,
            continuation.dispatch_block,
            continuation.resume_target,
            continuation.resume_parameter,
            continuation.handler_exit,
            block_map,
        )?;
        if let (Some(expected_type), Some(actual_type)) =
            (continuation.resume_parameter_type.as_ref(), function.value_types.get(&continuation.resume_parameter))
        {
            if expected_type != actual_type {
                return Err(ParseError::invalid(format!(
                    "控制流调度校验失败：`LIR` 函数 `{}` 的第 {} 个 continuation 恢复类型为 `{}`，但 block parameter 类型为 `{}`",
                    function.symbol,
                    index + 1,
                    display_type(expected_type),
                    display_type(actual_type)
                )));
            }
        }
    }
    Ok(())
}

fn validate_runtime_continuation(
    function_name: &str,
    function: &LirFunction,
    index: usize,
    runtime_continuation: &LirRuntimeContinuation,
) -> Result<(), ParseError> {
    let Some(continuation) = function.continuations.get(index)
    else {
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：函数 `{function_name}` 的第 {} 个 `LIR` runtime continuation 找不到对应的 continuation",
            index + 1
        )));
    };
    if runtime_continuation.dispatch_block != continuation.dispatch_block
        || runtime_continuation.resume_target != continuation.resume_target
        || runtime_continuation.resume_parameter != continuation.resume_parameter
        || runtime_continuation.resume_parameter_type != continuation.resume_parameter_type
        || runtime_continuation.handler_exit != continuation.handler_exit
    {
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：函数 `{function_name}` 的第 {} 个 `LIR` runtime continuation 与 continuation 元数据不一致",
            index + 1
        )));
    }
    Ok(())
}

fn validate_case_chains(function: &LirFunction, block_map: &BTreeMap<MirBlockRef, &LirBlock>) -> Result<(), ParseError> {
    for (index, case_chain) in function.case_chains.iter().enumerate() {
        if !block_map.contains_key(&case_chain.dispatch_block)
            || !block_map.contains_key(&case_chain.first_arm)
            || !block_map.contains_key(&case_chain.no_match_block)
            || !block_map.contains_key(&case_chain.exit_block)
        {
            return Err(ParseError::invalid(format!(
                "控制流调度校验失败：函数 `{}` 的第 {} 个 `LIR` case chain 指向了不存在的 block",
                function.symbol,
                index + 1
            )));
        }
        if case_chain.arms.is_empty() {
            return Err(ParseError::invalid(format!(
                "控制流调度校验失败：函数 `{}` 的第 {} 个 `LIR` case chain 不允许缺少 arm",
                function.symbol,
                index + 1
            )));
        }
        if case_chain.arms.first().map(|arm| arm.entry_block) != Some(case_chain.first_arm) {
            return Err(ParseError::invalid(format!(
                "控制流调度校验失败：函数 `{}` 的第 {} 个 `LIR` case chain 的 arm 入口与记录不一致",
                function.symbol,
                index + 1
            )));
        }
        for (arm_index, arm) in case_chain.arms.iter().enumerate() {
            if !block_map.contains_key(&arm.entry_block) || !block_map.contains_key(&arm.body_block) {
                return Err(ParseError::invalid(format!(
                    "控制流调度校验失败：函数 `{}` 的第 {} 个 `LIR` case chain 的第 {} 个 arm 指向了不存在的入口或 body block",
                    function.symbol,
                    index + 1,
                    arm_index + 1
                )));
            }
            if let Some(check_block) = arm.check_block {
                if !block_map.contains_key(&check_block) {
                    return Err(ParseError::invalid(format!(
                        "控制流调度校验失败：函数 `{}` 的第 {} 个 `LIR` case chain 的第 {} 个 arm 指向了不存在的 check block",
                        function.symbol,
                        index + 1,
                        arm_index + 1
                    )));
                }
            }
            if let Some(guard_block) = arm.guard_block {
                if !block_map.contains_key(&guard_block) {
                    return Err(ParseError::invalid(format!(
                        "控制流调度校验失败：函数 `{}` 的第 {} 个 `LIR` case chain 的第 {} 个 arm 指向了不存在的 guard block",
                        function.symbol,
                        index + 1,
                        arm_index + 1
                    )));
                }
            }
            if !block_map.contains_key(&arm.next_arm_target) || !block_map.contains_key(&arm.exit_target) {
                return Err(ParseError::invalid(format!(
                    "控制流调度校验失败：函数 `{}` 的第 {} 个 `LIR` case chain 的第 {} 个 arm 指向了不存在的后继或 exit block",
                    function.symbol,
                    index + 1,
                    arm_index + 1
                )));
            }
            if let Some(fallthrough_target) = arm.fallthrough_target {
                if !block_map.contains_key(&fallthrough_target) {
                    return Err(ParseError::invalid(format!(
                        "控制流调度校验失败：函数 `{}` 的第 {} 个 `LIR` case chain 的第 {} 个 arm 指向了不存在的 fallthrough 目标",
                        function.symbol,
                        index + 1,
                        arm_index + 1
                    )));
                }
            }
        }
    }
    Ok(())
}

fn validate_continuation_target(
    function_name: &str,
    index: usize,
    dispatch_block: MirBlockRef,
    resume_target: MirBlockRef,
    resume_parameter: MirValueRef,
    handler_exit: MirBlockRef,
    blocks: &BTreeMap<MirBlockRef, &LirBlock>,
) -> Result<(), ParseError> {
    let Some(resume_block) = blocks.get(&resume_target)
    else {
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：函数 `{function_name}` 的第 {} 个 `LIR` continuation 指向了不存在的 resume block",
            index + 1
        )));
    };
    if !blocks.contains_key(&dispatch_block) {
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：函数 `{function_name}` 的第 {} 个 `LIR` continuation 指向了不存在的 dispatch block",
            index + 1
        )));
    }
    if !blocks.contains_key(&handler_exit) {
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：函数 `{function_name}` 的第 {} 个 `LIR` continuation 指向了不存在的 handler exit block",
            index + 1
        )));
    }
    if !resume_block.parameters.contains(&resume_parameter) {
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：函数 `{function_name}` 的第 {} 个 `LIR` continuation 恢复参数不属于目标 resume block",
            index + 1
        )));
    }
    Ok(())
}

fn validate_suspend_target(
    function_name: &str,
    index: usize,
    suspend_point: &super::LirSuspendPoint,
    blocks: &BTreeMap<MirBlockRef, &LirBlock>,
) -> Result<(), ParseError> {
    if !blocks.contains_key(&suspend_point.suspend_block) {
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：函数 `{function_name}` 的第 {} 个 `LIR` suspend 点指向了不存在的挂起 block",
            index + 1
        )));
    }
    let Some(resume_block) = blocks.get(&suspend_point.resume_target)
    else {
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：函数 `{function_name}` 的第 {} 个 `LIR` suspend 点指向了不存在的恢复 block",
            index + 1
        )));
    };
    if resume_block.parameters.len() != suspend_point.resume_parameter_count {
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：函数 `{function_name}` 的第 {} 个 `LIR` suspend 点恢复参数个数为 {}，但目标恢复 block 实际有 {} 个参数",
            index + 1,
            suspend_point.resume_parameter_count,
            resume_block.parameters.len()
        )));
    }
    Ok(())
}

fn collect_reachable_blocks(function: &LirFunction) -> BTreeSet<MirBlockRef> {
    let mut reachable = BTreeSet::new();
    let mut worklist = vec![function.entry];
    let blocks: BTreeMap<_, _> = function.blocks.iter().map(|block| (block.id, block)).collect();

    while let Some(block_id) = worklist.pop() {
        if !reachable.insert(block_id) {
            continue;
        }
        let Some(block) = blocks.get(&block_id)
        else {
            continue;
        };
        match &block.terminator {
            LirTerminator::Jump { target, .. } => worklist.push(*target),
            LirTerminator::Branch { then_target, else_target, .. } => {
                worklist.push(*then_target);
                worklist.push(*else_target);
            }
            LirTerminator::PerformEffect { resume_target, .. } => worklist.push(*resume_target),
            LirTerminator::StateDispatch { cases, default_target, .. } => {
                worklist.push(*default_target);
                for (_, target) in cases {
                    worklist.push(*target);
                }
            }
            LirTerminator::YieldToRuntime { .. } => {}
            LirTerminator::Return { .. } | LirTerminator::Unreachable => {}
        }
    }

    reachable
}

fn validate_jump_target(
    function: &LirFunction,
    block: &LirBlock,
    target: MirBlockRef,
    arguments: &[LirOperand],
    blocks: &BTreeMap<MirBlockRef, usize>,
    block_map: &BTreeMap<MirBlockRef, &LirBlock>,
) -> Result<(), ParseError> {
    validate_jump_shape(&function.symbol, block.label.as_str(), target, arguments.len(), blocks)?;
    let Some(target_block) = block_map.get(&target)
    else {
        return Ok(());
    };
    for (index, (argument, parameter)) in arguments.iter().zip(target_block.parameters.iter()).enumerate() {
        let Some(argument_type) = infer_operand_static_type(function, argument)
        else {
            continue;
        };
        let Some(parameter_type) = function.value_types.get(parameter)
        else {
            continue;
        };
        if argument_type != *parameter_type {
            return Err(ParseError::invalid(format!(
                "控制流调度校验失败：`LIR` 函数 `{}` 的 block `{}` 跳向 `{}` 时，第 {} 个 Jump 参数类型为 `{}`，目标参数类型为 `{}`",
                function.symbol,
                block.label,
                target_block.label,
                index + 1,
                display_type(&argument_type),
                display_type(parameter_type)
            )));
        }
    }
    Ok(())
}

fn validate_effect_resume_target(
    function_name: &str,
    block_label: &str,
    effect: LirEffectKind,
    resume_target: MirBlockRef,
    blocks: &BTreeMap<MirBlockRef, usize>,
) -> Result<(), ParseError> {
    ensure_target_exists(function_name, block_label, resume_target, blocks)?;
    let actual_parameter_count = blocks.get(&resume_target).copied().unwrap_or_default();
    let expected_parameter_count = expected_effect_resume_parameter_count(effect);
    if actual_parameter_count != expected_parameter_count {
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：`LIR` 函数 `{function_name}` 的 block `{block_label}` 所指向的 effect 恢复点参数个数不合法，期望 {expected_parameter_count} 个，当前为 {actual_parameter_count}"
        )));
    }
    Ok(())
}

fn validate_effect_payload(function_name: &str, block_label: &str, effect: LirEffectKind, has_payload: bool) -> Result<(), ParseError> {
    if payload_required(effect) || has_payload {
        if has_payload {
            return Ok(());
        }
        if payload_required(effect) {
            return Err(ParseError::invalid(format!(
                "控制流调度校验失败：`LIR` 函数 `{function_name}` 的 block `{block_label}` 缺少 effect payload"
            )));
        }
    }
    Ok(())
}

fn validate_effect_payload_static_type(
    function_name: &str,
    block_label: &str,
    effect: LirEffectKind,
    payload_type: &ValkyrieType,
) -> Result<(), ParseError> {
    if matches!(effect, LirEffectKind::Await | LirEffectKind::AsyncSpawn | LirEffectKind::AsyncBlock)
        && future_resume_type(payload_type).is_none()
    {
        let effect_name = match effect {
            LirEffectKind::Await => "`await`",
            LirEffectKind::AsyncSpawn => "`awake`",
            LirEffectKind::AsyncBlock => "`block`",
            _ => unreachable!(),
        };
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：`LIR` 函数 `{function_name}` 的 block `{block_label}` 的 {effect_name} effect payload 类型 `{}` 不满足 `Future<T>` / `Promise<T>` 形状",
            display_type(payload_type)
        )));
    }
    if matches!(effect, LirEffectKind::DelegateYield) && generator_source_type(payload_type).is_none() {
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：`LIR` 函数 `{function_name}` 的 block `{block_label}` 的 `yield from` effect payload 类型 `{}` 不满足 `Generator<T>` / `Iterator<T>` 形状",
            display_type(payload_type)
        )));
    }
    Ok(())
}

fn validate_resume_block_parameter_static_type(
    function_name: &str,
    block_label: &str,
    function: &LirFunction,
    resume_block: &LirBlock,
    expected_type: &ValkyrieType,
) -> Result<(), ParseError> {
    let Some(parameter) = resume_block.parameters.first()
    else {
        return Ok(());
    };
    let Some(actual_type) = function.value_types.get(parameter)
    else {
        return Ok(());
    };
    if actual_type == expected_type {
        return Ok(());
    }
    Err(ParseError::invalid(format!(
        "控制流调度校验失败：`LIR` 函数 `{function_name}` 的 block `{block_label}` 所指向恢复点参数类型为 `{}`，期望 `{}`",
        display_type(actual_type),
        display_type(expected_type)
    )))
}

fn infer_operand_static_type(function: &LirFunction, operand: &LirOperand) -> Option<ValkyrieType> {
    match operand {
        LirOperand::Constant(constant) => Some(infer_constant_type(constant)),
        LirOperand::Value(value_ref) => function.value_types.get(value_ref).cloned(),
        LirOperand::Symbol(_) => None,
    }
}

fn infer_effect_resume_static_type(
    effect: LirEffectKind,
    payload_type: Option<&ValkyrieType>,
    carrier_type: Option<&ValkyrieType>,
) -> Option<ValkyrieType> {
    match effect {
        LirEffectKind::Yield | LirEffectKind::DelegateYield => Some(ValkyrieType::Unit),
        LirEffectKind::Await | LirEffectKind::AsyncBlock => payload_type.and_then(future_resume_type),
        LirEffectKind::AsyncSpawn => None,
        LirEffectKind::Raise => carrier_type.map(|_ty| ValkyrieType::Named(crate::types::Identifier::new("Never"))),
    }
}

/// 查找与给定 resume_target 关联的 suspend point 的 carrier_type。
///
/// 校验阶段需要 carrier_type 来推断 `Raise` 路径的 resume 参数类型；该函数从函数的
/// `suspend_points` 中匹配 `resume_target` 一致的挂起点并返回其 carrier_type。
fn carrier_type_for_block(function: &LirFunction, resume_target: MirBlockRef) -> Option<&ValkyrieType> {
    function.suspend_points.iter().find(|point| point.resume_target == resume_target).and_then(|point| point.carrier_type.as_ref())
}

fn infer_constant_type(constant: &MirConstant) -> ValkyrieType {
    match constant {
        MirConstant::Int(_) => ValkyrieType::Integer64 { signed: true },
        MirConstant::Float64(_) => ValkyrieType::Float64,
        MirConstant::Bool(_) => ValkyrieType::Boolean,
        MirConstant::Utf8(_) => ValkyrieType::Utf8,
        MirConstant::Utf16(_) => ValkyrieType::Utf16,
        MirConstant::Unit => ValkyrieType::Unit,
    }
}

fn future_resume_type(ty: &ValkyrieType) -> Option<ValkyrieType> {
    match ty {
        ValkyrieType::Apply(base, arguments) if arguments.len() == 1 && matches!(named_type_name(base), Some("Future" | "Promise")) => {
            arguments.first().cloned()
        }
        _ => None,
    }
}

fn generator_source_type(ty: &ValkyrieType) -> Option<ValkyrieType> {
    match ty {
        ValkyrieType::Apply(base, arguments)
            if !arguments.is_empty() && matches!(named_type_name(base), Some("Generator" | "Iterator" | "Coroutine")) =>
        {
            arguments.first().cloned()
        }
        ValkyrieType::Named(name) if matches!(name.as_str(), "Generator" | "Iterator" | "Coroutine") => Some(ValkyrieType::Unit),
        _ => None,
    }
}

fn named_type_name(ty: &ValkyrieType) -> Option<&str> {
    match ty {
        ValkyrieType::Named(name) => Some(name.as_str()),
        ValkyrieType::Apply(base, _) => named_type_name(base),
        _ => None,
    }
}

fn display_type(ty: &ValkyrieType) -> String {
    match ty {
        ValkyrieType::Void => "void".to_string(),
        ValkyrieType::Unit => "unit".to_string(),
        ValkyrieType::Boolean => "bool".to_string(),
        ValkyrieType::Integer8 { signed } => integer_type_name(*signed, 8),
        ValkyrieType::Integer16 { signed } => integer_type_name(*signed, 16),
        ValkyrieType::Integer32 { signed } => integer_type_name(*signed, 32),
        ValkyrieType::Integer64 { signed } => integer_type_name(*signed, 64),
        ValkyrieType::Integer128 { signed } => integer_type_name(*signed, 128),
        ValkyrieType::Float32 => "f32".to_string(),
        ValkyrieType::Float64 => "f64".to_string(),
        ValkyrieType::Character => "char".to_string(),
        ValkyrieType::Utf8 => "utf8".to_string(),
        ValkyrieType::Utf16 => "utf16".to_string(),
        ValkyrieType::Named(name) => name.to_string(),
        ValkyrieType::Apply(base, arguments) => {
            format!("{}<{}>", display_type(base), arguments.iter().map(display_type).collect::<Vec<_>>().join(", "))
        }
        ValkyrieType::Generic(generic) => generic.name.to_string(),
        ValkyrieType::Function(function) => format!(
            "micro({}) -> {}",
            function.params.iter().map(display_type).collect::<Vec<_>>().join(", "),
            display_type(&function.return_type)
        ),
        ValkyrieType::Tuple(items) => format!("({})", items.iter().map(display_type).collect::<Vec<_>>().join(", ")),
        ValkyrieType::Row(row) => format!(
            "{{ {} }}",
            row.methods
                .iter()
                .map(|method| {
                    format!(
                        "{}({}) -> {}",
                        method.name,
                        method.params.iter().map(display_type).collect::<Vec<_>>().join(", "),
                        display_type(&method.return_type)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ValkyrieType::Array(item) => format!("[{}]", display_type(item)),
        ValkyrieType::FixedArray { element, length } => format!("[{}; {}]", display_type(element), length),
        ValkyrieType::TypeLambda(lambda) => format!(
            "type lambda({}) -> {}",
            lambda.params.iter().map(|item| item.name.to_string()).collect::<Vec<_>>().join(", "),
            display_type(&lambda.body)
        ),
        ValkyrieType::TraitObject(object) => {
            format!("{}<{}>", object.trait_path, object.type_arguments.iter().map(display_type).collect::<Vec<_>>().join(", "))
        }
        ValkyrieType::Associated(associated) => format!("{}::{}", display_type(&associated.base), associated.name),
        ValkyrieType::AutoType => "auto".to_string(),
        ValkyrieType::SelfType => "Self".to_string(),
        ValkyrieType::Nullable(payload) => format!("{}?", display_type(payload)),
        ValkyrieType::Union(items) => items.iter().map(display_type).collect::<Vec<_>>().join(" | "),
        ValkyrieType::Intersection(items) => items.iter().map(display_type).collect::<Vec<_>>().join(" & "),
    }
}

fn integer_type_name(signed: bool, bits: u16) -> String {
    if signed { format!("i{bits}") } else { format!("u{bits}") }
}

fn expected_effect_resume_parameter_count(effect: LirEffectKind) -> usize {
    match effect {
        LirEffectKind::AsyncSpawn => 0,
        LirEffectKind::Raise | LirEffectKind::Yield | LirEffectKind::DelegateYield | LirEffectKind::Await | LirEffectKind::AsyncBlock => 1,
    }
}

fn payload_required(effect: LirEffectKind) -> bool {
    let _ = effect;
    true
}

fn validate_jump_shape(
    function_name: &str,
    block_label: &str,
    target: MirBlockRef,
    argument_count: usize,
    blocks: &BTreeMap<MirBlockRef, usize>,
) -> Result<(), ParseError> {
    let Some(expected_parameter_count) = blocks.get(&target).copied()
    else {
        return Err(ParseError::invalid(format!("控制流调度校验失败：函数 `{function_name}` 的 block `{block_label}` 跳转到了不存在的目标块")));
    };
    if expected_parameter_count != argument_count {
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：函数 `{function_name}` 的 block `{block_label}` 传出的参数数量与目标块参数数量不一致"
        )));
    }
    Ok(())
}

fn ensure_target_exists(
    function_name: &str,
    block_label: &str,
    target: MirBlockRef,
    blocks: &BTreeMap<MirBlockRef, usize>,
) -> Result<(), ParseError> {
    if blocks.contains_key(&target) {
        Ok(())
    }
    else {
        Err(ParseError::invalid(format!("控制流调度校验失败：函数 `{function_name}` 的 block `{block_label}` 指向了不存在的目标块")))
    }
}
