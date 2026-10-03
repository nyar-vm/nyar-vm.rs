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
        module: build_output.compiled_program().canonical().linked.module_name.clone(),
        stage: nyar_types::CompileStage::RepresentationPlan,
        detail: format!("目标 projection 合同失败: {error:?}"),
    })?;
    let linked = &build_output.compiled_program().canonical().linked;
    let backend_registry = emitter::bundled_backend_registry_from_canonical(&linked.fragments, &target_profile, &projection_policy);
    ArtifactPartitionPlan::from_canonical_program(
        build_output.compiled_program().canonical(), target, nyar::RewriteTheory::default(), projection_policy, backend_registry, clr_suspend_strategy,
    )
}

/// Assemble a platform [`AssembledFragment`] for the given partition.
pub fn assemble_fragment(
    build_output: &FrontendBuildOutput,
    plan: &ArtifactPartitionPlan,
    partition_index: usize,
) -> MietteResult<AssembledFragment> {
    if plan.module_name.to_string() != build_output.compiled_program().canonical().linked.module_name {
        return Err(miette!("前端计划与分区计划不匹配"));
    }
    if partition_index >= plan.partitions.len() {
        return Err(miette!("分区索引 `{partition_index}` 超出范围"));
    }
    let partition = &plan.partitions[partition_index];
    let fragment_view = plan.fragment_views.iter().find(|view| view.fragment_id == partition.fragment)
        .ok_or_else(|| miette!("分区 `{}` 缺少优化后的 Canonical 片段视图", partition.name))?;
    let linked = &build_output.compiled_program().canonical().linked;
    let fragment = linked.fragments.get(&partition.fragment)
        .ok_or_else(|| miette!("分区 `{}` 对应的 Canonical 语义片段不存在", partition.name))?;

    let fragment_requires_suspend = fragment.required_capabilities.iter().any(|capability| capability.as_str() == "suspend");
    if fragment_requires_suspend {
        return Err(miette!(
            "分区 `{}` 要求 suspend，但 Compiler 尚未提供已验证的 Canonical suspend 合同",
            partition.name
        ));
    }

    let external_import_links = canonical_fragment_import_links(linked, fragment)?;
    let exported_operations = canonical_fragment_operation_names(linked, &fragment.exported_operations)?;
    let entry_operation = fragment.entry_operation.as_ref().map(|instance| callable_name(linked, *instance)).transpose()?;
    let external_call_edges = canonical_external_call_edges(linked, fragment)?;
    let internal_call_edges = canonical_internal_call_edges(linked, fragment)?;
    let wasm_export_names = canonical_wasm_export_names(linked, fragment)?;
    let callable_roots = resolve_callable_roots(build_output.compiled_program(), &fragment.exported_operations, fragment.entry_operation.as_ref())?;

    Ok(AssembledFragment {
        module_name: build_output.compiled_program().canonical().linked.module_name.clone(),
        fragment_id: fragment.id.clone(),
        exported_operations,
        required_capabilities: fragment.required_capabilities.clone(),
        theory_bundle: fragment_view.theory_bundle.clone(),
        entry_operation,
        callable_roots,
        external_import_links,
        external_call_edges,
        internal_call_edges,
        witness_tables: Vec::new(),
        witness_calls: Vec::new(),
        control_flow: None,
        suspend_runtime: None,
        aggregate_layouts: build_output.compiled_program().canonical().linked.aggregate_layouts.clone(),
        sum_types: build_output.compiled_program().canonical().linked.sum_types.clone(),
        flags_types: build_output.compiled_program().canonical().linked.flags_types.clone(),
        compiled_program: build_output.compiled_program().clone(),
        singleton_instances: build_output.compiled_program().canonical().linked.singleton_instances.clone(),
        wasm_export_names,
    })
}

fn callable_name(linked: &nyar_types::LinkedSemanticProgram, instance: ItemInstanceId) -> MietteResult<QualifiedName> {
    linked.callable_names.get(&instance).cloned()
        .ok_or_else(|| miette!("Canonical callable `{instance:?}` 缺少 ABI 名称"))
}

fn canonical_fragment_operation_names(
    linked: &nyar_types::LinkedSemanticProgram,
    instances: &[ItemInstanceId],
) -> MietteResult<Vec<QualifiedName>> {
    instances.iter().copied().map(|instance| callable_name(linked, instance)).collect()
}

fn canonical_fragment_import_links(
    linked: &nyar_types::LinkedSemanticProgram,
    fragment: &nyar_types::CanonicalFragment,
) -> MietteResult<BTreeMap<QualifiedName, ExternalImportLink>> {
    fragment.external_imports.iter().map(|(instance, link)| Ok((callable_name(linked, *instance)?, link.clone()))).collect()
}

fn canonical_external_call_edges(
    linked: &nyar_types::LinkedSemanticProgram,
    fragment: &nyar_types::CanonicalFragment,
) -> MietteResult<Vec<nyar::ExternalCallEdge>> {
    fragment.external_call_edges.iter().map(|edge| {
        let callee = linked.imports.get(&edge.import)
            .ok_or_else(|| miette!("Canonical 外部调用缺少 ImportIndex `{}`", edge.import.index()))?;
        let callee_name = callable_name(linked, callee.callee)?;
        Ok(nyar::ExternalCallEdge::new(callable_name(linked, edge.caller)?, callee_name, edge.arguments.clone()))
    }).collect()
}

fn canonical_internal_call_edges(
    linked: &nyar_types::LinkedSemanticProgram,
    fragment: &nyar_types::CanonicalFragment,
) -> MietteResult<Vec<nyar::InternalCallEdge>> {
    fragment.internal_call_edges.iter().map(|edge| {
        Ok(nyar::InternalCallEdge::new(callable_name(linked, edge.caller)?, callable_name(linked, edge.callee)?))
    }).collect()
}

fn canonical_wasm_export_names(
    linked: &nyar_types::LinkedSemanticProgram,
    fragment: &nyar_types::CanonicalFragment,
) -> MietteResult<BTreeMap<QualifiedName, String>> {
    fragment.wasm_export_names.iter().map(|(instance, name)| Ok((callable_name(linked, *instance)?, name.clone()))).collect()
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
        let operations = program.canonical().linked.callable_names.keys().copied().collect::<Vec<_>>();
        let entry = program.canonical().linked.entries.keys().next().expect("源码有显式入口");
        let roots = resolve_callable_roots(program, &operations, Some(entry))
            .expect("Compiler 组装边界必须绑定精确实例根");
        assert_eq!(roots.iter().copied().collect::<BTreeSet<_>>(), program.canonical().mir.functions.keys().copied().collect());
        assert_eq!(roots.len(), operations.len(), "入口已在根集合中时不得重复追加");
    }

    #[test]
    fn unresolved_partition_root_fails_at_compiler_assembly() {
        let output = crate::ValkyrieCompiler::default().compile_source_to_build_output(
            "micro answer() -> i32 { return 23 }",
        ).expect("当前源码必须编译");
        let missing = nyar_types::ItemInstanceId::from_index(999).expect("测试 identity");
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
