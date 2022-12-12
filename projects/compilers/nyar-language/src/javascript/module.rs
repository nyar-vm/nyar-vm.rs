//! JavaScript / TypeScript host module model.

use std::path::{Path, PathBuf};

/// JavaScript / TypeScript source module.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct JavascriptModule {
    /// Logical module name.
    pub name: Option<String>,
    /// Source path for diagnostics.
    pub source_path: Option<PathBuf>,
    /// Raw source text.
    pub source: String,
}

impl JavascriptModule {
    /// Create from source text.
    pub fn new(source: impl Into<String>) -> Self {
        Self { name: None, source_path: None, source: source.into() }
    }

    /// Attach a source path.
    pub fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.source_path = Some(path.into());
        self
    }

    /// Canonical language identifier.
    pub fn language_id(&self) -> &'static str {
        "javascript"
    }

    /// Optional source path for diagnostics.
    pub fn source_path(&self) -> Option<&Path> {
        self.source_path.as_deref()
    }
}
