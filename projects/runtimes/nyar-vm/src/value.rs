//! Runtime values — re-exported from `nyar-gc` with bytecode constant lowering.

pub use nyar_gc::{CoroutineState, ObjectId, Value};

/// Converts a constant-pool entry into a runtime value.
pub fn value_from_constant(constant: &std_data::binary::nyar_ir::NyarConstant) -> Value {
    match constant {
        std_data::binary::nyar_ir::NyarConstant::Null => Value::Null,
        std_data::binary::nyar_ir::NyarConstant::Boolean(value) => Value::Bool(*value),
        std_data::binary::nyar_ir::NyarConstant::Integer32(value) => Value::I32(*value),
        std_data::binary::nyar_ir::NyarConstant::Float64(value) => Value::F64(*value),
        std_data::binary::nyar_ir::NyarConstant::String(value) => Value::String(value.clone()),
        std_data::binary::nyar_ir::NyarConstant::BigInt(_) => Value::Null,
    }
}
