use std_data::binary::nyar_ir::NyarInstruction;

use crate::{
    error::NyarRuntimeError,
    frame::Frame,
    ops::{ExecutionContext, StepResult},
};

/// Object-related opcode handlers (stub).
pub fn execute_object(
    instruction: NyarInstruction,
    _frame: &mut Frame,
    _ctx: &mut ExecutionContext<'_>,
) -> Result<StepResult, NyarRuntimeError> {
    Err(NyarRuntimeError::UnknownOpcode(instruction.code as u8))
}
