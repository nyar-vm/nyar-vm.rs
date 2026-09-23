use std::fmt::{Display, Formatter};

use miette::Diagnostic;

/// Runtime error raised while executing Nyar bytecode.
#[derive(Debug, Clone, PartialEq, Eq, Diagnostic)]
pub enum NyarRuntimeError {
    /// Stack underflow.
    StackUnderflow,
    /// Unknown or unsupported opcode.
    UnknownOpcode(u8),
    /// Function index out of range.
    FunctionIndexOutOfRange(i32),
    /// Local variable index out of range.
    LocalIndexOutOfRange(i32),
    /// Global slot index out of range.
    GlobalIndexOutOfRange(i32),
    /// Constant pool index out of range.
    ConstantIndexOutOfRange(i32),
    /// Entry function not found.
    EntryNotFound(String),
    /// Native function not registered.
    NativeNotRegistered(String),
    /// Type mismatch at runtime.
    TypeMismatch {
        /// Expected type name.
        expected: &'static str,
        /// Actual type name.
        actual: String,
    },
    /// Module load failure.
    ModuleLoad(String),
}

impl Display for NyarRuntimeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StackUnderflow => write!(f, "stack underflow"),
            Self::UnknownOpcode(op) => write!(f, "unknown opcode: 0x{op:02X}"),
            Self::FunctionIndexOutOfRange(idx) => write!(f, "function index out of range: {idx}"),
            Self::LocalIndexOutOfRange(idx) => write!(f, "local index out of range: {idx}"),
            Self::GlobalIndexOutOfRange(idx) => write!(f, "global index out of range: {idx}"),
            Self::ConstantIndexOutOfRange(idx) => write!(f, "constant index out of range: {idx}"),
            Self::EntryNotFound(name) => write!(f, "entry function not found: {name}"),
            Self::NativeNotRegistered(name) => write!(f, "native function not registered: {name}"),
            Self::TypeMismatch { expected, actual } => {
                write!(f, "type mismatch: expected {expected}, got {actual}")
            }
            Self::ModuleLoad(message) => write!(f, "failed to load module: {message}"),
        }
    }
}

impl std::error::Error for NyarRuntimeError {}
