//! Migrated from `src/codegen/android_compose_pack.rs` (`tests`).

use super::*;

use acorn_dex::dex::dex_contains_strings;
use std::path::PathBuf;

#[test]
fn compose_dex_has_required_symbols() {
    let (dex, mode) = resolve_android_compose_dex_with_mode().expect("dex");
    assert!(dex.starts_with(b"dex\n035"));
    for needle in ["AsgardComposeRuntime", "AsgardHostBridge", "ComponentActivity", "androidx/compose"] {
        assert!(dex_contains_strings(&dex, &[needle]).unwrap(), "missing class symbol {needle}");
    }
    assert!(
        dex_contains_strings(&dex, &["decodeAsgardUi", "AsgardRoot", "mount", "patch", "on_event", "invokeExport", "patchFromNative"]).unwrap()
    );
    if mode == AndroidComposePackMode::Vendor {
        validate_compose_shell_dex(&dex).expect("vendor shell");
    }
}

#[test]
fn vendor_mode_loads_compose_shell_file() {
    if load_compose_shell_dex().is_none() {
        return;
    }
    let (dex, mode) = resolve_android_compose_dex_with_mode().expect("resolve");
    assert_eq!(mode, AndroidComposePackMode::Vendor);
    assert_eq!(dex, load_compose_shell_dex().unwrap());
}

/// 维护者：写出 Kotlin 源（`--ignored`）。
#[test]
#[ignore = "maintainer: writes kotlin sources for SDK build"]
fn write_kotlin_sources_for_vendor_build() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/android-compose/build/kotlin");
    write_android_compose_kotlin_sources(&out).expect("kotlin sources");
}

/// 维护者：写出 vendor dex 到仓库（`--ignored`）。
#[test]
#[ignore = "maintainer: writes vendor/android-compose/dex"]
fn bootstrap_vendor_dex() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/android-compose");
    write_vendor_dex_bootstrap(&out).expect("bootstrap vendor dex");
}
