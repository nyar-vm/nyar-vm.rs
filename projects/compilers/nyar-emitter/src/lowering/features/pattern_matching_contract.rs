//! 后端模式匹配最小契约：MIR 已显式化 probe/bind/branch，后端只需处理
//! `MirTerminator::Branch` / `MirTerminator::Jump` 与 `MirInstructionKind::{Call,
//! FieldGet, AggregateCopy}` 等低层指令，不得重新解释高层 pattern。
//!
//! 本模块提供 [`validate_pattern_matching_invariants`]，由各后端 lowering 入口
//! 调用以校验 `MirCaseChain` 元数据一致性，确保后端不会因残留的高层 pattern
//! 语义或 dangling block 引用而生成错误代码。

use crate::{
    contracts::{CaseArm as MirCaseArm, CaseChain as MirCaseChain},
    executable_provider::{
        ExecutableBlockRef as MirBlockRef, ExecutableFunction as MirFunction, ExecutableInstructionKind as MirInstructionKind,
    },
};

/// 后端模式匹配不变量校验失败时返回的错误。
///
/// 后端在 lowering 入口处调用 [`validate_pattern_matching_invariants`]；若 MIR 残留
/// 了高层 `PatternMatch` 指令，或 `MirCaseChain` 引用了不存在的 block、fallthrough
/// 目标非法，则返回本错误。后端应将其转为诊断或在 debug 构建下 panic，避免生成
/// 语义错误的代码。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternMatchingContractError {
    /// MIR 中残留了 `MirInstructionKind::PatternMatch` 指令。
    ///
    /// 该指令应由 MIR 层（Task 5.4）拆解为显式的 probe/bind/branch；若后端仍见到它，
    /// 说明 MIR lowering 未完成，后端不得自行解释 pattern。
    ResidualPatternMatchInstruction {
        /// 残留指令所在 block 的 id。
        block: MirBlockRef,
    },
    /// `MirCaseChain` 引用了不存在的 block。
    ///
    /// `dispatch_block` / `first_arm` / `no_match_block` / `exit_block` 或某个 arm 的
    /// `entry_block` / `check_block` / `guard_block` / `body_block` /
    /// `next_arm_target` / `exit_target` 字段指向的 `MirBlockRef` 在
    /// `MirFunction::blocks` 中不存在。
    InvalidCaseChainLabel {
        /// 出错的 case chain 在 `MirFunction::case_chains` 中的索引。
        chain_index: usize,
        /// 出错的 arm 在 chain 中的索引；若是 chain 顶层字段出错则为 `None`。
        arm_index: Option<usize>,
        /// 被引用但不存在的 block id。
        block: MirBlockRef,
    },
    /// `MirCaseArm::fallthrough_target` 不指向下一个 arm 的 entry。
    ///
    /// fallthrough 语义要求 arm[i] 的 `fallthrough_target`（若存在）等于 arm[i+1] 的
    /// `entry_block`；否则后端无法正确衔接 case 链。最后一个 arm 不应有 fallthrough
    /// 目标（`expected` 为 `None`）。
    InvalidFallthroughTarget {
        /// 出错的 case chain 在 `MirFunction::case_chains` 中的索引。
        chain_index: usize,
        /// 出错的 arm 在 chain 中的索引。
        arm_index: usize,
        /// 实际的 `fallthrough_target` 值。
        actual: MirBlockRef,
        /// 期望的目标：`Some(next_arm.entry_block)` 表示应指向下一 arm entry，
        /// `None` 表示该 arm 是最后一个 arm，不应有 fallthrough。
        expected: Option<MirBlockRef>,
    },
}

/// 校验后端模式匹配最小契约的不变量。
///
/// 该函数只做“元数据一致性”校验，不重新解释 pattern：
/// - 检查 MIR 中是否残留 `MirInstructionKind::PatternMatch` 指令（MIR 层应已拆解）；
/// - 检查每条 `MirCaseChain` 引用的 block id 都在 `function.blocks` 中存在；
/// - 检查 arm 的 `fallthrough_target`（若存在）指向下一个 arm 的 `entry_block`，
///   且最后一个 arm 不带 fallthrough。
///
/// 后端 lowering 入口应调用本函数；若返回 `Err`，应 emit 诊断或 panic（debug 下）。
pub fn validate_pattern_matching_invariants(function: &MirFunction) -> Result<(), PatternMatchingContractError> {
    for block in &function.blocks {
        for instruction in &block.instructions {
            if matches!(instruction.kind, MirInstructionKind::PatternMatch { .. }) {
                return Err(PatternMatchingContractError::ResidualPatternMatchInstruction { block: block.id });
            }
        }
    }

    for (chain_index, chain) in function.case_chains.iter().enumerate() {
        validate_case_chain(function, chain, chain_index)?;
    }

    Ok(())
}

fn validate_case_chain(function: &MirFunction, chain: &MirCaseChain, chain_index: usize) -> Result<(), PatternMatchingContractError> {
    for block_ref in [chain.dispatch_block, chain.first_arm, chain.no_match_block, chain.exit_block] {
        if !block_exists(function, block_ref) {
            return Err(PatternMatchingContractError::InvalidCaseChainLabel { chain_index, arm_index: None, block: block_ref });
        }
    }

    for (arm_index, arm) in chain.arms.iter().enumerate() {
        validate_arm(function, chain, arm, chain_index, arm_index)?;
    }

    Ok(())
}

fn validate_arm(
    function: &MirFunction,
    chain: &MirCaseChain,
    arm: &MirCaseArm,
    chain_index: usize,
    arm_index: usize,
) -> Result<(), PatternMatchingContractError> {
    let required_blocks = [arm.entry_block, arm.body_block, arm.next_arm_target, arm.exit_target];
    for block_ref in required_blocks.iter().copied() {
        if !block_exists(function, block_ref) {
            return Err(PatternMatchingContractError::InvalidCaseChainLabel { chain_index, arm_index: Some(arm_index), block: block_ref });
        }
    }
    for block_ref in arm.check_block.into_iter().chain(arm.guard_block) {
        if !block_exists(function, block_ref) {
            return Err(PatternMatchingContractError::InvalidCaseChainLabel { chain_index, arm_index: Some(arm_index), block: block_ref });
        }
    }

    if let Some(fallthrough) = arm.fallthrough_target {
        match chain.arms.get(arm_index + 1) {
            Some(next_arm) => {
                if fallthrough != next_arm.entry_block {
                    return Err(PatternMatchingContractError::InvalidFallthroughTarget {
                        chain_index,
                        arm_index,
                        actual: fallthrough,
                        expected: Some(next_arm.entry_block),
                    });
                }
            }
            None => {
                return Err(PatternMatchingContractError::InvalidFallthroughTarget {
                    chain_index,
                    arm_index,
                    actual: fallthrough,
                    expected: None,
                });
            }
        }
    }

    Ok(())
}

fn block_exists(function: &MirFunction, block_ref: MirBlockRef) -> bool {
    function.blocks.get(block_ref.0 as usize).is_some()
}
