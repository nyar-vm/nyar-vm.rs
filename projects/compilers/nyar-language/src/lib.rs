#![doc = include_str!("readme.md")]
#![deny(missing_docs)]

pub mod transitional;

pub mod c;
pub mod javascript;
pub mod notedown;
pub mod powershell;
pub mod python;

pub use c::{CModule, CSemanticBridge, CValue, evaluate_c_script, evaluate_c_source};
pub use javascript::JavascriptModule;
pub use nyar_analyzer::residual::ResidualSink;
pub use powershell::{PowerShellModule, PowerShellSemanticBridge, PowerShellValue, evaluate_powershell_script, evaluate_powershell_source};
pub use python::PythonModule;
