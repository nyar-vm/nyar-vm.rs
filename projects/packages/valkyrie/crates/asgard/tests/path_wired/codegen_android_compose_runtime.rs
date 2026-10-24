//! Migrated from `src/codegen/android_compose_runtime.rs` (`tests`).

use super::*;

#[test]
fn runtime_contains_compose_mount_patch() {
    let rt = generate_android_compose_runtime();
    assert!(rt.contains("AsgardComposeRuntime"));
    assert!(rt.contains("fun mount("));
    assert!(rt.contains("fun patch("));
    assert!(rt.contains("fun on_event("));
    assert!(rt.contains("System.load"));
    assert!(rt.contains("findAsgardNativeSection"));
    assert!(rt.contains("resolveCallExport"));
    assert!(rt.contains("decodeAsgardUi"));
    assert!(rt.contains("patchFromNative"));
    assert!(rt.contains("\"Row\""));
    assert!(rt.contains("evalLoopCount"));
    assert!(rt.contains("mutableStateMapOf"));
}
