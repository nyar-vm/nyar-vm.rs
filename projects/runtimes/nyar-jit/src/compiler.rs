use crate::{JitCompileRequest, JitCompiledArtifact, JitError};

/// JIT compilation interface.
pub trait JitCompiler {
    /// Whether JIT is enabled for this backend.
    fn enabled(&self) -> bool;

    /// Compiles one module function to native code.
    fn compile_function(&mut self, request: &JitCompileRequest) -> Result<JitCompiledArtifact, JitError>;
}

/// Disabled JIT backend.
#[derive(Debug, Default)]
pub struct DisabledJit;

impl JitCompiler for DisabledJit {
    fn enabled(&self) -> bool {
        false
    }

    fn compile_function(&mut self, _request: &JitCompileRequest) -> Result<JitCompiledArtifact, JitError> {
        Err(JitError::Unsupported)
    }
}
