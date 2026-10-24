#![doc = include_str!("readme.md")]
#![deny(missing_docs)]

/// Tcl 脚本模型与 parser。
pub mod script;

pub mod interpret;
pub mod module;
pub mod semantic_bridge;

pub use interpret::{TclValue, evaluate_tcl_script, evaluate_tcl_source};
pub use module::TclModule;
pub use script::{TclError, TclScript};
pub use semantic_bridge::TclSemanticBridge;
