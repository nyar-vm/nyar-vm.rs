use std::fmt::{Display, Formatter};

/// Heap object identifier.
pub type ObjectId = usize;

/// Suspended coroutine state captured by `Yield` and restored by `Resume`.
///
/// Holds the minimal frame snapshot needed to resume execution: the function index,
/// the instruction pointer at the yield point (already advanced past the Yield opcode),
/// the local variable slots, the operand stack depth at suspension time, and the most
/// recently yielded value (embedded so a single `Coroutine` value fully represents
/// the suspended state observed by the caller of `Call` or `Resume`).
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
    /// Whether the coroutine has completed (returned) and should not be resumed again.
    ///
    /// Mutated to `true` by the executor when a coroutine frame runs to `Return`. Because
    /// `Value::Coroutine` holds a heap `ObjectId` (not an inline snapshot), updates to
    /// `done` are visible to every stack/local copy that references the same coroutine —
    /// this is what allows the `Resume` handler to reject double-resume attempts.
    pub done: bool,
    /// The value yielded to the caller by the most recent `Yield`.
    ///
    /// Embedded into the coroutine so the caller retrieves both the suspended frame
    /// and its yielded payload via a single `Coroutine` value on the operand stack.
    /// When the coroutine completes, this field is overwritten with the final return
    /// value so a single `Coroutine` value always carries the most recent observable
    /// payload — whether yielded or returned.
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
    /// First-class coroutine referencing a heap-allocated `CoroutineState`.
    ///
    /// Backed by `ObjectId` rather than an inline `Box<CoroutineState>` so that the
    /// `done` flag and `yielded_value` mutations performed by the executor on resume /
    /// completion are shared across every stack and local copy that holds the same
    /// coroutine — without this sharing, the `done` guard in `Resume` could not reject
    /// double-resume attempts issued from a stale local snapshot.
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

    /// Converts a constant-pool entry into a runtime value.
    pub fn from_constant(constant: &std_data::binary::nyar_ir::NyarConstant) -> Self {
        match constant {
            std_data::binary::nyar_ir::NyarConstant::Null => Self::Null,
            std_data::binary::nyar_ir::NyarConstant::Boolean(value) => Self::Bool(*value),
            std_data::binary::nyar_ir::NyarConstant::Integer32(value) => Self::I32(*value),
            std_data::binary::nyar_ir::NyarConstant::Float64(value) => Self::F64(*value),
            std_data::binary::nyar_ir::NyarConstant::String(value) => Self::String(value.clone()),
            std_data::binary::nyar_ir::NyarConstant::BigInt(_) => Self::Null,
        }
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
