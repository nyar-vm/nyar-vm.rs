//! Builtin / pattern helpers for MIR lowering.
//!
//! IntrinsicOpcode 表已删除；不得恢复 operator→opcode 映射。
//! 语义路径只消费 [`IntrinsicId`]，禁止按类型名末段猜；运算符没有单独身份表。
//!
//! **batch 7 收紧**：`resolve_intrinsic_id` 是迁移期遗留——允许调用点按符号路径段猜 intrinsic 身份。
//! 该旁路会绕过 adaptor/import binding，让没有后端实现的 intrinsic 悄悄生成 Semantic MIR。
//! 生产代码已不再调用此函数；调用方只能是测试或尚未迁移的临时调试入口。

use crate::types::NamePath;
use nyar_types::IntrinsicId;

/// 迁移期：由已进入 MIR 的符号路径映到 [`IntrinsicId`]。
///
/// **batch 7**：生产 lowering 路径已不再调用此函数。
/// 迁移期遗留——曾经允许调用点按符号路径段猜 intrinsic 身份，绕过 adaptor/import binding。
/// 仅保留给测试；新代码必须使用 [`HirResolvedCall.intrinsic`](crate::types::hir::HirResolvedCall::intrinsic) 结构化字段。
#[doc(hidden)]
#[deprecated(note = "迁移期旁路已移除；intrinsic 身份必须由 overload 阶段结构化绑定到 HirResolvedCall.intrinsic")]
pub fn resolve_intrinsic_id(symbol: &NamePath) -> Option<IntrinsicId> {
    let parts = symbol.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>();
    IntrinsicId::resolve_from_segments(&parts)
}
