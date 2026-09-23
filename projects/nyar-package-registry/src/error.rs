/// Registry-layer errors.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("registry request failed with status {status}: {message}")]
    Status { status: u16, message: String },
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("package not found: {0}")]
    NotFound(String),
    #[error("unsupported operation: {0}")]
    Unsupported(String),
    #[error("{0}")]
    Message(String),
}

impl RegistryError {
    pub fn status(status: u16, message: impl Into<String>) -> Self {
        Self::Status { status, message: message.into() }
    }

    pub fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::Unsupported(message.into())
    }
}
