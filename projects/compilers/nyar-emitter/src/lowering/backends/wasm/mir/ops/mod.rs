//! Contract-driven Wasm physical ops.
//!
//! These modules consume already-resolved Semantic MIR only. They must not
//! invent language semantics from short names, local valtypes, layout fallbacks,
//! or default zeros/nulls.

mod call;
mod operand;
mod sum;

pub(super) use call::*;
pub(super) use operand::*;
pub(super) use sum::*;
