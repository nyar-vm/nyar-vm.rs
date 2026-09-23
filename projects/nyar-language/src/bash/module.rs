//! `Bash` host script module model.

use std::path::{Path, PathBuf};

use std_data::text::bash::BashScript;

/// `Bash` host script module.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BashModule {
    /// Logical module name.
    pub name: Option<String>,
    /// Source path.
    pub source_path: Option<PathBuf>,
    /// Script text model.
    pub script: BashScript,
}

impl BashModule {
    /// Create a new `Bash` module.
    pub fn new(script: BashScript) -> Self {
        Self { name: None, source_path: None, script }
    }

    /// Attach a source path.
    pub fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.source_path = Some(path.into());
        self
    }

    /// Canonical language identifier.
    pub fn language_id(&self) -> &'static str {
        "bash"
    }

    /// Optional source path for diagnostics.
    pub fn source_path(&self) -> Option<&Path> {
        self.source_path.as_deref()
    }
}
