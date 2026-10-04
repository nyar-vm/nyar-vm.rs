//! Builtin / pattern helpers for MIR lowering.
//!
//! IntrinsicOpcode 表已删除；不得恢复 operator→opcode 映射。
//! 语义路径只消费 [`IntrinsicId`] / [`OperatorId`]，禁止按类型名末段猜。

use crate::types::NamePath;
use nyar_types::IntrinsicId;

/// 尚未支持的 primitive 类型模式不能宣称匹配。
pub(super) fn plain_type_pattern_matches(_ty: &crate::types::hir::ValkyrieType, _name: &NamePath) -> bool {
    false
}

/// 迁移期：由已进入 MIR 的符号路径映到 [`IntrinsicId`]。
///
/// 委托 [`IntrinsicId::resolve_from_segments`]；禁止按类型名末段猜。
pub(crate) fn resolve_intrinsic_id(symbol: &NamePath) -> Option<IntrinsicId> {
    let parts = symbol.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>();
    IntrinsicId::resolve_from_segments(&parts)
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
}
