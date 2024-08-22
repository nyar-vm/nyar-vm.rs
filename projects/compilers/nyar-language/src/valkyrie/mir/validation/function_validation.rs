//! 单个 MIR 函数的块、终结符与调用校验。

use super::{
    control_flow::{collect_reachable_blocks, ensure_target_exists, validate_jump_target},
    effect_validation::{
        validate_effect_payload, validate_effect_payload_static_type, validate_effect_resume_target,
        validate_resume_block_parameter_static_type,
    },
    type_inference::{infer_effect_resume_static_type, infer_operand_static_type},
};
use crate::{
    mir::{MirFunction, MirTerminator},
    valkyrie::hir::HirError,
};
use std::collections::BTreeMap;

pub(super) fn validate_function(function: &MirFunction) -> Result<(), HirError> {
    let blocks: BTreeMap<_, _> = function.blocks.iter().map(|block| (block.id, block.parameters.len())).collect();
    let block_map: BTreeMap<_, _> = function.blocks.iter().map(|block| (block.id, block)).collect();
    if !blocks.contains_key(&function.entry) {
        return Err(HirError::invalid(format!("控制流调度校验失败：函数 `{}` 的 entry block 不存在", function.display_symbol)));
    }

    let reachable_blocks = collect_reachable_blocks(function);
    for block in &function.blocks {
        if !reachable_blocks.contains(&block.id) {
            continue;
        }
        match &block.terminator {
            MirTerminator::Jump { target, arguments } => validate_jump_target(function, block, *target, arguments, &blocks, &block_map)?,
            MirTerminator::Branch { then_target, else_target, .. } => {
                ensure_target_exists(&function.display_symbol, block.label.as_str(), *then_target, &blocks)?;
                ensure_target_exists(&function.display_symbol, block.label.as_str(), *else_target, &blocks)?;
            }
            MirTerminator::PerformEffect { effect, payload, resume_target, .. } => {
                validate_effect_payload(&function.display_symbol, block.label.as_str(), *effect, payload.is_some())?;
                validate_effect_resume_target(&function.display_symbol, block.label.as_str(), *effect, *resume_target, &blocks)?;
                if let Some(payload_type) = payload.as_ref().and_then(|payload| infer_operand_static_type(function, payload)) {
                    validate_effect_payload_static_type(&function.display_symbol, block.label.as_str(), *effect, &payload_type)?;
                }
                if let (Some(expected_type), Some(resume_block)) = (
                    infer_effect_resume_static_type(
                        *effect,
                        payload.as_ref().and_then(|payload| infer_operand_static_type(function, payload)).as_ref(),
                    ),
                    block_map.get(resume_target),
                ) {
                    validate_resume_block_parameter_static_type(
                        &function.display_symbol,
                        block.label.as_str(),
                        function,
                        *resume_block,
                        &expected_type,
                    )?;
                }
            }
            MirTerminator::StateDispatch { state, cases, default_target } => {
                ensure_target_exists(&function.display_symbol, block.label.as_str(), *default_target, &blocks)?;
                if !function.value_types.contains_key(state) {
                    return Err(HirError::invalid(format!(
                        "控制流调度校验失败：函数 `{}` 的 dispatch block `{}` 引用了未知 state 值",
                        function.display_symbol, block.label
                    )));
                }
                for (case_key, target) in cases {
                    ensure_target_exists(&function.display_symbol, block.label.as_str(), *target, &blocks)?;
                    let _ = case_key;
                }
            }
            // suspend_plan / frame_layouts 不再挂在 Semantic MirFunction 上。
            MirTerminator::YieldToRuntime { effect, payload, resume_state: _ } => {
                validate_effect_payload(&function.display_symbol, block.label.as_str(), *effect, payload.is_some())?;
                if let Some(payload_type) = payload.as_ref().and_then(|payload| infer_operand_static_type(function, payload)) {
                    validate_effect_payload_static_type(&function.display_symbol, block.label.as_str(), *effect, &payload_type)?;
                }
            }
            MirTerminator::Return { .. } | MirTerminator::Unreachable => {}
        }
    }
    Ok(())
}
