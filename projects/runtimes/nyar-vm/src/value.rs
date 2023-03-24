//! Runtime values — re-exported from `nyar-gc` with bytecode constant lowering.

pub use nyar_gc::{CoroutineState, ObjectId, Value};

/// Converts a constant-pool entry into a runtime value.
pub fn value_from_constant(constant: &nyar_format::NyarConstant) -> Value {
    match constant {
        nyar_format::NyarConstant::Null => Value::Null,
        nyar_format::NyarConstant::Boolean(value) => Value::Bool(*value),
        nyar_format::NyarConstant::Integer32(value) => Value::I32(*value),
        nyar_format::NyarConstant::Float64(value) => Value::F64(*value),
        nyar_format::NyarConstant::String(value) => Value::String(value.clone()),
        nyar_format::NyarConstant::BigInt(_) => Value::Null,
    }
}
