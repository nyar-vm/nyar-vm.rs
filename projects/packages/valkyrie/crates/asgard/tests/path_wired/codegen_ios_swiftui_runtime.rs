//! Migrated from `src/codegen/ios_swiftui_runtime.rs` (`tests`).

use super::*;

#[test]
fn runtime_contains_swiftui_mount_patch() {
    let rt = generate_ios_swiftui_runtime();
    assert!(rt.contains("AsgardSwiftUiRuntime"));
    assert!(rt.contains("func mount("));
    assert!(rt.contains("func patch("));
    assert!(rt.contains("func on_event("));
    assert!(rt.contains("decodeAsgardUi"));
    assert!(rt.contains("findAsgardNativeSection"));
    assert!(rt.contains("loadNativeFromExecutable"));
    assert!(rt.contains("dlopen"));
}

#[test]
fn desktop_profile_has_no_main() {
    let rt = generate_swiftui_runtime(SwiftUiProfile::Desktop);
    assert!(!rt.contains("@main"));
    assert!(rt.contains("loadFromProduct"));
}
