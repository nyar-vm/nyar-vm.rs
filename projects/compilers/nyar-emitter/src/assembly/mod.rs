//! 将已验证的 `AssembledFragment` 转换为目标提交视图。
//!
//! 这里不重新解析语义；分区身份来自 `ArtifactPartition`，其余事实来自
//! 同一份 `CompiledProgram` Canonical 闭包，目标私有计划只在本边界生成。

use miette::miette;
use nyar::AssembledFragment;

use crate::{BackendPrivatePlan, FragmentSubmission};

/// 从编译器成功载荷生成唯一的后端提交视图。
pub(crate) fn fragment_submission_from_assembled(payload: AssembledFragment) -> miette::Result<FragmentSubmission> {
    let linked = &payload.compiled_program.canonical().linked;
    let partition = &payload.partition;
    let fragment = linked.fragments.get(&partition.fragment).ok_or_else(|| miette!("Canonical 片段 `{}` 不存在", partition.fragment))?;
    if partition.exported_operations != fragment.exported_operations || partition.entry_operation != fragment.entry_operation {
        return Err(miette!("分区 `{}` 的根与 Canonical 片段不一致", partition.name));
    }
    let mut roots = partition.exported_operations.clone();
    if let Some(entry) = partition.entry_operation {
        if !roots.contains(&entry) {
            roots.push(entry);
        }
    }
    let backend_plan =
        BackendPrivatePlan::from_compiled_program(&payload.compiled_program, &partition.fragment, payload.theory_bundle, &roots)?;

    Ok(FragmentSubmission { backend_plan: std::sync::Arc::new(backend_plan) })
}
