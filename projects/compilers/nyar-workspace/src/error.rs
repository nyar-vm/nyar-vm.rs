//! Errors for workspace cache I/O and encoding.

use std::path::PathBuf;

/// Errors produced by the workspace cache mechanism.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceCacheError {
    /// Filesystem I/O failure.
    #[error("workspace cache IO error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Failed to encode a cache entry as `.nyar`.
    #[error("failed to encode cache entry: {0}")]
    Encode(String),
    /// Failed to decode a cache entry.
    #[error("failed to decode cache entry at {path}: {detail}")]
    Decode { path: PathBuf, detail: String },
}

impl WorkspaceCacheError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io { path: path.into(), source }
    }
}
