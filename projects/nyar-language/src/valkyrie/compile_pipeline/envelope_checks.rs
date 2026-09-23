//! Minimal M2 envelope checks (result counts / id density) — not full MIR verification.
//!
//! Full ADT / Invoke / effect closure lives later. This only guards ADR 0012 envelope
//! invariants so the analysis stream has a fail-closed hook.

use nyar_types::{CompileStage, StageResult};

use crate::valkyrie::mir::{MirFunction, MirInstruction, MirOperation};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::valkyrie::mir::{MirConstant, MirOperand, MirValueRef};
    use nyar_types::InstructionId;

    #[test]
    fn store_var_rejects_results() {
        let insn = MirInstruction {
            id: InstructionId::from_index(0).unwrap(),
            results: vec![MirValueRef(0)],
            kind: MirOperation::StoreVar { name: "x".into(), value: MirOperand::Constant(MirConstant::Int(1)), ty: None },
            provenance: nyar_types::ProvenanceId::from_index(0).unwrap(),
        };
        assert!(check_instruction_envelope("m", &insn).is_err());
    }

    #[test]
    fn load_constant_accepts_one_result() {
        let insn = MirInstruction {
            id: InstructionId::from_index(1).unwrap(),
            results: vec![MirValueRef(0)],
            kind: MirOperation::LoadConstant { constant: MirConstant::Int(1), ty: None },
            provenance: nyar_types::ProvenanceId::from_index(0).unwrap(),
        };
        assert!(check_instruction_envelope("m", &insn).is_ok());
    }
}
