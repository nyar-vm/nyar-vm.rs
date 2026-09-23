#![doc = include_str!("readme.md")]

//! `Lua` host script module, semantic bridge, and tree interpreter.

pub mod interpret;
pub mod module;
pub mod semantic_bridge;
pub mod specialize;

pub use interpret::{LuaValue, evaluate_lua_script, evaluate_lua_source};
pub use module::LuaModule;
pub use semantic_bridge::LuaSemanticBridge;
pub use specialize::{specialize_lua_into, specialize_lua_script};
pub use std_data::text::lua::{LuaError, LuaScript};
