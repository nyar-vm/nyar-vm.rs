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
    /// Import table index out of range.
    ImportIndexOutOfRange(i32),
    /// Layouts 表下标越界（`ObjectNew`）。
    LayoutIndexOutOfRange(i32),
    /// 字段槽越界（`FieldGet` / `FieldSet`）。
    FieldSlotOutOfRange(i32),
    /// Entry function not found.
    EntryNotFound(String),
    /// `CallIntrinsic` 操作数不是已知 intrinsic 稠密下标。
    UnknownIntrinsic(i32),
    /// Type mismatch at runtime.
    TypeMismatch {
        /// Expected type name.
        expected: &'static str,
        /// Actual type name.
        actual: String,
    },
    /// Module load failure.
    ModuleLoad(String),
    /// Requested feature is not implemented (e.g. JIT while disabled).
    UnsupportedFeature(&'static str),
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
            Self::ImportIndexOutOfRange(idx) => write!(f, "import index out of range: {idx}"),
            Self::LayoutIndexOutOfRange(idx) => write!(f, "layout index out of range: {idx}"),
            Self::FieldSlotOutOfRange(idx) => write!(f, "field slot out of range: {idx}"),
            Self::EntryNotFound(name) => write!(f, "entry function not found: {name}"),
            Self::UnknownIntrinsic(index) => write!(f, "unknown intrinsic index: {index}"),
            Self::TypeMismatch { expected, actual } => {
                write!(f, "type mismatch: expected {expected}, got {actual}")
            }
            Self::ModuleLoad(message) => write!(f, "failed to load module: {message}"),
            Self::UnsupportedFeature(feature) => write!(f, "unsupported feature: {feature}"),
        }
    }
}

impl std::error::Error for NyarRuntimeError {}
