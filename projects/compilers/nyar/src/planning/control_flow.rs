//! Suspend artifacts carried from frontend to driver backends.
//!
//! Backends fall into two consumption models:
//! - [`SuspendConsumptionModel::FirstClass`]: native `PerformEffect` / `YieldToRuntime` + continuation runtime
//!   (nyar-vm, future CLR coroutine runtime).
//! - [`SuspendConsumptionModel::StateMachine`]: explicit `MoveNext` / asyncify lowering
//!   (JVM, WASM, legacy MSIL PE writer).

use nyar_types::QualifiedName;

use crate::packaging::TargetLane;

/// How a backend lane consumes suspend metadata at the driver boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuspendConsumptionModel {
    /// Native continuations; backend owns resume dispatch (`PerformEffect` / `YieldToRuntime` intact).
    FirstClass,
    /// Explicit state-machine rewrite (`ControlFlowPayload` with dispatch cases and frame types).
    StateMachine,
}

/// Resolve suspend consumption model for a target lane and optional backend strategies.
pub fn suspend_consumption_model_for_lane(
    lane: TargetLane,
    clr_strategy: crate::backends::clr::ClrSuspendStrategy,
    vm_strategy: crate::backends::vm::VmSuspendStrategy,
) -> SuspendConsumptionModel {
    match lane {
        TargetLane::Vm => vm_strategy.consumption_model(),
        TargetLane::Clr => clr_strategy.consumption_model(),
        _ => suspend_consumption_model(lane),
    }
}

/// Resolve suspend consumption model for a target lane.
pub fn suspend_consumption_model(lane: TargetLane) -> SuspendConsumptionModel {
    match lane {
        TargetLane::Vm => SuspendConsumptionModel::FirstClass,
        TargetLane::Clr | TargetLane::Jvm | TargetLane::Wasm | TargetLane::Native | TargetLane::Gpu => SuspendConsumptionModel::StateMachine,
    }
}

/// First-class suspend payload: continuation plan without state-machine type emission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuspendRuntimePayload {
    /// Each suspend-capable function's runtime continuation artifact.
    pub functions: Vec<SuspendRuntimeFunctionArtifact>,
}

/// Serialized catch/resume continuation metadata for driver consumption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuspendContinuationArtifact {
    /// Stable continuation index inside the owning function.
    pub index: usize,
    /// Lane-neutral continuation carrier name.
    pub carrier: String,
    /// Handler dispatch block label in semantic MIR.
    pub dispatch_block_label: String,
    /// Resume block label after handler arm matching.
    pub resume_block_label: String,
    /// Handler exit merge block label.
    pub handler_exit_block_label: String,
    /// Number of resume parameters expected by the resume block.
    pub resume_parameter_count: usize,
}

/// Single suspend function artifact for first-class backends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuspendRuntimeFunctionArtifact {
    /// Stable symbol.
    pub symbol: QualifiedName,
    /// Entry block label in semantic MIR (pre-rewrite).
    pub entry_block_label: String,
    /// Frame slot field names shared across suspend states.
    pub frame_fields: Vec<String>,
    /// Per-suspend-point state metadata.
    pub states: Vec<SuspendStateArtifact>,
    /// Catch/resume continuation metadata.
    pub continuations: Vec<SuspendContinuationArtifact>,
}

/// 控制流载荷：suspend 分区提交给驱动层的结构化状态机描述。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlFlowPayload {
    /// 每个可挂起函数的状态机 artifact。
    pub functions: Vec<SuspendFunctionArtifact>,
}

/// 单个 suspend 函数的状态机 artifact。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuspendFunctionArtifact {
    /// 稳定符号。
    pub symbol: QualifiedName,
    /// 嵌套 state-machine 类型名。
    pub state_machine_type: String,
    /// dispatch 读取的 state 字段名。
    pub state_field: String,
    /// frame 字段（含 spill slots，不含 state 字段）。
    pub frame_fields: Vec<String>,
    /// dispatch switch 映射。
    pub dispatch_cases: Vec<SuspendDispatchCase>,
    /// 各 suspend 状态描述。
    pub states: Vec<SuspendStateArtifact>,
    /// Catch/resume continuation metadata.
    pub continuations: Vec<SuspendContinuationArtifact>,
}

/// dispatch case：`case_key` → 目标 block 标签。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuspendDispatchCase {
    pub case_key: u32,
    pub block_label: String,
}

/// Witness dispatch slot referenced by a suspend state (e.g. `Iterator::next`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuspendWitnessBinding {
    /// Trait protocol name (`Iterator`, `Future`, …).
    pub trait_name: String,
    /// Method name on the witness table.
    pub method_name: String,
    /// Stable slot index in the witness table.
    pub method_index: u32,
    /// Statically resolved implementing type when known.
    pub type_name: Option<String>,
    /// Target lowering symbol when statically resolved.
    pub impl_symbol: Option<String>,
}

/// 单个 suspend 状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuspendStateArtifact {
    pub state_id: u32,
    pub effect: String,
    pub resume_case_key: u32,
    pub frame_carrier: String,
    pub spill_fields: Vec<String>,
    pub suspend_block_label: String,
    pub resume_block_label: String,
    pub resume_parameter_count: usize,
    /// Statically resolved witness calls consumed by this suspend state.
    pub witness_bindings: Vec<SuspendWitnessBinding>,
    /// Linked continuation index when suspend is an uncaught `Raise`.
    pub continuation_index: Option<usize>,
}
