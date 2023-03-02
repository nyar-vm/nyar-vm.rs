//! 已删除：IntrinsicOpcode 不得作为 Semantic MIR / Call 权威。
//!
//! 曾把共享指令面按 std/API/opcode 膨胀，绕过 `Invoke`，并让后端从注册表
//! 猜 lowering，而不是走 std adaptor 项。
//!
//! **禁止**恢复：本模块的 opcode enum、Call 侧 intrinsic 字段、
//! `MirModule.intrinsics`，以及 `emit_intrinsic_opcode*` 旁路。
//!
//! 正确路线：`std adaptor` → `ItemInstance` → `Invoke` →
//! 稀疏 RepresentationPlan → BackendPrivatePlan。

#![allow(dead_code)]

/// 墓碑：错误路线上的 intrinsic 注册表已移除。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum IntrinsicOpcodeDeleted {
    /// 仅作标记——没有任何变体可充当 Semantic MIR 权威。
    DoNotUse,
}
