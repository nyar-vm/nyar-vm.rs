//! JIT integration — re-exports `nyar-jit` and builds requests from loaded modules.

use crate::module::LoadedModule;

pub use nyar_jit::{
    DeoptFrame, DeoptMap, DeoptMapEntry, DisabledJit, FunctionStackMaps, JitCompileRequest, JitCompiledArtifact, JitCompiler, JitError,
    JitFunctionSpec, StackMapEntry, StackMapJit, build_baseline_deopt_map, build_conservative_stack_maps,
};

/// Builds a language-agnostic JIT request from a loaded `.nyar` module.
pub fn compile_request(module: &LoadedModule, function_index: usize) -> Result<JitCompileRequest, JitError> {
    let function = module
        .functions
        .get(function_index)
        .ok_or(JitError::InvalidFunctionIndex(function_index))?;
    let safepoint_indices = module
        .executable
        .get(function_index)
        .map(|exec| exec.safepoints.clone())
        .unwrap_or_default();
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
            safepoint_indices,
        },
    })
}

/// 为已加载函数构造保守 GC stack map（不要求 JIT 后端启用）。
pub fn stack_maps_for(module: &LoadedModule, function_index: usize) -> Result<FunctionStackMaps, JitError> {
    let request = compile_request(module, function_index)?;
    Ok(build_conservative_stack_maps(
        request.function_index,
        request.function.local_count,
        &request.function.safepoint_indices,
    ))
}
