//! Migrated from `src/codegen/ios_bridge.rs` (`tests`).

use super::*;

#[test]
fn ios_bridge_declares_patch_callback() {
    let src = generate_ios_bridge();
    assert!(src.contains("asgard_patch_native"));
    assert!(src.contains("asgard_patch_native_swift"));
}
