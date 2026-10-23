//! effect 恢复目标、载荷与静态类型校验。

use super::{
    control_flow::ensure_target_exists,
    diagnostics::{display_type, expected_effect_resume_parameter_count, payload_required},
    errors::MirValidationError,
    type_inference::{future_resume_type, generator_source_type},
};
use crate::{
    mir::{MirBlock, MirBlockRef, MirEffectKind, MirFunction},
    types::hir::ValkyrieType,
};
use std::collections::BTreeMap;

pub(super) fn validate_effect_resume_target(
    function_name: &str,
    block_label: &str,
    effect: MirEffectKind,
    resume_target: MirBlockRef,
    blocks: &BTreeMap<MirBlockRef, usize>,
) -> Result<(), MirValidationError> {
    ensure_target_exists(function_name, block_label, resume_target, blocks)?;
    let actual_parameter_count = blocks.get(&resume_target).copied().unwrap_or_default();
    let expected_parameter_count = expected_effect_resume_parameter_count(effect);
    if actual_parameter_count != expected_parameter_count {
        return Err(MirValidationError::EffectResumeParameterCountMismatch {
            function: function_name.to_string(),
            block: block_label.to_string(),
            expected: expected_parameter_count,
            actual: actual_parameter_count,
        });
    }
    Ok(())
}

pub(super) fn validate_effect_payload(
    function_name: &str,
    block_label: &str,
    effect: MirEffectKind,
    has_payload: bool,
) -> Result<(), MirValidationError> {
    if payload_required(effect) || has_payload {
        if has_payload {
            return Ok(());
        }
        if payload_required(effect) {
            return Err(MirValidationError::EffectPayloadMissing { function: function_name.to_string(), block: block_label.to_string() });
        }
    }
    Ok(())
}

pub(super) fn validate_effect_payload_static_type(
    function_name: &str,
    block_label: &str,
    effect: MirEffectKind,
    payload_type: &ValkyrieType,
) -> Result<(), MirValidationError> {
    if matches!(effect, MirEffectKind::Await | MirEffectKind::AsyncSpawn | MirEffectKind::AsyncBlock)
        && future_resume_type(payload_type).is_none()
    {
        return Err(MirValidationError::EffectPayloadFutureTypeMismatch {
            function: function_name.to_string(),
            block: block_label.to_string(),
            effect,
            payload_type: payload_type.clone(),
        });
    }
    if matches!(effect, MirEffectKind::DelegateYield) && generator_source_type(payload_type).is_none() {
        return Err(MirValidationError::EffectPayloadGeneratorTypeMismatch {
            function: function_name.to_string(),
            block: block_label.to_string(),
            payload_type: payload_type.clone(),
        });
    }
    Ok(())
}

pub(super) fn validate_resume_block_parameter_static_type(
    function_name: &str,
    block_label: &str,
    function: &MirFunction,
    resume_block: &MirBlock,
    expected_type: &ValkyrieType,
) -> Result<(), MirValidationError> {
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
    Err(MirValidationError::ResumeParameterTypeMismatch {
        function: function_name.to_string(),
        block: block_label.to_string(),
        actual_type: actual_type.clone(),
        expected_type: expected_type.clone(),
    })
}
