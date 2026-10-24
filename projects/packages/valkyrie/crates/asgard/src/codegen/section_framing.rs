//! 制品尾段 section framing：`magic (8) || u32le len || payload`。

use crate::codegen::mobile_ui_binary::{HOST_NATIVE_MAGIC, UI_BIN_MAGIC};

/// 在制品末尾附加一节（magic 须为 8 字节）。
pub fn embed_section(container: &mut Vec<u8>, magic: &[u8], payload: &[u8]) {
    assert_eq!(magic.len(), 8, "section magic must be exactly 8 bytes");
    container.extend_from_slice(magic);
    container.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    container.extend_from_slice(payload);
}

/// 扫描制品，取**最后**一段匹配 magic 的 payload。
pub fn find_section<'a>(container: &'a [u8], magic: &[u8]) -> Option<&'a [u8]> {
    assert_eq!(magic.len(), 8, "section magic must be exactly 8 bytes");
    let magic_len = magic.len();
    if container.len() < magic_len + 4 {
        return None;
    }
    let mut idx = 0usize;
    let mut last: Option<&[u8]> = None;
    while idx + magic_len + 4 <= container.len() {
        if &container[idx..idx + magic_len] != magic {
            idx += 1;
            continue;
        }
        let len = u32::from_le_bytes(container[idx + magic_len..idx + magic_len + 4].try_into().ok()?);
        let start = idx + magic_len + 4;
        let end = start.checked_add(len as usize)?;
        if end > container.len() {
            break;
        }
        last = Some(&container[start..end]);
        idx = end;
    }
    last
}

/// asgard ui 尾段。
pub fn embed_ui_section(container: &mut Vec<u8>, payload: &[u8]) {
    embed_section(container, UI_BIN_MAGIC, payload);
}

/// asgard ui 尾段扫描。
pub fn find_ui_section(container: &[u8]) -> Option<&[u8]> {
    find_section(container, UI_BIN_MAGIC)
}

/// native AOT 尾段。
pub fn embed_native_section(container: &mut Vec<u8>, payload: &[u8]) {
    embed_section(container, HOST_NATIVE_MAGIC, payload);
}

/// native AOT 尾段扫描。
pub fn find_native_section(container: &[u8]) -> Option<&[u8]> {
    find_section(container, HOST_NATIVE_MAGIC)
}

/// 将 `UI_BIN_MAGIC` 格式化为 JS / Kotlin 字节数组字面量片段（`0x41, 0x53, ...`）。
pub fn magic_bytes_literal(magic: &[u8]) -> String {
    magic.iter().map(|b| format!("0x{b:02X}")).collect::<Vec<_>>().join(", ")
}
#[cfg(test)]
#[path = "../../tests/path_wired/codegen_section_framing.rs"]
mod codegen_section_framing_tests;
