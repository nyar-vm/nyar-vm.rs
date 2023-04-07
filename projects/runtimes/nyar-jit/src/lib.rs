#![warn(missing_docs)]

//! Optional JIT acceleration for the Nyar bytecode runtime.
//!
//! Language-agnostic: consumes verified bytecode views only. Must not import
//! `nyar-language`, `nyar-vm`, or concrete language frontends.

mod artifact;
mod compiler;
mod error;
mod request;
mod stack_map;

pub use artifact::JitCompiledArtifact;
pub use compiler::{DisabledJit, JitCompiler, StackMapJit};
pub use error::JitError;
pub use request::{JitCompileRequest, JitFunctionSpec};
pub use stack_map::{FunctionStackMaps, StackMapEntry, build_conservative_stack_maps};
