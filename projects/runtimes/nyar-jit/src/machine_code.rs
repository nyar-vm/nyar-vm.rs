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
}

/// i32 二元运算种类（两 local → 返回值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum I32Binop {
    /// 加法。
    Add,
    /// 减法。
    Sub,
    /// 乘法。
    Mul,
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
    };
    let mut out = Vec::with_capacity(9);
    out.extend_from_slice(MACHINE_CODE_MAGIC);
    out.push(opcode);
    out.extend_from_slice(&a.to_le_bytes());
    out.extend_from_slice(&b.to_le_bytes());
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
        op::RET_I32_ADD_LOCALS | op::RET_I32_SUB_LOCALS | op::RET_I32_MUL_LOCALS => {
            if blob.len() != 9 {
                return Err(MachineCodeError::InvalidBlob);
            }
            let binop = match blob[4] {
                op::RET_I32_ADD_LOCALS => I32Binop::Add,
                op::RET_I32_SUB_LOCALS => I32Binop::Sub,
                op::RET_I32_MUL_LOCALS => I32Binop::Mul,
                _ => unreachable!(),
            };
            let a = u16::from_le_bytes([blob[5], blob[6]]);
            let b = u16::from_le_bytes([blob[7], blob[8]]);
            Ok(ScalarProgram::RetI32BinopLocals { binop, a, b })
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
            Ok(ScalarProgram::RetI32SelectCmpLocals {
                cmp,
                a,
                b,
                then_slot,
                else_slot,
            })
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
        for binop in [I32Binop::Add, I32Binop::Sub, I32Binop::Mul] {
            let blob = encode_ret_i32_binop_locals(binop, 0, 1);
            assert_eq!(
                decode_scalar_program(&blob).unwrap(),
                ScalarProgram::RetI32BinopLocals { binop, a: 0, b: 1 }
            );
        }
    }

    #[test]
    fn roundtrip_ret_i32_cmp_and_select() {
        let blob = encode_ret_i32_cmp_locals(I32Cmp::LtS, 0, 1);
        assert_eq!(
            decode_scalar_program(&blob).unwrap(),
            ScalarProgram::RetI32CmpLocals {
                cmp: I32Cmp::LtS,
                a: 0,
                b: 1
            }
        );
        let blob = encode_ret_i32_select_cmp_locals(I32Cmp::Eq, 0, 1, 2, 3);
        assert_eq!(
            decode_scalar_program(&blob).unwrap(),
            ScalarProgram::RetI32SelectCmpLocals {
                cmp: I32Cmp::Eq,
                a: 0,
                b: 1,
                then_slot: 2,
                else_slot: 3
            }
        );
    }
}
