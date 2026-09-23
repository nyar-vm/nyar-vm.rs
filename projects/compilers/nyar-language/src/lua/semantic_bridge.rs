//! Lua host script semantic bridge skeleton.

use std_data::text::lua::LuaScript;

/// Lua module semantic bridge result.
#[derive(Debug, Clone, PartialEq)]
pub struct LuaSemanticBridge {
    /// Parsed script.
    pub script: LuaScript,
    /// Exported function names.
    pub exported_functions: Vec<String>,
}

impl LuaSemanticBridge {
    /// Build a bridge from a parsed script.
    pub fn from_script(script: LuaScript) -> Self {
        Self { script, exported_functions: Vec::new() }
    }

    /// Language-neutral exported symbol names.
    pub fn exported_symbols(&self) -> &[String] {
        &self.exported_functions
    }
}
