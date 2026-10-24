//! 终端 TUI 打包：纯 V 源码（app.v）+ 原生可执行文件。
//!
//! 终端路径是完全纯 Valkyrie 实现：所有 TUI 业务逻辑（widget 树构建、绑定存储、
//! 焦点导航、渲染调度、事件循环）由 V 源码承担，宿主只提供字符级 I/O 原语
//!（`host_contract`）。本模块把已生成的 `app.v`（含 package graph 提供的 tui.v 运行时）
//! 与原生可执行文件骨架一起打包（V→native 编译器补全后接入）。

use crate::package::artifact::PackageArtifact;
use acorn_host::host_exe::build_host_executable;

/// 终端打包产物。
#[derive(Debug, Clone)]
pub struct TerminalPackageOutput {
    /// 平台标识（固定为 "terminal"）。
    pub platform: String,
    /// 相对路径 → 产物。
    pub files: Vec<(String, PackageArtifact)>,
}

/// 生成终端 TUI dist（纯 V 源码 app.v + 原生可执行文件）。
///
/// 与桌面打包不同，终端打包不调用 `embed_host_native_section` / `embed_asgard_ui_section`：
/// UI 逻辑全部在 `app.v`（V 源码）中，native_logic 通过 `build_host_executable` 的
/// rodata_tail 参数编入可执行文件（V→native 编译器补全后生效）。
pub fn package_terminal_project(app_v: String, module_name: &str, native_logic: &[u8]) -> TerminalPackageOutput {
    let host_platform = std::env::consts::OS;
    let exe_name = if host_platform == "windows" { format!("{module_name}.exe") } else { module_name.to_string() };
    let executable = build_host_executable(host_platform, None, native_logic).expect("终端宿主可执行构建失败");
    let manifest = format!("# Asgard terminal manifest\nplatform: terminal\nname: {module_name}\nexecutable: {exe_name}\nsource: app.v\n");
    let files = vec![
        ("manifest.toml".into(), PackageArtifact::Text(manifest)),
        (exe_name, PackageArtifact::Bytes(executable)),
        ("app.v".into(), PackageArtifact::Text(app_v)),
    ];
    TerminalPackageOutput { platform: "terminal".into(), files }
}
#[cfg(test)]
#[path = "../../../tests/path_wired/package_terminal_mod.rs"]
mod package_terminal_mod_tests;
