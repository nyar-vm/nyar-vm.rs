//! Migrated from `src/codegen/desktop_native_runtime.rs` (`tests`).

use super::*;

#[test]
fn linux_native_runtime_has_uihost_abi() {
    let rt = generate_linux_native_runtime();
    assert!(rt.contains("ASGARD_UI_MAGIC"));
    assert!(rt.contains("ASGARD_NATIVE_MAGIC"));
    assert!(rt.contains("asgard_desktop_on_event"));
    assert!(rt.contains("asgard_invoke_export"));
    assert!(rt.contains("asgard_patch_native"));
    assert!(!rt.contains("asgard_linux_dispatch"));
}

#[test]
fn winui_runtime_has_on_event_and_magic() {
    let rt = generate_winui_runtime();
    assert!(rt.contains("OnEvent"));
    assert!(rt.contains("ResolveCallExport"));
    assert!(rt.contains("asgard_invoke_export"));
    assert!(rt.contains("FindAsgardNativeSection"));
    assert!(rt.contains("valueKind"));
    assert!(!rt.contains("Handlers.TryGetValue"));
}

#[test]
fn macos_desktop_has_no_main() {
    let rt = generate_macos_swiftui_runtime();
    assert!(rt.contains("AsgardSwiftUiRuntime"));
    assert!(rt.contains("loadFromProduct"));
    assert!(!rt.contains("@main"));
}

#[test]
fn asset_names_have_no_gtk() {
    for platform in ["windows", "linux", "macos"] {
        let name = desktop_runtime_asset_name(platform);
        assert!(!name.to_ascii_lowercase().contains("gtk"), "{name}");
    }
}
