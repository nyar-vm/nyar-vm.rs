//! Suspend 载荷构造器（过渡桩）。
//!
//! Semantic MirFunction 不再携带 suspend_points / continuations / suspend_plan。
//! 在 RepresentationPlan 拥有的 suspend 证据接通前，产出空载荷。

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
