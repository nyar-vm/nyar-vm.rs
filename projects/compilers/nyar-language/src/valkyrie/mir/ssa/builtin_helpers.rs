//! Builtin / pattern helpers for MIR lowering.
//!
//! ADR 0010: IntrinsicOpcode tables deleted. Do not restore operator→opcode maps.
//! ADR 0013: 语义路径只消费 [`IntrinsicId`] / [`OperatorId`]，禁止按类型名末段猜。

use std::collections::BTreeMap;

use crate::types::{
    NamePath,
    hir::{HirAttribute, HirFunction, HirModule, ValkyrieType},
};
use nyar_types::{IntrinsicId, builtin_operator};

use super::{MirOperand, MirValueRef, infer_builder_operand_type};

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

/// 迁移期：由已进入 MIR 的符号路径映到 [`IntrinsicId`]。
///
/// 仅识别：
/// - 私有 `[intrinsic(...)]` 种子名（`__array_len` 等，可带限定前缀）
/// - 语言 builtin 路径 `builtin.array.*` / `builtin.ref.deref`
///
/// **禁止**按 `Array.get` / 类型名末段猜 —— 那会把 `HashMap.get` 等绑错。
pub(crate) fn resolve_intrinsic_id(symbol: &NamePath) -> Option<IntrinsicId> {
    let parts = symbol.parts();
    if parts.len() == 3 && parts[0].as_str() == "builtin" {
        return match (parts[1].as_str(), parts[2].as_str()) {
            ("array", "push") => Some(IntrinsicId::ArrayPush),
            ("array", "length") | ("array", "len") => Some(IntrinsicId::ArrayLen),
            ("array", "get") => Some(IntrinsicId::ArrayGet),
            ("array", "set") => Some(IntrinsicId::ArraySet),
            ("ref", "deref") => Some(IntrinsicId::RefDeref),
            _ => None,
        };
    }
    match parts.last().map(|part| part.as_str()) {
        Some("__array_len") => Some(IntrinsicId::ArrayLen),
        Some("__array_get") => Some(IntrinsicId::ArrayGet),
        Some("__array_set") => Some(IntrinsicId::ArraySet),
        Some("__ref_deref") => Some(IntrinsicId::RefDeref),
        _ => None,
    }
}

/// Language operators lower as `Call` to display names during migration — not registry-linked.
/// When HIR omits `resolved.return_type`, infer via [`OperatorId`] 旁表。
pub(super) fn language_operator_call_return_type(
    symbol: &NamePath,
    arguments: &[MirOperand],
    value_types: &BTreeMap<MirValueRef, ValkyrieType>,
) -> Option<ValkyrieType> {
    let name = symbol.parts().last().map(|part| part.as_str()).unwrap_or("");
    let operator_id = builtin_operator::lookup_display_name(name)?;
    if builtin_operator::is_boolean_result(operator_id) {
        Some(ValkyrieType::Boolean)
    }
    else if builtin_operator::is_numeric_result(operator_id) {
        arguments.first().and_then(|arg| infer_builder_operand_type(arg, value_types))
    }
    else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Identifier;

    #[test]
    fn resolve_intrinsic_id_maps_builtin_paths_and_private_seeds() {
        let push = NamePath::new(vec![Identifier::new("builtin"), Identifier::new("array"), Identifier::new("push")]);
        assert_eq!(resolve_intrinsic_id(&push), Some(IntrinsicId::ArrayPush));
        let len = NamePath::new(vec![Identifier::new("__array_len")]);
        assert_eq!(resolve_intrinsic_id(&len), Some(IntrinsicId::ArrayLen));
        let qualified_deref = NamePath::new(vec![Identifier::new("marker"), Identifier::new("__ref_deref")]);
        assert_eq!(resolve_intrinsic_id(&qualified_deref), Some(IntrinsicId::RefDeref));
        // 禁止按类型名末段猜
        let hashmap_get = NamePath::new(vec![Identifier::new("HashMap"), Identifier::new("get")]);
        assert_eq!(resolve_intrinsic_id(&hashmap_get), None);
        let array_get_method = NamePath::new(vec![Identifier::new("Array"), Identifier::new("get")]);
        assert_eq!(resolve_intrinsic_id(&array_get_method), None);
    }

    #[test]
    fn language_operator_return_type_uses_operator_id() {
        let eq = NamePath::new(vec![Identifier::new("infix ==")]);
        assert_eq!(language_operator_call_return_type(&eq, &[], &BTreeMap::new()), Some(ValkyrieType::Boolean));
    }
}
