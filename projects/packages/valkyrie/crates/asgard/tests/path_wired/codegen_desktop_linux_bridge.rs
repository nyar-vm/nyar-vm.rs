//! Migrated from `src/codegen/desktop_linux_bridge.rs` (`tests`).

use super::*;

#[test]
fn linux_bridge_forwards_patch() {
    let src = generate_desktop_linux_bridge();
    assert!(src.contains("asgard_patch_native"));
    assert!(src.contains("asgard_linux_patch"));
}
