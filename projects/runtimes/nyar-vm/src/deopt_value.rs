//! deopt `Provided` 载荷与 [`crate::Value`] 之间的最小编解码。
//!
//! 格式（大端无关：整数用 little-endian）：
//! - `0x00` Null
//! - `0x01` Bool + `u8`（0/1）
//! - `0x02` I32 + `i32`
//! - `0x03` I64 + `i64`
//! - `0x04` F32 + `u32` bits
//! - `0x05` F64 + `u64` bits
//! - `0x06` String + `u32` len + UTF-8
//!
//! 堆引用不得编码裸 `ObjectId`：须经 [`HostRoots`] 固定为 `RootHandle`（tag `0x07`）。

use nyar_gc::{HostRoots, RootHandle};

use crate::{error::NyarRuntimeError, value::Value};

const TAG_NULL: u8 = 0;
const TAG_BOOL: u8 = 1;
const TAG_I32: u8 = 2;
const TAG_I64: u8 = 3;
const TAG_F32: u8 = 4;
const TAG_F64: u8 = 5;
const TAG_STRING: u8 = 6;
/// 宿主持久根句柄（`u32` 槽下标），解码时从 [`HostRoots`] 取回 Value。
const TAG_ROOT_HANDLE: u8 = 0x07;

/// 将标量 / 字符串 [`Value`] 编码为 deopt `Provided` 字节。
pub fn encode_value_for_deopt(value: &Value) -> Result<Vec<u8>, NyarRuntimeError> {
    match value {
        Value::Null => Ok(vec![TAG_NULL]),
        Value::Bool(v) => Ok(vec![TAG_BOOL, u8::from(*v)]),
        Value::I32(v) => {
            let mut out = vec![TAG_I32];
            out.extend_from_slice(&v.to_le_bytes());
            Ok(out)
        }
        Value::I64(v) => {
            let mut out = vec![TAG_I64];
            out.extend_from_slice(&v.to_le_bytes());
            Ok(out)
        }
        Value::F32(v) => {
            let mut out = vec![TAG_F32];
            out.extend_from_slice(&v.to_bits().to_le_bytes());
            Ok(out)
        }
        Value::F64(v) => {
            let mut out = vec![TAG_F64];
            out.extend_from_slice(&v.to_bits().to_le_bytes());
            Ok(out)
        }
        Value::String(s) => {
            let bytes = s.as_bytes();
            let len = u32::try_from(bytes.len())
                .map_err(|_| NyarRuntimeError::UnsupportedFeature("deopt string longer than u32::MAX"))?;
            let mut out = Vec::with_capacity(1 + 4 + bytes.len());
            out.push(TAG_STRING);
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(bytes);
            Ok(out)
        }
        Value::Object(_) | Value::Coroutine(_) => Err(NyarRuntimeError::UnsupportedFeature(
            "deopt Provided codec refuses bare heap ids; use encode_value_for_deopt_pinned",
        )),
    }
}

/// 编码值；堆引用先 [`HostRoots::pin`] 再写入 `RootHandle` 槽下标。
pub fn encode_value_for_deopt_pinned(value: &Value, roots: &mut HostRoots) -> Result<Vec<u8>, NyarRuntimeError> {
    match value {
        Value::Object(_) | Value::Coroutine(_) => {
            let handle = roots.pin(value.clone());
            let mut out = vec![TAG_ROOT_HANDLE];
            let index = u32::try_from(handle.index())
                .map_err(|_| NyarRuntimeError::UnsupportedFeature("root handle index exceeds u32"))?;
            out.extend_from_slice(&index.to_le_bytes());
            Ok(out)
        }
        other => encode_value_for_deopt(other),
    }
}

/// 解码 deopt `Provided` 字节为 [`Value`]（无根表；`RootHandle` 标签失败）。
pub fn decode_value_from_deopt(bytes: &[u8]) -> Result<Value, NyarRuntimeError> {
    decode_value_from_deopt_with_roots(bytes, None)
}

/// 解码；若载荷为 `RootHandle`，必须提供 [`HostRoots`]。
pub fn decode_value_from_deopt_with_roots(
    bytes: &[u8],
    roots: Option<&HostRoots>,
) -> Result<Value, NyarRuntimeError> {
    let Some((tag, rest)) = bytes.split_first()
    else {
        return Err(NyarRuntimeError::UnsupportedFeature("empty deopt Provided payload"));
    };
    match *tag {
        TAG_NULL => {
            if !rest.is_empty() {
                return Err(NyarRuntimeError::UnsupportedFeature("trailing bytes after Null tag"));
            }
            Ok(Value::Null)
        }
        TAG_BOOL => {
            let Some((b, rest)) = rest.split_first()
            else {
                return Err(NyarRuntimeError::UnsupportedFeature("truncated Bool payload"));
            };
            if !rest.is_empty() {
                return Err(NyarRuntimeError::UnsupportedFeature("trailing bytes after Bool"));
            }
            Ok(Value::Bool(*b != 0))
        }
        TAG_I32 => {
            if rest.len() != 4 {
                return Err(NyarRuntimeError::UnsupportedFeature("I32 payload must be 4 bytes"));
            }
            let mut buf = [0u8; 4];
            buf.copy_from_slice(rest);
            Ok(Value::I32(i32::from_le_bytes(buf)))
        }
        TAG_I64 => {
            if rest.len() != 8 {
                return Err(NyarRuntimeError::UnsupportedFeature("I64 payload must be 8 bytes"));
            }
            let mut buf = [0u8; 8];
            buf.copy_from_slice(rest);
            Ok(Value::I64(i64::from_le_bytes(buf)))
        }
        TAG_F32 => {
            if rest.len() != 4 {
                return Err(NyarRuntimeError::UnsupportedFeature("F32 payload must be 4 bytes"));
            }
            let mut buf = [0u8; 4];
            buf.copy_from_slice(rest);
            Ok(Value::F32(f32::from_bits(u32::from_le_bytes(buf))))
        }
        TAG_F64 => {
            if rest.len() != 8 {
                return Err(NyarRuntimeError::UnsupportedFeature("F64 payload must be 8 bytes"));
            }
            let mut buf = [0u8; 8];
            buf.copy_from_slice(rest);
            Ok(Value::F64(f64::from_bits(u64::from_le_bytes(buf))))
        }
        TAG_STRING => {
            if rest.len() < 4 {
                return Err(NyarRuntimeError::UnsupportedFeature("truncated String length"));
            }
            let mut len_buf = [0u8; 4];
            len_buf.copy_from_slice(&rest[..4]);
            let len = u32::from_le_bytes(len_buf) as usize;
            let data = &rest[4..];
            if data.len() != len {
                return Err(NyarRuntimeError::UnsupportedFeature("String payload length mismatch"));
            }
            let s = std::str::from_utf8(data)
                .map_err(|_| NyarRuntimeError::UnsupportedFeature("String payload is not UTF-8"))?;
            Ok(Value::String(s.to_string()))
        }
        TAG_ROOT_HANDLE => {
            if rest.len() != 4 {
                return Err(NyarRuntimeError::UnsupportedFeature("RootHandle payload must be 4 bytes"));
            }
            let mut buf = [0u8; 4];
            buf.copy_from_slice(rest);
            let index = u32::from_le_bytes(buf) as usize;
            let roots = roots.ok_or(NyarRuntimeError::UnsupportedFeature(
                "RootHandle deopt payload requires HostRoots",
            ))?;
            roots
                .get(RootHandle::from_index(index))
                .cloned()
                .ok_or(NyarRuntimeError::UnsupportedFeature("RootHandle slot is empty or invalid"))
        }
        _ => Err(NyarRuntimeError::UnsupportedFeature("unknown deopt value tag")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_scalars_and_string() {
        for value in [
            Value::Null,
            Value::Bool(true),
            Value::I32(-7),
            Value::I64(99),
            Value::F32(1.5),
            Value::F64(-2.25),
            Value::String("phase".into()),
        ] {
            let bytes = encode_value_for_deopt(&value).expect("encode");
            let decoded = decode_value_from_deopt(&bytes).expect("decode");
            assert_eq!(decoded, value);
        }
    }

    #[test]
    fn rejects_bare_heap_refs_without_pin() {
        assert!(encode_value_for_deopt(&Value::Object(1)).is_err());
        assert!(encode_value_for_deopt(&Value::Coroutine(2)).is_err());
    }

    #[test]
    fn roundtrips_object_via_root_handle() {
        let mut roots = HostRoots::new();
        let bytes = encode_value_for_deopt_pinned(&Value::Object(7), &mut roots).expect("encode");
        let decoded = decode_value_from_deopt_with_roots(&bytes, Some(&roots)).expect("decode");
        assert_eq!(decoded, Value::Object(7));
        assert!(decode_value_from_deopt(&bytes).is_err());
    }
}
