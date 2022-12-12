//! Shared witness resolution for suspend state-machine lowering.

use nyar::{SuspendFunctionArtifact, SuspendStateArtifact, SuspendWitnessBinding, WitnessMethodSlotSubmission, WitnessSubmission};

use crate::FragmentSubmission;

/// Resolved witness slot for suspend MoveNext lowering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WitnessSlot {
    pub table_label: String,
    pub impl_symbol: String,
    pub method_index: u32,
    pub method_name: String,
    /// 实现类型名（来自 witness table `type_name`），供后端解析真实 Valkyrie 方法符号
    /// `{type_name}.{method_name}`，而不是回退到 `impl_symbol` 桩。
    pub type_name: String,
    /// Trait 名（来自 witness table `trait_name`），供后端诊断与符号解析。
    pub trait_name: String,
}

pub(crate) fn first_spill_field(state: &SuspendStateArtifact) -> Option<&str> {
    state.spill_fields.first().map(String::as_str)
}

/// Witness receiver field: `spill_fields[0]` or `__witness_payload_0` fallback.
pub(crate) fn witness_receiver_field(state: &SuspendStateArtifact) -> Option<&str> {
    first_spill_field(state).or(Some("__witness_payload_0"))
}

pub(crate) fn frame_has_field(artifact: &SuspendFunctionArtifact, field: &str) -> bool {
    artifact.frame_fields.iter().any(|name| name == field)
        || artifact.states.iter().any(|state| state.spill_fields.iter().any(|name| name == field))
}

pub(crate) fn resolve_witness_slot(submission: &FragmentSubmission, binding: &SuspendWitnessBinding) -> Option<WitnessSlot> {
    if let Some(impl_symbol) = binding.impl_symbol.as_ref().filter(|symbol| !symbol.is_empty()) {
        let table = find_witness_table(submission, binding)?;
        return Some(WitnessSlot {
            table_label: table.table_label.clone(),
            impl_symbol: impl_symbol.clone(),
            method_index: binding.method_index,
            method_name: binding.method_name.clone(),
            type_name: table.type_name.clone(),
            trait_name: table.trait_name.clone(),
        });
    }

    let table = find_witness_table(submission, binding)?;
    let method = resolve_method_slot(table, binding)?;
    Some(WitnessSlot {
        table_label: table.table_label.clone(),
        impl_symbol: method.impl_symbol.clone(),
        method_index: method.method_index,
        method_name: method.method_name.clone(),
        type_name: table.type_name.clone(),
        trait_name: table.trait_name.clone(),
    })
}

pub(crate) fn resolve_method_slot<'a>(
    table: &'a WitnessSubmission,
    binding: &SuspendWitnessBinding,
) -> Option<&'a WitnessMethodSlotSubmission> {
    if let Some(method) = table.methods.iter().find(|method| method.method_name == binding.method_name) {
        return Some(method);
    }
    table.methods.iter().find(|method| method.method_index == binding.method_index)
}

fn find_witness_table<'a>(submission: &'a FragmentSubmission, binding: &SuspendWitnessBinding) -> Option<&'a WitnessSubmission> {
    if let Some(type_name) = binding.type_name.as_deref().filter(|name| !name.is_empty()) {
        if let Some(table) =
            submission.witness_tables.iter().find(|table| table.trait_name == binding.trait_name && table.type_name == type_name)
        {
            return Some(table);
        }
    }
    submission.witness_tables.iter().find(|table| table.trait_name == binding.trait_name)
}

pub(crate) fn primary_witness_binding(state: &SuspendStateArtifact) -> Option<&SuspendWitnessBinding> {
    state.witness_bindings.first()
}

/// 返回 suspend 状态的第二条 witness 绑定（若存在）。
///
/// 对于 `Await` / `AsyncBlock` effect，前端会发射两条绑定：index 0 为 `Future::poll`，
/// index 1 为 `Future::output`。后端在 `poll` 返回 ready 后通过该访问器取出 `output`
/// 绑定，再调用 `resolve_witness_slot` 解析槽位，从而显式取出恢复值 `T`。
///
/// 对于单绑定工件（例如旧测试 fixture、`DelegateYield`、`AsyncSpawn`），返回 `None`，
/// 后端应回退到旧有行为（不调用 `output`），以保持向后兼容。
pub(crate) fn secondary_witness_binding(state: &SuspendStateArtifact) -> Option<&SuspendWitnessBinding> {
    state.witness_bindings.get(1)
}

/// 返回 suspend 状态的第三条 witness 绑定（若存在）。
///
/// spec Task 5.1 扩展：当 `Future` impl 同时声明了 `is_cancelled` 方法时，前端会额外
/// 追加一条绑定（index 2）。后端在 `poll` 返回 true 后通过该访问器取出 `is_cancelled`
/// 绑定，调用以判断是否被取消；若返回 true 则跳过会触发 panic 的 `output` 调用，
/// 直接以 null/unit 完成恢复。
///
/// 对于只有两条绑定（无 `is_cancelled`）的工件，返回 `None`，后端应保持原有行为
/// （直接调用 `output`），以保持向后兼容。
pub(crate) fn tertiary_witness_binding(state: &SuspendStateArtifact) -> Option<&SuspendWitnessBinding> {
    state.witness_bindings.get(2)
}

pub(crate) fn submission_has_witness_tables(submission: &FragmentSubmission) -> bool {
    !submission.witness_tables.is_empty()
}
