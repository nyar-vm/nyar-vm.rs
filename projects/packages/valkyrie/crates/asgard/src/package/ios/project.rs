//! iOS 二进制交付打包（Mach-O 编入 RenderIR；无 Swift/Xcode 源码工程）。

use crate::{
    awsl::LoweredComponent,
    codegen::{
        embed_asgard_ui_section, embed_host_native_section, encode_mobile_ui_package, generate_ios_bridge, generate_ios_swiftui_runtime,
    },
    package::artifact::PackageArtifact,
};
use acorn_mach_o::mach_o::MachOImageBuilder;

/// iOS 打包产物。
#[derive(Debug, Clone)]
pub struct IosPackageOutput {
    /// Bundle identifier。
    pub bundle_id: String,
    /// 应用显示名。
    pub app_name: String,
    /// 相对路径 → 产物。
    pub files: Vec<(String, PackageArtifact)>,
}

/// 生成 iOS 二进制 dist（无 `.swift`；RenderIR 编入 `AsgardHost` Mach-O）。
pub fn package_ios_project(components: &[LoweredComponent], module_name: &str, native_logic: &[u8]) -> IosPackageOutput {
    let app_name = swift_app_name(module_name);
    let bundle_id = bundle_identifier(module_name);
    let ui = encode_mobile_ui_package(components);
    let mut executable = MachOImageBuilder::new().build_executable().expect("Mach-O 构建失败");
    embed_host_native_section(&mut executable, native_logic);
    embed_asgard_ui_section(&mut executable, &ui);
    let ios_bridge = generate_ios_bridge();
    let files = vec![
        ("Info.plist".into(), PackageArtifact::Text(info_plist(&bundle_id, module_name))),
        ("AsgardHost".into(), PackageArtifact::Bytes(executable)),
        ("AsgardSwiftUiRuntime.swift".into(), PackageArtifact::Text(generate_ios_swiftui_runtime())),
        ("native/asgard_ios_bridge.c".into(), PackageArtifact::Text(ios_bridge)),
    ];
    IosPackageOutput { bundle_id, app_name, files }
}

fn swift_app_name(module_name: &str) -> String {
    let mut out = String::new();
    let mut upper_next = true;
    for ch in module_name.chars() {
        if ch.is_ascii_alphanumeric() {
            if upper_next {
                out.push(ch.to_ascii_uppercase());
                upper_next = false;
            }
            else {
                out.push(ch);
            }
        }
        else {
            upper_next = true;
        }
    }
    if out.is_empty() { "AsgardApp".into() } else { out }
}

fn bundle_identifier(module_name: &str) -> String {
    let mut parts = Vec::new();
    for part in module_name.split(|c: char| !c.is_ascii_alphanumeric()) {
        if part.is_empty() {
            continue;
        }
        let mut s = part.to_ascii_lowercase();
        if s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            s = format!("n{s}");
        }
        parts.push(s);
    }
    if parts.is_empty() { "com.asgard.app".into() } else { format!("com.asgard.{}", parts.join("_")) }
}

fn info_plist(bundle_id: &str, display_name: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDisplayName</key>
    <string>{display_name}</string>
    <key>CFBundleExecutable</key>
    <string>AsgardHost</string>
    <key>CFBundleIdentifier</key>
    <string>{bundle_id}</string>
    <key>CFBundleName</key>
    <string>{display_name}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
</dict>
</plist>
"#
    )
}
