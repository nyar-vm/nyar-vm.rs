//! Nyar .nyar / .legion 外码格式合同（储存码）。
//!
//! 本 crate 是 opcode、section、encode/decode 与纯格式结构的唯一所有者。
//! 内码、解释器、GC 与 JIT 不得在此定义运行时执行表示。

use std::fmt::{Display, Formatter};

/// Magic value `NYAR` big-endian (`0x4E594152`).
pub const NYAR_MAGIC: u32 = 0x4E59_4152;

/// Current `.legion` / `.nyar` format version.
pub const NYAR_VERSION: u32 = 2;

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
    #[default]
    Nop = 0x00,
    Jump = 0x01,
    JumpIfTrue = 0x02,
    JumpIfFalse = 0x03,
    Call = 0x04,
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
    CallStatic = 0x0F,
    Const = 0x10,
    Pop = 0x11,
    Dup = 0x12,
    LoadLocal = 0x20,
    StoreLocal = 0x21,
    LoadArg = 0x22,
    LoadGlobal = 0x23,
    StoreGlobal = 0x24,
    I32Add = 0x30,
    I32Sub = 0x31,
    I32Mul = 0x32,
    I32DivS = 0x33,
    I32RemS = 0x35,
    I32Eq = 0x40,
    I32Ne = 0x41,
    I32LtS = 0x42,
    I32LeS = 0x44,
    I32GtS = 0x46,
    I32GeS = 0x48,
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
            0x10 => Some(Self::Const),
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
    Plain,
    Imm1,
    Imm2,
    Imm3,
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
            | Self::Const
            | Self::Yield
            | Self::PerformEffect
            | Self::LoadLocal
            | Self::StoreLocal
            | Self::LoadArg
            |             Self::LoadGlobal
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
    pub operand1: i32,
    pub operand2: i32,
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

/// Constant pool entry kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NyarConstantKind {
    Null = 0x00,
    Boolean = 0x01,
    String = 0x05,
    BigInt = 0x06,
    Integer32 = 0x11,
    Float64 = 0x22,
}

/// Constant pool entry.
#[derive(Debug, Clone, PartialEq)]
pub enum NyarConstant {
    Null,
    Boolean(bool),
    Integer32(i32),
    Float64(f64),
    String(String),
    BigInt(Vec<u8>),
}

/// Function metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NyarFunction {
    pub name: String,
    pub arity: i32,
    pub local_count: i32,
    pub code_offset: i32,
    pub code_length: i32,
}

/// Import entry kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NyarImportKind {
    Function = 0,
}

/// Import table entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NyarImport {
    pub kind: NyarImportKind,
    pub module_name: String,
    pub symbol_name: String,
}

/// Export entry kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NyarExportKind {
    Function = 0,
    Global = 1,
}

/// Export table entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NyarExport {
    pub kind: NyarExportKind,
    pub symbol_name: String,
    /// Function index when `kind == Function`; global index when `kind == Global`.
    pub function_index: i32,
}

/// Module-level global slot (singleton instance storage, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NyarGlobal {
    /// Slot name, e.g. `Counter.INSTANCE`.
    pub name: String,
    /// Aggregate type name used for allocation.
    pub type_name: String,
}

/// Witness dispatch entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NyarWitnessDispatchEntry {
    pub method_id: i32,
    pub type_id: i32,
    pub method_name: String,
    pub function_index: i32,
    pub interface_id: i32,
    pub interface_method_index: i32,
}

/// Section kind in a `.legion` module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NyarSectionKind {
    Constants = 0x01,
    Functions = 0x02,
    Code = 0x03,
    Imports = 0x04,
    Exports = 0x05,
    WitnessEntries = 0x06,
    Globals = 0x07,
    InitFunctions = 0x08,
    /// 稠密布局描述表；`ObjectNew` 的 layout_id 索引此段。
    Layouts = 0x09,
}

/// 语言中立的布局描述：仅槽位数，不含字段名。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NyarLayout {
    /// 字段槽数量（`FieldGet` / `FieldSet` 的 field_slot 上界）。
    pub field_count: i32,
}

/// Decoded Nyar module binary view.
#[derive(Debug, Clone, PartialEq)]
pub struct NyarModuleData {
    pub version: u32,
    pub name: String,
    pub constants: Vec<NyarConstant>,
    pub functions: Vec<NyarFunction>,
    pub imports: Vec<NyarImport>,
    pub exports: Vec<NyarExport>,
    pub witness_entries: Vec<NyarWitnessDispatchEntry>,
    pub code_bytes: Vec<u8>,
    /// Module-level global slots.
    pub globals: Vec<NyarGlobal>,
    /// Function indices to run for eager singleton / module initialization.
    pub init_function_indices: Vec<i32>,
    /// Layout descriptors；`layout_id` 为稠密下标。
    pub layouts: Vec<NyarLayout>,
}

/// `.legion` decode error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NyarDecodeError {
    TooShort,
    InvalidMagic(u32),
    InvalidConstantKind(u8),
    InvalidImportKind(u8),
    InvalidExportKind(u8),
    InvalidSectionKind(u8),
    InvalidUtf8,
}

impl Display for NyarDecodeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort => write!(f, ".legion buffer too short"),
            Self::InvalidMagic(magic) => write!(f, "invalid .legion magic: 0x{magic:08X}"),
            Self::InvalidConstantKind(kind) => write!(f, "invalid constant kind: {kind}"),
            Self::InvalidImportKind(kind) => write!(f, "invalid import kind: {kind}"),
            Self::InvalidExportKind(kind) => write!(f, "invalid export kind: {kind}"),
            Self::InvalidSectionKind(kind) => write!(f, "invalid section kind: {kind}"),
            Self::InvalidUtf8 => write!(f, "invalid UTF-8 in .legion module"),
        }
    }
}

impl std::error::Error for NyarDecodeError {}

/// Decode a `.legion` module from bytes.
pub fn decode_module(data: &[u8]) -> Result<NyarModuleData, NyarDecodeError> {
    if data.len() < HEADER_SIZE {
        return Err(NyarDecodeError::TooShort);
    }

    let magic = u32::from_be_bytes(data[0..4].try_into().expect("header"));
    if magic != NYAR_MAGIC {
        return Err(NyarDecodeError::InvalidMagic(magic));
    }

    let version = u32::from_le_bytes(data[4..8].try_into().expect("version"));
    let section_count = i32::from_le_bytes(data[8..12].try_into().expect("sections")) as usize;
    let name_offset = i32::from_le_bytes(data[12..16].try_into().expect("name")) as usize;

    let headers_start = HEADER_SIZE;
    let headers_end = headers_start + section_count * SECTION_HEADER_SIZE;
    if data.len() < headers_end {
        return Err(NyarDecodeError::TooShort);
    }

    let mut sections = Vec::with_capacity(section_count);
    for i in 0..section_count {
        let base = headers_start + i * SECTION_HEADER_SIZE;
        sections.push(NyarSectionHeader {
            kind: data[base],
            offset: i32::from_le_bytes(data[base + 1..base + 5].try_into().expect("offset")),
            size: i32::from_le_bytes(data[base + 5..base + 9].try_into().expect("size")),
        });
    }

    let name = read_length_prefixed_string(data, name_offset)?;

    let mut module = NyarModuleData {
        version,
        name,
        constants: Vec::new(),
        functions: Vec::new(),
        imports: Vec::new(),
        exports: Vec::new(),
        witness_entries: Vec::new(),
        code_bytes: Vec::new(),
        globals: Vec::new(),
        init_function_indices: Vec::new(),
        layouts: Vec::new(),
    };

    for section in sections {
        let offset = section.offset as usize;
        let size = section.size as usize;
        if offset + size > data.len() {
            return Err(NyarDecodeError::TooShort);
        }

        let slice = &data[offset..offset + size];
        match section.kind {
            0x01 => decode_constants(slice, &mut module.constants)?,
            0x02 => decode_functions(slice, &mut module.functions)?,
            0x03 => module.code_bytes = slice.to_vec(),
            0x04 => decode_imports(slice, &mut module.imports)?,
            0x05 => decode_exports(slice, &mut module.exports)?,
            0x06 => decode_witness_entries(slice, &mut module.witness_entries)?,
            0x07 => decode_globals(slice, &mut module.globals)?,
            0x08 => decode_init_functions(slice, &mut module.init_function_indices)?,
            0x09 => decode_layouts(slice, &mut module.layouts)?,
            kind => return Err(NyarDecodeError::InvalidSectionKind(kind)),
        }
    }

    Ok(module)
}

#[derive(Debug, Clone, Copy)]
struct NyarSectionHeader {
    kind: u8,
    offset: i32,
    size: i32,
}

fn read_length_prefixed_string(data: &[u8], offset: usize) -> Result<String, NyarDecodeError> {
    if offset + 4 > data.len() {
        return Err(NyarDecodeError::TooShort);
    }
    let len = i32::from_le_bytes(data[offset..offset + 4].try_into().expect("len")) as usize;
    let start = offset + 4;
    let end = start + len;
    if end > data.len() {
        return Err(NyarDecodeError::TooShort);
    }
    String::from_utf8(data[start..end].to_vec()).map_err(|_| NyarDecodeError::InvalidUtf8)
}

fn decode_constants(slice: &[u8], out: &mut Vec<NyarConstant>) -> Result<(), NyarDecodeError> {
    if slice.len() < 4 {
        return Err(NyarDecodeError::TooShort);
    }
    let count = i32::from_le_bytes(slice[0..4].try_into().expect("count")) as usize;
    let mut offset = 4;
    for _ in 0..count {
        if offset >= slice.len() {
            return Err(NyarDecodeError::TooShort);
        }
        let kind = slice[offset];
        offset += 1;
        let (constant, consumed) = decode_constant(kind, &slice[offset..])?;
        offset += consumed;
        out.push(constant);
    }
    Ok(())
}

fn decode_constant(kind: u8, slice: &[u8]) -> Result<(NyarConstant, usize), NyarDecodeError> {
    match kind {
        0x00 => Ok((NyarConstant::Null, 0)),
        0x01 => {
            if slice.is_empty() {
                return Err(NyarDecodeError::TooShort);
            }
            Ok((NyarConstant::Boolean(slice[0] != 0), 1))
        }
        0x11 => {
            if slice.len() < 4 {
                return Err(NyarDecodeError::TooShort);
            }
            Ok((NyarConstant::Integer32(i32::from_le_bytes(slice[0..4].try_into().expect("i32"))), 4))
        }
        0x22 => {
            if slice.len() < 8 {
                return Err(NyarDecodeError::TooShort);
            }
            Ok((NyarConstant::Float64(f64::from_le_bytes(slice[0..8].try_into().expect("f64"))), 8))
        }
        0x05 => {
            if slice.len() < 4 {
                return Err(NyarDecodeError::TooShort);
            }
            let len = i32::from_le_bytes(slice[0..4].try_into().expect("len")) as usize;
            if slice.len() < 4 + len {
                return Err(NyarDecodeError::TooShort);
            }
            let value = String::from_utf8(slice[4..4 + len].to_vec()).map_err(|_| NyarDecodeError::InvalidUtf8)?;
            Ok((NyarConstant::String(value), 4 + len))
        }
        0x06 => {
            if slice.len() < 4 {
                return Err(NyarDecodeError::TooShort);
            }
            let len = i32::from_le_bytes(slice[0..4].try_into().expect("len")) as usize;
            if slice.len() < 4 + len {
                return Err(NyarDecodeError::TooShort);
            }
            Ok((NyarConstant::BigInt(slice[4..4 + len].to_vec()), 4 + len))
        }
        kind => Err(NyarDecodeError::InvalidConstantKind(kind)),
    }
}

fn decode_functions(slice: &[u8], out: &mut Vec<NyarFunction>) -> Result<(), NyarDecodeError> {
    if slice.len() < 4 {
        return Err(NyarDecodeError::TooShort);
    }
    let count = i32::from_le_bytes(slice[0..4].try_into().expect("count")) as usize;
    let mut offset = 4;
    for _ in 0..count {
        let (name, consumed) = read_string_at(slice, offset)?;
        offset += consumed;
        if offset + 16 > slice.len() {
            return Err(NyarDecodeError::TooShort);
        }
        out.push(NyarFunction {
            name,
            arity: i32::from_le_bytes(slice[offset..offset + 4].try_into().expect("arity")),
            local_count: i32::from_le_bytes(slice[offset + 4..offset + 8].try_into().expect("locals")),
            code_offset: i32::from_le_bytes(slice[offset + 8..offset + 12].try_into().expect("off")),
            code_length: i32::from_le_bytes(slice[offset + 12..offset + 16].try_into().expect("len")),
        });
        offset += 16;
    }
    Ok(())
}

fn decode_imports(slice: &[u8], out: &mut Vec<NyarImport>) -> Result<(), NyarDecodeError> {
    if slice.len() < 4 {
        return Err(NyarDecodeError::TooShort);
    }
    let count = i32::from_le_bytes(slice[0..4].try_into().expect("count")) as usize;
    let mut offset = 4;
    for _ in 0..count {
        if offset >= slice.len() {
            return Err(NyarDecodeError::TooShort);
        }
        let kind = match slice[offset] {
            0 => NyarImportKind::Function,
            kind => return Err(NyarDecodeError::InvalidImportKind(kind)),
        };
        offset += 1;
        let (module_name, consumed) = read_string_at(slice, offset)?;
        offset += consumed;
        let (symbol_name, consumed) = read_string_at(slice, offset)?;
        offset += consumed;
        out.push(NyarImport { kind, module_name, symbol_name });
    }
    Ok(())
}

fn decode_exports(slice: &[u8], out: &mut Vec<NyarExport>) -> Result<(), NyarDecodeError> {
    if slice.len() < 4 {
        return Err(NyarDecodeError::TooShort);
    }
    let count = i32::from_le_bytes(slice[0..4].try_into().expect("count")) as usize;
    let mut offset = 4;
    for _ in 0..count {
        if offset >= slice.len() {
            return Err(NyarDecodeError::TooShort);
        }
        let kind = match slice[offset] {
            0 => NyarExportKind::Function,
            1 => NyarExportKind::Global,
            kind => return Err(NyarDecodeError::InvalidExportKind(kind)),
        };
        offset += 1;
        let (symbol_name, consumed) = read_string_at(slice, offset)?;
        offset += consumed;
        if offset + 4 > slice.len() {
            return Err(NyarDecodeError::TooShort);
        }
        out.push(NyarExport { kind, symbol_name, function_index: i32::from_le_bytes(slice[offset..offset + 4].try_into().expect("idx")) });
        offset += 4;
    }
    Ok(())
}

fn decode_globals(slice: &[u8], out: &mut Vec<NyarGlobal>) -> Result<(), NyarDecodeError> {
    if slice.len() < 4 {
        return Err(NyarDecodeError::TooShort);
    }
    let count = i32::from_le_bytes(slice[0..4].try_into().expect("count")) as usize;
    let mut offset = 4;
    for _ in 0..count {
        let (name, consumed) = read_string_at(slice, offset)?;
        offset += consumed;
        let (type_name, consumed) = read_string_at(slice, offset)?;
        offset += consumed;
        out.push(NyarGlobal { name, type_name });
    }
    Ok(())
}

fn decode_init_functions(slice: &[u8], out: &mut Vec<i32>) -> Result<(), NyarDecodeError> {
    if slice.len() < 4 {
        return Err(NyarDecodeError::TooShort);
    }
    let count = i32::from_le_bytes(slice[0..4].try_into().expect("count")) as usize;
    let mut offset = 4;
    for _ in 0..count {
        if offset + 4 > slice.len() {
            return Err(NyarDecodeError::TooShort);
        }
        out.push(i32::from_le_bytes(slice[offset..offset + 4].try_into().expect("idx")));
        offset += 4;
    }
    Ok(())
}

fn decode_layouts(slice: &[u8], out: &mut Vec<NyarLayout>) -> Result<(), NyarDecodeError> {
    if slice.len() < 4 {
        return Err(NyarDecodeError::TooShort);
    }
    let count = i32::from_le_bytes(slice[0..4].try_into().expect("count")) as usize;
    let mut offset = 4;
    for _ in 0..count {
        if offset + 4 > slice.len() {
            return Err(NyarDecodeError::TooShort);
        }
        let field_count = i32::from_le_bytes(slice[offset..offset + 4].try_into().expect("field_count"));
        if field_count < 0 {
            return Err(NyarDecodeError::TooShort);
        }
        out.push(NyarLayout { field_count });
        offset += 4;
    }
    Ok(())
}

fn decode_witness_entries(slice: &[u8], out: &mut Vec<NyarWitnessDispatchEntry>) -> Result<(), NyarDecodeError> {
    if slice.len() < 4 {
        return Err(NyarDecodeError::TooShort);
    }
    let count = i32::from_le_bytes(slice[0..4].try_into().expect("count")) as usize;
    let mut offset = 4;
    for _ in 0..count {
        if offset + 12 > slice.len() {
            return Err(NyarDecodeError::TooShort);
        }
        let method_id = i32::from_le_bytes(slice[offset..offset + 4].try_into().expect("mid"));
        let type_id = i32::from_le_bytes(slice[offset + 4..offset + 8].try_into().expect("tid"));
        offset += 8;
        let (method_name, consumed) = read_string_at(slice, offset)?;
        offset += consumed;
        if offset + 12 > slice.len() {
            return Err(NyarDecodeError::TooShort);
        }
        out.push(NyarWitnessDispatchEntry {
            method_id,
            type_id,
            method_name,
            function_index: i32::from_le_bytes(slice[offset..offset + 4].try_into().expect("fi")),
            interface_id: i32::from_le_bytes(slice[offset + 4..offset + 8].try_into().expect("ii")),
            interface_method_index: i32::from_le_bytes(slice[offset + 8..offset + 12].try_into().expect("imi")),
        });
        offset += 12;
    }
    Ok(())
}

fn read_string_at(slice: &[u8], offset: usize) -> Result<(String, usize), NyarDecodeError> {
    if offset + 4 > slice.len() {
        return Err(NyarDecodeError::TooShort);
    }
    let len = i32::from_le_bytes(slice[offset..offset + 4].try_into().expect("len")) as usize;
    let start = offset + 4;
    let end = start + len;
    if end > slice.len() {
        return Err(NyarDecodeError::TooShort);
    }
    let value = String::from_utf8(slice[start..end].to_vec()).map_err(|_| NyarDecodeError::InvalidUtf8)?;
    Ok((value, 4 + len))
}

/// Encode a minimal `.legion` module.
pub fn encode_module(data: &NyarModuleData) -> Vec<u8> {
    let mut sections: Vec<(NyarSectionKind, Vec<u8>)> = Vec::new();

    if !data.constants.is_empty() {
        sections.push((NyarSectionKind::Constants, encode_constants(&data.constants)));
    }
    if !data.functions.is_empty() {
        sections.push((NyarSectionKind::Functions, encode_functions(&data.functions)));
    }
    if !data.imports.is_empty() {
        sections.push((NyarSectionKind::Imports, encode_imports(&data.imports)));
    }
    if !data.exports.is_empty() {
        sections.push((NyarSectionKind::Exports, encode_exports(&data.exports)));
    }
    if !data.witness_entries.is_empty() {
        sections.push((NyarSectionKind::WitnessEntries, encode_witness_entries(&data.witness_entries)));
    }
    if !data.globals.is_empty() {
        sections.push((NyarSectionKind::Globals, encode_globals(&data.globals)));
    }
    if !data.init_function_indices.is_empty() {
        sections.push((NyarSectionKind::InitFunctions, encode_init_functions(&data.init_function_indices)));
    }
    if !data.layouts.is_empty() {
        sections.push((NyarSectionKind::Layouts, encode_layouts(&data.layouts)));
    }
    if !data.code_bytes.is_empty() {
        sections.push((NyarSectionKind::Code, data.code_bytes.clone()));
    }

    let name_bytes = data.name.as_bytes();
    let data_start = HEADER_SIZE + sections.len() * SECTION_HEADER_SIZE + 4 + name_bytes.len();
    let mut out = Vec::with_capacity(data_start + sections.iter().map(|(_, d)| d.len()).sum::<usize>());

    out.extend_from_slice(&NYAR_MAGIC.to_be_bytes());
    out.extend_from_slice(&data.version.to_le_bytes());
    out.extend_from_slice(&(sections.len() as i32).to_le_bytes());
    out.extend_from_slice(&(HEADER_SIZE as i32 + sections.len() as i32 * SECTION_HEADER_SIZE as i32).to_le_bytes());

    let mut offset = data_start as i32;
    for (kind, section_data) in &sections {
        out.push(*kind as u8);
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&(section_data.len() as i32).to_le_bytes());
        offset += section_data.len() as i32;
    }

    out.extend_from_slice(&(name_bytes.len() as i32).to_le_bytes());
    out.extend_from_slice(name_bytes);

    for (_, section_data) in sections {
        out.extend_from_slice(&section_data);
    }

    out
}

fn encode_constants(constants: &[NyarConstant]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(constants.len() as i32).to_le_bytes());
    for constant in constants {
        match constant {
            NyarConstant::Null => {
                out.push(0x00);
            }
            NyarConstant::Boolean(value) => {
                out.push(0x01);
                out.push(u8::from(*value));
            }
            NyarConstant::Integer32(value) => {
                out.push(0x11);
                out.extend_from_slice(&value.to_le_bytes());
            }
            NyarConstant::Float64(value) => {
                out.push(0x22);
                out.extend_from_slice(&value.to_le_bytes());
            }
            NyarConstant::String(value) => {
                out.push(0x05);
                let bytes = value.as_bytes();
                out.extend_from_slice(&(bytes.len() as i32).to_le_bytes());
                out.extend_from_slice(bytes);
            }
            NyarConstant::BigInt(value) => {
                out.push(0x06);
                out.extend_from_slice(&(value.len() as i32).to_le_bytes());
                out.extend_from_slice(value);
            }
        }
    }
    out
}

fn encode_functions(functions: &[NyarFunction]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(functions.len() as i32).to_le_bytes());
    for function in functions {
        write_string(&mut out, &function.name);
        out.extend_from_slice(&function.arity.to_le_bytes());
        out.extend_from_slice(&function.local_count.to_le_bytes());
        out.extend_from_slice(&function.code_offset.to_le_bytes());
        out.extend_from_slice(&function.code_length.to_le_bytes());
    }
    out
}

fn encode_imports(imports: &[NyarImport]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(imports.len() as i32).to_le_bytes());
    for import in imports {
        out.push(import.kind as u8);
        write_string(&mut out, &import.module_name);
        write_string(&mut out, &import.symbol_name);
    }
    out
}

fn encode_exports(exports: &[NyarExport]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(exports.len() as i32).to_le_bytes());
    for export in exports {
        out.push(export.kind as u8);
        write_string(&mut out, &export.symbol_name);
        out.extend_from_slice(&export.function_index.to_le_bytes());
    }
    out
}

fn encode_witness_entries(entries: &[NyarWitnessDispatchEntry]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(entries.len() as i32).to_le_bytes());
    for entry in entries {
        out.extend_from_slice(&entry.method_id.to_le_bytes());
        out.extend_from_slice(&entry.type_id.to_le_bytes());
        write_string(&mut out, &entry.method_name);
        out.extend_from_slice(&entry.function_index.to_le_bytes());
        out.extend_from_slice(&entry.interface_id.to_le_bytes());
        out.extend_from_slice(&entry.interface_method_index.to_le_bytes());
    }
    out
}

fn encode_globals(globals: &[NyarGlobal]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(globals.len() as i32).to_le_bytes());
    for global in globals {
        write_string(&mut out, &global.name);
        write_string(&mut out, &global.type_name);
    }
    out
}

fn encode_init_functions(indices: &[i32]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(indices.len() as i32).to_le_bytes());
    for index in indices {
        out.extend_from_slice(&index.to_le_bytes());
    }
    out
}

fn encode_layouts(layouts: &[NyarLayout]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(layouts.len() as i32).to_le_bytes());
    for layout in layouts {
        out.extend_from_slice(&layout.field_count.to_le_bytes());
    }
    out
}

fn write_string(out: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    out.extend_from_slice(&(bytes.len() as i32).to_le_bytes());
    out.extend_from_slice(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structural_opcodes_are_imm1() {
        assert_eq!(NyarHeadCode::ObjectNew.form(), NyarInstructionForm::Imm1);
        assert_eq!(NyarHeadCode::FieldGet.form(), NyarInstructionForm::Imm1);
        assert_eq!(NyarHeadCode::FieldSet.form(), NyarInstructionForm::Imm1);
        assert_eq!(NyarHeadCode::from_u8(0x50), Some(NyarHeadCode::ObjectNew));
        assert_eq!(NyarHeadCode::from_u8(0x51), Some(NyarHeadCode::FieldGet));
        assert_eq!(NyarHeadCode::from_u8(0x52), Some(NyarHeadCode::FieldSet));
    }

    #[test]
    fn bytecode_format_version_tracks_nyar_version() {
        assert_eq!(BYTECODE_FORMAT_VERSION, NYAR_VERSION);
        assert_eq!(BYTECODE_FORMAT_VERSION, 2);
    }

    #[test]
    fn obsolete_call_native_is_not_a_live_head_code() {
        assert_eq!(OBSOLETE_CALL_NATIVE, 0xD1);
        assert_eq!(NyarHeadCode::from_u8(OBSOLETE_CALL_NATIVE), None);
        assert_ne!(NyarHeadCode::CallImport as u8, OBSOLETE_CALL_NATIVE);
    }

    #[test]
    fn emit_helpers_round_trip_with_decode_at() {
        let mut code = Vec::new();
        emit_plain(&mut code, NyarHeadCode::Return);
        emit_imm1(&mut code, NyarHeadCode::Const, 7);
        emit_imm2(&mut code, NyarHeadCode::CallImport, 1, 2);
        let ret = decode_at(&code, 0);
        assert_eq!(ret.code, NyarHeadCode::Return);
        assert_eq!(ret.size, 1);
        let c = decode_at(&code, 1);
        assert_eq!(c.code, NyarHeadCode::Const);
        assert_eq!(c.operand1, 7);
        assert_eq!(c.size, 5);
        let call = decode_at(&code, 6);
        assert_eq!(call.code, NyarHeadCode::CallImport);
        assert_eq!(call.operand1, 1);
        assert_eq!(call.operand2, 2);
        assert_eq!(call.size, 9);
    }

    #[test]
    fn layouts_section_round_trips() {
        let data = NyarModuleData {
            version: NYAR_VERSION,
            name: "layout-test".into(),
            constants: Vec::new(),
            functions: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            witness_entries: Vec::new(),
            code_bytes: Vec::new(),
            globals: Vec::new(),
            init_function_indices: Vec::new(),
            layouts: vec![NyarLayout { field_count: 2 }, NyarLayout { field_count: 0 }],
        };
        let bytes = encode_module(&data);
        let decoded = decode_module(&bytes).expect("decode");
        assert_eq!(decoded.layouts, data.layouts);
    }
}
