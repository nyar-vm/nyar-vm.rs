#![doc = include_str!("readme.md")]
#![deny(missing_docs)]

/// Lua 脚本模型与 parser。
pub mod script;

pub mod interpret;
pub mod module;
pub mod semantic_bridge;
pub mod specialize;

pub use interpret::{LuaValue, evaluate_lua_script, evaluate_lua_source};
pub use module::LuaModule;
pub use script::{LuaError, LuaScript};
pub use semantic_bridge::LuaSemanticBridge;
pub use specialize::{specialize_lua_into, specialize_lua_script};
