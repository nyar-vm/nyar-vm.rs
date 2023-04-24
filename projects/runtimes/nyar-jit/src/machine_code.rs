//! 基线标量「机器码」编码（WP17 第一层）。
//!
//! 这不是 OS 可执行页上的原生指令，而是版本化的可解释/可降低中间字节，
//! 供后续后端翻译为真实机器码或由 VM 快路径执行。禁止在未经验证的路径上
//! `transmute` 为函数指针。

/// 魔数：`NJ1\0`（Nyar JIT v1）。
pub const MACHINE_CODE_MAGIC: &[u8; 4] = b"NJ1\0";

/// 操作码。
pub mod op {
    /// `return locals[slot]`（i32 槽语义由调用约定解释）。
    pub const RET_LOCAL: u8 = 0x01;
    /// `return (i32)locals[a] + (i32)locals[b]`。
    pub const RET_I32_ADD_LOCALS: u8 = 0x02;
}

/// 将 `RetLocal` 编码为 blob。
pub fn encode_ret_local(slot: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(7);
    out.extend_from_slice(MACHINE_CODE_MAGIC);
    out.push(op::RET_LOCAL);
    out.extend_from_slice(&slot.to_le_bytes());
    out
}

/// 将 `RetI32AddLocals` 编码为 blob。
pub fn encode_ret_i32_add_locals(a: u16, b: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(9);
    out.extend_from_slice(MACHINE_CODE_MAGIC);
    out.push(op::RET_I32_ADD_LOCALS);
    out.extend_from_slice(&a.to_le_bytes());
    out.extend_from_slice(&b.to_le_bytes());
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
    /// 两 local 做 i32 加后返回。
    RetI32AddLocals {
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
        op::RET_I32_ADD_LOCALS => {
            if blob.len() != 9 {
                return Err(MachineCodeError::InvalidBlob);
            }
            let a = u16::from_le_bytes([blob[5], blob[6]]);
            let b = u16::from_le_bytes([blob[7], blob[8]]);
            Ok(ScalarProgram::RetI32AddLocals { a, b })
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
    fn roundtrip_ret_i32_add() {
        let blob = encode_ret_i32_add_locals(0, 1);
        assert_eq!(decode_scalar_program(&blob).unwrap(), ScalarProgram::RetI32AddLocals { a: 0, b: 1 });
    }
}
