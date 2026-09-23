//! `Tcl` host script module model.

use std::path::{Path, PathBuf};

use std_data::text::tcl::TclScript;

/// `Tcl` host script module.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TclModule {
    /// Logical module name.
    pub name: Option<String>,
    /// Source path.
    pub source_path: Option<PathBuf>,
    /// Script text model.
    pub script: TclScript,
}

impl TclModule {
    /// Create a new `Tcl` module.
    pub fn new(script: TclScript) -> Self {
        Self { name: None, source_path: None, script }
    }

    /// Canonical language identifier.
    pub fn language_id(&self) -> &'static str {
        "tcl"
    }

    /// Optional source path for diagnostics.
    pub fn source_path(&self) -> Option<&Path> {
        self.source_path.as_deref()
    }
}
