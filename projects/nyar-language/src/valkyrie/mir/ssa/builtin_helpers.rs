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
