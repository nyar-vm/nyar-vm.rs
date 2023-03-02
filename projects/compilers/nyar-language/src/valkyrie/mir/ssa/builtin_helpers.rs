//! Builtin / pattern helpers for MIR lowering.
//!
//! IntrinsicOpcode 表已删除；不得恢复 operator→opcode 映射。
//! 语义路径只消费 [`IntrinsicId`] / [`OperatorId`]，禁止按类型名末段猜。

use std::collections::BTreeMap;

use crate::types::{
    NamePath,
    hir::{HirAttribute, HirFunction, HirModule, ValkyrieType},
};
use nyar_types::{IntrinsicId, builtin_operator};

use super::{MirOperand, MirValueRef, infer_builder_operand_type};

/// 失败关闭桩：IntrinsicOpcode / plain-type 模式表已删除。
pub(super) fn plain_type_pattern_matches(_ty: &ValkyrieType, _name: &NamePath) -> bool {
    false
}

/// 已删除：不得向 MirModule 收集 IntrinsicOpcode。
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
/// 委托 [`IntrinsicId::resolve_from_segments`]；禁止按类型名末段猜。
pub(crate) fn resolve_intrinsic_id(symbol: &NamePath) -> Option<IntrinsicId> {
    let parts = symbol.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>();
    IntrinsicId::resolve_from_segments(&parts)
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
