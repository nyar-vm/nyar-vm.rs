//! C host script semantic bridge skeleton.

use std_data::text::c::{CItem, CScript};

/// C module semantic bridge result.
#[derive(Debug, Clone, PartialEq)]
pub struct CSemanticBridge {
    /// Parsed script.
    pub script: CScript,
    /// Exported function names.
    pub exported_functions: Vec<String>,
}

impl CSemanticBridge {
    /// Build a bridge from a parsed script, collecting function symbols.
    pub fn from_script(script: CScript) -> Self {
        let exported_functions = script
            .items
            .iter()
            .filter_map(|item| match item {
                CItem::Function(func) => Some(func.name.clone()),
                CItem::GlobalVar(_) => None,
            })
            .collect();
        Self { script, exported_functions }
    }

    /// Language-neutral exported symbol names.
    pub fn exported_symbols(&self) -> &[String] {
        &self.exported_functions
    }
}
