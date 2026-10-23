//! effect 恢复目标、载荷与静态类型校验。

use super::{
    control_flow::ensure_target_exists,
    diagnostics::{display_type, expected_effect_resume_parameter_count, payload_required},
    type_inference::{future_resume_type, generator_source_type},
};
use crate::{
    mir::{MirBlock, MirBlockRef, MirEffectKind, MirFunction},
    types::hir::ValkyrieType,
    valkyrie::frontend::ParseError,
};
use std::collections::BTreeMap;

pub(super) fn validate_effect_resume_target(
    function_name: &str,
    block_label: &str,
    effect: MirEffectKind,
    resume_target: MirBlockRef,
    blocks: &BTreeMap<MirBlockRef, usize>,
) -> Result<(), ParseError> {
    ensure_target_exists(function_name, block_label, resume_target, blocks)?;
    let actual_parameter_count = blocks.get(&resume_target).copied().unwrap_or_default();
    let expected_parameter_count = expected_effect_resume_parameter_count(effect);
    if actual_parameter_count != expected_parameter_count {
        return Err(ParseError::Invalid { message: format!(
            "控制流调度校验失败：`MIR` 函数 `{function_name}` 的 block `{block_label}` 所指向的 effect 恢复点参数个数不合法，期望 {expected_parameter_count} 个，当前为 {actual_parameter_count}"
        ), span: None });
    }
    Ok(())
}

pub(super) fn validate_effect_payload(
    function_name: &str,
    block_label: &str,
    effect: MirEffectKind,
    has_payload: bool,
) -> Result<(), ParseError> {
    if payload_required(effect) || has_payload {
        if has_payload {
            return Ok(());
        }
        if payload_required(effect) {
            return Err(ParseError::Invalid { message: format!(
                "控制流调度校验失败：`MIR` 函数 `{function_name}` 的 block `{block_label}` 缺少 effect payload"
            ), span: None });
        }
    }
    Ok(())
}

pub(super) fn validate_effect_payload_static_type(
    function_name: &str,
    block_label: &str,
    effect: MirEffectKind,
    payload_type: &ValkyrieType,
) -> Result<(), ParseError> {
    if matches!(effect, MirEffectKind::Await | MirEffectKind::AsyncSpawn | MirEffectKind::AsyncBlock)
        && future_resume_type(payload_type).is_none()
    {
        let effect_name = match effect {
            MirEffectKind::Await => "`await`",
            MirEffectKind::AsyncSpawn => "`awake`",
            MirEffectKind::AsyncBlock => "`block`",
            _ => unreachable!(),
        };
        return Err(ParseError::Invalid { message: format!(
            "控制流调度校验失败：`MIR` 函数 `{function_name}` 的 block `{block_label}` 的 {effect_name} effect payload 类型 `{}` 不满足 `Future<T>` / `Promise<T>` 形状",
            display_type(payload_type)
        ), span: None });
    }
    if matches!(effect, MirEffectKind::DelegateYield) && generator_source_type(payload_type).is_none() {
        return Err(ParseError::Invalid { message: format!(
            "控制流调度校验失败：`MIR` 函数 `{function_name}` 的 block `{block_label}` 的 `yield from` effect payload 类型 `{}` 不满足 `Generator<T>` / `Iterator<T>` 形状",
            display_type(payload_type)
        ), span: None });
    }
    Ok(())
}

pub(super) fn validate_resume_block_parameter_static_type(
    function_name: &str,
    block_label: &str,
    function: &MirFunction,
    resume_block: &MirBlock,
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
    Err(ParseError::Invalid { message: format!(
        "控制流调度校验失败：`MIR` 函数 `{function_name}` 的 block `{block_label}` 所指向恢复点参数类型为 `{}`，期望 `{}`",
        display_type(actual_type),
        display_type(expected_type)
    ), span: None })
}
