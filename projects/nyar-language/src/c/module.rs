//! `C` host script module model.

use std::path::{Path, PathBuf};

use std_data::text::c::CScript;

/// `C` host script module.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CModule {
    /// Logical module name.
    pub name: Option<String>,
    /// Source path.
    pub source_path: Option<PathBuf>,
    /// Script text model.
    pub script: CScript,
}

impl CModule {
    /// Create a new `C` module.
    pub fn new(script: CScript) -> Self {
        Self { name: None, source_path: None, script }
    }

    /// Attach a source path.
    pub fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.source_path = Some(path.into());
        self
    }

    /// Canonical language identifier.
    pub fn language_id(&self) -> &'static str {
        "c"
    }

    /// Optional source path for diagnostics.
    pub fn source_path(&self) -> Option<&Path> {
        self.source_path.as_deref()
    }
}
