//! Builtin / pattern helpers for MIR lowering.
//!
//! ADR 0010: IntrinsicOpcode tables deleted. Do not restore operator→opcode maps.

use std::collections::BTreeMap;

use crate::types::{
    NamePath,
    hir::{HirAttribute, HirFunction, HirModule, ValkyrieType},
};

use super::{MirOperand, MirValueRef};

/// Fail-closed stub: IntrinsicOpcode / plain-type pattern tables deleted (ADR 0010).
pub(super) fn plain_type_pattern_matches(_ty: &ValkyrieType, _name: &NamePath) -> bool {
    false
}

/// DELETED: do not collect IntrinsicOpcode into MirModule.
pub(super) fn collect_intrinsic_opcodes(_module: &HirModule) -> BTreeMap<String, ()> {
    BTreeMap::new()
}

pub(crate) fn intrinsic_opcode_for_function(_function: &HirFunction) -> Option<()> {
    None
}

pub(super) fn extract_intrinsic_opcode(_attribute: &HirAttribute) -> Option<()> {
    None
}

pub(super) fn intrinsic_opcode_output_type(
    _opcode: &(),
    _arguments: &[MirOperand],
    _value_types: &BTreeMap<MirValueRef, ValkyrieType>,
) -> Option<ValkyrieType> {
    None
}

pub(super) fn intrinsic_opcode_for_operator(_name: &str) -> Option<()> {
    None
}

pub(super) fn array_index_call_output_type(
    _arguments: &[MirOperand],
    _value_types: &BTreeMap<MirValueRef, ValkyrieType>,
) -> Option<ValkyrieType> {
    None
}

/// `std.collection.Array.length` lowers to a call of private `[intrinsic("array.len")]` `__array_len`.
/// That symbol is not linked into the executable registry — emit `ArrayLength` instead of `Call`.
pub(super) fn is_array_len_intrinsic_symbol(symbol: &NamePath) -> bool {
    symbol.parts().last().is_some_and(|part| part.as_str() == "__array_len")
}

/// `marker.__ref_deref` is `[intrinsic("ref.deref")]` identity on class handles — not a linked function.
pub(super) fn is_ref_deref_intrinsic_symbol(symbol: &NamePath) -> bool {
    symbol.parts().last().is_some_and(|part| part.as_str() == "__ref_deref")
}

/// Overload registry routes `[intrinsic("array.push")]` to `builtin.array.push` (not a linked function).
pub(super) fn is_language_builtin_array_push_symbol(symbol: &NamePath) -> bool {
    let parts = symbol.parts();
    parts.len() == 3
        && parts[0].as_str() == "builtin"
        && parts[1].as_str() == "array"
        && parts[2].as_str() == "push"
}
