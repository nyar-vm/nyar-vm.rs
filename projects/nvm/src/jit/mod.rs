//! JIT compiler integration (disabled stub).

use crate::{error::NyarRuntimeError, module::LoadedModule};

/// JIT compilation interface.
pub trait JitCompiler {
    /// Whether JIT is enabled.
    fn enabled(&self) -> bool;

    /// Compiles a module function to native code.
    fn compile_function(&mut self, module: &LoadedModule, function_index: usize) -> Result<(), NyarRuntimeError>;
}

/// Disabled JIT backend.
#[derive(Debug, Default)]
pub struct DisabledJit;

impl JitCompiler for DisabledJit {
    fn enabled(&self) -> bool {
        false
    }

    fn compile_function(&mut self, _module: &LoadedModule, _function_index: usize) -> Result<(), NyarRuntimeError> {
        Ok(())
    }
}
