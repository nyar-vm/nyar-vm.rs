//! 桌面原生 GUI 打包（Windows / Linux / macOS）— 一手 native runtime（WinUI / SwiftUI / Linux native）。

use crate::{
    awsl::LoweredComponent,
    codegen::{
        desktop_runtime_asset_name, embed_asgard_ui_section, embed_host_native_section, encode_mobile_ui_package,
        generate_desktop_linux_bridge, generate_desktop_native_runtime, generate_desktop_win_bridge,
    },
    package::artifact::PackageArtifact,
};
use acorn_host::host_exe::build_host_executable;

/// 桌面打包产物。
#[derive(Debug, Clone)]
pub struct DesktopPackageOutput {
    /// 平台标识。
    pub platform: String,
    /// 相对路径 → 产物。
    pub files: Vec<(String, PackageArtifact)>,
}

/// 生成桌面原生 GUI dist（RenderIR 编入宿主可执行；runtime 为 wire 解码资产）。
pub fn package_desktop_project(
    components: &[LoweredComponent],
    module_name: &str,
    platform: &str,
    native_logic: &[u8],
) -> DesktopPackageOutput {
    let ui = encode_mobile_ui_package(components);
    let mut executable = build_host_executable(platform, None, &[]).expect("桌面宿主可执行构建失败");
    embed_host_native_section(&mut executable, native_logic);
    embed_asgard_ui_section(&mut executable, &ui);
    let exe_name = match platform {
        "windows" => format!("{module_name}.exe"),
        _ => module_name.to_string(),
    };
    let manifest = format!("# Asgard desktop manifest\nplatform: {platform}\nname: {module_name}\nexecutable: {exe_name}\n");
    let runtime_name = desktop_runtime_asset_name(platform);
    let mut files = vec![
        ("manifest.toml".into(), PackageArtifact::Text(manifest)),
        (exe_name.clone(), PackageArtifact::Bytes(executable)),
        (runtime_name.into(), PackageArtifact::Text(generate_desktop_native_runtime(platform))),
    ];
    if platform == "linux" {
        files.push(("native/asgard_desktop_linux_bridge.c".into(), PackageArtifact::Text(generate_desktop_linux_bridge())));
    }
    else if platform == "windows" {
        files.push(("native/asgard_win_bridge.c".into(), PackageArtifact::Text(generate_desktop_win_bridge())));
    }
    DesktopPackageOutput { platform: platform.into(), files }
}
#[cfg(test)]
#[path = "../../../tests/path_wired/package_desktop_mod.rs"]
mod package_desktop_mod_tests;
