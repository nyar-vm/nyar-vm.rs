#![doc = include_str!("readme.md")]
#![deny(missing_docs)]

/// PowerShell 脚本模型与 parser。
pub mod script;

pub mod interpret;
pub mod module;
pub mod semantic_bridge;

pub use interpret::{PowerShellValue, evaluate_powershell_script, evaluate_powershell_source};
pub use module::PowerShellModule;
pub use script::{PowerShellError, PowerShellScript};
pub use semantic_bridge::PowerShellSemanticBridge;
