use std_data::binary::nyar_ir::{NyarHeadCode, NyarInstruction};

use crate::{error::NyarRuntimeError, frame::Frame, ops::StepResult, stack::ValueStack, value::Value};

/// Integer arithmetic and comparison opcode handlers.
pub fn execute_arithmetic(instruction: NyarInstruction, frame: &mut Frame, stack: &mut ValueStack) -> Result<StepResult, NyarRuntimeError> {
    match instruction.code {
        NyarHeadCode::I32Add | NyarHeadCode::I32Sub | NyarHeadCode::I32Mul | NyarHeadCode::I32DivS | NyarHeadCode::I32RemS => {
            let rhs = pop_i32(stack)?;
            let lhs = pop_i32(stack)?;
            let result = match instruction.code {
                NyarHeadCode::I32Add => lhs.wrapping_add(rhs),
                NyarHeadCode::I32Sub => lhs.wrapping_sub(rhs),
                NyarHeadCode::I32Mul => lhs.wrapping_mul(rhs),
                NyarHeadCode::I32DivS => {
                    if rhs == 0 {
                        0
                    }
                    else {
                        lhs.wrapping_div(rhs)
                    }
                }
                NyarHeadCode::I32RemS => {
                    if rhs == 0 {
                        0
                    }
                    else {
                        lhs.wrapping_rem(rhs)
                    }
                }
                _ => unreachable!(),
            };
            stack.push(Value::I32(result));
        }
        NyarHeadCode::I32Eq
        | NyarHeadCode::I32Ne
        | NyarHeadCode::I32LtS
        | NyarHeadCode::I32LeS
        | NyarHeadCode::I32GtS
        | NyarHeadCode::I32GeS => {
            let rhs = pop_i32(stack)?;
            let lhs = pop_i32(stack)?;
            let result = match instruction.code {
                NyarHeadCode::I32Eq => lhs == rhs,
                NyarHeadCode::I32Ne => lhs != rhs,
                NyarHeadCode::I32LtS => lhs < rhs,
                NyarHeadCode::I32LeS => lhs <= rhs,
                NyarHeadCode::I32GtS => lhs > rhs,
                NyarHeadCode::I32GeS => lhs >= rhs,
                _ => unreachable!(),
            };
            stack.push(Value::Bool(result));
        }
        _ => return Err(NyarRuntimeError::UnknownOpcode(instruction.code as u8)),
    }

    frame.ip += instruction.size as usize;
    Ok(StepResult::Continue)
}

fn pop_i32(stack: &mut ValueStack) -> Result<i32, NyarRuntimeError> {
    match stack.pop()? {
        Value::I32(value) => Ok(value),
        other => Err(NyarRuntimeError::TypeMismatch { expected: "i32", actual: other.type_name().to_string() }),
    }
}
