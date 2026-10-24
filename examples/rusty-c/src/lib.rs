#![doc = include_str!("readme.md")]
#![deny(missing_docs)]

/// C 脚本模型与 parser。
pub mod script;

pub mod interpret;
pub mod module;
pub mod semantic_bridge;

pub use interpret::{CValue, evaluate_c_script, evaluate_c_source};
pub use module::CModule;
pub use script::{CError, CScript};
pub use semantic_bridge::CSemanticBridge;
