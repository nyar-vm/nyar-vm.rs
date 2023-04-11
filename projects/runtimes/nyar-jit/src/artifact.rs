use crate::stack_map::FunctionStackMaps;
use crate::deopt::DeoptMap;

/// Opaque native code artifact produced by a JIT backend.
///
/// Backends attach machine code, stack maps, and deoptimization metadata here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JitCompiledArtifact {
    /// Function index this artifact was compiled from.
    pub function_index: usize,
    /// GC stack maps keyed by inner-code safepoint instruction indices.
    pub stack_maps: FunctionStackMaps,
    /// 去优化元数据（与 stack map 分离；可为空表）。
    pub deopt_map: DeoptMap,
}

impl JitCompiledArtifact {
    /// 仅含 stack map / 基线 deopt、尚无机器码入口的占位产物（分析 / 差分用）。
    pub fn stack_maps_only(stack_maps: FunctionStackMaps) -> Self {
        let deopt_map = DeoptMap::empty(stack_maps.function_index);
        Self {
            function_index: stack_maps.function_index,
            stack_maps,
            deopt_map,
        }
    }

    /// 同时附带基线 deopt 表的分析产物。
    pub fn with_baseline_deopt(stack_maps: FunctionStackMaps, deopt_map: DeoptMap) -> Self {
        Self {
            function_index: stack_maps.function_index,
            stack_maps,
            deopt_map,
        }
    }
}
