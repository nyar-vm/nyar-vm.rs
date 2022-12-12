#![doc = include_str!("readme.md")]

//! `Bash` host script module, semantic bridge, and tree interpreter.

pub mod interpret;
pub mod module;
pub mod semantic_bridge;

pub use interpret::{BashValue, evaluate_bash_script, evaluate_bash_source};
pub use module::BashModule;
pub use semantic_bridge::BashSemanticBridge;
pub use std_data::text::bash::{BashError, BashScript};
