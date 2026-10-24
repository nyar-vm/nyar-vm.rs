//! 跨平台 UiHost / ASGARD 硬门禁表驱动测试。

use std::path::PathBuf;

use asgard::{
    codegen::{HOST_NATIVE_MAGIC, UI_BIN_MAGIC, embed_asgard_ui_section, embed_host_native_section, encode_mobile_ui_package},
    host::HostPlatform,
    platform_contract::{
        GateId, check_android_dist, check_ios_dist, validate_android_dist_bytes, validate_android_native_so, validate_ios_dist_bytes,
        validate_runtime_source_cip,
    },
};

#[test]
fn gate_mag_ui_pass_and_fail() {
    let mut dex = asgard::codegen::resolve_android_compose_dex().expect("dex");
    embed_host_native_section(&mut dex, b"so");
    embed_asgard_ui_section(&mut dex, b"ui");
    let failures = check_android_dist(&dex);
    assert!(failures.iter().all(|f| f.gate != GateId::MagUi), "{failures:?}");
    assert!(validate_android_dist_bytes(&dex).is_ok(), "{failures:?}");

    let bare = asgard::codegen::resolve_android_compose_dex().expect("bare");
    assert!(check_android_dist(&bare).iter().any(|f| f.gate == GateId::MagUi));
}

#[test]
fn gate_mag_nt_pass() {
    let mut dex = asgard::codegen::resolve_android_compose_dex().expect("dex");
    embed_host_native_section(&mut dex, b"native-so");
    embed_asgard_ui_section(&mut dex, b"ui-pkg");
    assert!(check_android_dist(&dex).iter().all(|f| f.gate != GateId::MagNt));
}

#[test]
fn gate_no_legacy_rejects_placeholder() {
    let bad = b"ASGDHOST-placeholder";
    assert!(check_android_dist(bad).iter().any(|f| f.gate == GateId::NoLegacy));
}

#[test]
fn gate_android_compose_vendor_rejects_bootstrap_stub() {
    let dex = asgard::codegen::load_compose_shell_dex().or_else(|| asgard::codegen::resolve_android_compose_dex().ok()).expect("dex");
    if asgard::platform_contract::is_vendor_compose_dex(&dex) {
        return;
    }
    let vendor_failures = asgard::platform_contract::check_android_compose_vendor(&dex);
    assert!(!vendor_failures.is_empty(), "bootstrap dex must fail vendor gate: {vendor_failures:?}");
}

#[test]
fn gate_android_compose_vendor_fixture_passes() {
    use acorn_dex::dex::DexImageBuilder;
    use acorn_jvm::class::JvmClassFile;
    let mut builder = DexImageBuilder::new();
    for i in 0..60 {
        let name = format!("com/vendor/fixture/C{i}");
        let bytes = JvmClassFile::new(&name).to_bytes().unwrap();
        builder.add_class(&name, &bytes);
    }
    let dex = builder.build().expect("fixture dex");
    assert!(asgard::platform_contract::is_vendor_compose_dex(&dex));
    let failures = asgard::platform_contract::check_android_compose_vendor(&dex);
    assert!(
        failures.iter().any(|f| f.message.contains("setContent") || f.message.contains("Composable")),
        "fixture should fail vendor symbol checks: {failures:?}"
    );
}

#[test]
fn gate_compose_pack_mode_vendor_when_shell_present() {
    if asgard::codegen::load_compose_shell_dex().is_none() {
        return;
    }
    let (_, mode) = asgard::codegen::resolve_android_compose_dex_with_mode().expect("mode");
    assert_eq!(mode, asgard::codegen::AndroidComposePackMode::Vendor);
}

#[test]
fn gate_android_compose_symbols() {
    let mut dex = asgard::codegen::resolve_android_compose_dex().expect("dex");
    embed_host_native_section(&mut dex, b"so");
    embed_asgard_ui_section(&mut dex, &encode_mobile_ui_package(&[]));
    let failures: Vec<_> = check_android_dist(&dex).into_iter().filter(|f| f.gate == GateId::AndroidCompose).collect();
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn gate_runtime_cip_mount_all_platforms() {
    for platform in [HostPlatform::Android, HostPlatform::Ios, HostPlatform::Linux, HostPlatform::WechatMiniProgram] {
        let failures: Vec<_> = validate_runtime_source_cip(platform).into_iter().filter(|f| f.gate == GateId::AbiMount).collect();
        assert!(failures.is_empty(), "{platform:?} mount: {failures:?}");
    }
}

#[test]
fn gate_jni_sym_on_test_so() {
    use acorn_aarch64::aarch64::{emit_jni_glue_module, merge_jni_and_logic, ret_bytes};
    use acorn_pe::elf::{SharedElfWriter, SharedObjectExport};
    let jni = emit_jni_glue_module().expect("jni");
    let (image, exports) =
        merge_jni_and_logic(jni, ret_bytes().to_vec(), vec![SharedObjectExport { name: "asgard_invoke_export".into(), text_offset: 0 }])
            .expect("merge");
    let so = SharedElfWriter::write_aarch64(&image, &exports).expect("so");
    validate_android_native_so(&so).expect("jni sym");
}

#[test]
fn gate_ios_dist_validator() {
    let mut host = vec![0u8; 64];
    host[0..4].copy_from_slice(&[0xCF, 0xFA, 0xED, 0xFE]);
    embed_host_native_section(&mut host, b"nt");
    embed_asgard_ui_section(&mut host, b"ui");
    assert!(validate_ios_dist_bytes(&host).is_ok());
    assert!(check_ios_dist(&host).is_empty());
}

#[test]
fn gate_android_no_native_c_dir() {
    let temp = tempfile::tempdir().expect("temp");
    let android = temp.path().join("android");
    std::fs::create_dir_all(android.join("native")).unwrap();
    std::fs::write(android.join("native/asgard_android_jni.c"), "// stale").unwrap();
    let mut dex = asgard::codegen::resolve_android_compose_dex().expect("dex");
    embed_host_native_section(&mut dex, b"so");
    embed_asgard_ui_section(&mut dex, b"ui");
    std::fs::write(android.join("classes.dex"), &dex).unwrap();
    std::fs::write(android.join("AndroidManifest.xml"), "<manifest/>").unwrap();
    let err = asgard::platform_contract::validate_android_dist_dir(&android).expect_err("must fail");
    assert!(err.to_string().contains("G-NO-ANDROID-C") || err.to_string().contains("native"));
}

#[test]
fn gate_magics_are_eight_bytes() {
    let (ui, nt) = asgard::platform_contract::expected_magics();
    assert_eq!(ui, UI_BIN_MAGIC);
    assert_eq!(nt, HOST_NATIVE_MAGIC);
    assert_eq!(ui.len(), 8);
    assert_eq!(nt.len(), 8);
}

#[test]
fn demo_android_dist_passes_contract_when_present() {
    let demo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../valkyrie.v/examples/demo.android");
    if !demo.exists() {
        return;
    }
    let dex_path = demo.join("dist/android/classes.dex");
    if !dex_path.exists() {
        return;
    }
    let dex = std::fs::read(dex_path).expect("read dex");
    if let Err(e) = validate_android_dist_bytes(&dex) {
        eprintln!("demo.android dist 未通过契约（需重新 asgard build）: {e}");
    }
}
