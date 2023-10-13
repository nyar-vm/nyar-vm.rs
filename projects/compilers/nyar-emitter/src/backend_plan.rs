//! 从 `CompiledProgram` 生成目标私有的稳定索引计划。
//!
//! 本模块不保存 HIR、Semantic MIR、旧 executable provider 或宿主摘要；
//! 它只把已验证的语义 ID 冻结为当前目标编码所需的物理索引。

use std::collections::BTreeMap;

use nyar::{TargetBackendFamily};
use nyar_types::{CompiledProgram, InstructionId, ItemInstanceId, MirValueId, ValueIdentity};

/// 目标准备失败的结构化边界。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendPlanError {
    /// 该目标没有接入新的语义计划入口。
    UnsupportedTarget { target: TargetBackendFamily },
}

/// Wasm/NyarVM 编码器共享的稳定私有索引计划。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendPrivatePlan {
    /// 已选择的目标后端。
    pub target: TargetBackendFamily,
    /// Compiler callable identity 到目标函数索引。
    pub function_indices: BTreeMap<ItemInstanceId, u32>,
    /// `(callable, block)` 到目标块索引。
    pub block_indices: BTreeMap<(ItemInstanceId, u32), u32>,
    /// SSA value identity 到目标 local 槽。
    pub value_slots: BTreeMap<ValueIdentity, u32>,
    /// Semantic instruction identity 到目标调用索引。
    pub invoke_indices: BTreeMap<InstructionId, u32>,
    /// Compiler 选定的公开导出表。
    pub exports: BTreeMap<ItemInstanceId, String>,
    /// Compiler 选定的导入槽。
    pub imports: BTreeMap<nyar_types::ImportIndex, ItemInstanceId>,
}

/// 从完整 `CompiledProgram` 一次性规划目标私有索引。
pub fn prepare_backend_plan(program: &CompiledProgram, target: TargetBackendFamily) -> Result<BackendPrivatePlan, BackendPlanError> {
    if !matches!(target, TargetBackendFamily::Wasm | TargetBackendFamily::NyarVm) {
        return Err(BackendPlanError::UnsupportedTarget { target });
    }

    let canonical = program.canonical();
    let mut function_indices = BTreeMap::new();
    let mut block_indices = BTreeMap::new();
    let mut value_slots = BTreeMap::new();
    let mut invoke_indices = BTreeMap::new();

    for (function_index, (function, body)) in canonical.mir.functions.iter().enumerate() {
        let function_index = u32::try_from(function_index).expect("目标函数索引溢出");
        function_indices.insert(*function, function_index);
        for (slot, value) in body.value_types.keys().enumerate() {
            value_slots.insert(ValueIdentity::new(*function, *value), u32::try_from(slot).expect("目标 local 索引溢出"));
        }
        for (block_index, (block, block_body)) in body.blocks.iter().enumerate() {
            block_indices.insert((*function, block.0), u32::try_from(block_index).expect("目标块索引溢出"));
            for instruction in &block_body.instructions {
                if matches!(instruction.operation, nyar_types::CanonicalOperation::Invoke { .. }) {
                    let index = u32::try_from(invoke_indices.len()).expect("目标调用索引溢出");
                    invoke_indices.insert(instruction.id, index);
                }
            }
        }
    }

    let exports = canonical.linked.exports.iter().map(|(item, export)| (*item, export.exported_name.clone())).collect();
    let imports = canonical.linked.imports.iter().map(|(index, import)| (*index, import.callee)).collect();

    Ok(BackendPrivatePlan { target, function_indices, block_indices, value_slots, invoke_indices, exports, imports })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nyar_types::{CanonicalProgram, CompiledProgram, layout_choice::RepresentationPlan};

    fn empty_program() -> CompiledProgram {
        CompiledProgram::new(CanonicalProgram::default(), RepresentationPlan::default()).expect("空程序也必须有完整成功合同")
    }

    #[test]
    fn preparation_consumes_compiled_program_and_freezes_empty_identity_tables() {
        let plan = prepare_backend_plan(&empty_program(), TargetBackendFamily::Wasm).expect("Wasm 目标准备");
        assert_eq!(plan.target, TargetBackendFamily::Wasm);
        assert!(plan.function_indices.is_empty());
        assert!(plan.value_slots.is_empty());
        assert!(plan.invoke_indices.is_empty());
    }

    #[test]
    fn unsupported_targets_fail_at_preparation_boundary() {
        assert_eq!(
            prepare_backend_plan(&empty_program(), TargetBackendFamily::Clr),
            Err(BackendPlanError::UnsupportedTarget { target: TargetBackendFamily::Clr })
        );
    }
}
