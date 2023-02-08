//! JIT integration — re-exports `nyar-jit` and builds requests from loaded modules.

use crate::module::LoadedModule;

pub use nyar_jit::{
    DisabledJit, JitCompileRequest, JitCompiledArtifact, JitCompiler, JitError, JitFunctionSpec,
};

/// Builds a language-agnostic JIT request from a loaded `.nyar` module.
pub fn compile_request(module: &LoadedModule, function_index: usize) -> Result<JitCompileRequest, JitError> {
    let function = module
        .functions
        .get(function_index)
        .ok_or(JitError::InvalidFunctionIndex(function_index))?;
    Ok(JitCompileRequest {
        module_version: module.version,
        module_name: module.name.clone(),
        code_bytes: module.code_bytes.clone(),
        function_index,
        function: JitFunctionSpec {
            code_offset: function.code_offset,
            code_length: function.code_length,
            local_count: function.local_count,
            arity: function.arity,
        },
    })
}
