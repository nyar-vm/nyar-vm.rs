//! Bash host-script semantic bridge.

use crate::script::{BashScript, BashStmt};

/// Bridge over a parsed Bash script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BashSemanticBridge {
    /// Parsed script.
    pub script: BashScript,
    /// Exported function / command names.
    pub exported_commands: Vec<String>,
}

impl BashSemanticBridge {
    /// Build from a script, collecting top-level function names.
    pub fn from_script(script: BashScript) -> Self {
        let exported_commands = script
            .statements
            .iter()
            .filter_map(|stmt| match stmt {
                BashStmt::FunctionDef { name, .. } => Some(name.clone()),
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
