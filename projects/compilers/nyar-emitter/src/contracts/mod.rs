//! Driver-side backend executable contracts (backend-private).
//!
//! **Rule**: backend lowering code must depend on driver-owned executable views and helpers,
//! not on `nyar-language` MIR types.
//!
//! Dispatch / receiver enums below are **emitter-private** routing hints for BackendPrivatePlan.
//! They must not be reintroduced onto `nyar_types::InstructionKind::Call` (ADR 0010).

pub use nyar_types::{
    ArrayInitialization, Block, BlockRef, CarrierTable, CaseArm, CaseChain, Constant, Continuation, Diagnostic, EffectKind, ExecutableFunction,
    FrameLayout, FrameSlot, Instruction, InstructionKind, Operand, StorageKind, SuspendLoweringPlan, SuspendPoint, SuspendState, Terminator,
    Value, ValueOrigin, ValueRef,
};

/// Backend-private call routing (not Semantic MIR authority).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DispatchKind {
    /// Direct / known callee.
    Static,
    /// Function-value / indirect call.
    Indirect,
    /// Witness / trait dispatch.
    Witness,
}

/// Backend-private receiver ABI hint (not Semantic MIR authority).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReceiverPassingKind {
    /// Pass by value.
    ByValue,
    /// Pass by address / reference.
    ByAddress,
}

/// Primary SSA result of an instruction (`results[0]`), replacing deleted `Instruction.output`.
#[inline]
pub fn instruction_primary_result(instruction: &Instruction) -> Option<ValueRef> {
    instruction.results.first().copied()
}
