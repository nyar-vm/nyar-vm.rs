//! 基线标量「机器码」编码。
//!
//! 这不是 OS 可执行页上的原生指令，而是版本化的可解释/可降低中间字节，
//! 供后续后端翻译为真实机器码或由 VM 快路径执行。禁止在未经验证的路径上
//! `transmute` 为函数指针。

/// 魔数：`NJ1\0`（Nyar JIT v1）。
pub const MACHINE_CODE_MAGIC: &[u8; 4] = b"NJ1\0";

/// 操作码。
pub mod op {
    /// `return locals[slot]`。
    pub const RET_LOCAL: u8 = 0x01;
    /// `return (i32)locals[a] + (i32)locals[b]`。
    pub const RET_I32_ADD_LOCALS: u8 = 0x02;
    /// `return (i32)locals[a] - (i32)locals[b]`。
    pub const RET_I32_SUB_LOCALS: u8 = 0x03;
    /// `return (i32)locals[a] * (i32)locals[b]`。
    pub const RET_I32_MUL_LOCALS: u8 = 0x04;
    /// `return (i32)locals[a] cmp (i32)locals[b]` → `0`/`1`。
    pub const RET_I32_CMP_LOCALS: u8 = 0x05;
    /// `return locals[then] if locals[a] cmp locals[b] else locals[else_slot]`。
    pub const RET_I32_SELECT_CMP_LOCALS: u8 = 0x06;
    /// `return (i32)locals[a] / (i32)locals[b]`（rhs==0 → 0，与解释器一致）。
    pub const RET_I32_DIV_LOCALS: u8 = 0x07;
    /// `return (i32)locals[a] % (i32)locals[b]`（rhs==0 → 0，与解释器一致）。
    pub const RET_I32_REM_LOCALS: u8 = 0x08;
    /// `return imm_i32`。
    pub const RET_CONST_I32: u8 = 0x09;
    /// `return imm_i32 binop locals[slot]` 或 `locals[slot] binop imm`（见 flags）。
    pub const RET_I32_BINOP_IMM_LOCAL: u8 = 0x0A;
    /// `return imm_i32 cmp locals[slot]` 或反向（见 flags）→ `0`/`1`。
    pub const RET_I32_CMP_IMM_LOCAL: u8 = 0x0B;
    /// `return then_imm if locals[a] cmp locals[b] else else_imm`。
    pub const RET_I32_SELECT_CMP_CONSTS: u8 = 0x0C;
    /// `return then/else` 混合 local 与立即数（见 [`SELECT_THEN_IMM`] / [`SELECT_ELSE_IMM`]）。
    pub const RET_I32_SELECT_CMP_MIXED: u8 = 0x0D;
    /// `return` 无值（void 叶）。
    pub const RET_VOID: u8 = 0x0E;
}

/// 混合 select：真分支为立即数（假分支为 local）。
pub const SELECT_THEN_IMM: u8 = 0x01;
/// 混合 select：假分支为立即数（真分支为 local）。
pub const SELECT_ELSE_IMM: u8 = 0x02;

/// select 臂：local 槽或立即 i32。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectArm {
    /// 拷贝 `locals[slot]`。
    Local(u16),
    /// 立即 i32。
    Imm(i32),
}

/// `RET_I32_*_IMM_LOCAL`：立即数在二元运算左侧。
pub const IMM_ON_LEFT: u8 = 0;
/// `RET_I32_*_IMM_LOCAL`：立即数在二元运算右侧。
pub const IMM_ON_RIGHT: u8 = 1;

/// i32 二元运算种类（两 local → 返回值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum I32Binop {
    /// 加法。
    Add,
    /// 减法。
    Sub,
    /// 乘法。
    Mul,
    /// 有符号除法。
    DivS,
    /// 有符号取余。
    RemS,
}

impl I32Binop {
    /// 编码为单字节。
    pub fn to_u8(self) -> u8 {
        match self {
            Self::Add => 0,
            Self::Sub => 1,
            Self::Mul => 2,
            Self::DivS => 3,
            Self::RemS => 4,
        }
    }

    /// 从单字节解码。
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Add),
            1 => Some(Self::Sub),
            2 => Some(Self::Mul),
            3 => Some(Self::DivS),
            4 => Some(Self::RemS),
            _ => None,
        }
    }
}

/// i32 比较种类（两 local → `0`/`1`，或条件选择）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum I32Cmp {
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// 有符号 `<`
    LtS,
    /// 有符号 `<=`
    LeS,
    /// 有符号 `>`
    GtS,
    /// 有符号 `>=`
    GeS,
}

impl I32Cmp {
    /// 编码为单字节（写入 NJ1 blob）。
    pub fn to_u8(self) -> u8 {
        match self {
            Self::Eq => 0,
            Self::Ne => 1,
            Self::LtS => 2,
            Self::LeS => 3,
            Self::GtS => 4,
            Self::GeS => 5,
        }
    }

    /// 从单字节解码。
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Eq),
            1 => Some(Self::Ne),
            2 => Some(Self::LtS),
            3 => Some(Self::LeS),
            4 => Some(Self::GtS),
            5 => Some(Self::GeS),
            _ => None,
        }
    }

    /// 计算比较结果（真 → `true`）。
    pub fn eval(self, lhs: i32, rhs: i32) -> bool {
        match self {
            Self::Eq => lhs == rhs,
            Self::Ne => lhs != rhs,
            Self::LtS => lhs < rhs,
            Self::LeS => lhs <= rhs,
            Self::GtS => lhs > rhs,
            Self::GeS => lhs >= rhs,
        }
    }
}

/// 将 `RetLocal` 编码为 blob。
pub fn encode_ret_local(slot: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(7);
    out.extend_from_slice(MACHINE_CODE_MAGIC);
    out.push(op::RET_LOCAL);
    out.extend_from_slice(&slot.to_le_bytes());
    out
}

/// 将两 local 的 i32 二元运算编码为 blob。
pub fn encode_ret_i32_binop_locals(binop: I32Binop, a: u16, b: u16) -> Vec<u8> {
    let opcode = match binop {
        I32Binop::Add => op::RET_I32_ADD_LOCALS,
        I32Binop::Sub => op::RET_I32_SUB_LOCALS,
        I32Binop::Mul => op::RET_I32_MUL_LOCALS,
        I32Binop::DivS => op::RET_I32_DIV_LOCALS,
        I32Binop::RemS => op::RET_I32_REM_LOCALS,
    };
    let mut out = Vec::with_capacity(9);
    out.extend_from_slice(MACHINE_CODE_MAGIC);
    out.push(opcode);
    out.extend_from_slice(&a.to_le_bytes());
    out.extend_from_slice(&b.to_le_bytes());
    out
}

/// 将立即 i32 返回编码为 blob。
pub fn encode_ret_const_i32(value: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity(9);
    out.extend_from_slice(MACHINE_CODE_MAGIC);
    out.push(op::RET_CONST_I32);
    out.extend_from_slice(&value.to_le_bytes());
    out
}

/// 立即数与单 local 的 i32 二元运算。
pub fn encode_ret_i32_binop_imm_local(binop: I32Binop, imm: i32, local: u16, imm_on_left: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(13);
    out.extend_from_slice(MACHINE_CODE_MAGIC);
    out.push(op::RET_I32_BINOP_IMM_LOCAL);
    out.push(binop.to_u8());
    out.push(if imm_on_left { IMM_ON_LEFT } else { IMM_ON_RIGHT });
    out.extend_from_slice(&imm.to_le_bytes());
    out.extend_from_slice(&local.to_le_bytes());
    out
}

/// 立即数与单 local 的 i32 比较（结果 `0`/`1`）。
pub fn encode_ret_i32_cmp_imm_local(cmp: I32Cmp, imm: i32, local: u16, imm_on_left: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(13);
    out.extend_from_slice(MACHINE_CODE_MAGIC);
    out.push(op::RET_I32_CMP_IMM_LOCAL);
    out.push(cmp.to_u8());
    out.push(if imm_on_left { IMM_ON_LEFT } else { IMM_ON_RIGHT });
    out.extend_from_slice(&imm.to_le_bytes());
    out.extend_from_slice(&local.to_le_bytes());
    out
}

/// 兼容旧名。
pub fn encode_ret_i32_add_locals(a: u16, b: u16) -> Vec<u8> {
    encode_ret_i32_binop_locals(I32Binop::Add, a, b)
}

/// 将两 local 的 i32 比较编码为 blob（结果 `0`/`1`）。
pub fn encode_ret_i32_cmp_locals(cmp: I32Cmp, a: u16, b: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(10);
    out.extend_from_slice(MACHINE_CODE_MAGIC);
    out.push(op::RET_I32_CMP_LOCALS);
    out.push(cmp.to_u8());
    out.extend_from_slice(&a.to_le_bytes());
    out.extend_from_slice(&b.to_le_bytes());
    out
}

/// 条件选择：比较成立返回 `then_slot`，否则返回 `else_slot`。
pub fn encode_ret_i32_select_cmp_locals(cmp: I32Cmp, a: u16, b: u16, then_slot: u16, else_slot: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(14);
    out.extend_from_slice(MACHINE_CODE_MAGIC);
    out.push(op::RET_I32_SELECT_CMP_LOCALS);
    out.push(cmp.to_u8());
    out.extend_from_slice(&a.to_le_bytes());
    out.extend_from_slice(&b.to_le_bytes());
    out.extend_from_slice(&then_slot.to_le_bytes());
    out.extend_from_slice(&else_slot.to_le_bytes());
    out
}

/// 条件选择：比较成立返回 `then_imm`，否则返回 `else_imm`。
pub fn encode_ret_i32_select_cmp_consts(cmp: I32Cmp, a: u16, b: u16, then_imm: i32, else_imm: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity(18);
    out.extend_from_slice(MACHINE_CODE_MAGIC);
    out.push(op::RET_I32_SELECT_CMP_CONSTS);
    out.push(cmp.to_u8());
    out.extend_from_slice(&a.to_le_bytes());
    out.extend_from_slice(&b.to_le_bytes());
    out.extend_from_slice(&then_imm.to_le_bytes());
    out.extend_from_slice(&else_imm.to_le_bytes());
    out
}

/// 条件选择：一臂 local、一臂立即数（`flags` 恰有一位）。
pub fn encode_ret_i32_select_cmp_mixed(cmp: I32Cmp, a: u16, b: u16, then_arm: SelectArm, else_arm: SelectArm) -> Vec<u8> {
    let (flags, then_bytes, else_bytes) = match (then_arm, else_arm) {
        (SelectArm::Imm(then_imm), SelectArm::Local(else_slot)) => {
            let mut else_pad = [0u8; 4];
            else_pad[..2].copy_from_slice(&else_slot.to_le_bytes());
            (SELECT_THEN_IMM, then_imm.to_le_bytes(), else_pad)
        }
        (SelectArm::Local(then_slot), SelectArm::Imm(else_imm)) => {
            let mut then_pad = [0u8; 4];
            then_pad[..2].copy_from_slice(&then_slot.to_le_bytes());
            (SELECT_ELSE_IMM, then_pad, else_imm.to_le_bytes())
        }
        _ => panic!("encode_ret_i32_select_cmp_mixed requires exactly one Imm arm"),
    };
    let mut out = Vec::with_capacity(19);
    out.extend_from_slice(MACHINE_CODE_MAGIC);
    out.push(op::RET_I32_SELECT_CMP_MIXED);
    out.push(cmp.to_u8());
    out.extend_from_slice(&a.to_le_bytes());
    out.extend_from_slice(&b.to_le_bytes());
    out.push(flags);
    out.extend_from_slice(&then_bytes);
    out.extend_from_slice(&else_bytes);
    out
}

/// void 返回叶。
pub fn encode_ret_void() -> Vec<u8> {
    let mut out = Vec::with_capacity(5);
    out.extend_from_slice(MACHINE_CODE_MAGIC);
    out.push(op::RET_VOID);
    out
}

/// 解码失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MachineCodeError {
    /// 魔数或长度不匹配。
    InvalidBlob,
    /// 未知操作码。
    UnknownOpcode(u8),
}

impl std::fmt::Display for MachineCodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBlob => write!(f, "invalid NJ1 machine-code blob"),
            Self::UnknownOpcode(op) => write!(f, "unknown NJ1 opcode {op:#x}"),
        }
    }
}

impl std::error::Error for MachineCodeError {}

/// 已解码的一条基线标量程序（单出口）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScalarProgram {
    /// 返回指定 local。
    RetLocal {
        /// local 下标。
        slot: u16,
    },
    /// 两 local 做 i32 二元运算后返回。
    RetI32BinopLocals {
        /// 运算种类。
        binop: I32Binop,
        /// 左操作数 local。
        a: u16,
        /// 右操作数 local。
        b: u16,
    },
    /// 两 local 做 i32 比较，返回 `0`/`1`。
    RetI32CmpLocals {
        /// 比较种类。
        cmp: I32Cmp,
        /// 左操作数 local。
        a: u16,
        /// 右操作数 local。
        b: u16,
    },
    /// 比较成立返回 `then_slot`，否则返回 `else_slot`（值原样拷贝，不强制 i32）。
    RetI32SelectCmpLocals {
        /// 比较种类。
        cmp: I32Cmp,
        /// 左操作数 local（须为 i32）。
        a: u16,
        /// 右操作数 local（须为 i32）。
        b: u16,
        /// 真分支 local。
        then_slot: u16,
        /// 假分支 local。
        else_slot: u16,
    },
    /// 返回立即 i32。
    RetConstI32 {
        /// 常量值。
        value: i32,
    },
    /// 立即数与单 local 做 i32 二元运算后返回。
    RetI32BinopImmLocal {
        /// 运算种类。
        binop: I32Binop,
        /// 立即数。
        imm: i32,
        /// local 下标。
        local: u16,
        /// 立即数是否为左操作数。
        imm_on_left: bool,
    },
    /// 立即数与单 local 做 i32 比较，返回 `0`/`1`。
    RetI32CmpImmLocal {
        /// 比较种类。
        cmp: I32Cmp,
        /// 立即数。
        imm: i32,
        /// local 下标。
        local: u16,
        /// 立即数是否为左操作数。
        imm_on_left: bool,
    },
    /// 比较成立返回 `then_imm`，否则返回 `else_imm`。
    RetI32SelectCmpConsts {
        /// 比较种类。
        cmp: I32Cmp,
        /// 左操作数 local。
        a: u16,
        /// 右操作数 local。
        b: u16,
        /// 真分支立即数。
        then_imm: i32,
        /// 假分支立即数。
        else_imm: i32,
    },
    /// 比较成立返回真臂，否则假臂（一臂 local、一臂立即数）。
    RetI32SelectCmpMixed {
        /// 比较种类。
        cmp: I32Cmp,
        /// 左操作数 local。
        a: u16,
        /// 右操作数 local。
        b: u16,
        /// 真分支。
        then_arm: SelectArm,
        /// 假分支。
        else_arm: SelectArm,
    },
    /// 无返回值。
    RetVoid,
}

/// 解码 NJ1 blob。
pub fn decode_scalar_program(blob: &[u8]) -> Result<ScalarProgram, MachineCodeError> {
    if blob.len() < 5 || &blob[..4] != MACHINE_CODE_MAGIC {
        return Err(MachineCodeError::InvalidBlob);
    }
    match blob[4] {
        op::RET_LOCAL => {
            if blob.len() != 7 {
                return Err(MachineCodeError::InvalidBlob);
            }
            let slot = u16::from_le_bytes([blob[5], blob[6]]);
            Ok(ScalarProgram::RetLocal { slot })
        }
        op::RET_I32_ADD_LOCALS | op::RET_I32_SUB_LOCALS | op::RET_I32_MUL_LOCALS | op::RET_I32_DIV_LOCALS | op::RET_I32_REM_LOCALS => {
            if blob.len() != 9 {
                return Err(MachineCodeError::InvalidBlob);
            }
            let binop = match blob[4] {
                op::RET_I32_ADD_LOCALS => I32Binop::Add,
                op::RET_I32_SUB_LOCALS => I32Binop::Sub,
                op::RET_I32_MUL_LOCALS => I32Binop::Mul,
                op::RET_I32_DIV_LOCALS => I32Binop::DivS,
                op::RET_I32_REM_LOCALS => I32Binop::RemS,
                _ => unreachable!(),
            };
            let a = u16::from_le_bytes([blob[5], blob[6]]);
            let b = u16::from_le_bytes([blob[7], blob[8]]);
            Ok(ScalarProgram::RetI32BinopLocals { binop, a, b })
        }
        op::RET_CONST_I32 => {
            if blob.len() != 9 {
                return Err(MachineCodeError::InvalidBlob);
            }
            let value = i32::from_le_bytes([blob[5], blob[6], blob[7], blob[8]]);
            Ok(ScalarProgram::RetConstI32 { value })
        }
        op::RET_I32_CMP_LOCALS => {
            if blob.len() != 10 {
                return Err(MachineCodeError::InvalidBlob);
            }
            let cmp = I32Cmp::from_u8(blob[5]).ok_or(MachineCodeError::InvalidBlob)?;
            let a = u16::from_le_bytes([blob[6], blob[7]]);
            let b = u16::from_le_bytes([blob[8], blob[9]]);
            Ok(ScalarProgram::RetI32CmpLocals { cmp, a, b })
        }
        op::RET_I32_SELECT_CMP_LOCALS => {
            if blob.len() != 14 {
                return Err(MachineCodeError::InvalidBlob);
            }
            let cmp = I32Cmp::from_u8(blob[5]).ok_or(MachineCodeError::InvalidBlob)?;
            let a = u16::from_le_bytes([blob[6], blob[7]]);
            let b = u16::from_le_bytes([blob[8], blob[9]]);
            let then_slot = u16::from_le_bytes([blob[10], blob[11]]);
            let else_slot = u16::from_le_bytes([blob[12], blob[13]]);
            Ok(ScalarProgram::RetI32SelectCmpLocals { cmp, a, b, then_slot, else_slot })
        }
        op::RET_I32_BINOP_IMM_LOCAL => {
            if blob.len() != 13 {
                return Err(MachineCodeError::InvalidBlob);
            }
            let binop = I32Binop::from_u8(blob[5]).ok_or(MachineCodeError::InvalidBlob)?;
            let imm_on_left = match blob[6] {
                IMM_ON_LEFT => true,
                IMM_ON_RIGHT => false,
                _ => return Err(MachineCodeError::InvalidBlob),
            };
            let imm = i32::from_le_bytes([blob[7], blob[8], blob[9], blob[10]]);
            let local = u16::from_le_bytes([blob[11], blob[12]]);
            Ok(ScalarProgram::RetI32BinopImmLocal { binop, imm, local, imm_on_left })
        }
        op::RET_I32_CMP_IMM_LOCAL => {
            if blob.len() != 13 {
                return Err(MachineCodeError::InvalidBlob);
            }
            let cmp = I32Cmp::from_u8(blob[5]).ok_or(MachineCodeError::InvalidBlob)?;
            let imm_on_left = match blob[6] {
                IMM_ON_LEFT => true,
                IMM_ON_RIGHT => false,
                _ => return Err(MachineCodeError::InvalidBlob),
            };
            let imm = i32::from_le_bytes([blob[7], blob[8], blob[9], blob[10]]);
            let local = u16::from_le_bytes([blob[11], blob[12]]);
            Ok(ScalarProgram::RetI32CmpImmLocal { cmp, imm, local, imm_on_left })
        }
        op::RET_I32_SELECT_CMP_CONSTS => {
            if blob.len() != 18 {
                return Err(MachineCodeError::InvalidBlob);
            }
            let cmp = I32Cmp::from_u8(blob[5]).ok_or(MachineCodeError::InvalidBlob)?;
            let a = u16::from_le_bytes([blob[6], blob[7]]);
            let b = u16::from_le_bytes([blob[8], blob[9]]);
            let then_imm = i32::from_le_bytes([blob[10], blob[11], blob[12], blob[13]]);
            let else_imm = i32::from_le_bytes([blob[14], blob[15], blob[16], blob[17]]);
            Ok(ScalarProgram::RetI32SelectCmpConsts { cmp, a, b, then_imm, else_imm })
        }
        op::RET_I32_SELECT_CMP_MIXED => {
            if blob.len() != 19 {
                return Err(MachineCodeError::InvalidBlob);
            }
            let cmp = I32Cmp::from_u8(blob[5]).ok_or(MachineCodeError::InvalidBlob)?;
            let a = u16::from_le_bytes([blob[6], blob[7]]);
            let b = u16::from_le_bytes([blob[8], blob[9]]);
            let flags = blob[10];
            let then_raw = i32::from_le_bytes([blob[11], blob[12], blob[13], blob[14]]);
            let else_raw = i32::from_le_bytes([blob[15], blob[16], blob[17], blob[18]]);
            let (then_arm, else_arm) = match flags {
                SELECT_THEN_IMM => (SelectArm::Imm(then_raw), SelectArm::Local(u16::from_le_bytes([blob[15], blob[16]]))),
                SELECT_ELSE_IMM => (SelectArm::Local(u16::from_le_bytes([blob[11], blob[12]])), SelectArm::Imm(else_raw)),
                _ => return Err(MachineCodeError::InvalidBlob),
            };
            Ok(ScalarProgram::RetI32SelectCmpMixed { cmp, a, b, then_arm, else_arm })
        }
        op::RET_VOID => {
            if blob.len() != 5 {
                return Err(MachineCodeError::InvalidBlob);
            }
            Ok(ScalarProgram::RetVoid)
        }
        other => Err(MachineCodeError::UnknownOpcode(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_ret_local() {
        let blob = encode_ret_local(3);
        assert_eq!(decode_scalar_program(&blob).unwrap(), ScalarProgram::RetLocal { slot: 3 });
    }

    #[test]
    fn roundtrip_ret_i32_binops() {
        for binop in [I32Binop::Add, I32Binop::Sub, I32Binop::Mul, I32Binop::DivS, I32Binop::RemS] {
            let blob = encode_ret_i32_binop_locals(binop, 0, 1);
            assert_eq!(decode_scalar_program(&blob).unwrap(), ScalarProgram::RetI32BinopLocals { binop, a: 0, b: 1 });
        }
    }

    #[test]
    fn roundtrip_ret_const_i32() {
        let blob = encode_ret_const_i32(-42);
        assert_eq!(decode_scalar_program(&blob).unwrap(), ScalarProgram::RetConstI32 { value: -42 });
    }

    #[test]
    fn roundtrip_ret_i32_cmp_and_select() {
        let blob = encode_ret_i32_cmp_locals(I32Cmp::LtS, 0, 1);
        assert_eq!(decode_scalar_program(&blob).unwrap(), ScalarProgram::RetI32CmpLocals { cmp: I32Cmp::LtS, a: 0, b: 1 });
        let blob = encode_ret_i32_select_cmp_locals(I32Cmp::Eq, 0, 1, 2, 3);
        assert_eq!(
            decode_scalar_program(&blob).unwrap(),
            ScalarProgram::RetI32SelectCmpLocals { cmp: I32Cmp::Eq, a: 0, b: 1, then_slot: 2, else_slot: 3 }
        );
    }

    #[test]
    fn roundtrip_ret_i32_select_cmp_consts() {
        let blob = encode_ret_i32_select_cmp_consts(I32Cmp::Eq, 0, 1, 7, 9);
        assert_eq!(
            decode_scalar_program(&blob).unwrap(),
            ScalarProgram::RetI32SelectCmpConsts { cmp: I32Cmp::Eq, a: 0, b: 1, then_imm: 7, else_imm: 9 }
        );
    }

    #[test]
    fn roundtrip_ret_i32_imm_local() {
        let blob = encode_ret_i32_binop_imm_local(I32Binop::Add, 10, 0, true);
        assert_eq!(
            decode_scalar_program(&blob).unwrap(),
            ScalarProgram::RetI32BinopImmLocal { binop: I32Binop::Add, imm: 10, local: 0, imm_on_left: true }
        );
        let blob = encode_ret_i32_cmp_imm_local(I32Cmp::LtS, 5, 1, false);
        assert_eq!(
            decode_scalar_program(&blob).unwrap(),
            ScalarProgram::RetI32CmpImmLocal { cmp: I32Cmp::LtS, imm: 5, local: 1, imm_on_left: false }
        );
    }

    #[test]
    fn roundtrip_ret_i32_select_cmp_mixed_and_void() {
        let blob = encode_ret_i32_select_cmp_mixed(I32Cmp::Eq, 0, 1, SelectArm::Imm(7), SelectArm::Local(2));
        assert_eq!(
            decode_scalar_program(&blob).unwrap(),
            ScalarProgram::RetI32SelectCmpMixed { cmp: I32Cmp::Eq, a: 0, b: 1, then_arm: SelectArm::Imm(7), else_arm: SelectArm::Local(2) }
        );
        let blob = encode_ret_i32_select_cmp_mixed(I32Cmp::Ne, 0, 1, SelectArm::Local(3), SelectArm::Imm(9));
        assert_eq!(
            decode_scalar_program(&blob).unwrap(),
            ScalarProgram::RetI32SelectCmpMixed { cmp: I32Cmp::Ne, a: 0, b: 1, then_arm: SelectArm::Local(3), else_arm: SelectArm::Imm(9) }
        );
        assert_eq!(decode_scalar_program(&encode_ret_void()).unwrap(), ScalarProgram::RetVoid);
    }
}
