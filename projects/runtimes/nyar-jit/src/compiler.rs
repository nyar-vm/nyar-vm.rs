use crate::{
    JitCompileRequest, JitCompiledArtifact, JitError, build_conservative_stack_maps,
};

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

/// 仅产出保守 stack map、不生成机器码的分析后端（WP17 骨架 / 差分用）。
#[derive(Debug, Default)]
pub struct StackMapJit;

impl JitCompiler for StackMapJit {
    fn enabled(&self) -> bool {
        true
    }

    fn compile_function(&mut self, request: &JitCompileRequest) -> Result<JitCompiledArtifact, JitError> {
        let _ = request.function_code()?;
        let maps = build_conservative_stack_maps(
            request.function_index,
            request.function.local_count,
            &request.function.safepoint_indices,
        );
        Ok(JitCompiledArtifact::stack_maps_only(maps))
    }
}
