//! 内码：由已验证外码确定性预解码得到的执行表示。
//!
//! 内码不是第二套可分发字节码。跳转目标已解析为函数内指令下标；
//! `source_pc` 仅用于诊断与将来的 deopt 映射。

use std::collections::BTreeMap;

use nyar_bytecode::{NyarFunction, NyarHeadCode, NyarInstruction, decode_at};

use crate::error::NyarRuntimeError;

/// 函数内指令下标。
pub type InstructionIndex = u32;

/// 单条预解码执行指令。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecOp {
    /// 操作码。
    pub code: NyarHeadCode,
    /// 第一立即数（local / const / import / layout 等下标，或未解析时的原始跳转偏移）。
    pub operand1: i32,
    /// 第二立即数（如 argc）。
    pub operand2: i32,
    /// 对 `Jump` / `JumpIf*`：函数内目标指令下标；其它指令为 0。
    pub target: InstructionIndex,
    /// 外码字节 pc（指令起始），供诊断使用。
    pub source_pc: u32,
}

impl ExecOp {
    /// 构造供旧版 handler 使用的合成指令；`size` 恒为 1，使 `ip += size` 变为内码步进。
    pub fn as_instruction(self) -> NyarInstruction {
        NyarInstruction {
            code: self.code,
            size: 1,
            operand1: self.operand1,
            operand2: self.operand2,
            operand3: 0,
        }
    }
}

/// 单个函数的内码。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutableFunction {
    /// 预解码指令序列。
    pub ops: Vec<ExecOp>,
    /// 可作为 GC safepoint 的指令下标（调用、分配、挂起、导入等）。
    pub safepoints: Vec<InstructionIndex>,
    /// 外码 `code_offset`（诊断）。
    pub source_code_offset: u32,
    /// 外码 `code_length`（诊断）。
    pub source_code_length: u32,
}

impl ExecutableFunction {
    /// 空函数。
    pub fn empty() -> Self {
        Self {
            ops: Vec::new(),
            safepoints: Vec::new(),
            source_code_offset: 0,
            source_code_length: 0,
        }
    }
}

/// 从已验证模块的函数范围构建内码。
pub fn build_executable_function(code_bytes: &[u8], function: &NyarFunction) -> Result<ExecutableFunction, NyarRuntimeError> {
    if function.code_offset < 0 || function.code_length < 0 {
        return Err(NyarRuntimeError::ModuleLoad("negative function code range".into()));
    }
    let start = function.code_offset as usize;
    let length = function.code_length as usize;
    let end = start.checked_add(length).ok_or_else(|| NyarRuntimeError::ModuleLoad("function code range overflow".into()))?;
    if end > code_bytes.len() {
        return Err(NyarRuntimeError::ModuleLoad(format!(
            "function code range [{start}, {end}) exceeds code section {}",
            code_bytes.len()
        )));
    }

    if length == 0 {
        return Ok(ExecutableFunction {
            ops: Vec::new(),
            safepoints: Vec::new(),
            source_code_offset: start as u32,
            source_code_length: 0,
        });
    }

    let mut decoded: Vec<(usize, NyarInstruction)> = Vec::new();
    let mut pc = start;
    while pc < end {
        let instruction = decode_at(code_bytes, pc);
        if instruction.size == 0 {
            return Err(NyarRuntimeError::ModuleLoad(format!("truncated instruction at pc {pc}")));
        }
        let next = pc + instruction.size as usize;
        if next > end {
            return Err(NyarRuntimeError::ModuleLoad(format!("instruction at pc {pc} crosses function end")));
        }
        decoded.push((pc, instruction));
        pc = next;
    }

    let mut pc_to_index: BTreeMap<usize, InstructionIndex> = BTreeMap::new();
    for (index, (byte_pc, _)) in decoded.iter().enumerate() {
        pc_to_index.insert(*byte_pc, index as InstructionIndex);
    }

    let mut ops = Vec::with_capacity(decoded.len());
    let mut safepoints = Vec::new();
    for (index, (byte_pc, instruction)) in decoded.iter().enumerate() {
        let mut target: InstructionIndex = 0;
        if matches!(
            instruction.code,
            NyarHeadCode::Jump | NyarHeadCode::JumpIfTrue | NyarHeadCode::JumpIfFalse
        ) {
            let target_pc = byte_pc.wrapping_add(instruction.operand1 as usize);
            target = *pc_to_index.get(&target_pc).ok_or_else(|| {
                NyarRuntimeError::ModuleLoad(format!(
                    "inner-code jump from pc {byte_pc} to {target_pc} is not an instruction boundary"
                ))
            })?;
        }
        if is_safepoint(instruction.code) {
            safepoints.push(index as InstructionIndex);
        }
        ops.push(ExecOp {
            code: instruction.code,
            operand1: instruction.operand1,
            operand2: instruction.operand2,
            target,
            source_pc: *byte_pc as u32,
        });
    }

    Ok(ExecutableFunction {
        ops,
        safepoints,
        source_code_offset: start as u32,
        source_code_length: length as u32,
    })
}

fn is_safepoint(code: NyarHeadCode) -> bool {
    matches!(
        code,
        NyarHeadCode::Call
            | NyarHeadCode::CallStatic
            | NyarHeadCode::CallImport
            | NyarHeadCode::CallIntrinsic
            | NyarHeadCode::ObjectNew
            | NyarHeadCode::Yield
            | NyarHeadCode::Resume
            | NyarHeadCode::PerformEffect
            | NyarHeadCode::Return
    )
}

/// 为模块中每个函数构建内码表。
pub fn build_executable_table(code_bytes: &[u8], functions: &[NyarFunction]) -> Result<Vec<ExecutableFunction>, NyarRuntimeError> {
    functions.iter().map(|function| build_executable_function(code_bytes, function)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nyar_bytecode::{NyarHeadCode, emit_imm1};

    #[test]
    fn resolves_jump_to_instruction_index() {
        let mut code = Vec::new();
        // 0: Jump -> Return (byte pc of Return)
        let jump_pc = 0usize;
        emit_imm1(&mut code, NyarHeadCode::Jump, 0);
        // unreachable Const
        emit_imm1(&mut code, NyarHeadCode::Const, 0);
        let return_pc = code.len();
        code.push(NyarHeadCode::Return as u8);
        let offset = (return_pc as i32) - (jump_pc as i32);
        code[1..5].copy_from_slice(&offset.to_le_bytes());

        let function = NyarFunction {
            name: "main".into(),
            arity: 0,
            local_count: 0,
            code_offset: 0,
            code_length: code.len() as i32,
        };
        let exec = build_executable_function(&code, &function).expect("build");
        assert_eq!(exec.ops.len(), 3);
        assert_eq!(exec.ops[0].code, NyarHeadCode::Jump);
        assert_eq!(exec.ops[0].target, 2); // Return is instruction index 2
        assert_eq!(exec.ops[2].code, NyarHeadCode::Return);
        assert!(exec.safepoints.contains(&2));
    }
}
