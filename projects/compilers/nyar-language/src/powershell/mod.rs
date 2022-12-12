#![doc = include_str!("readme.md")]

//! PowerShell host-script module, semantic bridge, and tree interpreter.

pub mod interpret;
pub mod module;
pub mod semantic_bridge;

pub use interpret::{PowerShellValue, evaluate_powershell_script, evaluate_powershell_source};
pub use module::PowerShellModule;
pub use semantic_bridge::PowerShellSemanticBridge;
pub use std_data::text::powershell::{PowerShellError, PowerShellScript, PsExpr, PsStmt};
