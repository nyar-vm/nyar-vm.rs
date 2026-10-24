//! 解释执行 NJ1 基线标量机器码（非原生可执行页）。

use nyar_jit::{I32Binop, ScalarProgram, decode_scalar_program};

use crate::{error::NyarRuntimeError, value::Value};

/// 在已填好的 local 槽上执行 NJ1 blob，返回结果值。
pub fn execute_nj1_blob(blob: &[u8], locals: &[Value]) -> Result<Value, NyarRuntimeError> {
    let program = decode_scalar_program(blob).map_err(|error| {
        NyarRuntimeError::UnsupportedFeature(match error {
            nyar_jit::MachineCodeError::InvalidBlob => "invalid NJ1 machine-code blob",
            nyar_jit::MachineCodeError::UnknownOpcode(_) => "unknown NJ1 opcode",
        })
    })?;
    execute_scalar_program(&program, locals)
}

/// 执行已解码的标量程序。
pub fn execute_scalar_program(program: &ScalarProgram, locals: &[Value]) -> Result<Value, NyarRuntimeError> {
    match program {
        ScalarProgram::RetLocal { slot } => local_at(locals, *slot).cloned(),
        ScalarProgram::RetI32BinopLocals { binop, a, b } => {
            let lhs = i32_local(locals, *a)?;
            let rhs = i32_local(locals, *b)?;
            let result = match binop {
                I32Binop::Add => lhs.wrapping_add(rhs),
                I32Binop::Sub => lhs.wrapping_sub(rhs),
                I32Binop::Mul => lhs.wrapping_mul(rhs),
                I32Binop::DivS => {
                    if rhs == 0 {
                        0
                    }
                    else {
                        lhs.wrapping_div(rhs)
                    }
                }
                I32Binop::RemS => {
                    if rhs == 0 {
                        0
                    }
                    else {
                        lhs.wrapping_rem(rhs)
                    }
                }
            };
            Ok(Value::I32(result))
        }
        ScalarProgram::RetConstI32 { value } => Ok(Value::I32(*value)),
        ScalarProgram::RetI32BinopImmLocal { binop, imm, local, imm_on_left } => {
            let slot = i32_local(locals, *local)?;
            let (lhs, rhs) = if *imm_on_left { (*imm, slot) } else { (slot, *imm) };
            let result = match binop {
                I32Binop::Add => lhs.wrapping_add(rhs),
                I32Binop::Sub => lhs.wrapping_sub(rhs),
                I32Binop::Mul => lhs.wrapping_mul(rhs),
                I32Binop::DivS => {
                    if rhs == 0 {
                        0
                    }
                    else {
                        lhs.wrapping_div(rhs)
                    }
                }
                I32Binop::RemS => {
                    if rhs == 0 {
                        0
                    }
                    else {
                        lhs.wrapping_rem(rhs)
                    }
                }
            };
            Ok(Value::I32(result))
        }
        ScalarProgram::RetI32CmpImmLocal { cmp, imm, local, imm_on_left } => {
            let slot = i32_local(locals, *local)?;
            let (lhs, rhs) = if *imm_on_left { (*imm, slot) } else { (slot, *imm) };
            Ok(Value::I32(i32::from(cmp.eval(lhs, rhs))))
        }
        ScalarProgram::RetI32CmpLocals { cmp, a, b } => {
            let lhs = i32_local(locals, *a)?;
            let rhs = i32_local(locals, *b)?;
            Ok(Value::I32(i32::from(cmp.eval(lhs, rhs))))
        }
        ScalarProgram::RetI32SelectCmpLocals { cmp, a, b, then_slot, else_slot } => {
            let lhs = i32_local(locals, *a)?;
            let rhs = i32_local(locals, *b)?;
            let slot = if cmp.eval(lhs, rhs) { *then_slot } else { *else_slot };
            local_at(locals, slot).cloned()
        }
        ScalarProgram::RetI32SelectCmpConsts { cmp, a, b, then_imm, else_imm } => {
            let lhs = i32_local(locals, *a)?;
            let rhs = i32_local(locals, *b)?;
            Ok(Value::I32(if cmp.eval(lhs, rhs) { *then_imm } else { *else_imm }))
        }
        ScalarProgram::RetI32SelectCmpMixed { cmp, a, b, then_arm, else_arm } => {
            let lhs = i32_local(locals, *a)?;
            let rhs = i32_local(locals, *b)?;
            let arm = if cmp.eval(lhs, rhs) { then_arm } else { else_arm };
            match arm {
                nyar_jit::SelectArm::Local(slot) => local_at(locals, *slot).cloned(),
                nyar_jit::SelectArm::Imm(value) => Ok(Value::I32(*value)),
            }
        }
        ScalarProgram::RetVoid => Ok(Value::Null),
    }
}

fn local_at(locals: &[Value], slot: u16) -> Result<&Value, NyarRuntimeError> {
    locals.get(slot as usize).ok_or(NyarRuntimeError::LocalIndexOutOfRange(slot as i32))
}

fn i32_local(locals: &[Value], slot: u16) -> Result<i32, NyarRuntimeError> {
    match local_at(locals, slot)? {
        Value::I32(value) => Ok(*value),
        other => Err(NyarRuntimeError::TypeMismatch { expected: "i32", actual: other.type_name().to_string() }),
    }
}
