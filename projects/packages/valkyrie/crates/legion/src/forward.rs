//! 将未知子命令转发到 PATH 上的 `legion-{name}` 伴随工具。

use std::{
    ffi::OsString,
    path::PathBuf,
    process::{Command, ExitCode},
};

const BUILTIN_SUBCOMMANDS: &[&str] = &[
    "build",
    "check",
    "run",
    "spy",
    "bootstrap",
    "doc",
    "test",
    "cov",
    "coverage",
    "bench",
    "benchmark",
    "fmt",
    "format",
    "publish",
    "login",
    "logout",
    "whoami",
    "install",
    "add",
    "remove",
    "update",
    "search",
    "info",
    "vendor",
    "audit",
    "registry",
    "help",
    "version",
];

/// 若 `argv[1]` 不是内建子命令且存在 `legion-{argv[1]}`，则转发并返回退出码。
pub fn try_forward_companion(argv: &[OsString]) -> Option<ExitCode> {
    if argv.len() < 2 {
        return None;
    }

    let subcommand = argv[1].to_string_lossy();
    if BUILTIN_SUBCOMMANDS.iter().any(|name| *name == subcommand.as_ref()) {
        return None;
    }

    let companion_name = format!("legion-{subcommand}");
    let companion_path = resolve_companion_executable(&companion_name)?;

    let status = Command::new(&companion_path).args(&argv[2..]).status().ok()?;
    let code = status.code().unwrap_or(1);
    Some(ExitCode::from(code as u8))
}

fn resolve_companion_executable(name: &str) -> Option<PathBuf> {
    let with_exe = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };

    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(&with_exe);
            if candidate.is_file() {
                return Some(candidate);
            }
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    let current_exe = std::env::current_exe().ok()?;
    let sibling = current_exe.parent()?.join(&with_exe);
    if sibling.is_file() {
        return Some(sibling);
    }

    None
}

/// 是否为 `legion` 内建子命令（非内建命令会尝试转发到 `legion-{name}`）。
pub fn is_builtin_subcommand(name: &str) -> bool {
    BUILTIN_SUBCOMMANDS.iter().any(|item| *item == name)
}
#[cfg(test)]
#[path = "../tests/path_wired/forward.rs"]
mod forward_tests;
