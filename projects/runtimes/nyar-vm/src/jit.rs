//! JIT integration — re-exports `nyar-jit` and builds requests from loaded modules.

use crate::module::LoadedModule;

pub use nyar_jit::{
    BaselineScalarJit, DeoptFrame, DeoptMap, DeoptMapEntry, DeoptRestoreError, DisabledJit, FunctionStackMaps, I32Binop,
    I32Cmp, InlineFrameSpec, JitAssumption, JitCompileRequest, JitCompiledArtifact, JitCompiler, JitError,
    JitFunctionSpec, MACHINE_CODE_MAGIC, MachineCodeError, RestoredInterpreterFrame, RestoredLocal, ScalarProgram,
    StackMapEntry, StackMapJit, baseline_scalar_assumptions, build_baseline_deopt_map, build_conservative_stack_maps,
    build_inline_deopt_map, decode_scalar_program, encode_ret_i32_add_locals, encode_ret_const_i32,
    encode_ret_i32_binop_imm_local, encode_ret_i32_binop_locals, encode_ret_i32_cmp_imm_local,
    encode_ret_i32_cmp_locals, encode_ret_i32_select_cmp_locals, encode_ret_local, match_scalar_program,
    materialize_interpreter_frames,
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
    let constant_i32 = module
        .constants
        .iter()
        .map(|constant| match constant {
            nyar_bytecode::NyarConstant::Integer32(value) => Some(*value),
            _ => None,
        })
        .collect();
    Ok(JitCompileRequest {
        module_version: module.version,
        module_name: module.name.clone(),
        code_bytes: module.code_bytes.clone(),
        constant_i32,
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
