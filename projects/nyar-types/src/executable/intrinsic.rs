//! DELETED — ADR 0010 / 0011.
//!
//! `IntrinsicOpcode` as Semantic MIR / Call / FragmentSubmission authority is God IR:
//! it expands the shared instruction surface by std/API/opcode, bypasses `Invoke`,
//! and lets backends invent lowering from registry tables instead of std adaptor items.
//!
//! Do **not** reintroduce this module's opcode enum, Call-side intrinsic fields,
//! `MirModule.intrinsics`, or `emit_intrinsic_opcode*` bypass paths.
//!
//! Correct route (not wired in this cleanup slice):
//! `std adaptor` → `ItemInstance` → `Invoke` → sparse RepresentationPlan → BackendPrivatePlan.

#![allow(dead_code)]

/// Tombstone: wrong-route intrinsic registry removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum IntrinsicOpcodeDeleted {
    /// Marker only — no variants are legal Semantic MIR authority.
    DoNotUse,
}
