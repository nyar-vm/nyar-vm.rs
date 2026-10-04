//! 为 Nyar 可执行函数规划稳定的单槽 local。

use std::collections::BTreeMap;

use crate::backend_plan_views::{ExecutableBlockRef, ExecutableFunction, ExecutableValueRef};
use nyar::NyarType;

pub struct ExecutableSlotPlan {
    pub local_types: Vec<NyarType>,
    pub value_locals: BTreeMap<ExecutableValueRef, u16>,
    pub block_param_locals: BTreeMap<(ExecutableBlockRef, usize), u16>,
}

impl ExecutableSlotPlan {
    /// 按完整的 SSA 定义规划全部 local；后端只编码该计划，不再动态补槽。
    pub fn plan_nyar(function: &ExecutableFunction) -> Self {
        let mut plan = Self {
            local_types: Vec::new(),
            value_locals: BTreeMap::new(),
            block_param_locals: BTreeMap::new(),
        };

        let entry = function
            .blocks
            .iter()
            .find(|block| block.id == function.entry)
            .expect("validated entry block");
        for block in std::iter::once(entry).chain(function.blocks.iter().filter(|block| block.id != function.entry)) {
            for (index, parameter) in block.parameters.iter().enumerate() {
                let ty = function.value_types.get(parameter).expect("validated block parameter type");
                let local = plan.alloc_local(ty);
                plan.block_param_locals.insert((block.id, index), local);
                assert!(plan.value_locals.insert(*parameter, local).is_none(), "duplicate SSA definition");
            }
        }
        for block in &function.blocks {
            for instruction in &block.instructions {
                for output in &instruction.results {
                    let ty = function.value_types.get(output).expect("validated instruction result type");
                    let local = plan.alloc_local(ty);
                    assert!(plan.value_locals.insert(*output, local).is_none(), "duplicate SSA definition");
                }
            }
        }
        plan
    }

    fn alloc_local(&mut self, ty: &NyarType) -> u16 {
        let index = u16::try_from(self.local_types.len()).expect("Nyar local count exceeds u16");
        self.local_types.push(ty.clone());
        index
    }

    /// 返回指令的唯一主结果；多结果指令必须在 Semantic MIR 合同中被拒绝或拆分。
    pub fn instruction_output(&self, instruction: &crate::backend_plan_views::ExecutableInstruction) -> Option<ExecutableValueRef> {
        instruction.results.first().copied()
    }
}
