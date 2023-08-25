//! Assemble language `FrontendBuildOutput` into platform/driver fragment payloads.
//!
//! Layering: `nyar-language` -> `emitter` -> `std-data`.
//! Shared ABI (`AssembledFragment`) lives in `nyar`; the driver only wraps it.

mod executable_closure;
mod link;
mod nullable;
mod suspend_payload;

use std::collections::{BTreeMap, BTreeSet};

use emitter::fragment_submission_from_assembled;
use miette::{Result as MietteResult, miette};
use nyar::{
    ArtifactPartitionPlan, BackendRegistry, CanonicalTarget, ClrSuspendStrategy, ExternalImportLink, Identifier, PlanningError,
    ProjectionPolicy, QualifiedName, SuspendConsumptionModel, TheoryBundle, VmSuspendStrategy, suspend_consumption_model_for_lane,
};
use crate::valkyrie::compile_pipeline::CanonicalRepresentationPlanner;
use nyar_types::pipeline::RepresentationPlanStage;

use crate::{
    FrontendBuildOutput, FrontendNeutralPlan, MirLowerer, NyarPlanningContract, collect_singleton_instance_plans, compute_nominal_layouts,
};

pub use link::link_reachable_dependency_mir;
pub use nullable::{FragmentNullableBoolProfile, FragmentNullableIntrinsicKind, FragmentNullableIntrinsicUse, FragmentNullableTryCall};
pub use nyar::AssembledFragment;
pub use suspend_payload::{build_first_class_suspend_payload, build_state_machine_suspend_payload};

/// Plan artifacts from `FrontendBuildOutput` using injected target and projection policy.
pub fn plan_artifacts_from_build_output(
    build_output: &FrontendBuildOutput,
    target: CanonicalTarget,
    projection_policy: ProjectionPolicy,
    backend_registry: BackendRegistry,
    clr_suspend_strategy: ClrSuspendStrategy,
) -> Result<ArtifactPartitionPlan, PlanningError> {
    let canonical = build_output.canonical_program().map_err(|error| PlanningError::SemanticContract {
        module: build_output.neutral_plan().module_name.to_string(),
        stage: nyar_types::CompileStage::ValidateMir,
        detail: format!("CanonicalProgram 建立失败: {error:?}"),
    })?;
    CanonicalRepresentationPlanner.plan(&canonical).map_err(|error| PlanningError::SemanticContract {
        module: build_output.neutral_plan().module_name.to_string(),
        stage: nyar_types::CompileStage::RepresentationPlan,
        detail: format!("RepresentationPlan 建立失败: {error:?}"),
    })?;
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

    let mir = build_output.semantic_mir().clone();
    let mut mir_seed_operations = fragment.exported_operations.clone();
    // Witness 表里的 impl 方法（`imply Type: Trait { micro method }`）在 MIR 层
    // 已经按 `{Type}.{method}` 约定降级为独立函数，但它们不会被 entry 可达闭包
    // 扫到（调用点走 witness 符号，不走 `{Type}.{method}` 直接 Call）。这里把它们
    // 作为种子加入，确保后端能拿到真实的 Valkyrie 方法体，而不是退回到 Rust mock。
    for table in &fragment.witness_tables {
        for method in &table.methods {
            let seed = QualifiedName::new(vec![Identifier::new(&table.type_name), Identifier::new(&method.method_name)]);
            if !mir_seed_operations.iter().any(|operation| operation == &seed) {
                mir_seed_operations.push(seed);
            }
        }
    }
    let executable_functions = executable_closure::build_reachable_mir_functions(&mir_seed_operations, &mir)?;

    let external_import_links =
        merge_program_external_import_links(&fragment.external_import_links, &build_output.neutral_plan().program_facts.functions)?;

    // Post-link MIR already carries dependency sum layouts (e.g. MsilOpcode from
    // CLR helpers linked into a Node ArtifactSet). Recomputing from consumer HIR
    // alone drops those and triggers SMIR006 on SumNew — same class of bug as
    // aggregate_layouts, which already reuse MIR-final plans.
    let mut sum_types = mir.sum_types.clone();
    let (hir_sum_types, flags_types) = compute_nominal_layouts(hir_module);
    for sum in hir_sum_types {
        if !sum_types.iter().any(|existing| existing.name == sum.name) {
            sum_types.push(sum);
        }
    }

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
        operation_literal_returns: fragment.operation_literal_returns.clone(),
        operation_void_returns: fragment.operation_void_returns.clone(),
        witness_tables: fragment.witness_tables.clone(),
        witness_calls: fragment.witness_calls.clone(),
        control_flow,
        suspend_runtime,
        aggregate_layouts: mir.aggregate_layouts.clone(),
        sum_types,
        flags_types,
        executable_functions,
        singleton_instances: collect_singleton_instance_plans(hir_module),
        wasm_export_names: fragment.wasm_export_names.clone(),
    })
}

/// Assemble a driver [`emitter::FragmentSubmission`] for the given partition.
///
/// Language is the upper layer and may depend on `emitter`; the driver must not
/// depend back on this crate.
pub fn assemble_fragment_submission(
    build_output: &FrontendBuildOutput,
    plan: &ArtifactPartitionPlan,
    partition_index: usize,
) -> MietteResult<emitter::FragmentSubmission> {
    let payload = assemble_fragment(build_output, plan, partition_index)?;
    Ok(fragment_submission_from_assembled(payload))
}

/// Plan artifacts from a `FrontendNeutralPlan` (for callers that only have the neutral plan).
pub fn plan_artifacts_from_neutral_plan(
    neutral_plan: &FrontendNeutralPlan,
    target: CanonicalTarget,
    projection_policy: ProjectionPolicy,
    backend_registry: BackendRegistry,
    clr_suspend_strategy: ClrSuspendStrategy,
) -> Result<ArtifactPartitionPlan, PlanningError> {
    neutral_plan.artifact_plan(target, projection_policy, backend_registry, clr_suspend_strategy)
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
