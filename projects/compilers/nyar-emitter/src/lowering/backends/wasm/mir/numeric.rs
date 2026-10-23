//! WASM 数值指令绑定的唯一助记符表。
//!
//! `[wasm_opcode("i32.add")]` 只声明目标指令助记符；助记符到编码的映射归 wasm
//! backend 所有。本模块只接受下表列出的助记符，未知助记符返回 `None`，由调用方
//! 结构化失败，禁止猜默认指令。
//!
//! pin 的 `acorn_wasm`（e7ed84a）只导出部分 I32 / F64 变体与 `VALTYPE_I32/I64/F64`，
//! 没有 `VALTYPE_F32`；wasm 后端的 `Float32` 也归一到 `f64` 载体（见
//! `representation.rs`），因此本表不提供 f32 绑定，也不发明 f32 载体。
//!
//! 有 `WasmOpcode` 变体的字节走变体；其余按标准 WASM opcode 字节发射。
//! acorn 补齐这些变体后删除本地字节发射。

use acorn_wasm::{VALTYPE_F64, VALTYPE_I32, VALTYPE_I64, WasmOpcode};

/// 数值绑定的结果栈类型合同。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WasmNumericResult {
    /// 与实参的目标标量类型相同（算术 / 位运算 / neg）。
    SameAsOperand,
    /// 比较结果固定为 `i32`（0/1）。
    I32,
}

/// 助记符的实参形态与指令编码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WasmNumericArity {
    /// 二元运算：两个实参后接指令。
    Binary { opcode: u8 },
    /// 整数 neg：先 `const 0` 再实参和 `sub`（`0 - x`，与原生 neg 等价）。
    IntegerNegate { sub_opcode: u8 },
    /// 浮点 neg：原生 `f64.neg`；不能写成 `0.0 - x`，否则会丢 `-0.0`。
    NativeNegate { opcode: u8 },
}

/// 一条已冻结的目标数值绑定：目标标量类型、实参形态与结果类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct WasmNumericBinding {
    /// 实参的目标标量类型（wasm 栈类型字节）。
    pub scalar: u8,
    /// 实参形态与指令编码。
    pub arity: WasmNumericArity,
    /// 结果栈类型。
    pub result: WasmNumericResult,
}

const fn binary(scalar: u8, opcode: u8, result: WasmNumericResult) -> WasmNumericBinding {
    WasmNumericBinding { scalar, arity: WasmNumericArity::Binary { opcode }, result }
}

const fn integer_negate(scalar: u8, sub_opcode: u8) -> WasmNumericBinding {
    WasmNumericBinding { scalar, arity: WasmNumericArity::IntegerNegate { sub_opcode }, result: WasmNumericResult::SameAsOperand }
}

const fn native_negate(scalar: u8, opcode: u8) -> WasmNumericBinding {
    WasmNumericBinding { scalar, arity: WasmNumericArity::NativeNegate { opcode }, result: WasmNumericResult::SameAsOperand }
}

impl WasmNumericBinding {
    /// 唯一助记符表。未知助记符返回 `None`。
    pub(super) fn from_mnemonic(mnemonic: &str) -> Option<Self> {
        Some(match mnemonic {
            // i32 算术 / 位运算：结果与实参同型。
            "i32.add" => binary(VALTYPE_I32, 0x6A, WasmNumericResult::SameAsOperand),
            "i32.sub" => binary(VALTYPE_I32, 0x6B, WasmNumericResult::SameAsOperand),
            "i32.mul" => binary(VALTYPE_I32, 0x6C, WasmNumericResult::SameAsOperand),
            "i32.div_s" => binary(VALTYPE_I32, 0x6D, WasmNumericResult::SameAsOperand),
            "i32.rem_s" => binary(VALTYPE_I32, 0x6F, WasmNumericResult::SameAsOperand),
            "i32.and" => binary(VALTYPE_I32, 0x71, WasmNumericResult::SameAsOperand),
            "i32.or" => binary(VALTYPE_I32, 0x72, WasmNumericResult::SameAsOperand),
            "i32.xor" => binary(VALTYPE_I32, 0x73, WasmNumericResult::SameAsOperand),
            "i32.shl" => binary(VALTYPE_I32, 0x74, WasmNumericResult::SameAsOperand),
            "i32.shr_s" => binary(VALTYPE_I32, 0x75, WasmNumericResult::SameAsOperand),
            // i32 比较：结果固定为 i32（0/1）。
            "i32.eq" => binary(VALTYPE_I32, 0x46, WasmNumericResult::I32),
            "i32.ne" => binary(VALTYPE_I32, 0x47, WasmNumericResult::I32),
            "i32.lt_s" => binary(VALTYPE_I32, 0x48, WasmNumericResult::I32),
            "i32.le_s" => binary(VALTYPE_I32, 0x4C, WasmNumericResult::I32),
            "i32.gt_s" => binary(VALTYPE_I32, 0x4A, WasmNumericResult::I32),
            "i32.ge_s" => binary(VALTYPE_I32, 0x4E, WasmNumericResult::I32),
            // 整数 neg 没有原生指令，按 `const 0` + `sub` 编码。
            "i32.neg" => integer_negate(VALTYPE_I32, 0x6B),

            // i64 算术 / 位运算：acorn 目前只导出 I64ShrU，其余按标准字节发射。
            "i64.add" => binary(VALTYPE_I64, 0x7C, WasmNumericResult::SameAsOperand),
            "i64.sub" => binary(VALTYPE_I64, 0x7D, WasmNumericResult::SameAsOperand),
            "i64.mul" => binary(VALTYPE_I64, 0x7E, WasmNumericResult::SameAsOperand),
            "i64.div_s" => binary(VALTYPE_I64, 0x7F, WasmNumericResult::SameAsOperand),
            "i64.rem_s" => binary(VALTYPE_I64, 0x81, WasmNumericResult::SameAsOperand),
            "i64.and" => binary(VALTYPE_I64, 0x83, WasmNumericResult::SameAsOperand),
            "i64.or" => binary(VALTYPE_I64, 0x84, WasmNumericResult::SameAsOperand),
            "i64.xor" => binary(VALTYPE_I64, 0x85, WasmNumericResult::SameAsOperand),
            "i64.shl" => binary(VALTYPE_I64, 0x86, WasmNumericResult::SameAsOperand),
            "i64.shr_s" => binary(VALTYPE_I64, 0x87, WasmNumericResult::SameAsOperand),
            // i64 比较：结果固定为 i32（0/1）。
            "i64.eq" => binary(VALTYPE_I64, 0x51, WasmNumericResult::I32),
            "i64.ne" => binary(VALTYPE_I64, 0x52, WasmNumericResult::I32),
            "i64.lt_s" => binary(VALTYPE_I64, 0x53, WasmNumericResult::I32),
            "i64.le_s" => binary(VALTYPE_I64, 0x57, WasmNumericResult::I32),
            "i64.gt_s" => binary(VALTYPE_I64, 0x55, WasmNumericResult::I32),
            "i64.ge_s" => binary(VALTYPE_I64, 0x59, WasmNumericResult::I32),
            "i64.neg" => integer_negate(VALTYPE_I64, 0x7D),

            // f64 算术：acorn 有 add/sub/mul/div 变体。
            "f64.add" => binary(VALTYPE_F64, 0xA0, WasmNumericResult::SameAsOperand),
            "f64.sub" => binary(VALTYPE_F64, 0xA1, WasmNumericResult::SameAsOperand),
            "f64.mul" => binary(VALTYPE_F64, 0xA2, WasmNumericResult::SameAsOperand),
            "f64.div" => binary(VALTYPE_F64, 0xA3, WasmNumericResult::SameAsOperand),
            // f64 比较：结果固定为 i32（0/1）。
            "f64.eq" => binary(VALTYPE_F64, 0x61, WasmNumericResult::I32),
            "f64.ne" => binary(VALTYPE_F64, 0x62, WasmNumericResult::I32),
            "f64.lt" => binary(VALTYPE_F64, 0x63, WasmNumericResult::I32),
            "f64.le" => binary(VALTYPE_F64, 0x65, WasmNumericResult::I32),
            "f64.gt" => binary(VALTYPE_F64, 0x64, WasmNumericResult::I32),
            "f64.ge" => binary(VALTYPE_F64, 0x66, WasmNumericResult::I32),
            // 浮点 neg 必须保留 -0.0，走原生 f64.neg 而不是 `0.0 - x`。
            "f64.neg" => native_negate(VALTYPE_F64, 0x9A),

            _ => return None,
        })
    }
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
