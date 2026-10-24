//! Migrated from `src/codegen/android_shell.rs` (`tests`).

use super::*;

#[test]
fn shell_dex_contains_bridge_and_activity() {
    let dex = emit_android_shell_dex().expect("dex");
    assert!(dex.starts_with(b"dex\n035"));
    let text = String::from_utf8_lossy(&dex);
    assert!(text.contains("AsgardHostBridge"));
    assert!(text.contains("MainActivity"));
    assert!(text.contains("invokeExport"));
}
