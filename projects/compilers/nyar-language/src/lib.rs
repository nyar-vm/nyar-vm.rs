#![doc = include_str!("readme.md")]
#![deny(missing_docs)]

pub mod transitional;

pub mod bash;
pub mod c;
pub mod javascript;
pub mod lua;
pub mod notedown;
pub mod pe;
pub mod powershell;
pub mod python;
pub mod tcl;

pub use bash::{BashModule, BashSemanticBridge, BashValue, evaluate_bash_script, evaluate_bash_source};
pub use c::{CModule, CSemanticBridge, CValue, evaluate_c_script, evaluate_c_source};
pub use javascript::JavascriptModule;
pub use lua::{LuaModule, LuaSemanticBridge, LuaValue, evaluate_lua_script, evaluate_lua_source, specialize_lua_into, specialize_lua_script};
pub use pe::ResidualSink;
pub use powershell::{PowerShellModule, PowerShellSemanticBridge, PowerShellValue, evaluate_powershell_script, evaluate_powershell_source};
pub use python::PythonModule;
pub use tcl::{TclModule, TclSemanticBridge, TclValue, evaluate_tcl_script, evaluate_tcl_source};

pub use nyar::{
    self, ArtifactKind, ArtifactPartitionPlan, ArtifactPolicy, ArtifactSet, CanonicalAbi, CanonicalArch, CanonicalSpecification,
    CanonicalTarget, CanonicalTargetParseError, CanonicalVendor, CompilationOptions, EntryPolicy, HostProjectionBoundary, ProgramFacts,
    PublishFormat, ReferenceManagement, RunnerFamily, RunnerSelector, TargetHostKind, TargetMode, TargetProfile, WrapStrategy,
};
