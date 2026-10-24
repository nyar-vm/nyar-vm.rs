#![doc = include_str!("readme.md")]
#![deny(missing_docs)]

/// Bash 脚本模型与 parser。
pub mod script;

pub mod interpret;
pub mod module;
pub mod semantic_bridge;

pub use interpret::{BashValue, evaluate_bash_script, evaluate_bash_source};
pub use module::BashModule;
pub use script::{BashError, BashScript};
pub use semantic_bridge::BashSemanticBridge;
