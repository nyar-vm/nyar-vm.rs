//! 基线标量「机器码」编码（WP17 第一层）。
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
}
