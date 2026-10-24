//! Legacy VM compilers.
//!
//! Host-script PE product path: [`native_residual`] -> x64 / Windows PE.
//! [`residual::StackResidualSink`] / [`bytecode`] are probe-only (`.nyar`).

mod bytecode;
mod generate;
mod native_residual;
mod pe;
mod residual;
mod stack;

pub use bytecode::NyarBytecodeCompiler;
pub use generate::GenerateModule;
pub use native_residual::{
    NativeResidualFunction, NativeResidualModule, NativeResidualOp, NativeResidualSink, NativeResidualTarget, ResidualValue, emit_native_pe,
    eval_native_residual, lower_native_residual_to_x64, specialize_language,
};
pub use pe::PeCompiler;
pub use residual::StackResidualSink;
pub use stack::StackCompiler;

/// Artifact from [`crate::LegacyVmRunner::compile_module`].
#[derive(Debug, Clone, PartialEq)]
pub enum CompileArtifact {
    /// Host-script Futamura product: native-bound residual (-> PE).
    Native(NativeResidualModule),
    /// Bytecode-pipe probe only (JS/Python stubs) — **not** host-script PE destination.
    BytecodeProbe(GenerateModule),
}
