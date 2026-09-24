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
    eprintln!(
        "[seed-debug] fragment-assembly-start index={partition_index} name={} exported={}",
        partition.name,
        partition.exported_operations.len()
    );
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
    eprintln!("[seed-debug] fragment-mir-lowered index={partition_index} functions={}", mir.functions.len());
    eprintln!(
        "[seed-debug] fragment-layout-plan index={} layouts={} tuple_block_ref={}",
        partition_index,
        mir.aggregate_layouts.layouts.len(),
        mir.aggregate_layouts.layouts.iter().any(|layout| layout.name == "__tuple_ExecutableBlockRef_ExecutableBlockRef")
    );
    let singleton_names = hir_module.singletons.iter().map(|singleton| singleton.name.as_str()).collect::<Vec<_>>();
    let mut mir_seed_operations = fragment.exported_operations.clone();
    for seed in executable_closure::find_node_cli_glue_seeds(&mir) {
        if !mir_seed_operations.iter().any(|operation| operation == &seed) {
            mir_seed_operations.push(seed);
        }
    }
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
    let executable_functions = executable_closure::build_reachable_mir_functions(&mir_seed_operations, &mir, &singleton_names);
    eprintln!(
        "[seed-debug] fragment-closure-done index={partition_index} seeds={} functions={}",
        mir_seed_operations.len(),
        executable_functions.len()
    );

    let external_import_links =
        merge_program_external_import_links(&fragment.external_import_links, &build_output.neutral_plan().program_facts.functions);

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
    eprintln!("[seed-debug] fragment-layouts-done index={partition_index} sums={} flags={}", sum_types.len(), flags_types.len());

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
) -> BTreeMap<QualifiedName, ExternalImportLink> {
    let mut links = fragment_links.clone();
    for function in functions {
        let Some(link) = function.external_import_link.as_ref()
        else {
            continue;
        };
        links.entry(function.symbol.clone()).or_insert_with(|| link.clone());
    }
    resolve_host_contract_links(&mut links, functions);
    links
}

fn resolve_host_contract_links(links: &mut BTreeMap<QualifiedName, ExternalImportLink>, functions: &[nyar::FunctionAnalysis]) {
    let mut provider_for_contract: BTreeMap<QualifiedName, QualifiedName> = BTreeMap::new();
    for function in functions {
        if let Some(contract) = &function.host_provider_for {
            provider_for_contract.entry(contract.clone()).or_insert_with(|| function.symbol.clone());
        }
    }
    if provider_for_contract.is_empty() {
        return;
    }
    let contracts_to_resolve: Vec<QualifiedName> = links
        .iter()
        .filter(|(_, link)| link.matches_boundary("host") && link.locator_segments().is_empty())
        .map(|(key, _)| key.clone())
        .collect();
    for contract in contracts_to_resolve {
        let Some(provider) = provider_for_contract.get(&contract)
        else {
            continue;
        };
        let Some(resolved_link) = find_provider_ffi_link(links, provider, &contract)
        else {
            continue;
        };
        links.insert(contract, resolved_link.clone());
    }
}

fn find_provider_ffi_link<'a>(
    links: &'a BTreeMap<QualifiedName, ExternalImportLink>,
    provider: &QualifiedName,
    contract: &QualifiedName,
) -> Option<&'a ExternalImportLink> {
    let provider_parts = provider.parts();
    let provider_ns_len = provider_parts.len().saturating_sub(1);
    let contract_last = contract.parts().last()?;
    let mut best: Option<(MatchScore, &'a ExternalImportLink)> = None;
    for (key, link) in links.iter() {
        if !link.matches_boundary("host") || link.locator_segments().is_empty() {
            continue;
        }
        let key_parts = key.parts();
        if key_parts.len() <= provider_ns_len || key_parts.len() < 2 {
            continue;
        }
        if key_parts[..provider_ns_len] != provider_parts[..provider_ns_len] {
            continue;
        }
        let ffi_last = key_parts.last()?;
        let score = score_ffi_match(ffi_last.as_str(), contract_last.as_str());
        if score == MatchScore::None {
            continue;
        }
        match &best {
            None => best = Some((score, link)),
            Some((current, _)) if *current < score => best = Some((score, link)),
            _ => {}
        }
    }
    best.map(|(_, link)| link)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MatchScore {
    None,
    Partial,
    Exact,
}

fn score_ffi_match(ffi_last: &str, contract_last: &str) -> MatchScore {
    let stripped = ffi_last.strip_prefix("__").unwrap_or(ffi_last);
    if stripped == contract_last {
        return MatchScore::Exact;
    }
    if stripped.ends_with(&format!("_{}", contract_last)) {
        return MatchScore::Partial;
    }
    MatchScore::None
}
