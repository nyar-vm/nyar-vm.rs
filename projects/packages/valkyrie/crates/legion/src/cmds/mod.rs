#![doc = include_str!("readme.md")]

/// 审计依赖漏洞与许可证。
pub mod audit;
pub mod bench;
pub mod bootstrap;
/// 构建项目并落盘产物。
pub mod build;
/// 检查项目可编译性。
pub mod check;
/// 清理构建产物目录。
pub mod clean;
pub mod cov;
pub mod doc;
pub mod fmt;
/// 依赖安装/添加/移除/更新。
pub mod install;
/// 前端语义 lint（不生成后端产物）。
pub mod lint;
/// 注册表登录/登出/whoami。
pub mod login;
pub mod plan;
pub mod project_input;
/// 发布包到注册表。
pub mod publish;
/// 管理注册表源 endpoint。
pub mod registry;
pub mod report;
/// 运行项目产物。
pub mod run;
/// 搜索/查看注册表中的包。
pub mod search;
pub mod source_hygiene;
pub mod spy;
pub mod test;
pub mod test_engine;
/// 注册表认证管理。
pub mod vendor;

use std::path::Path;

/// CLI 日志用路径：尽量相对 cwd，或收敛到仓名相对片段，禁止原样打印本机绝对路径。
pub(crate) fn path_for_cli_log(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let stripped = raw.strip_prefix(r"\\?\").unwrap_or(raw.as_ref());
    let normalized = stripped.replace('\\', "/");
    if let Ok(cwd) = std::env::current_dir() {
        let cwd_norm = cwd.to_string_lossy().replace('\\', "/");
        let cwd_trim = cwd_norm.trim_end_matches('/');
        if let Some(rel) = normalized.strip_prefix(cwd_trim) {
            let rel = rel.trim_start_matches('/');
            if !rel.is_empty() {
                return rel.to_string();
            }
        }
    }
    for marker in ["valkyrie.rs/", "valkyrie.v/", "nyar-vm.rs/", "leetcode.v/", "project-euler.v/"] {
        if let Some(i) = normalized.find(marker) {
            return normalized[i..].to_string();
        }
    }
    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| normalized)
}
