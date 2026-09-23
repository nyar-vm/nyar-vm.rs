//! PowerShell host-script module model.

use std::path::{Path, PathBuf};

use std_data::text::powershell::PowerShellScript;

/// PowerShell host-script module.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PowerShellModule {
    /// Module logical name.
    pub name: Option<String>,
    /// Source path.
    pub source_path: Option<PathBuf>,
    /// Parsed script.
    pub script: PowerShellScript,
}

impl PowerShellModule {
    /// Create a module from a parsed script.
    pub fn new(script: PowerShellScript) -> Self {
        Self { name: None, source_path: None, script }
    }

    /// Canonical language identifier.
    pub fn language_id(&self) -> &'static str {
        "powershell"
    }

    /// Optional source path for diagnostics.
    pub fn source_path(&self) -> Option<&Path> {
        self.source_path.as_deref()
    }
}
