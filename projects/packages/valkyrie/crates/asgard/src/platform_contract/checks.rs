//! 平台契约检查实现。

use std::path::Path;

use acorn_dex::dex::{dex_class_defs_count, dex_contains_strings};
use miette::{Result, miette};

use crate::{
    codegen::{
        HOST_NATIVE_MAGIC, UI_BIN_MAGIC, find_asgard_native_section, find_asgard_ui_section, generate_android_compose_runtime,
        generate_desktop_native_runtime, generate_ios_swiftui_runtime, generate_mp_runtime, resolve_call_export,
    },
    compile::validate_elf_shared_object,
    host::HostPlatform,
};

use super::gates::GateId;

/// 单条门禁失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateFailure {
    /// 门禁 ID。
    pub gate: GateId,
    /// 失败原因。
    pub message: String,
}

/// 校验 Android `classes.dex` dist 制品。
pub fn validate_android_dist_bytes(dex: &[u8]) -> Result<()> {
    fail_if_any(check_android_dist(dex))
}

/// 校验 iOS `AsgardHost` dist 制品。
pub fn validate_ios_dist_bytes(host: &[u8]) -> Result<()> {
    fail_if_any(check_ios_dist(host))
}

/// 校验 Android dist 目录布局。
pub fn validate_android_dist_dir(dist_dir: &Path) -> Result<()> {
    let mut failures = Vec::new();
    let native = dist_dir.join("native");
    if native.exists() {
        failures.push(GateFailure { gate: GateId::NoAndroidC, message: format!("禁止 android/native/ 目录: {}", native.display()) });
    }
    for entry in walk_files(&native) {
        if entry.extension().and_then(|e| e.to_str()) == Some("c") {
            failures.push(GateFailure { gate: GateId::NoAndroidC, message: format!("禁止 android/native/*.c: {}", entry.display()) });
        }
    }
    let dex_path = dist_dir.join("classes.dex");
    if dex_path.exists() {
        let dex = std::fs::read(&dex_path).map_err(|e| miette!("{e}"))?;
        failures.extend(check_android_dist(&dex));
    }
    if failures.is_empty() { Ok(()) } else { Err(miette!(format_gate_failures(&failures))) }
}

/// 返回 Android dex 门禁失败列表（测试 / 报告用）。
pub fn check_android_dist(dex: &[u8]) -> Vec<GateFailure> {
    let mut failures = Vec::new();
    if find_asgard_ui_section(dex).is_none() {
        failures.push(GateFailure { gate: GateId::MagUi, message: "缺少 ASGARDUI 尾段".into() });
    }
    if find_asgard_native_section(dex).is_none() {
        failures.push(GateFailure { gate: GateId::MagNt, message: "缺少 ASGARDNT 尾段".into() });
    }
    if dex.starts_with(b"ASGDHOST") {
        failures.push(GateFailure { gate: GateId::NoLegacy, message: "占位 ASGDHOST magic".into() });
    }
    let lossy = String::from_utf8_lossy(dex);
    if lossy.contains("asgard/Placeholder") {
        failures.push(GateFailure { gate: GateId::NoLegacy, message: "含 asgard/Placeholder".into() });
    }
    failures.extend(check_android_compose_runtime(dex));
    failures.extend(check_android_runtime_abi_in_dex(dex));
    failures
}

/// 校验 iOS dist 产物是否带 ASGARDUI 与 ASGARDNT 段。
pub fn check_ios_dist(host: &[u8]) -> Vec<GateFailure> {
    let mut failures = Vec::new();
    if find_asgard_ui_section(host).is_none() {
        failures.push(GateFailure { gate: GateId::MagUi, message: "缺少 ASGARDUI".into() });
    }
    if find_asgard_native_section(host).is_none() {
        failures.push(GateFailure { gate: GateId::MagNt, message: "缺少 ASGARDNT".into() });
    }
    failures
}

/// 校验 Android `.so` 编译制品（G-JNI-SYM）。
pub fn validate_android_native_so(so: &[u8]) -> Result<()> {
    validate_elf_shared_object(so)
}

/// 校验生成源 runtime 模板（CIP）。
pub fn validate_runtime_source_cip(platform: HostPlatform) -> Vec<GateFailure> {
    let mut failures = Vec::new();
    match platform {
        HostPlatform::Android => {
            let src = generate_android_compose_runtime();
            failures.extend(check_runtime_abi_source(&src, "fun mount(", GateId::AbiMount));
            failures.extend(check_runtime_abi_source(&src, "fun patch(", GateId::AbiPatch));
            failures.extend(check_runtime_abi_source(&src, "fun on_event(", GateId::AbiEvent));
            failures.extend(check_runtime_abi_source(&src, "patchFromNative", GateId::AbiPatch));
            if !src.contains("invokeExport") {
                failures.push(GateFailure { gate: GateId::AbiEvent, message: "Kotlin 模板缺少 invokeExport".into() });
            }
        }
        HostPlatform::Ios => {
            let src = generate_ios_swiftui_runtime();
            failures.extend(check_runtime_abi_source(&src, "mount", GateId::AbiMount));
            failures.extend(check_runtime_abi_source(&src, "patch", GateId::AbiPatch));
            failures.extend(check_runtime_abi_source(&src, "on_event", GateId::AbiEvent));
        }
        HostPlatform::Windows | HostPlatform::Linux | HostPlatform::MacOs => {
            let plat = match platform {
                HostPlatform::Windows => "windows",
                HostPlatform::Linux => "linux",
                _ => "macos",
            };
            let src = generate_desktop_native_runtime(plat);
            failures.extend(check_runtime_abi_source(&src, "mount", GateId::AbiMount));
            failures.extend(check_runtime_abi_source(&src, "patch", GateId::AbiPatch));
            failures.extend(check_runtime_abi_source(&src, "on_event", GateId::AbiEvent));
        }
        HostPlatform::WechatMiniProgram => {
            let src = generate_mp_runtime("asgard-app");
            failures.extend(check_runtime_abi_source(&src, "on_event", GateId::AbiEvent));
        }
        _ => {}
    }
    if resolve_call_export("on_tap") != "awsl_call_on_tap" {
        failures.push(GateFailure { gate: GateId::AbiExport, message: "resolve_call_export 命名错误".into() });
    }
    failures
}

fn check_android_compose_runtime(dex: &[u8]) -> Vec<GateFailure> {
    let mut failures = check_android_compose_bootstrap(dex);
    if is_vendor_compose_dex(dex) {
        failures.extend(check_android_compose_vendor(dex));
    }
    failures
}

/// Bootstrap 层：符号 / 描述符契约（Rust stub 可通过）。
pub fn check_android_compose_bootstrap(dex: &[u8]) -> Vec<GateFailure> {
    let mut failures = Vec::new();
    let required_descriptors = [
        "Lcom/asgard/runtime/AsgardComposeRuntime;",
        "Lcom/asgard/runtime/AsgardHostBridge;",
        "Landroidx/activity/ComponentActivity;",
        "Landroidx/compose/",
    ];
    let required_methods = ["decodeAsgardUi", "AsgardRoot", "mount", "patch", "on_event", "invokeExport", "patchFromNative"];

    match dex_contains_strings(dex, &required_descriptors) {
        Ok(true) => {}
        Ok(false) => failures
            .push(GateFailure {
                gate: GateId::AndroidCompose, message: format!("dex 缺少 Compose class 描述符: {required_descriptors:?}")
            }),
        Err(e) => failures.push(GateFailure { gate: GateId::AndroidCompose, message: e.to_string() }),
    }
    match dex_contains_strings(dex, &required_methods) {
        Ok(true) => {}
        Ok(false) => failures
            .push(GateFailure { gate: GateId::AndroidCompose, message: format!("dex 缺少 Compose 运行时方法: {required_methods:?}") }),
        Err(e) => failures.push(GateFailure { gate: GateId::AndroidCompose, message: e.to_string() }),
    }
    failures
}

/// Vendor 层：SDK 编译 dex 须满足的更强契约。
pub fn check_android_compose_vendor(dex: &[u8]) -> Vec<GateFailure> {
    let mut failures = Vec::new();
    const MIN_CLASS_DEFS: usize = 50;

    match dex_class_defs_count(dex) {
        Ok(count) if count > MIN_CLASS_DEFS => {}
        Ok(count) => failures
            .push(GateFailure {
                gate: GateId::AndroidCompose, message: format!("vendor dex class_defs 过少（{count} <= {MIN_CLASS_DEFS}）")
            }),
        Err(e) => failures.push(GateFailure { gate: GateId::AndroidCompose, message: e.to_string() }),
    }

    for needle in ["setContent", "Composable"] {
        match dex_contains_strings(dex, &[needle]) {
            Ok(true) => {}
            Ok(false) => failures.push(GateFailure { gate: GateId::AndroidCompose, message: format!("vendor dex 缺少 `{needle}`") }),
            Err(e) => failures.push(GateFailure { gate: GateId::AndroidCompose, message: e.to_string() }),
        }
    }

    if String::from_utf8_lossy(dex).contains("stub()V") {
        failures.push(GateFailure { gate: GateId::AndroidCompose, message: "vendor dex 含 bootstrap stub()V".into() });
    }
    failures
}

/// dex 是否视为 SDK vendor Compose（触发 vendor 层门禁）。
pub fn is_vendor_compose_dex(dex: &[u8]) -> bool {
    dex_class_defs_count(dex).map(|c| c > 50).unwrap_or(false)
}

fn check_android_runtime_abi_in_dex(dex: &[u8]) -> Vec<GateFailure> {
    let mut failures = Vec::new();
    for (needle, gate) in [
        ("mount", GateId::AbiMount),
        ("patch", GateId::AbiPatch),
        ("on_event", GateId::AbiEvent),
        ("patchFromNative", GateId::AbiPatch),
        ("invokeExport", GateId::AbiEvent),
    ] {
        if !String::from_utf8_lossy(dex).contains(needle) {
            match dex_contains_strings(dex, &[needle]) {
                Ok(false) => failures.push(GateFailure { gate, message: format!("dex 缺少 `{needle}`") }),
                Err(e) => failures.push(GateFailure { gate, message: e.to_string() }),
                Ok(true) => {}
            }
        }
    }
    failures
}

fn check_runtime_abi_source(src: &str, needle: &str, gate: GateId) -> Vec<GateFailure> {
    if src.contains(needle) { Vec::new() } else { vec![GateFailure { gate, message: format!("生成源缺少 `{needle}`") }] }
}

/// 将门禁失败列表格式化为稳定 `G-*` 文本（测试 / 报告用）。
pub fn format_gate_failures(failures: &[GateFailure]) -> String {
    failures.iter().map(|f| format!("{}: {}", f.gate.as_str(), f.message)).collect::<Vec<_>>().join("; ")
}

fn fail_if_any(failures: Vec<GateFailure>) -> Result<()> {
    if failures.is_empty() { Ok(()) } else { Err(miette!(format_gate_failures(&failures))) }
}

fn walk_files(dir: &Path) -> Vec<std::path::PathBuf> {
    if !dir.exists() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&current) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                }
                else {
                    out.push(path);
                }
            }
        }
    }
    out
}

/// 校验 APK 内嵌 `classes.dex`（pack 后）。
pub fn validate_android_apk_dex(apk_dex: &[u8]) -> Result<()> {
    validate_android_dist_bytes(apk_dex)
}

/// 校验 `dist/terminal/` 目录符合终端交付契约（纯 V 路径：app.v + 可执行文件）。
pub fn validate_terminal_dist_dir(dir: &Path) -> Result<()> {
    let manifest = std::fs::read_to_string(dir.join("manifest.toml")).map_err(|_| miette!("缺少 manifest.toml"))?;
    if !manifest.contains("platform: terminal") {
        return Err(miette!("manifest.toml 的 platform 字段必须为 terminal"));
    }
    let has_exe = std::fs::read_dir(dir).map_err(|_| miette!("无法读取 dist/terminal 目录"))?.filter_map(|e| e.ok()).any(|e| {
        let name = e.file_name();
        let name = name.to_string_lossy();
        let known_non_exe = name == "manifest.toml" || name == "app.v" || name == "debug";
        !known_non_exe && (name.ends_with(".exe") || !name.contains('.'))
    });
    if !has_exe {
        return Err(miette!("dist/terminal 缺少可执行文件"));
    }
    let app_v = dir.join("app.v");
    if !app_v.exists() {
        return Err(miette!("缺少 app.v（V 源码产物）"));
    }
    for forbidden in ["index.html", "classes.dex", "AsgardHost", "AsgardTerminalRuntime.c"] {
        if dir.join(forbidden).exists() {
            return Err(miette!("dist/terminal 不得包含 `{forbidden}`"));
        }
    }
    Ok(())
}

/// 禁用过时 magic 字符串。
pub fn check_no_legacy_magic(bytes: &[u8]) -> Vec<GateFailure> {
    let mut failures = Vec::new();
    let text = String::from_utf8_lossy(bytes);
    for bad in ["ASGDHOST", "ASGDUI", "ASGDNAT"] {
        if text.contains(bad) {
            failures.push(GateFailure { gate: GateId::NoLegacy, message: format!("含过时 magic `{bad}`") });
        }
    }
    failures
}

/// UI / native magic 常量（测试对照）。
pub fn expected_magics() -> (&'static [u8], &'static [u8]) {
    (UI_BIN_MAGIC, HOST_NATIVE_MAGIC)
}
#[cfg(test)]
#[path = "../../tests/path_wired/platform_contract_checks.rs"]
mod platform_contract_checks_tests;
