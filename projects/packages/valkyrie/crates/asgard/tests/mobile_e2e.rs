//! 移动端 E2E 打包冒烟测试。

use std::{io::Read, path::PathBuf};

#[test]
fn demo_android_pack_produces_apk() {
    let demo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../valkyrie.v/examples/demo.android");
    if !demo.exists() {
        return;
    }
    let report =
        asgard::compile_voa_project(&asgard::CompileOptions { project_dir: demo.clone(), output_dir: Some(demo.join("dist")), target: None })
            .expect("demo.android compile");
    assert!(report.output_dir.join("android/classes.dex").exists());
    assert!(!report.output_dir.join("android/native").exists());
    if asgard::codegen::load_compose_shell_dex().is_some() {
        let (_, mode) = asgard::codegen::resolve_android_compose_dex_with_mode().expect("compose mode");
        assert_eq!(mode, asgard::codegen::AndroidComposePackMode::Vendor, "vendor compose-shell.dex 应启用 Vendor 模式");
    }
    asgard::platform_contract::validate_android_dist_dir(&report.output_dir.join("android")).expect("dist contract");
    let pack = asgard::pack_voa_delivery(&asgard::PackOptions {
        input: report.output_dir.clone(),
        output: Some(demo.join("pack-out")),
        target: asgard::PackTarget::Apk,
        project_name: None,
        wasm_name: None,
    })
    .expect("apk pack");
    assert_eq!(pack.artifact_path.extension().and_then(|e| e.to_str()), Some("apk"));
    let apk_bytes = std::fs::read(&pack.artifact_path).expect("read apk");
    let dex = extract_zip_entry(&apk_bytes, "classes.dex").expect("apk classes.dex");
    asgard::platform_contract::validate_android_apk_dex(&dex).expect("apk dex contract");
}

fn extract_zip_entry(apk: &[u8], name: &str) -> Option<Vec<u8>> {
    let cursor = std::io::Cursor::new(apk);
    let mut archive = zip::ZipArchive::new(cursor).ok()?;
    let mut file = archive.by_name(name).ok()?;
    let mut out = Vec::new();
    file.read_to_end(&mut out).ok()?;
    Some(out)
}

#[test]
fn demo_ios_pack_produces_ipa() {
    let demo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../valkyrie.v/examples/demo.ios");
    if !demo.exists() {
        return;
    }
    let report =
        asgard::compile_voa_project(&asgard::CompileOptions { project_dir: demo.clone(), output_dir: Some(demo.join("dist")), target: None })
            .expect("demo.ios compile");
    assert!(report.output_dir.join("ios/AsgardHost").exists());
    let pack = asgard::pack_voa_delivery(&asgard::PackOptions {
        input: report.output_dir.clone(),
        output: Some(demo.join("pack-out")),
        target: asgard::PackTarget::Ipa,
        project_name: None,
        wasm_name: None,
    })
    .expect("ipa pack");
    assert_eq!(pack.artifact_path.extension().and_then(|e| e.to_str()), Some("ipa"));
}
