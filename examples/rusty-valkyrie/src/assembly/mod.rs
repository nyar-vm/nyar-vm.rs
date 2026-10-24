//! 从唯一 `CompiledProgram` 选择分区并交给目标 preparation。
//!
//! 分区选择不重新生产语义事实；共享载荷携带已验证程序与明确的目标分区。

use miette::{Result as MietteResult, miette};
use nyar::{ArtifactPartitionPlan, CanonicalTarget, ClrSuspendStrategy, Identifier, PlanningError, projection_policy_for_target_profile};
use nyar_types::CompiledProgram;

pub use nyar::AssembledFragment;

/// 返回已解析的导出/入口数量；装配器不直接读取语义计划。
#[doc(hidden)]
pub fn build_output_surface_counts(compiled_program: &CompiledProgram) -> (usize, usize) {
    let linked = &compiled_program.canonical().linked;
    (linked.exports.len(), linked.entries.len())
}
/// 根据完整程序、目标与 projection 合同规划产物分区。
#[doc(hidden)]
pub fn plan_artifacts_from_compiled_program(
    compiled_program: &CompiledProgram,
    target: CanonicalTarget,
    clr_suspend_strategy: ClrSuspendStrategy,
) -> Result<ArtifactPartitionPlan, PlanningError> {
    let target_profile = target.to_profile(None);
    let projection_policy = projection_policy_for_target_profile(&target_profile).map_err(|error| PlanningError::SemanticContract {
        module: compiled_program.canonical().linked.module_name.clone(),
        stage: nyar_types::CompileStage::RepresentationPlan,
        detail: format!("目标 projection 合同失败: {error:?}"),
    })?;
    let linked = &compiled_program.canonical().linked;
    let backend_registry = nyar_emitter::bundled_backend_registry_from_canonical(&linked.fragments, &target_profile, &projection_policy);
    ArtifactPartitionPlan::from_canonical_program(
        compiled_program.canonical(),
        target,
        nyar::RewriteTheory::default(),
        projection_policy,
        backend_registry,
        clr_suspend_strategy,
    )
}

/// 从指定分区装配目标载荷，拒绝任何与 Canonical 身份不一致的计划。
#[doc(hidden)]
pub fn assemble_fragment(
    compiled_program: &CompiledProgram,
    plan: &ArtifactPartitionPlan,
    partition_index: usize,
) -> MietteResult<AssembledFragment> {
    if plan.module_name.to_string() != compiled_program.canonical().linked.module_name {
        return Err(miette!("前端计划与分区计划不匹配"));
    }
    if partition_index >= plan.partitions.len() {
        return Err(miette!("分区索引 `{partition_index}` 超出范围"));
    }
    let partition = &plan.partitions[partition_index];
    let fragment_view = plan
        .fragment_views
        .iter()
        .find(|view| view.fragment_id == partition.fragment)
        .ok_or_else(|| miette!("分区 `{}` 缺少优化后的 Canonical 片段视图", partition.name))?;
    let linked = &compiled_program.canonical().linked;
    let fragment =
        linked.fragments.get(&partition.fragment).ok_or_else(|| miette!("分区 `{}` 对应的 Canonical 语义片段不存在", partition.name))?;
    if partition.exported_operations != fragment.exported_operations
        || partition.entry_operation != fragment.entry_operation
        || fragment_view.canonical_operations != fragment.exported_operations
    {
        return Err(miette!("分区 `{}` 的 callable 身份与 Canonical 片段合同不一致", partition.name));
    }

    let fragment_requires_suspend = fragment.required_capabilities.iter().any(|capability| capability.as_str() == "suspend");
    if fragment_requires_suspend {
        return Err(miette!("分区 `{}` 要求 suspend，但 Compiler 尚未提供已验证的 Canonical suspend 合同", partition.name));
    }

    Ok(AssembledFragment {
        partition: partition.clone(),
        theory_bundle: fragment_view.theory_bundle.clone(),
        compiled_program: compiled_program.clone(),
    })
}
