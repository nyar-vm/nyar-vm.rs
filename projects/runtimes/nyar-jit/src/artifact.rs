use crate::assumption::JitAssumption;
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
    /// 可选 NJ1（或后续版本）机器码 blob；`None` 表示仅分析产物。
    pub machine_code: Option<Vec<u8>>,
    /// 本产物依赖的假设；任一失效则宿主须丢弃对应缓存。
    pub assumptions: Vec<JitAssumption>,
}

impl JitCompiledArtifact {
    /// 仅含 stack map / 基线 deopt、尚无机器码入口的占位产物（分析 / 差分用）。
    pub fn stack_maps_only(stack_maps: FunctionStackMaps) -> Self {
        let deopt_map = DeoptMap::empty(stack_maps.function_index);
        Self {
            function_index: stack_maps.function_index,
            stack_maps,
            deopt_map,
            machine_code: None,
            assumptions: Vec::new(),
        }
    }

    /// 同时附带基线 deopt 表的分析产物。
    pub fn with_baseline_deopt(stack_maps: FunctionStackMaps, deopt_map: DeoptMap) -> Self {
        Self {
            function_index: stack_maps.function_index,
            stack_maps,
            deopt_map,
            machine_code: None,
            assumptions: Vec::new(),
        }
    }

    /// 分析元数据 + NJ1 机器码 blob。
    pub fn with_machine_code(stack_maps: FunctionStackMaps, deopt_map: DeoptMap, machine_code: Vec<u8>) -> Self {
        Self {
            function_index: stack_maps.function_index,
            stack_maps,
            deopt_map,
            machine_code: Some(machine_code),
            assumptions: Vec::new(),
        }
    }

    /// 附带假设列表。
    pub fn with_assumptions(mut self, assumptions: Vec<JitAssumption>) -> Self {
        self.assumptions = assumptions;
        self
    }
}
