//! Assemble language `FrontendBuildOutput` into platform/driver fragment payloads.
//!
//! Layering: `nyar-language` -> `emitter` -> `std-data`.
//! Shared ABI (`AssembledFragment`) lives in `nyar`; the driver only wraps it.

mod nullable;
mod suspend_payload;

use std::collections::{BTreeMap, BTreeSet};

use miette::{Result as MietteResult, miette};
use nyar::{
    ArtifactPartitionPlan, CanonicalTarget, ClrSuspendStrategy, ExternalImportLink, Identifier, PlanningError,
    QualifiedName, SuspendConsumptionModel, TheoryBundle, VmSuspendStrategy, projection_policy_for_target_profile,
    suspend_consumption_model_for_lane,
};
use crate::{
    FrontendBuildOutput, collect_singleton_instance_plans,
};

pub use nullable::{FragmentNullableBoolProfile, FragmentNullableIntrinsicKind, FragmentNullableIntrinsicUse, FragmentNullableTryCall};
pub use nyar::AssembledFragment;
pub use suspend_payload::{build_first_class_suspend_payload, build_state_machine_suspend_payload};

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

    let hir_module = build_output.hir_module();
    let fragment_requires_suspend = fragment.required_capabilities.iter().any(|capability| capability.as_str() == "suspend");
    let (control_flow, suspend_runtime) = if fragment_requires_suspend {
        match suspend_consumption_model_for_lane(partition.lane, partition.clr_suspend_strategy, VmSuspendStrategy::default()) {
            SuspendConsumptionModel::FirstClass => (None, Some(build_first_class_suspend_payload(hir_module, &fragment.exported_operations))),
            SuspendConsumptionModel::StateMachine => {
                (Some(build_state_machine_suspend_payload(hir_module, &fragment.exported_operations)), None)
            }
        }
    }
    else {
        (None, None)
    };

    let mir = build_output.semantic_mir();

    let external_import_links =
        merge_program_external_import_links(&fragment.external_import_links, &build_output.neutral_plan().program_facts.functions)?;

    Ok(AssembledFragment {
        module_name: build_output.neutral_plan().module_name.to_string(),
        fragment_id: fragment.id.clone(),
        exported_operations: fragment.exported_operations.clone(),
        required_capabilities: fragment.required_capabilities.clone(),
        theory_bundle: TheoryBundle { shared: build_output.neutral_plan().rewrite_theory.clone(), fragment: fragment.rewrite_theory.clone() },
        entry_operation: fragment.entry_operation.clone(),
        external_import_links,
        external_call_edges: fragment.external_call_edges.clone(),
        internal_call_edges: fragment.internal_call_edges.clone(),
        witness_tables: fragment.witness_tables.clone(),
        witness_calls: fragment.witness_calls.clone(),
        control_flow,
        suspend_runtime,
        aggregate_layouts: mir.aggregate_layouts.clone(),
        sum_types: mir.sum_types.iter().map(crate::mir::MirSumDeclaration::physical_layout).collect(),
        flags_types: mir.flags_types.clone(),
        compiled_program: build_output.compiled_program().clone(),
        singleton_instances: collect_singleton_instance_plans(hir_module),
        wasm_export_names: fragment.wasm_export_names.clone(),
    })
}

fn merge_program_external_import_links(
    fragment_links: &BTreeMap<QualifiedName, ExternalImportLink>,
    functions: &[nyar::FunctionAnalysis],
) -> MietteResult<BTreeMap<QualifiedName, ExternalImportLink>> {
    let mut links = fragment_links.clone();
    for function in functions {
        let Some(link) = function.external_import_link.as_ref()
        else {
            continue;
        };
        if let Some(existing) = links.get(&function.symbol) {
            if existing != link {
                return Err(miette!("导入身份 `{}` 对应冲突合同", function.symbol));
            }
        } else {
            links.insert(function.symbol.clone(), link.clone());
        }
    }
    for (symbol, link) in &links {
        if link.matches_boundary("host") && link.locator_segments().is_empty() {
            return Err(miette!("host 合同 `{symbol}` 尚未由 Compiler 绑定 provider"));
        }
    }
    Ok(links)
}

#[cfg(test)]
mod import_contract_tests {
    use super::*;

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

    #[test]
    fn unresolved_host_contract_does_not_select_similarly_named_ffi() {
        let contract = QualifiedName::new(vec![Identifier::new("std"), Identifier::new("write")]);
        let ffi = QualifiedName::new(vec![Identifier::new("std"), Identifier::new("__console_write")]);
        let links = BTreeMap::from([
            (contract, ExternalImportLink::host(None, Vec::new())),
            (ffi, ExternalImportLink::host(Some(Identifier::new("wasm")), vec!["env".to_owned(), "write".to_owned()])),
        ]);
        let error = merge_program_external_import_links(&links, &[]).expect_err("unresolved provider");
        assert!(error.to_string().contains("Compiler"));
    }

    #[test]
    fn explicit_ffi_import_keeps_exact_contract() {
        let symbol = QualifiedName::new(vec![Identifier::new("binding"), Identifier::new("write")]);
        let links = BTreeMap::from([
            (symbol, ExternalImportLink::host(Some(Identifier::new("wasm")), vec!["env".to_owned(), "write".to_owned()])),
        ]);
        assert_eq!(merge_program_external_import_links(&links, &[]).expect("explicit import"), links);
    }
}
