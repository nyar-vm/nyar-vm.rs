//! `Lua` host script module model.

use std::path::{Path, PathBuf};

use std_data::text::lua::LuaScript;

/// `Lua` host script module.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LuaModule {
    /// Logical module name.
    pub name: Option<String>,
    /// Source path.
    pub source_path: Option<PathBuf>,
    /// Script text model.
    pub script: LuaScript,
}

impl LuaModule {
    /// Create a new `Lua` module.
    pub fn new(script: LuaScript) -> Self {
        Self { name: None, source_path: None, script }
    }

    /// Canonical language identifier.
    pub fn language_id(&self) -> &'static str {
        "lua"
    }

    /// Optional source path for diagnostics.
    pub fn source_path(&self) -> Option<&Path> {
        self.source_path.as_deref()
    }
}
