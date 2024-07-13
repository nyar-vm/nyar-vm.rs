//! 可达块、跳转目标与跳转形状校验。

use super::{diagnostics::display_type, type_inference::infer_operand_static_type};
use crate::{
    mir::{MirBlock, MirBlockRef, MirFunction, MirOperand, MirTerminator},
    valkyrie::frontend::ParseError,
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn collect_reachable_blocks(function: &MirFunction) -> BTreeSet<MirBlockRef> {
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
            MirTerminator::Jump { target, .. } => worklist.push(*target),
            MirTerminator::Branch { then_target, else_target, .. } => {
                worklist.push(*then_target);
                worklist.push(*else_target);
            }
            MirTerminator::PerformEffect { resume_target, .. } => worklist.push(*resume_target),
            MirTerminator::StateDispatch { cases, default_target, .. } => {
                worklist.push(*default_target);
                for (_, target) in cases {
                    worklist.push(*target);
                }
            }
            MirTerminator::YieldToRuntime { .. } => {}
            MirTerminator::Return { .. } | MirTerminator::Unreachable => {}
        }
    }

    reachable
}

pub(super) fn validate_jump_target(
    function: &MirFunction,
    block: &MirBlock,
    target: MirBlockRef,
    arguments: &[MirOperand],
    blocks: &BTreeMap<MirBlockRef, usize>,
    block_map: &BTreeMap<MirBlockRef, &MirBlock>,
) -> Result<(), ParseError> {
    validate_jump_shape(&function.display_symbol, block.label.as_str(), target, arguments.len(), blocks)?;
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
                "控制流调度校验失败：`MIR` 函数 `{}` 的 block `{}` 跳向 `{}` 时，第 {} 个 Jump 参数类型为 `{}`，目标参数类型为 `{}`",
                function.display_symbol,
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
pub(super) fn validate_jump_shape(
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

pub(super) fn ensure_target_exists(
    function_name: &str,
    block_label: &str,
    target: MirBlockRef,
    blocks: &BTreeMap<MirBlockRef, usize>,
) -> Result<(), ParseError> {
    if blocks.contains_key(&target) {
        Ok(())
    }
    else {
        Err(ParseError::invalid(format!("控制流调度校验失败：函数 `{function_name}` �?block `{block_label}` 指向了不存在的目标块")))
    }
}
