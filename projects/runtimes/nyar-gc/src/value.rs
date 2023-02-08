use std::fmt::{Display, Formatter};

/// Heap object identifier.
pub type ObjectId = usize;

/// Suspended coroutine state captured by `Yield` and restored by `Resume`.
#[derive(Debug, Clone, PartialEq)]
pub struct CoroutineState {
    /// Function index in the owning module.
    pub function_index: usize,
    /// Instruction pointer to resume from (already advanced past the `Yield` opcode).
    pub ip: usize,
    /// Captured local variable slots.
    pub locals: Vec<Value>,
    /// Operand stack depth at the time of suspension.
    pub stack_base: usize,
    /// Whether the coroutine has completed and must not be resumed again.
    pub done: bool,
    /// Most recently yielded or final return value.
    pub yielded_value: Value,
}

/// Runtime value carried on the operand stack and in locals.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Null value.
    Null,
    /// Boolean value.
    Bool(bool),
    /// 32-bit signed integer.
    I32(i32),
    /// 64-bit signed integer.
    I64(i64),
    /// 32-bit float.
    F32(f32),
    /// 64-bit float.
    F64(f64),
    /// UTF-8 string.
    String(String),
    /// Heap-allocated object reference.
    Object(ObjectId),
    /// Heap-backed coroutine reference.
    Coroutine(ObjectId),
}

impl Value {
    /// Returns a human-readable type name.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::I32(_) => "i32",
            Self::I64(_) => "i64",
            Self::F32(_) => "f32",
            Self::F64(_) => "f64",
            Self::String(_) => "string",
            Self::Object(_) => "object",
            Self::Coroutine(_) => "coroutine",
        }
    }

    /// Converts the value to a boolean for control-flow instructions.
    pub fn to_bool(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(value) => *value,
            Self::I32(value) => *value != 0,
            Self::I64(value) => *value != 0,
            Self::F32(value) => *value != 0.0,
            Self::F64(value) => *value != 0.0,
            Self::String(value) => !value.is_empty(),
            Self::Object(_) => true,
            Self::Coroutine(_) => true,
        }
    }

    /// Returns heap object ids referenced by this value (shallow).
    pub fn heap_ids(&self) -> impl Iterator<Item = ObjectId> + use<> {
        match self {
            Self::Object(id) | Self::Coroutine(id) => Some(*id),
            _ => None,
        }
        .into_iter()
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Null => write!(f, "null"),
            Self::Bool(value) => write!(f, "{value}"),
            Self::I32(value) => write!(f, "{value}"),
            Self::I64(value) => write!(f, "{value}"),
            Self::F32(value) => write!(f, "{value}"),
            Self::F64(value) => write!(f, "{value}"),
            Self::String(value) => write!(f, "{value}"),
            Self::Object(id) => write!(f, "object#{id}"),
            Self::Coroutine(id) => write!(f, "coroutine#{id}"),
        }
    }
}
