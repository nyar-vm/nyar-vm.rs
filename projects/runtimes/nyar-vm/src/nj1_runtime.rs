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
                    } else {
                        lhs.wrapping_div(rhs)
                    }
                }
                I32Binop::RemS => {
                    if rhs == 0 {
                        0
                    } else {
                        lhs.wrapping_rem(rhs)
                    }
                }
            };
            Ok(Value::I32(result))
        }
        ScalarProgram::RetConstI32 { value } => Ok(Value::I32(*value)),
        ScalarProgram::RetI32CmpLocals { cmp, a, b } => {
            let lhs = i32_local(locals, *a)?;
            let rhs = i32_local(locals, *b)?;
            Ok(Value::I32(i32::from(cmp.eval(lhs, rhs))))
        }
        ScalarProgram::RetI32SelectCmpLocals {
            cmp,
            a,
            b,
            then_slot,
            else_slot,
        } => {
            let lhs = i32_local(locals, *a)?;
            let rhs = i32_local(locals, *b)?;
            let slot = if cmp.eval(lhs, rhs) { *then_slot } else { *else_slot };
            local_at(locals, slot).cloned()
        }
    }
}

fn local_at(locals: &[Value], slot: u16) -> Result<&Value, NyarRuntimeError> {
    locals.get(slot as usize).ok_or(NyarRuntimeError::LocalIndexOutOfRange(slot as i32))
}

fn i32_local(locals: &[Value], slot: u16) -> Result<i32, NyarRuntimeError> {
    match local_at(locals, slot)? {
        Value::I32(value) => Ok(*value),
        other => Err(NyarRuntimeError::TypeMismatch {
            expected: "i32",
            actual: other.type_name().to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nyar_jit::{
        I32Cmp, encode_ret_const_i32, encode_ret_i32_binop_locals, encode_ret_i32_cmp_locals, encode_ret_i32_select_cmp_locals,
        encode_ret_local,
    };

    #[test]
    fn executes_ret_local() {
        let blob = encode_ret_local(1);
        let locals = vec![Value::I32(1), Value::I32(99)];
        assert_eq!(execute_nj1_blob(&blob, &locals).unwrap(), Value::I32(99));
    }

    #[test]
    fn executes_ret_i32_binops() {
        let locals = vec![Value::I32(40), Value::I32(2)];
        assert_eq!(
            execute_nj1_blob(&encode_ret_i32_binop_locals(I32Binop::Add, 0, 1), &locals).unwrap(),
            Value::I32(42)
        );
        assert_eq!(
            execute_nj1_blob(&encode_ret_i32_binop_locals(I32Binop::DivS, 0, 1), &locals).unwrap(),
            Value::I32(20)
        );
        assert_eq!(
            execute_nj1_blob(&encode_ret_i32_binop_locals(I32Binop::RemS, 0, 1), &locals).unwrap(),
            Value::I32(0)
        );
        assert_eq!(
            execute_nj1_blob(&encode_ret_i32_binop_locals(I32Binop::DivS, 0, 1), &[Value::I32(7), Value::I32(0)]).unwrap(),
            Value::I32(0)
        );
    }

    #[test]
    fn executes_ret_const_i32() {
        assert_eq!(execute_nj1_blob(&encode_ret_const_i32(99), &[]).unwrap(), Value::I32(99));
    }

    #[test]
    fn executes_ret_i32_cmp_and_select() {
        let locals = vec![Value::I32(1), Value::I32(2), Value::I32(10), Value::I32(20)];
        assert_eq!(
            execute_nj1_blob(&encode_ret_i32_cmp_locals(I32Cmp::LtS, 0, 1), &locals).unwrap(),
            Value::I32(1)
        );
        assert_eq!(
            execute_nj1_blob(&encode_ret_i32_cmp_locals(I32Cmp::Eq, 0, 1), &locals).unwrap(),
            Value::I32(0)
        );
        assert_eq!(
            execute_nj1_blob(&encode_ret_i32_select_cmp_locals(I32Cmp::LtS, 0, 1, 2, 3), &locals).unwrap(),
            Value::I32(10)
        );
        assert_eq!(
            execute_nj1_blob(&encode_ret_i32_select_cmp_locals(I32Cmp::GtS, 0, 1, 2, 3), &locals).unwrap(),
            Value::I32(20)
        );
    }
}
