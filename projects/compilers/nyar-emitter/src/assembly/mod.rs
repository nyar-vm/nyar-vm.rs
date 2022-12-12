//! Convert platform [`nyar::AssembledFragment`] into driver [`FragmentSubmission`]s.
//!
//! Layering: `nyar-language` → `emitter` → `std-data`.
//! The driver must not depend on language crates; frontends hand over [`AssembledFragment`].

use std::sync::Arc;

use nyar::AssembledFragment;

use crate::{
    FragmentSubmission,
    executable_provider::{ExecutableProvider, MirFunctionMapProvider},
};

/// Build a [`FragmentSubmission`] from a frontend [`AssembledFragment`].
pub fn fragment_submission_from_assembled(payload: AssembledFragment) -> FragmentSubmission {
    let executable = Some(Arc::new(MirFunctionMapProvider::new(payload.executable_functions)) as Arc<dyn ExecutableProvider>);
    FragmentSubmission {
        module_name: payload.module_name,
        fragment_id: payload.fragment_id,
        exported_operations: payload.exported_operations,
        required_capabilities: payload.required_capabilities,
        theory_bundle: payload.theory_bundle,
        entry_operation: payload.entry_operation,
        external_import_links: payload.external_import_links,
        external_call_edges: payload.external_call_edges,
        internal_call_edges: payload.internal_call_edges,
        operation_literal_returns: payload.operation_literal_returns,
        operation_void_returns: payload.operation_void_returns,
        witness_tables: payload.witness_tables,
        witness_calls: payload.witness_calls,
        control_flow: payload.control_flow,
        suspend_runtime: payload.suspend_runtime,
        aggregate_layouts: payload.aggregate_layouts,
        sum_types: payload.sum_types,
        flags_types: payload.flags_types,
        executable,
        singleton_instances: payload.singleton_instances,
    }
}
