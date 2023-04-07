use crate::stack_map::FunctionStackMaps;

/// Opaque native code artifact produced by a JIT backend.
///
/// Backends attach machine code, stack maps, and deoptimization metadata here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JitCompiledArtifact {
    /// Function index this artifact was compiled from.
    pub function_index: usize,
    /// GC stack maps keyed by inner-code safepoint instruction indices.
    pub stack_maps: FunctionStackMaps,
}

impl JitCompiledArtifact {
    /// 仅含 stack map、尚无机器码入口的占位产物（分析 / 差分用）。
    pub fn stack_maps_only(stack_maps: FunctionStackMaps) -> Self {
        Self { function_index: stack_maps.function_index, stack_maps }
    }
}
