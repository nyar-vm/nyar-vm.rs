//! Suspend payload builders (ADR 0011 transitional stubs).
//!
//! Semantic MirFunction no longer carries suspend_points / continuations / suspend_plan.
//! Emit empty payloads until RepresentationPlan-owned suspend evidence is wired.

use crate::types::hir::HirModule;
use nyar::{ControlFlowPayload, QualifiedName, SuspendRuntimePayload};

/// Build state-machine control-flow payload (currently empty under slim MIR).
pub fn build_state_machine_suspend_payload(_hir_module: &HirModule, _operations: &[QualifiedName]) -> ControlFlowPayload {
    ControlFlowPayload { functions: Vec::new() }
}

/// Build first-class suspend runtime payload (currently empty under slim MIR).
pub fn build_first_class_suspend_payload(_hir_module: &HirModule, _operations: &[QualifiedName]) -> SuspendRuntimePayload {
    SuspendRuntimePayload { functions: Vec::new() }
}
