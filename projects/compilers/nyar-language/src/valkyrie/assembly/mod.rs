//! 从唯一 `CompiledProgram` 选择分区并交给目标 preparation。
//!
//! 分区选择不重新生产语义事实；共享提交载荷只携带已验证程序及其稳定实例根。

use std::collections::BTreeSet;

use miette::{Result as MietteResult, miette};
use nyar::{ArtifactPartitionPlan, CanonicalTarget, ClrSuspendStrategy, Identifier, PlanningError, projection_policy_for_target_profile};
use nyar_types::ItemInstanceId;
use nyar_types::CompiledProgram;

pub use nyar::AssembledFragment;

/// 返回已解析的导出/入口数量；装配器不直接读取语义计划。
pub(crate) fn build_output_surface_counts(compiled_program: &CompiledProgram) -> (usize, usize) {
    let linked = &compiled_program.canonical().linked;
    (linked.exports.len(), linked.entries.len())
}
/// 根据完整程序、目标与 projection 合同规划产物分区。
pub(crate) fn plan_artifacts_from_compiled_program(
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
    let backend_registry = emitter::bundled_backend_registry_from_canonical(&linked.fragments, &target_profile, &projection_policy);
    ArtifactPartitionPlan::from_canonical_program(
        compiled_program.canonical(), target, nyar::RewriteTheory::default(), projection_policy, backend_registry, clr_suspend_strategy,
    )
}

/// 从指定分区装配目标载荷，拒绝任何与 Canonical 身份不一致的计划。
pub(crate) fn assemble_fragment(
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
    let fragment_view = plan.fragment_views.iter().find(|view| view.fragment_id == partition.fragment)
        .ok_or_else(|| miette!("分区 `{}` 缺少优化后的 Canonical 片段视图", partition.name))?;
    let linked = &compiled_program.canonical().linked;
    let fragment = linked.fragments.get(&partition.fragment)
        .ok_or_else(|| miette!("分区 `{}` 对应的 Canonical 语义片段不存在", partition.name))?;
    if partition.exported_operations != fragment.exported_operations
        || partition.entry_operation != fragment.entry_operation
        || fragment_view.canonical_operations != fragment.exported_operations
    {
        return Err(miette!("分区 `{}` 的 callable 身份与 Canonical 片段合同不一致", partition.name));
    }

    let fragment_requires_suspend = fragment.required_capabilities.iter().any(|capability| capability.as_str() == "suspend");
    if fragment_requires_suspend {
        return Err(miette!(
            "分区 `{}` 要求 suspend，但 Compiler 尚未提供已验证的 Canonical suspend 合同",
            partition.name
        ));
    }

    let callable_roots = resolve_callable_roots(compiled_program, &partition.exported_operations, partition.entry_operation.as_ref())?;

    Ok(AssembledFragment {
        module_name: compiled_program.canonical().linked.module_name.clone(),
        fragment_id: fragment.id.clone(),
        theory_bundle: fragment_view.theory_bundle.clone(),
        callable_roots,
        compiled_program: compiled_program.clone(),
    })
}

fn resolve_callable_roots(
    program: &nyar_types::CompiledProgram,
    exported_operations: &[ItemInstanceId],
    entry_operation: Option<&ItemInstanceId>,
) -> MietteResult<Vec<ItemInstanceId>> {
    let mut roots = exported_operations.to_vec();
    if let Some(entry) = entry_operation {
        if !roots.iter().any(|instance| instance == entry) {
            roots.push(*entry);
        }
    }
    roots.into_iter().map(|instance| {
        program.canonical().mir.functions.contains_key(&instance)
            .then_some(instance)
            .ok_or_else(|| miette!("Compiler callable `{instance:?}` 缺少稳定实例身份"))
    }).collect()
}

#[cfg(test)]
mod import_contract_tests {
    use super::*;

    #[test]
    fn source_callable_roots_bind_to_compiler_instances() {
        let output = crate::ValkyrieCompiler::default().compile_source_to_program(
            "[export(name: \"first\")] [main] micro first() -> unit { return } \
             [export(name: \"second\")] micro second() -> unit { return }",
        ).expect("当前源码必须形成完整成功载荷");
        let program = &output;
        let operations = program.canonical().linked.callable_names.keys().copied().collect::<Vec<_>>();
        let entry = program.canonical().linked.entries.keys().next().expect("源码有显式入口");
        let roots = resolve_callable_roots(program, &operations, Some(entry))
            .expect("Compiler 组装边界必须绑定精确实例根");
        assert_eq!(roots.iter().copied().collect::<BTreeSet<_>>(), program.canonical().mir.functions.keys().copied().collect());
        assert_eq!(roots.len(), operations.len(), "入口已在根集合中时不得重复追加");
    }

    #[test]
    fn unresolved_partition_root_fails_at_compiler_assembly() {
        let output = crate::ValkyrieCompiler::default().compile_source_to_program(
            "micro answer() -> i32 { return 23 }",
        ).expect("当前源码必须编译");
        let missing = nyar_types::ItemInstanceId::from_index(999).expect("测试 identity");
        let error = resolve_callable_roots(&output, &[missing], None)
            .expect_err("分区根缺失必须在 Compiler 边界失败，不得推迟到 emitter 猜测");
        assert!(error.to_string().contains("缺少稳定实例身份"), "{error}");
    }

    #[test]
    fn compiler_surface_counts_consume_verified_canonical_exports() {
        let output = crate::ValkyrieCompiler::default().compile_source_to_program(
            "[export(name: \"first\")] [main] micro first() -> unit { return }\n[export(name: \"second\")] micro second() -> unit { return }",
        ).expect("多导出源码必须形成完整成功载荷");
        assert_eq!(build_output_surface_counts(&output), (2, 1));
        let linked = &output.canonical().linked;
        assert_eq!(linked.exports.len(), 2);
        assert!(linked.exports.keys().all(|instance| output.canonical().mir.functions.contains_key(instance)));
    }

    #[test]
    fn compiler_surface_rejects_duplicate_public_names_before_assembly() {
        let error = crate::ValkyrieCompiler::default().compile_source_to_program(
            "[export(name: \"same\")] micro first() -> unit { return }\n[export(name: \"same\")] micro second() -> unit { return }",
        ).expect_err("重复公开名不能进入装配");
        assert!(error.to_string().contains("DuplicateExportName"), "{error}");
    }

    fn source_partition_contract() -> (CompiledProgram, ArtifactPartitionPlan) {
        let program = crate::ValkyrieCompiler::default().compile_source_to_program(
            "[export(name: \"first\")] [main] micro first() -> unit { return } \
             [export(name: \"second\")] micro second() -> unit { return }",
        ).expect("当前多导出源码必须形成完整成功载荷");
        let plan = plan_artifacts_from_compiled_program(&program, CanonicalTarget::wasm(), ClrSuspendStrategy::default())
            .expect("分区必须消费已验证的 Canonical 合同");
        (program, plan)
    }

    #[test]
    fn partition_identity_survives_planning_and_assembly() {
        let (program, plan) = source_partition_contract();
        assert!(!plan.partitions.is_empty());
        for (index, partition) in plan.partitions.iter().enumerate() {
            let fragment = program.canonical().linked.fragments.get(&partition.fragment).expect("Canonical 片段");
            assert_eq!(partition.exported_operations, fragment.exported_operations);
            assert_eq!(partition.entry_operation, fragment.entry_operation);
            let assembled = assemble_fragment(&program, &plan, index).expect("身份一致的分区必须装配成功");
            assert_eq!(assembled.callable_roots, resolve_callable_roots(&program, &fragment.exported_operations, fragment.entry_operation.as_ref()).unwrap());
        }
    }

    #[test]
    fn assembly_rejects_changed_partition_and_view_identities() {
        let (program, plan) = source_partition_contract();
        let missing = ItemInstanceId::from_index(999).expect("测试 identity");
        for mutation in 0..3 {
            let mut changed = plan.clone();
            match mutation {
                0 => changed.partitions[0].exported_operations = vec![missing],
                1 => changed.partitions[0].entry_operation = Some(missing),
                _ => {
                    let identity = changed.partitions[0].fragment.clone();
                    changed.fragment_views.iter_mut().find(|view| view.fragment_id == identity)
                        .expect("片段视图").canonical_operations = vec![missing];
                }
            }
            let error = assemble_fragment(&program, &changed, 0).expect_err("不一致身份不得被 Canonical 原始根兜底掩盖");
            assert!(error.to_string().contains("callable 身份与 Canonical 片段合同不一致"), "{error}");
        }
    }

    #[test]
    fn algebraic_name_equations_cannot_rebind_callable_roots() {
        let (program, original) = source_partition_contract();
        let linked = &program.canonical().linked;
        let mut names = linked.callable_names.values();
        let left = names.next().expect("第一个 callable").clone();
        let right = names.next().expect("第二个 callable").clone();
        let target = CanonicalTarget::wasm();
        let profile = target.to_profile(None);
        let policy = projection_policy_for_target_profile(&profile).expect("目标 projection");
        let registry = emitter::bundled_backend_registry_from_canonical(&linked.fragments, &profile, &policy);
        let mut theory = nyar::RewriteTheory::default();
        theory.equate(nyar::RewriteEquation {
            left, right, phase: nyar::RewritePhase::Normalize, required_capabilities: Vec::new(),
        });
        let rewritten = ArtifactPartitionPlan::from_canonical_program(
            program.canonical(), target, theory, policy, registry, ClrSuspendStrategy::default(),
        ).expect("名称等式优化不得改写函数绑定");
        assert_ne!(rewritten.optimization.program.dimensions, original.optimization.program.dimensions,
            "测试必须实际改变等式名称视图，不能以未触发优化证明身份隔离");
        assert_eq!(rewritten.partitions.len(), original.partitions.len());
        for (index, partition) in rewritten.partitions.iter().enumerate() {
            assert_eq!(partition.exported_operations, original.partitions[index].exported_operations);
            assert_eq!(partition.entry_operation, original.partitions[index].entry_operation);
            let assembled = assemble_fragment(&program, &rewritten, index).expect("优化后稳定实例仍可装配");
            assert_eq!(assembled.callable_roots, assemble_fragment(&program, &original, index).unwrap().callable_roots);
        }
    }

}
