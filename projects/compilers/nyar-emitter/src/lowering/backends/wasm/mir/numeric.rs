//! WASM 数值指令绑定的唯一助记符表。
//!
//! `[wasm_opcode("i32.add")]` 只声明目标指令助记符；助记符到**冻结配方**的映射归
//! wasm backend 所有，并在 `TargetExecutionPlan` 构造时一次性写入计划（arity、
//! 操作数发射配方、操作数/结果载体与 opcode）。emit 侧只编码计划里的配方，不再
//! 从助记符字符串推断 arity 或操作数改写。
//!
//! 本模块只接受下表列出的助记符，未知助记符返回 `None`，计划据此冻结为
//! `TargetOpcodeBinding::Unfrozen`，由 emit 结构化失败，禁止猜默认指令。
//!
//! pin 的 `acorn_wasm`（e7ed84a）只导出部分 I32 / F64 变体与 `VALTYPE_I32/I64/F64`，
//! 没有 `VALTYPE_F32`；wasm 后端的 `Float32` 也归一到 `f64` 载体（见
//! `representation.rs`），因此本表不提供 f32 绑定，也不发明 f32 载体。
//!
//! 有 `WasmOpcode` 变体的字节走变体；其余按标准 WASM opcode 字节发射。
//! acorn 补齐这些变体后删除本地字节发射。

use acorn_wasm::WasmOpcode;

use crate::target_instruction::{
    TargetInstruction, TargetInstructionEncoding, TargetOperandRecipe, TargetOperandStep, TargetOperandType,
};

/// 整数取负没有原生指令：计划显式冻结 `0 - x` 的发射配方。
const I32_NEGATE_STEPS: &[TargetOperandStep] = &[
    TargetOperandStep::IntegerConstant { operand_type: TargetOperandType::I32, value: 0 },
    TargetOperandStep::Arguments,
];
const I64_NEGATE_STEPS: &[TargetOperandStep] = &[
    TargetOperandStep::IntegerConstant { operand_type: TargetOperandType::I64, value: 0 },
    TargetOperandStep::Arguments,
];

const fn binary(operand_type: TargetOperandType, result_type: TargetOperandType, opcode: u8) -> TargetInstruction {
    TargetInstruction {
        arity: 2,
        operand_type,
        result_type,
        recipe: TargetOperandRecipe::Operands,
        encoding: TargetInstructionEncoding::Wasm(opcode),
    }
}

const fn unary(operand_type: TargetOperandType, opcode: u8) -> TargetInstruction {
    TargetInstruction {
        arity: 1,
        operand_type,
        result_type: operand_type,
        recipe: TargetOperandRecipe::Operands,
        encoding: TargetInstructionEncoding::Wasm(opcode),
    }
}

const fn integer_negate(operand_type: TargetOperandType, steps: &'static [TargetOperandStep], sub_opcode: u8) -> TargetInstruction {
    TargetInstruction {
        arity: 1,
        operand_type,
        result_type: operand_type,
        recipe: TargetOperandRecipe::Steps(steps),
        encoding: TargetInstructionEncoding::Wasm(sub_opcode),
    }
}

/// 唯一助记符表。未知助记符返回 `None`。
pub(crate) fn freeze_wasm_numeric_binding(mnemonic: &str) -> Option<TargetInstruction> {
    Some(match mnemonic {
        // i32 算术 / 位运算：结果与实参同型。
        "i32.add" => binary(TargetOperandType::I32, TargetOperandType::I32, 0x6A),
        "i32.sub" => binary(TargetOperandType::I32, TargetOperandType::I32, 0x6B),
        "i32.mul" => binary(TargetOperandType::I32, TargetOperandType::I32, 0x6C),
        "i32.div_s" => binary(TargetOperandType::I32, TargetOperandType::I32, 0x6D),
        "i32.div_u" => binary(TargetOperandType::I32, TargetOperandType::I32, 0x6E),
        "i32.rem_s" => binary(TargetOperandType::I32, TargetOperandType::I32, 0x6F),
        "i32.rem_u" => binary(TargetOperandType::I32, TargetOperandType::I32, 0x70),
        "i32.and" => binary(TargetOperandType::I32, TargetOperandType::I32, 0x71),
        "i32.or" => binary(TargetOperandType::I32, TargetOperandType::I32, 0x72),
        "i32.xor" => binary(TargetOperandType::I32, TargetOperandType::I32, 0x73),
        "i32.shl" => binary(TargetOperandType::I32, TargetOperandType::I32, 0x74),
        "i32.shr_s" => binary(TargetOperandType::I32, TargetOperandType::I32, 0x75),
        "i32.shr_u" => binary(TargetOperandType::I32, TargetOperandType::I32, 0x77),
        // i32 比较：结果固定为 i32（0/1）。
        "i32.eq" => binary(TargetOperandType::I32, TargetOperandType::I32, 0x46),
        "i32.ne" => binary(TargetOperandType::I32, TargetOperandType::I32, 0x47),
        "i32.lt_s" => binary(TargetOperandType::I32, TargetOperandType::I32, 0x48),
        "i32.lt_u" => binary(TargetOperandType::I32, TargetOperandType::I32, 0x49),
        "i32.gt_s" => binary(TargetOperandType::I32, TargetOperandType::I32, 0x4A),
        "i32.gt_u" => binary(TargetOperandType::I32, TargetOperandType::I32, 0x4B),
        "i32.le_s" => binary(TargetOperandType::I32, TargetOperandType::I32, 0x4C),
        "i32.le_u" => binary(TargetOperandType::I32, TargetOperandType::I32, 0x4D),
        "i32.ge_s" => binary(TargetOperandType::I32, TargetOperandType::I32, 0x4E),
        "i32.ge_u" => binary(TargetOperandType::I32, TargetOperandType::I32, 0x4F),
        // 整数 neg 没有原生指令，计划冻结为 `const 0` + `sub` 的显式发射配方。
        "i32.neg" => integer_negate(TargetOperandType::I32, I32_NEGATE_STEPS, 0x6B),

        // i64 算术 / 位运算：acorn 目前只导出 I64ShrU，其余按标准字节发射。
        "i64.add" => binary(TargetOperandType::I64, TargetOperandType::I64, 0x7C),
        "i64.sub" => binary(TargetOperandType::I64, TargetOperandType::I64, 0x7D),
        "i64.mul" => binary(TargetOperandType::I64, TargetOperandType::I64, 0x7E),
        "i64.div_s" => binary(TargetOperandType::I64, TargetOperandType::I64, 0x7F),
        "i64.div_u" => binary(TargetOperandType::I64, TargetOperandType::I64, 0x80),
        "i64.rem_s" => binary(TargetOperandType::I64, TargetOperandType::I64, 0x81),
        "i64.rem_u" => binary(TargetOperandType::I64, TargetOperandType::I64, 0x82),
        "i64.and" => binary(TargetOperandType::I64, TargetOperandType::I64, 0x83),
        "i64.or" => binary(TargetOperandType::I64, TargetOperandType::I64, 0x84),
        "i64.xor" => binary(TargetOperandType::I64, TargetOperandType::I64, 0x85),
        "i64.shl" => binary(TargetOperandType::I64, TargetOperandType::I64, 0x86),
        "i64.shr_s" => binary(TargetOperandType::I64, TargetOperandType::I64, 0x87),
        "i64.shr_u" => binary(TargetOperandType::I64, TargetOperandType::I64, 0x88),
        // i64 比较：结果固定为 i32（0/1）。
        "i64.eq" => binary(TargetOperandType::I64, TargetOperandType::I32, 0x51),
        "i64.ne" => binary(TargetOperandType::I64, TargetOperandType::I32, 0x52),
        "i64.lt_s" => binary(TargetOperandType::I64, TargetOperandType::I32, 0x53),
        "i64.lt_u" => binary(TargetOperandType::I64, TargetOperandType::I32, 0x54),
        "i64.gt_s" => binary(TargetOperandType::I64, TargetOperandType::I32, 0x55),
        "i64.gt_u" => binary(TargetOperandType::I64, TargetOperandType::I32, 0x56),
        "i64.le_s" => binary(TargetOperandType::I64, TargetOperandType::I32, 0x57),
        "i64.le_u" => binary(TargetOperandType::I64, TargetOperandType::I32, 0x58),
        "i64.ge_s" => binary(TargetOperandType::I64, TargetOperandType::I32, 0x59),
        "i64.ge_u" => binary(TargetOperandType::I64, TargetOperandType::I32, 0x5A),
        "i64.neg" => integer_negate(TargetOperandType::I64, I64_NEGATE_STEPS, 0x7D),

        // f64 算术：acorn 有 add/sub/mul/div 变体。
        "f64.add" => binary(TargetOperandType::F64, TargetOperandType::F64, 0xA0),
        "f64.sub" => binary(TargetOperandType::F64, TargetOperandType::F64, 0xA1),
        "f64.mul" => binary(TargetOperandType::F64, TargetOperandType::F64, 0xA2),
        "f64.div" => binary(TargetOperandType::F64, TargetOperandType::F64, 0xA3),
        // f64 比较：结果固定为 i32（0/1）。
        "f64.eq" => binary(TargetOperandType::F64, TargetOperandType::I32, 0x61),
        "f64.ne" => binary(TargetOperandType::F64, TargetOperandType::I32, 0x62),
        "f64.lt" => binary(TargetOperandType::F64, TargetOperandType::I32, 0x63),
        "f64.le" => binary(TargetOperandType::F64, TargetOperandType::I32, 0x65),
        "f64.gt" => binary(TargetOperandType::F64, TargetOperandType::I32, 0x64),
        "f64.ge" => binary(TargetOperandType::F64, TargetOperandType::I32, 0x66),
        // 浮点 neg 必须保留 -0.0，走原生 f64.neg 而不是 `0.0 - x`。
        "f64.neg" => unary(TargetOperandType::F64, 0x9A),

        _ => return None,
    })
}

/// 发射一条目标数值指令。
///
/// 有 `WasmOpcode` 变体时优先走变体；acorn（pin e7ed84a）尚未导出的字节按标准
/// opcode 编码，补齐后删除本地字节发射。
pub(super) fn emit_opcode(byte: u8, code: &mut Vec<u8>) {
    match WasmOpcode::from_u8(byte) {
        Some(opcode) => opcode.encode(code),
        None => code.push(byte),
    }
}
