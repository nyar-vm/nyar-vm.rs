//! Driver-side executable query interface.
//!
//! `emitter` should treat language executable structure as private and only
//! consume it through this query surface and the helper types defined here.

pub type NyarType = nyar::NyarType;
pub type ExecutableValueRef = crate::contracts::ValueRef;
pub type ExecutableBlockRef = crate::contracts::BlockRef;
pub type ExecutableOperand = crate::contracts::Operand;
pub type ExecutableConstant = crate::contracts::Constant;
pub type ExecutableDispatchKind = crate::contracts::DispatchKind;
pub type ExecutableStorageKind = crate::contracts::StorageKind;
pub type ExecutableReceiverPassingKind = crate::contracts::ReceiverPassingKind;
pub type ExecutableInstruction = crate::contracts::Instruction;
pub type ExecutableInstructionKind = crate::contracts::InstructionKind;
pub type ExecutableTerminator = crate::contracts::Terminator;
pub type ExecutableDiagnostic = crate::contracts::Diagnostic;
pub type ExecutableValue = crate::contracts::Value;
pub type ExecutableSuspendPoint = crate::contracts::SuspendPoint;
pub type ExecutableFrameLayout = crate::contracts::FrameLayout;
pub type ExecutableContinuation = crate::contracts::Continuation;
pub type ExecutableCaseChain = crate::contracts::CaseChain;
pub type ExecutableBlock = crate::contracts::Block;
pub type ExecutableSuspendPlan = crate::contracts::SuspendLoweringPlan;
pub type ExecutableFunction = crate::contracts::ExecutableFunction;

/// A driver-side view of function-level executable semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionView {
    pub function: ExecutableFunction,
}

impl FunctionView {
    pub fn symbol(&self) -> &str {
        &self.function.symbol
    }

    pub fn blocks(&self) -> &[ExecutableBlock] {
        &self.function.blocks
    }

    pub fn suspend_points(&self) -> &[ExecutableSuspendPoint] {
        &self.function.suspend_points
    }

    pub fn case_chains(&self) -> &[ExecutableCaseChain] {
        &self.function.case_chains
    }
}

/// Minimal suspend metadata view required by suspend-aware backends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuspendMetadataView {
    pub function_symbol: String,
    pub suspend_points: Vec<ExecutableSuspendPoint>,
    pub frame_layouts: Vec<ExecutableFrameLayout>,
    pub continuations: Vec<ExecutableContinuation>,
    pub case_chains: Vec<ExecutableCaseChain>,
}

impl SuspendMetadataView {
    pub fn from_function(function: &ExecutableFunction) -> Option<Self> {
        let suspend_plan = function.suspend_plan.as_ref()?;
        // `suspend_plan` is canonical; still keep points/layouts as explicit slices for consumers.
        let _ = suspend_plan;
        Some(Self {
            function_symbol: function.symbol.clone(),
            suspend_points: function.suspend_points.clone(),
            frame_layouts: function.frame_layouts.clone(),
            continuations: function.continuations.clone(),
            case_chains: function.case_chains.clone(),
        })
    }
}
