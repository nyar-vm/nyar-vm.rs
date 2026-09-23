//! PowerShell host-script semantic bridge.

use std_data::text::powershell::{PowerShellScript, PsStmt};

/// Bridge over a parsed PowerShell script.
#[derive(Debug, Clone, PartialEq)]
pub struct PowerShellSemanticBridge {
    /// Parsed script.
    pub script: PowerShellScript,
    /// Exported function / command names.
    pub exported_commands: Vec<String>,
}

impl PowerShellSemanticBridge {
    /// Build from a script, collecting top-level `function` names.
    pub fn from_script(script: PowerShellScript) -> Self {
        let exported_commands = script
            .statements
            .iter()
            .filter_map(|stmt| match stmt {
                PsStmt::Function { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        Self { script, exported_commands }
    }

    /// Language-neutral exported symbol names.
    pub fn exported_symbols(&self) -> &[String] {
        &self.exported_commands
    }
}
