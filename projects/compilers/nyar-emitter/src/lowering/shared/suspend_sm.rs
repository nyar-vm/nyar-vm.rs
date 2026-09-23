//! Shared suspend state-machine helpers for driver lowering.

use nyar::{ControlFlowPayload, SuspendFunctionArtifact, SuspendStateArtifact};

use crate::FragmentSubmission;

/// Dispatch case keys for `__state` compare/jump lowering (CLR/JVM/WASM/Native aligned).
pub(crate) fn dispatch_case_keys(artifact: &SuspendFunctionArtifact) -> Vec<u32> {
    if !artifact.dispatch_cases.is_empty() {
        let mut keys: Vec<u32> = artifact.dispatch_cases.iter().map(|case| case.case_key).collect();
        keys.sort_unstable();
        keys.dedup();
        keys
    }
    else {
        (0..=artifact.states.len() as u32).collect()
    }
}

/// Resolve the suspend state artifact for a dispatch case key.
pub(crate) fn resolve_state_for_case<'a>(artifact: &'a SuspendFunctionArtifact, case_key: u32) -> Option<&'a SuspendStateArtifact> {
    if case_key == 0 { artifact.states.first() } else { artifact.states.iter().find(|state| state.resume_case_key == case_key) }
}

/// Total suspend steps across all functions (sum of state counts).
pub(crate) fn suspend_state_count(submission: &FragmentSubmission) -> u32 {
    submission
        .control_flow
        .as_ref()
        .map(|payload| payload.functions.iter().map(|function| u32::try_from(function.states.len()).unwrap_or(u32::MAX)).sum())
        .unwrap_or(0)
}

/// First suspend function artifact when present.
pub(crate) fn first_suspend_function(submission: &FragmentSubmission) -> Option<&SuspendFunctionArtifact> {
    submission.control_flow.as_ref()?.functions.first()
}

/// Whether submission carries statically resolved witness metadata.
pub(crate) fn has_resolved_witness_metadata(submission: &FragmentSubmission) -> bool {
    submission
        .control_flow
        .as_ref()
        .is_some_and(|payload| payload.functions.iter().flat_map(|function| &function.states).any(|state| !state.witness_bindings.is_empty()))
        || !submission.witness_calls.is_empty()
}

pub(crate) fn effect_is(effect: &str, expected: &str) -> bool {
    effect == expected
}

pub(crate) fn state_has_witness(state: &SuspendStateArtifact, trait_name: &str) -> bool {
    state.witness_bindings.iter().any(|binding| binding.trait_name == trait_name)
}

pub(crate) fn payload_from_control_flow(payload: &ControlFlowPayload) -> &ControlFlowPayload {
    payload
}
