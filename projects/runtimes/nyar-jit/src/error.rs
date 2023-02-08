use std::fmt::{Display, Formatter};

/// JIT compilation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JitError {
    /// JIT is disabled or not implemented for this backend.
    Unsupported,
    /// Function index out of range for the supplied module view.
    InvalidFunctionIndex(usize),
    /// Bytecode layout is invalid for compilation.
    InvalidBytecode(String),
}

impl Display for JitError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported => write!(f, "jit is not supported"),
            Self::InvalidFunctionIndex(index) => write!(f, "function index {index} out of range"),
            Self::InvalidBytecode(message) => write!(f, "invalid bytecode: {message}"),
        }
    }
}

impl std::error::Error for JitError {}
