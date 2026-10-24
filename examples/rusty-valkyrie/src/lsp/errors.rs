use std::{
    error::Error,
    fmt::{Display, Formatter},
};

#[derive(Debug)]
pub enum LspError {
    InvalidParams(String),
    NotFound(String),
    Serialization(String),
    Internal(String),
}

impl Display for LspError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            LspError::InvalidParams(msg) => write!(f, "Invalid parameters: {}", msg),
            LspError::NotFound(msg) => write!(f, "Not found: {}", msg),
            LspError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            LspError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl Error for LspError {}

impl From<&str> for LspError {
    fn from(s: &str) -> Self {
        LspError::Internal(s.to_string())
    }
}

impl From<String> for LspError {
    fn from(s: String) -> Self {
        LspError::Internal(s)
    }
}

pub type LspResult<T> = std::result::Result<T, LspError>;
