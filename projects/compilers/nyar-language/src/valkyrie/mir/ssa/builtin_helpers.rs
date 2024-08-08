//! Builtin / pattern helpers for MIR lowering.
//!
//! IntrinsicOpcode 表已删除；不得恢复 operator→opcode 映射。
//! 语义路径只消费 [`IntrinsicId`]，禁止按类型名末段猜；运算符没有单独身份表。

use crate::types::NamePath;
use nyar_types::IntrinsicId;

/// 迁移期：由已进入 MIR 的符号路径映到 [`IntrinsicId`]。
///
/// 委托 [`IntrinsicId::resolve_from_segments`]；禁止按类型名末段猜。
#[doc(hidden)]
pub fn resolve_intrinsic_id(symbol: &NamePath) -> Option<IntrinsicId> {
    let parts = symbol.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>();
    IntrinsicId::resolve_from_segments(&parts)
}
