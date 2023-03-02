//! 驱动侧后端可执行合同（backend-private）。
//!
//! **规则**：后端 lowering 只依赖驱动拥有的可执行视图与辅助物，
//! 不得依赖 `nyar-language` 的 MIR 类型。
//!
//! 下列 dispatch / receiver 枚举是 emitter 私有的 BackendPrivatePlan 路由提示，
//! 不得重新挂回 [`nyar_types::InstructionKind::Call`]。

pub use nyar_types::{
    ArrayInitialization, Block, BlockRef, CarrierTable, CaseArm, CaseChain, Constant, Continuation, Diagnostic, EffectKind, ExecutableFunction,
    FrameLayout, FrameSlot, Instruction, InstructionKind, Operand, StorageKind, SuspendLoweringPlan, SuspendPoint, SuspendState, Terminator,
    Value, ValueOrigin, ValueRef,
};

/// 后端私有的调用路由（不是 Semantic MIR 权威）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DispatchKind {
    /// 直接 / 已知 callee。
    Static,
    /// 函数值 / 间接调用。
    Indirect,
    /// Witness / trait 分派。
    Witness,
}

/// 后端私有的 receiver ABI 提示（不是 Semantic MIR 权威）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReceiverPassingKind {
    /// 按值传递。
    ByValue,
    /// 按地址 / 引用传递。
    ByAddress,
}

/// Primary SSA result of an instruction (`results[0]`), replacing deleted `Instruction.output`.
#[inline]
pub fn instruction_primary_result(instruction: &Instruction) -> Option<ValueRef> {
    instruction.results.first().copied()
}
