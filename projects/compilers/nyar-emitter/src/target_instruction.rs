//! 目标指令绑定的冻结配方。
//!
//! 源码 `[wasm_opcode("...")]` / `[vm_opcode("...")]` 只在
//! [`TargetExecutionPlan`](crate::TargetExecutionPlan) 构造时解析一次：助记符必须
//! 解析为完整的 arity、操作数发射配方、操作数/结果载体与 opcode。backend 只编码
//! 这里已冻结的配方，不得再从助记符字符串推断 arity 或操作数改写，也不得为未冻结
//! 的绑定补默认指令。

use nyar_bytecode::NyarHeadCode;

/// 目标标量载体。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOperandType {
    /// 32 位整数载体。
    I32,
    /// 64 位整数载体。
    I64,
    /// 64 位浮点载体。
    F64,
}

/// 已冻结的操作数发射配方。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOperandRecipe {
    /// 按实参顺序发射全部实参，再执行冻结指令。
    Operands,
    /// 先发射目标载体零常量，再发射唯一的实参，再执行二元减法（`0 - x`）。
    IntegerNegate,
    /// 发射唯一的实参后执行原生取负（保留 `-0.0`）。
    NativeNegate,
}

/// 已冻结的指令编码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetInstructionEncoding {
    /// WASM 单字节 opcode。
    Wasm(u8),
    /// NyarVM 头码。
    NyarVm(NyarHeadCode),
}

/// 一条已冻结的目标指令绑定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetInstruction {
    /// 冻结的实参个数；编码前必须与调用实参个数一致。
    pub arity: usize,
    /// 实参的目标标量载体。
    pub operand_type: TargetOperandType,
    /// 结果的目标标量载体。
    pub result_type: TargetOperandType,
    /// 操作数发射配方。
    pub recipe: TargetOperandRecipe,
    /// 指令编码。
    pub encoding: TargetInstructionEncoding,
}

/// 计划对 `[wasm_opcode]` / `[vm_opcode]` 绑定的冻结状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOpcodeBinding {
    /// 已冻结完整编码配方。
    Frozen(TargetInstruction),
    /// 声明了目标指令绑定，但该目标没有可编码配方；emit 必须结构化失败。
    Unfrozen,
}
