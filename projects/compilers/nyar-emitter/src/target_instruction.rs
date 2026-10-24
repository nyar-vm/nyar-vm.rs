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

/// 已冻结配方中的一步发射动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOperandStep {
    /// 按实参顺序发射全部实参。
    Arguments,
    /// 发射一个载体化的整数常量；整数取负所需的零常量在此显式冻结。
    IntegerConstant {
        /// 常量在目标栈上的标量载体。
        operand_type: TargetOperandType,
        /// 常量值。
        value: i32,
    },
}

/// 已冻结的操作数发射配方。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOperandRecipe {
    /// 按实参顺序发射全部实参，再执行冻结指令。
    Operands,
    /// 按给定顺序发射每一步，再执行冻结指令；backend 只机械执行配方，不按助记符
    /// 推断取负、零常量或减法。
    Steps(&'static [TargetOperandStep]),
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
