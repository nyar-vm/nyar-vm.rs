//! Runtime values — re-exported from `nyar-gc` with bytecode constant lowering.

pub use nyar_gc::{CoroutineState, ObjectId, Value};

/// Converts a constant-pool entry into a runtime value.
pub fn value_from_constant(constant: &nyar_bytecode::NyarConstant) -> Value {
    match constant {
        nyar_bytecode::NyarConstant::Null => Value::Null,
        nyar_bytecode::NyarConstant::Boolean(value) => Value::Bool(*value),
        nyar_bytecode::NyarConstant::Integer32(value) => Value::I32(*value),
        nyar_bytecode::NyarConstant::Float64(value) => Value::F64(*value),
        nyar_bytecode::NyarConstant::String(value) => Value::String(value.clone()),
        nyar_bytecode::NyarConstant::BigInt(_) => Value::Null,
    }
}
