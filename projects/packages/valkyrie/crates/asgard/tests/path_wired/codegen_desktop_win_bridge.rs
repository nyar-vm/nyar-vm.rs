//! Migrated from `src/codegen/desktop_win_bridge.rs` (`tests`).

use super::*;

#[test]
fn win_bridge_exports_symbols() {
    let src = generate_desktop_win_bridge();
    assert!(src.contains("asgard_invoke_export"));
    assert!(src.contains("asgard_patch_native"));
}
