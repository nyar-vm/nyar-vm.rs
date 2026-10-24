//! Runtime values — re-exported from `nyar-gc` with bytecode constant lowering.

pub use nyar_gc::{CoroutineState, ObjectId, Value};

/// Converts a constant-pool entry into a runtime value.
pub fn value_from_constant(constant: &odin_bytecode::NyarConstant) -> Value {
    match constant {
        odin_bytecode::NyarConstant::Null => Value::Null,
        odin_bytecode::NyarConstant::Boolean(value) => Value::Bool(*value),
        odin_bytecode::NyarConstant::Integer32(value) => Value::I32(*value),
        odin_bytecode::NyarConstant::Float64(value) => Value::F64(*value),
        odin_bytecode::NyarConstant::String(value) => Value::String(value.clone()),
        odin_bytecode::NyarConstant::BigInt(_) => Value::Null,
    }
}
