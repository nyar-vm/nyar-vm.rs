use std::fmt::Display;

/// Minimal shared error wrapper for compatibility during bootstrap.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NyarError {
    /// Numeric error code.
    pub code: u32,
    /// Structured error payload.
    pub kind: NyarErrorKind,
}

/// 只包含必要的字段，不得耦合 message
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum NyarErrorKind {}

impl Display for NyarErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            _ => {
                write!(f, "NyarErrorKind")
            }
        }
    }
}

impl NyarErrorKind {
    /// Returns a stable machine-readable key for compatibility shims.
    pub fn key(&self) -> &'static str {
        "nyar_error"
    }
}
