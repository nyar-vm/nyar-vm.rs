#![doc = include_str!("readme.md")]

//! `Tcl` host script module, semantic bridge, and tree interpreter.

pub mod interpret;
pub mod module;
pub mod semantic_bridge;

pub use interpret::{TclValue, evaluate_tcl_script, evaluate_tcl_source};
pub use module::TclModule;
pub use semantic_bridge::TclSemanticBridge;
pub use std_data::text::tcl::{TclError, TclScript};
