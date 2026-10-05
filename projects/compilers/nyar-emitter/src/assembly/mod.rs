//! 将已验证的 `AssembledFragment` 转换为目标提交视图。
//!
//! 这里不重新解析语义；所有调用、导入、布局和导出事实都从同一份
//! `CompiledProgram` Canonical 闭包读取，目标私有计划只在本边界生成。

use miette::miette;
use nyar::AssembledFragment;

use crate::{BackendPrivatePlan, FragmentSubmission};

/// 从编译器成功载荷生成唯一的后端提交视图。
pub(crate) fn fragment_submission_from_assembled(payload: AssembledFragment) -> miette::Result<FragmentSubmission> {
    let linked = &payload.compiled_program.canonical().linked;
    let fragment = linked.fragments.get(&payload.fragment_id)
        .ok_or_else(|| miette!("Canonical 片段 `{}` 不存在", payload.fragment_id))?;
    if fragment.required_capabilities.iter().any(|capability| matches!(capability.as_str(), "suspend" | "trait-witness" | "open-witness" | "witness-dispatch")) {
        return Err(miette!("片段 `{}` 缺少已验证的 Canonical 控制流或 witness 合同", payload.fragment_id));
    }

    let exported_operations = fragment.exported_operations.clone();
    let entry_operation = fragment.entry_operation;
    let wasm_export_names = fragment.wasm_export_names.clone();
    let external_import_links = fragment.external_imports.clone();
    let external_call_edges = fragment.external_call_edges.clone();
    let internal_call_edges = fragment.internal_call_edges.clone();
    let backend_plan = BackendPrivatePlan::from_compiled_program(&payload.compiled_program, &payload.callable_roots)?;

    Ok(FragmentSubmission {
        module_name: payload.module_name,
        fragment_id: payload.fragment_id,
        exported_operations,
        required_capabilities: fragment.required_capabilities.clone(),
        theory_bundle: payload.theory_bundle,
        entry_operation,
        wasm_export_names,
        external_import_links,
        external_call_edges,
        internal_call_edges,
        witness_tables: Vec::new(),
        witness_calls: Vec::new(),
        control_flow: None,
        suspend_runtime: None,
        aggregate_layouts: linked.aggregate_layouts.clone(),
        aggregate_layout_by_nominal: linked.aggregate_layout_by_nominal.clone(),
        aggregate_layout_by_field: linked.aggregate_layout_by_field.clone(),
        aggregate_layout_by_type: linked.aggregate_layout_by_type.clone(),
        sum_variant_ids: linked.variants.keys().copied().collect(),
        sum_types: linked.sum_types.clone(),
        flags_types: linked.flags_types.clone(),
        backend_plan: std::sync::Arc::new(backend_plan),
        singleton_instances: linked.singleton_instances.clone(),
    })
}
