//! 头码、编码形与指令编解码。
//!
//! 本模块是 opcode 数值、指令编码形、指令解码与写入 helper 的唯一所有者。
//! 它只描述字节流形状，不解释运行时语义。

/// Magic value `NYAR` big-endian (`0x4E594152`).
pub const NYAR_MAGIC: u32 = 0x4E59_4152;

/// Current `.legion` / `.nyar` format version.
///
/// 未正式发布前统一为 `0`（v0 基线）；旧版本的模块一律拒绝，不做兼容读取。
pub const NYAR_VERSION: u32 = 0;

/// Bytecode format contract version（S-W6 provenance / cache 键）。
/// 与 [`NYAR_VERSION`] 同步；格式破坏时两者一并递增。
pub const BYTECODE_FORMAT_VERSION: u32 = NYAR_VERSION;

/// 已删除的 v1 `CallNative` 头码（`0xD1`）。
///
/// 不得再进入 [`NyarHeadCode`]；加载期验证器须拒绝含此字节的 v2 模块，改用 [`NyarHeadCode::CallImport`]。
pub const OBSOLETE_CALL_NATIVE: u8 = 0xD1;

/// File header size in bytes.
pub const HEADER_SIZE: usize = 16;

/// Section header size in bytes (kind + offset + size).
pub const SECTION_HEADER_SIZE: usize = 9;

/// Nyar instruction head code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum NyarHeadCode {
    /// 无操作。
    #[default]
    Nop = 0x00,
    /// 无条件跳转（operand1 = 目标 ip 偏移）。
    Jump = 0x01,
    /// 条件为真时跳转（operand1 = 目标 ip 偏移）。
    JumpIfTrue = 0x02,
    /// 条件为假时跳转（operand1 = 目标 ip 偏移）。
    JumpIfFalse = 0x03,
    /// 从当前帧返回。
    Return = 0x05,
    /// 异步让出：弹出栈顶值作为 yielded 值，暂停当前帧并交还控制权。
    /// operand1 = resume_state（恢复后跳转的 ip 偏移标识）。
    Yield = 0x0A,
    /// 异步恢复：弹出栈顶的 Coroutine 值与 resume 值，恢复挂起帧继续执行。
    Resume = 0x0B,
    /// 触发代数效应（raise）：弹出栈顶的 effect payload，捕获当前续延并调用 handler。
    /// handler 可选择 resume（控制流回到 resume_target，resume 值在栈顶）或不 resume。
    /// 若 `Effectful::Resume = !`，resume 路径不可达，VM 在尝试 resume 时报错。
    PerformEffect = 0x0D,
    /// 加载常量（operand1 = 常量池索引）。
    Constant = 0x10,
    /// 弹出栈顶值。
    Pop = 0x11,
    /// 复制栈顶值。
    Dup = 0x12,
    /// 读取局部变量（operand1 = 槽位索引）。
    LoadLocal = 0x20,
    /// 写入局部变量（operand1 = 槽位索引）。
    StoreLocal = 0x21,
    /// 读取形参（operand1 = 参数索引）。
    LoadArg = 0x22,
    /// 读取全局槽（operand1 = 全局索引）。
    LoadGlobal = 0x23,
    /// 写入全局槽（operand1 = 全局索引）。
    StoreGlobal = 0x24,
    /// 32 位有符号整数加法。
    I32Add = 0x30,
    /// 32 位有符号整数减法。
    I32Sub = 0x31,
    /// 32 位有符号整数乘法。
    I32Mul = 0x32,
    /// 32 位有符号整数除法。
    I32DivS = 0x33,
    /// 32 位有符号整数取余。
    I32RemS = 0x35,
    /// 32 位有符号整数相等比较。
    I32Eq = 0x40,
    /// 32 位有符号整数不等比较。
    I32Ne = 0x41,
    /// 32 位有符号整数小于比较。
    I32LtS = 0x42,
    /// 32 位有符号整数小于等于比较。
    I32LeS = 0x44,
    /// 32 位有符号整数大于比较。
    I32GtS = 0x46,
    /// 32 位有符号整数大于等于比较。
    I32GeS = 0x48,
    /// 调用函数（operand1 = 函数索引）。
    Call = 0x04,
    /// 静态调用（operand1 = 函数索引，operand2 = 参数个数）。
    CallStatic = 0x0F,
    /// 调用内置（operand1 = intrinsic 稠密下标，operand2 = argc）。
    CallIntrinsic = 0xD0,
    /// 调用模块导入表项（operand1 = import index，operand2 = argc）。
    /// 热路径只消费下标；链接名留在 imports section。
    CallImport = 0xD2,
    /// 按 layout_id 分配稠密槽位对象（operand1 = layout index）。
    /// 热路径不读字段名；布局描述在 layouts section。
    ObjectNew = 0x50,
    /// 按字段槽读取（operand1 = field_slot）；栈：`[obj] -> [value]`。
    FieldGet = 0x51,
    /// 按字段槽写入（operand1 = field_slot）；栈：`[obj, value] -> [obj]`。
    FieldSet = 0x52,
}

impl NyarHeadCode {
    /// Parse a head code from a raw byte.
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x00 => Some(Self::Nop),
            0x01 => Some(Self::Jump),
            0x02 => Some(Self::JumpIfTrue),
            0x03 => Some(Self::JumpIfFalse),
            0x04 => Some(Self::Call),
            0x05 => Some(Self::Return),
            0x0A => Some(Self::Yield),
            0x0B => Some(Self::Resume),
            0x0D => Some(Self::PerformEffect),
            0x0F => Some(Self::CallStatic),
            0x10 => Some(Self::Constant),
            0x11 => Some(Self::Pop),
            0x12 => Some(Self::Dup),
            0x20 => Some(Self::LoadLocal),
            0x21 => Some(Self::StoreLocal),
            0x22 => Some(Self::LoadArg),
            0x23 => Some(Self::LoadGlobal),
            0x24 => Some(Self::StoreGlobal),
            0x30 => Some(Self::I32Add),
            0x31 => Some(Self::I32Sub),
            0x32 => Some(Self::I32Mul),
            0x33 => Some(Self::I32DivS),
            0x35 => Some(Self::I32RemS),
            0x40 => Some(Self::I32Eq),
            0x41 => Some(Self::I32Ne),
            0x42 => Some(Self::I32LtS),
            0x44 => Some(Self::I32LeS),
            0x46 => Some(Self::I32GtS),
            0x48 => Some(Self::I32GeS),
            0xD0 => Some(Self::CallIntrinsic),
            0xD2 => Some(Self::CallImport),
            0x50 => Some(Self::ObjectNew),
            0x51 => Some(Self::FieldGet),
            0x52 => Some(Self::FieldSet),
            _ => None,
        }
    }
}

/// Instruction encoding form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NyarInstructionForm {
    /// 仅头码，无立即数。
    Plain,
    /// 头码 + 一个 `i32` 立即数。
    Imm1,
    /// 头码 + 两个 `i32` 立即数。
    Imm2,
    /// 头码 + 三个 `i32` 立即数。
    Imm3,
    /// 非法或未知编码形。
    Invalid,
}

impl NyarHeadCode {
    /// Returns the encoding form for this head code.
    pub fn form(self) -> NyarInstructionForm {
        match self {
            Self::Jump
            | Self::JumpIfTrue
            | Self::JumpIfFalse
            | Self::Call
            | Self::CallStatic
            | Self::Constant
            | Self::Yield
            | Self::PerformEffect
            | Self::LoadLocal
            | Self::StoreLocal
            | Self::LoadArg
            | Self::LoadGlobal
            | Self::StoreGlobal
            | Self::ObjectNew
            | Self::FieldGet
            | Self::FieldSet => NyarInstructionForm::Imm1,
            Self::CallIntrinsic | Self::CallImport => NyarInstructionForm::Imm2,
            Self::Nop
            | Self::Return
            | Self::Resume
            | Self::Pop
            | Self::Dup
            | Self::I32Add
            | Self::I32Sub
            | Self::I32Mul
            | Self::I32DivS
            | Self::I32RemS
            | Self::I32Eq
            | Self::I32Ne
            | Self::I32LtS
            | Self::I32LeS
            | Self::I32GtS
            | Self::I32GeS => NyarInstructionForm::Plain,
        }
    }

    /// Returns the encoded instruction size for this head code.
    pub fn code_size(self) -> u8 {
        match self.form() {
            NyarInstructionForm::Plain => 1,
            NyarInstructionForm::Imm1 => 5,
            NyarInstructionForm::Imm2 => 9,
            NyarInstructionForm::Imm3 => 13,
            NyarInstructionForm::Invalid => 0,
        }
    }
}

/// Pre-decoded instruction view used by the interpreter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NyarInstruction {
    /// Instruction head code.
    pub code: NyarHeadCode,
    /// Encoded size in the bytecode stream; `0` means invalid.
    pub size: u8,
    /// 第一个立即数操作数。
    pub operand1: i32,
    /// 第二个立即数操作数。
    pub operand2: i32,
    /// 第三个立即数操作数。
    pub operand3: i32,
}

impl NyarInstruction {
    /// Creates a decoded instruction.
    pub fn new(code: NyarHeadCode, operand1: i32, operand2: i32, operand3: i32) -> Self {
        Self { code, size: code.code_size(), operand1, operand2, operand3 }
    }

    /// Whether this instruction is valid.
    pub fn is_valid(self) -> bool {
        self.size != 0
    }
}

/// Decode one instruction at `pc` from `bytecode`.
pub fn decode_at(bytecode: &[u8], pc: usize) -> NyarInstruction {
    if pc >= bytecode.len() {
        return NyarInstruction::default();
    }

    let Some(code) = NyarHeadCode::from_u8(bytecode[pc])
    else {
        return NyarInstruction::default();
    };

    let size = try_instruction_size(bytecode, pc, code);
    if size == 0 {
        return NyarInstruction::default();
    }

    let mut operand1 = 0;
    let mut operand2 = 0;
    let mut operand3 = 0;

    if size >= 5 && pc + 4 < bytecode.len() {
        operand1 = i32::from_le_bytes(bytecode[pc + 1..pc + 5].try_into().expect("slice"));
    }
    if size >= 9 && pc + 8 < bytecode.len() {
        operand2 = i32::from_le_bytes(bytecode[pc + 5..pc + 9].try_into().expect("slice"));
    }
    if size >= 13 && pc + 12 < bytecode.len() {
        operand3 = i32::from_le_bytes(bytecode[pc + 9..pc + 13].try_into().expect("slice"));
    }

    NyarInstruction { code, size, operand1, operand2, operand3 }
}

/// 写入无操作数指令（`Plain` 形）。
pub fn emit_plain(code: &mut Vec<u8>, opcode: NyarHeadCode) {
    debug_assert_eq!(opcode.form(), NyarInstructionForm::Plain);
    code.push(opcode as u8);
}

/// 写入单立即数指令（`Imm1` 形，小端 `i32`）。
pub fn emit_imm1(code: &mut Vec<u8>, opcode: NyarHeadCode, operand: i32) {
    debug_assert_eq!(opcode.form(), NyarInstructionForm::Imm1);
    code.push(opcode as u8);
    code.extend_from_slice(&operand.to_le_bytes());
}

/// 写入双立即数指令（`Imm2` 形，两个小端 `i32`）。
pub fn emit_imm2(code: &mut Vec<u8>, opcode: NyarHeadCode, operand1: i32, operand2: i32) {
    debug_assert_eq!(opcode.form(), NyarInstructionForm::Imm2);
    code.push(opcode as u8);
    code.extend_from_slice(&operand1.to_le_bytes());
    code.extend_from_slice(&operand2.to_le_bytes());
}

fn try_instruction_size(bytecode: &[u8], pc: usize, code: NyarHeadCode) -> u8 {
    match code.form() {
        NyarInstructionForm::Plain => 1,
        NyarInstructionForm::Imm1 => {
            if pc + 4 < bytecode.len() {
                5
            }
            else {
                0
            }
        }
        NyarInstructionForm::Imm2 => {
            if pc + 8 < bytecode.len() {
                9
            }
            else {
                0
            }
        }
        NyarInstructionForm::Imm3 => {
            if pc + 12 < bytecode.len() {
                13
            }
            else {
                0
            }
        }
        NyarInstructionForm::Invalid => 0,
    }
}
