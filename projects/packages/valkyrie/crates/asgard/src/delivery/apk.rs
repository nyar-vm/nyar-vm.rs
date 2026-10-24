//! APK 组装。

use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use miette::{IntoDiagnostic, Result, WrapErr};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use super::util::{copy_dir_recursive, read_optional_file};
use crate::{delivery::PackReport, platform_contract::validate_android_dist_bytes};

/// 从 VOA android dist 组装 APK。
pub fn pack_apk(input: &Path, output: Option<&Path>) -> Result<PackReport> {
    let android_src = resolve_android_dist(input)?;
    let out = output.map(PathBuf::from).unwrap_or_else(|| input.join("apk-out"));
    fs::create_dir_all(&out).into_diagnostic().wrap_err("create output dir")?;
    copy_dir_recursive(&android_src, &out)?;

    let manifest = read_optional_file(&out.join("AndroidManifest.xml"))?;
    let dex = read_optional_file(&out.join("classes.dex"))?;
    let legacy_ui = read_optional_file(&out.join("ui.bin"))?;

    if legacy_ui.is_some() {
        eprintln!("asgard pack: 警告 — 独立 ui.bin 已废弃；元数据应编入 classes.dex 尾段");
    }

    if manifest.is_none() || dex.is_none() {
        return Ok(PackReport {
            artifact_path: out.clone(),
            message: format!("已复制二进制 dist 到 {}；缺少 AndroidManifest.xml 或 classes.dex，无法组装 APK", out.display()),
        });
    }

    validate_android_dist(dex.as_ref().unwrap())?;

    let apk_path = out.join("app-debug.apk");
    let file = File::create(&apk_path).into_diagnostic()?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

    zip.start_file("AndroidManifest.xml", options).into_diagnostic()?;
    zip.write_all(manifest.as_ref().unwrap()).into_diagnostic()?;
    zip.start_file("classes.dex", options).into_diagnostic()?;
    zip.write_all(dex.as_ref().unwrap()).into_diagnostic()?;
    zip.start_file("resources.arsc", options).into_diagnostic()?;
    zip.write_all(MINIMAL_RESOURCES_ARSC).into_diagnostic()?;
    zip.start_file("assets/asgard/classes.dex", options).into_diagnostic()?;
    zip.write_all(dex.as_ref().unwrap()).into_diagnostic()?;
    write_debug_apk_signature(&mut zip, options)?;
    zip.finish().into_diagnostic()?;
    Ok(PackReport { artifact_path: apk_path.clone(), message: format!("APK {}", apk_path.display()) })
}

fn resolve_android_dist(input: &Path) -> Result<PathBuf> {
    for candidate in [input.join("android"), input.join("dist/android"), input.to_path_buf()] {
        if candidate.join("classes.dex").exists() {
            return Ok(candidate);
        }
    }
    Err(miette::miette!("未找到 Android 二进制 dist（需 classes.dex）。请先 `asgard build`（platform: android）"))
}

fn validate_android_dist(dex: &[u8]) -> Result<()> {
    validate_android_dist_bytes(dex)
}

/// 最小 `resources.arsc`（空 resource 表）。
const MINIMAL_RESOURCES_ARSC: &[u8] = &[
    0x02, 0x00, 0x0c, 0x00, 0x04, 0x00, 0x00, 0x00, 0x01, 0x00, 0x1c, 0x00, 0x7c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
];

fn write_debug_apk_signature(zip: &mut ZipWriter<File>, options: SimpleFileOptions) -> Result<()> {
    const MANIFEST: &str = "Manifest-Version: 1.0\nCreated-By: asgard pack\n\n";
    zip.start_file("META-INF/MANIFEST.MF", options).into_diagnostic()?;
    zip.write_all(MANIFEST.as_bytes()).into_diagnostic()?;
    zip.start_file("META-INF/CERT.SF", options).into_diagnostic()?;
    zip.write_all(b"Signature-Version: 1.0\nCreated-By: asgard pack\n\n").into_diagnostic()?;
    zip.start_file("META-INF/CERT.RSA", options).into_diagnostic()?;
    zip.write_all(b"\x00").into_diagnostic()?;
    Ok(())
}
