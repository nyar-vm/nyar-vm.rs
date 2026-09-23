//! Tcl host script semantic bridge.

use std_data::text::tcl::{TclCommand, TclScript};

/// Tcl module semantic bridge result.
#[derive(Debug, Clone, PartialEq)]
pub struct TclSemanticBridge {
    /// Parsed script.
    pub script: TclScript,
    /// Exported procedure names.
    pub exported_procedures: Vec<String>,
}

impl TclSemanticBridge {
    /// Build a bridge from a parsed script, collecting `proc` names.
    pub fn from_script(script: TclScript) -> Self {
        let exported_procedures = script
            .commands
            .iter()
            .filter_map(|command| match command {
                TclCommand::Proc { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        Self { script, exported_procedures }
    }

    /// Language-neutral exported symbol names.
    pub fn exported_symbols(&self) -> &[String] {
        &self.exported_procedures
    }
}
