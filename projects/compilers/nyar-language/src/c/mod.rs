#![doc = include_str!("readme.md")]

//! `C` host script module, semantic bridge, and tree interpreter.

pub mod interpret;
pub mod module;
pub mod semantic_bridge;

pub use interpret::{CValue, evaluate_c_script, evaluate_c_source};
pub use module::CModule;
pub use semantic_bridge::CSemanticBridge;
pub use crate::transitional::guest_scripts::c::{CError, CScript};
