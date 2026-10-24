//! IPA 组装。

use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use miette::{IntoDiagnostic, Result, WrapErr};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use super::util::{copy_dir_recursive, parse_plist_bundle_name, read_optional_file};
use crate::{
    codegen::{find_asgard_native_section, find_asgard_ui_section},
    delivery::PackReport,
};

/// 从 VOA ios dist 组装 IPA。
pub fn pack_ipa(input: &Path, output: Option<&Path>) -> Result<PackReport> {
    let ios_src = resolve_ios_dist(input)?;
    let out = output.map(PathBuf::from).unwrap_or_else(|| input.join("ipa-out"));
    fs::create_dir_all(&out).into_diagnostic().wrap_err("create output dir")?;
    copy_dir_recursive(&ios_src, &out)?;

    let plist = read_optional_file(&out.join("Info.plist"))?;
    let host = read_optional_file(&out.join("AsgardHost"))?.or(read_optional_file(&out.join("host.bin"))?);
    let legacy_ui = read_optional_file(&out.join("ui.bin"))?;

    if legacy_ui.is_some() {
        eprintln!("asgard pack: 警告 — 独立 ui.bin 已废弃；RenderIR 应编入 AsgardHost");
    }

    if plist.is_none() || host.is_none() {
        return Ok(PackReport {
            artifact_path: out.clone(),
            message: format!("已复制二进制 dist 到 {}；缺少 Info.plist 或可执行文件，无法组装 IPA", out.display()),
        });
    }

    validate_ios_dist(host.as_ref().unwrap())?;

    let app_name = parse_plist_bundle_name(plist.as_ref().unwrap()).unwrap_or_else(|| "AsgardApp".into());
    let payload_root = format!("Payload/{app_name}.app");
    let ipa_path = out.join(format!("{app_name}.ipa"));
    let file = File::create(&ipa_path).into_diagnostic()?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    zip.start_file(format!("{payload_root}/Info.plist"), options.clone()).into_diagnostic()?;
    zip.write_all(plist.as_ref().unwrap()).into_diagnostic()?;
    zip.start_file(format!("{payload_root}/AsgardHost"), options.clone()).into_diagnostic()?;
    zip.write_all(host.as_ref().unwrap()).into_diagnostic()?;
    if let Some(ui_bytes) = legacy_ui {
        zip.start_file(format!("{payload_root}/ui.bin"), options).into_diagnostic()?;
        zip.write_all(&ui_bytes).into_diagnostic()?;
    }
    zip.finish().into_diagnostic()?;
    Ok(PackReport { artifact_path: ipa_path.clone(), message: format!("IPA {}", ipa_path.display()) })
}

fn resolve_ios_dist(input: &Path) -> Result<PathBuf> {
    for candidate in [input.join("ios"), input.join("dist/ios"), input.to_path_buf()] {
        if candidate.join("AsgardHost").exists() || candidate.join("host.bin").exists() || candidate.join("Info.plist").exists() {
            return Ok(candidate);
        }
    }
    Err(miette::miette!("未找到 iOS 二进制 dist（需 AsgardHost / Info.plist）。请先 `asgard build`（platform: ios）"))
}

fn validate_ios_dist(host: &[u8]) -> Result<()> {
    if find_asgard_native_section(host).is_none() {
        return Err(miette::miette!("AsgardHost 缺少 ASGARDNT 段（native AOT 须编入 Mach-O 尾段）"));
    }
    if find_asgard_ui_section(host).is_none() {
        return Err(miette::miette!("AsgardHost 缺少 ASGARDUI 段（asgard ui wire 须编入尾段）"));
    }
    Ok(())
}
