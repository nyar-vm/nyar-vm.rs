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
            };
            Ok(Value::I32(result))
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
    use nyar_jit::{encode_ret_i32_binop_locals, encode_ret_local};

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
            execute_nj1_blob(&encode_ret_i32_binop_locals(I32Binop::Sub, 0, 1), &locals).unwrap(),
            Value::I32(38)
        );
        assert_eq!(
            execute_nj1_blob(&encode_ret_i32_binop_locals(I32Binop::Mul, 0, 1), &locals).unwrap(),
            Value::I32(80)
        );
    }
}
