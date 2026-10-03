//! Assemble language `FrontendBuildOutput` into platform/driver fragment payloads.
//!
//! Layering: `nyar-language` -> `emitter` -> `std-data`.
//! Shared ABI (`AssembledFragment`) lives in `nyar`; the driver only wraps it.

mod nullable;

use std::collections::{BTreeMap, BTreeSet};

use miette::{Result as MietteResult, miette};
use nyar::{
    ArtifactPartitionPlan, CanonicalTarget, ClrSuspendStrategy, ExternalImportLink, Identifier, PlanningError,
    QualifiedName, TheoryBundle, projection_policy_for_target_profile,
};
use nyar_types::ItemInstanceId;
use crate::{
    FrontendBuildOutput,
};

pub use nullable::{FragmentNullableBoolProfile, FragmentNullableIntrinsicKind, FragmentNullableIntrinsicUse, FragmentNullableTryCall};
pub use nyar::AssembledFragment;

/// 返回已解析的导出/入口数量；装配器不直接读取语义计划。
pub fn build_output_surface_counts(build_output: &FrontendBuildOutput) -> (usize, usize) {
    let linked = &build_output.compiled_program().canonical().linked;
    (linked.exports.len(), linked.entries.len())
}

/// Plan artifacts from `FrontendBuildOutput` using injected target and projection policy.
pub fn plan_artifacts_from_build_output(
    build_output: &FrontendBuildOutput,
    target: CanonicalTarget,
    clr_suspend_strategy: ClrSuspendStrategy,
) -> Result<ArtifactPartitionPlan, PlanningError> {
    let target_profile = target.to_profile(None);
    let projection_policy = projection_policy_for_target_profile(&target_profile).map_err(|error| PlanningError::SemanticContract {
        module: build_output.neutral_plan().module_name.to_string(),
        stage: nyar_types::CompileStage::RepresentationPlan,
        detail: format!("目标 projection 合同失败: {error:?}"),
    })?;
    let backend_registry = emitter::bundled_backend_registry(&build_output.neutral_plan().semantic_fragments, &target_profile, &projection_policy);
    build_output.neutral_plan().artifact_plan(target, projection_policy, backend_registry, clr_suspend_strategy)
}

/// Assemble a platform [`AssembledFragment`] for the given partition.
pub fn assemble_fragment(
    build_output: &FrontendBuildOutput,
    plan: &ArtifactPartitionPlan,
    partition_index: usize,
) -> MietteResult<AssembledFragment> {
    if plan.module_name != build_output.neutral_plan().module_name {
        return Err(miette!("前端计划与分区计划不匹配"));
    }
    if partition_index >= plan.partitions.len() {
        return Err(miette!("分区索引 `{partition_index}` 超出范围"));
    }
    let partition = &plan.partitions[partition_index];
    let fragment = build_output
        .neutral_plan()
        .semantic_fragments
        .iter()
        .find(|fragment| fragment.id == partition.fragment)
        .ok_or_else(|| miette!("分区 `{}` 对应的语义片段不存在", partition.name))?;

    let fragment_requires_suspend = fragment.required_capabilities.iter().any(|capability| capability.as_str() == "suspend");
    if fragment_requires_suspend {
        return Err(miette!(
            "分区 `{}` 要求 suspend，但 Compiler 尚未提供已验证的 Canonical suspend 合同",
            partition.name
        ));
    }

    let external_import_links = canonical_external_import_links(build_output.compiled_program())?;
    let callable_roots = resolve_callable_roots(build_output.compiled_program(), &fragment.exported_operations, fragment.entry_operation.as_ref())?;

    Ok(AssembledFragment {
        module_name: build_output.neutral_plan().module_name.to_string(),
        fragment_id: fragment.id.clone(),
        exported_operations: fragment.exported_operations.clone(),
        required_capabilities: fragment.required_capabilities.clone(),
        theory_bundle: TheoryBundle { shared: build_output.neutral_plan().rewrite_theory.clone(), fragment: fragment.rewrite_theory.clone() },
        entry_operation: fragment.entry_operation.clone(),
        callable_roots,
        external_import_links,
        external_call_edges: fragment.external_call_edges.clone(),
        internal_call_edges: fragment.internal_call_edges.clone(),
        witness_tables: fragment.witness_tables.clone(),
        witness_calls: fragment.witness_calls.clone(),
        control_flow: None,
        suspend_runtime: None,
        aggregate_layouts: build_output.compiled_program().canonical().linked.aggregate_layouts.clone(),
        sum_types: build_output.compiled_program().canonical().linked.sum_types.clone(),
        flags_types: build_output.compiled_program().canonical().linked.flags_types.clone(),
        compiled_program: build_output.compiled_program().clone(),
        singleton_instances: build_output.compiled_program().canonical().linked.singleton_instances.clone(),
        wasm_export_names: fragment.wasm_export_names.clone(),
    })
}

fn resolve_callable_roots(
    program: &nyar_types::CompiledProgram,
    exported_operations: &[QualifiedName],
    entry_operation: Option<&QualifiedName>,
) -> MietteResult<Vec<ItemInstanceId>> {
    let mut names = exported_operations.to_vec();
    if let Some(entry) = entry_operation {
        if !names.iter().any(|name| name == entry) {
            names.push(entry.clone());
        }
    }
    names.into_iter().map(|name| {
        let mut candidates = program.canonical().linked.callable_names.iter().filter(|(_, candidate)| *candidate == &name);
        let instance = candidates.next().map(|(instance, _)| *instance)
            .ok_or_else(|| miette!("Compiler callable `{name}` 缺少稳定实例身份"))?;
        if candidates.next().is_some() {
            return Err(miette!("Compiler callable ABI 名称 `{name}` 对应多个实例，拒绝选择第一个实例"));
        }
        Ok(instance)
    }).collect()
}

fn canonical_external_import_links(
    program: &nyar_types::CompiledProgram,
) -> MietteResult<BTreeMap<QualifiedName, ExternalImportLink>> {
    let linked = &program.canonical().linked;
    linked.imports.values().map(|record| {
        let symbol = linked.callable_names.get(&record.callee)
            .ok_or_else(|| miette!("Compiler 外部导入 `{}` 缺少 callable identity", record.capability))?
            .clone();
        Ok((symbol, record.link.clone()))
    }).collect()
}

#[cfg(test)]
mod import_contract_tests {
    use super::*;

    #[test]
    fn source_callable_roots_bind_to_compiler_instances() {
        let output = crate::ValkyrieCompiler::default().compile_source_to_build_output(
            "[export(name: \"first\")] [main] micro first() -> unit { return } \
             [export(name: \"second\")] micro second() -> unit { return }",
        ).expect("当前源码必须形成完整成功载荷");
        let program = output.compiled_program();
        let operations = program.canonical().linked.callable_names.values().cloned().collect::<Vec<_>>();
        let entry = program.canonical().linked.entries.keys().next().expect("源码有显式入口");
        let entry_name = &program.canonical().linked.callable_names[entry];
        let roots = resolve_callable_roots(program, &operations, Some(entry_name))
            .expect("Compiler 组装边界必须绑定精确实例根");
        assert_eq!(roots.iter().copied().collect::<BTreeSet<_>>(), program.canonical().mir.functions.keys().copied().collect());
        assert_eq!(roots.len(), operations.len(), "入口已在根集合中时不得重复追加");
    }

    #[test]
    fn unresolved_partition_root_fails_at_compiler_assembly() {
        let output = crate::ValkyrieCompiler::default().compile_source_to_build_output(
            "micro answer() -> i32 { return 23 }",
        ).expect("当前源码必须编译");
        let missing = QualifiedName::new(vec![Identifier::new("missing")]);
        let error = resolve_callable_roots(output.compiled_program(), &[missing], None)
            .expect_err("分区根缺失必须在 Compiler 边界失败，不得推迟到 emitter 猜测");
        assert!(error.to_string().contains("缺少稳定实例身份"), "{error}");
    }

    #[test]
    fn compiler_surface_counts_consume_verified_canonical_exports() {
        let output = crate::ValkyrieCompiler::default().compile_source_to_build_output(
            "[export(name: \"first\")] [main] micro first() -> unit { return }\n[export(name: \"second\")] micro second() -> unit { return }",
        ).expect("多导出源码必须形成完整成功载荷");
        assert_eq!(build_output_surface_counts(&output), (2, 1));
        let linked = &output.compiled_program().canonical().linked;
        assert_eq!(linked.exports.len(), 2);
        assert!(linked.exports.keys().all(|instance| output.compiled_program().canonical().mir.functions.contains_key(instance)));
    }

    #[test]
    fn compiler_surface_rejects_duplicate_public_names_before_assembly() {
        let error = crate::ValkyrieCompiler::default().compile_source_to_build_output(
            "[export(name: \"same\")] micro first() -> unit { return }\n[export(name: \"same\")] micro second() -> unit { return }",
        ).expect_err("重复公开名不能进入装配");
        assert!(error.to_string().contains("DuplicateExportName"), "{error}");
    }

}
