//! 最小 M2 信封检查（结果个数 / id 稠密度）— 非完整 MIR 校验。
//!
//! 完整 ADT / Invoke / effect 闭包稍后。此处只守信封不变量，
//! 让分析流有失败关闭钩子。

use nyar_types::{CompileStage, StageResult};

use crate::mir::{MirFunction, MirInstruction, MirOperation};

use super::diagnostics::fail_stage;

/// Result-arity contract for known operations (0 = void / store-like).
pub fn expected_result_count(operation: &MirOperation) -> Option<usize> {
    match operation {
        MirOperation::StoreVar { .. } => Some(0),
        MirOperation::LoadConstant { .. } | MirOperation::LoadSymbol { .. } | MirOperation::Copy { .. } | MirOperation::Call { .. } => Some(1),
        // Unknown / transitional ops: skip until Invoke + Sum* contracts land.
        _ => None,
    }
}

/// Fail-closed envelope check for a single instruction.
pub fn check_instruction_envelope(module: &str, instruction: &MirInstruction) -> StageResult<()> {
    if let Some(expected) = expected_result_count(instruction.operation()) {
        if instruction.results.len() != expected {
            return fail_stage(
                CompileStage::ValidateMir,
                "SMIR-ENV01",
                module,
                format!(
                    "instruction {} result count {} != expected {} for {:?}",
                    instruction.id.index(),
                    instruction.results.len(),
                    expected,
                    instruction.operation()
                ),
            );
        }
    }
    Ok(())
}

/// Scan a function body for envelope contract violations (stub M2 entry).
pub fn check_function_envelopes(module: &str, function: &MirFunction) -> StageResult<()> {
    for block in &function.blocks {
        for instruction in &block.instructions {
            check_instruction_envelope(module, instruction)?;
        }
    }
    Ok(())
}
