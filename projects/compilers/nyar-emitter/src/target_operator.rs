//! 目标 operator 能力绑定的冻结合同。
//!
//! `operator` 表示**目标后端能力**，不是 Valkyrie 语言层的 operator。一个 operator
//! 绑定可以是一条目标指令、多条指令组合、参数重排或常量注入，未来也可以表达
//! host call、ABI 转换或 allocation/runtime 操作；因此合同用配方
//! （[`TargetOperatorRecipe`]）描述发射步骤，而不是假定「一条指令」。
//!
//! 源码属性只在 [`TargetExecutionPlan`](crate::TargetExecutionPlan) 构造时解析一次，
//! 助记符必须解析为完整的 arity、操作数发射配方、操作数/结果载体与编码，写入
//! [`TargetOperatorPlan`]。backend 只编码这里已冻结的配方，不得再从助记符字符串
//! 推断 arity、操作数改写或结果类型，也不得为未冻结的绑定补默认指令。

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

/// 已冻结的 operator 发射配方。
///
/// operator 不保证是单条指令：配方是 backend 唯一允许执行的机械步骤序列。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOperatorRecipe {
    /// 按实参顺序发射全部实参，再执行冻结编码。
    Operands,
    /// 按给定顺序发射每一步，再执行冻结编码；backend 只机械执行配方，不按助记符
    /// 推断取负、零常量或减法。
    Steps(&'static [TargetOperandStep]),
}

/// 已冻结的 operator 编码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOperatorEncoding {
    /// WASM 单字节 opcode。
    Wasm(u8),
    /// NyarVM 头码。
    NyarVm(NyarHeadCode),
}

/// 一个已冻结的目标 operator 计划。
///
/// 计划冻结调用点所需的全部物理事实：实参个数、操作数载体、结果载体、发射配方与
/// 编码。backend 不得在此基础上再做任何推断或补全。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetOperatorPlan {
    /// 冻结的实参个数；编码前必须与调用实参个数一致。
    pub arity: usize,
    /// 实参的目标标量载体。
    pub operand_type: TargetOperandType,
    /// 结果的目标标量载体。
    pub result_type: TargetOperandType,
    /// 操作数发射配方。
    pub recipe: TargetOperatorRecipe,
    /// operator 编码。
    pub encoding: TargetOperatorEncoding,
}

/// 计划对目标 operator 绑定的冻结状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOperatorBinding {
    /// 已冻结完整 operator 计划。
    Frozen(TargetOperatorPlan),
    /// 声明了目标 operator 绑定，但该目标没有可编码配方；emit 必须结构化失败。
    Unfrozen,
}
