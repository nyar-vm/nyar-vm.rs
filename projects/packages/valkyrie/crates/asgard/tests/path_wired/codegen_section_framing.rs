//! Migrated from `src/codegen/section_framing.rs` (`tests`).

use super::*;

#[test]
fn embed_and_find_roundtrip() {
    let mut blob = vec![1, 2, 3];
    embed_ui_section(&mut blob, b"pkg");
    assert_eq!(find_ui_section(&blob), Some(b"pkg".as_slice()));
}

#[test]
fn native_magic_is_eight_bytes() {
    assert_eq!(HOST_NATIVE_MAGIC.len(), 8);
    assert_eq!(UI_BIN_MAGIC.len(), 8);
}
