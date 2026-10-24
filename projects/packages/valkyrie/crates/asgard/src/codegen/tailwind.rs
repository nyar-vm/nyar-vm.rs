//! 可选 Tailwind CLI 构建与 CSS 合并。

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use miette::{IntoDiagnostic, Result, WrapErr};

use crate::{config::TailwindConfig, tailwind::StyleCollector};

/// Tailwind 构建结果。
#[derive(Debug, Clone)]
pub struct TailwindBuildOutput {
    /// 生成的 CSS 文本。
    pub css: String,
}

/// 若 `config.enabled`，调用 Tailwind CLI 构建 utility CSS；失败时仅 warn 并返回 `None`。
pub fn maybe_build_tailwind_css(
    project_dir: &Path,
    output_dir: &Path,
    config: &TailwindConfig,
    collector: &StyleCollector,
    aws_entries: &[PathBuf],
) -> Option<TailwindBuildOutput> {
    if !config.enabled {
        return None;
    }
    let entry =
        config.entry.as_deref().map(|path| project_dir.join(path)).filter(|path| path.exists()).or_else(|| aws_entries.first().cloned());
    let Some(entry_path) = entry
    else {
        eprintln!("asgard: tailwind.enabled 但未找到 entry .aws 文件，跳过 Tailwind 构建");
        return None;
    };

    match build_tailwind_css(project_dir, output_dir, config, collector, &entry_path) {
        Ok(output) => Some(output),
        Err(error) => {
            eprintln!("asgard: Tailwind CLI 构建失败（已保留 content manifest）: {error}");
            None
        }
    }
}

fn build_tailwind_css(
    project_dir: &Path,
    output_dir: &Path,
    config: &TailwindConfig,
    collector: &StyleCollector,
    entry_path: &Path,
) -> Result<TailwindBuildOutput> {
    let work_dir = output_dir.join(".asgard").join("tailwind-build");
    fs::create_dir_all(&work_dir).into_diagnostic().wrap_err("创建 tailwind-build 目录失败")?;

    let content_path = work_dir.join("content.txt");
    let content = collector.utilities().collect::<Vec<_>>().join("\n");
    fs::write(&content_path, content).into_diagnostic().wrap_err("写入 Tailwind content 失败")?;

    let out_css = work_dir.join("tailwind.out.css");
    let mut command = Command::new(if cfg!(windows) { "npx.cmd" } else { "npx" });
    command.arg("tailwindcss");
    command.arg("-i").arg(entry_path);
    command.arg("-o").arg(&out_css);
    if let Some(config_path) = config.config_path.as_deref() {
        let resolved = project_dir.join(config_path);
        if resolved.exists() {
            command.arg("-c").arg(resolved);
        }
    }
    command.arg("--content").arg(&content_path);
    command.current_dir(project_dir);

    let status = command.status().into_diagnostic().wrap_err("执行 tailwindcss 失败")?;
    if !status.success() {
        return Err(miette::miette!("tailwindcss 退出码非零"));
    }
    let css = fs::read_to_string(&out_css).into_diagnostic().wrap_err("读取 Tailwind 输出 CSS 失败")?;
    Ok(TailwindBuildOutput { css })
}

/// 将 Tailwind 产出追加到合并 CSS 文本末尾。
pub fn append_tailwind_css(merged_css: &mut String, tailwind_css: &str) {
    if tailwind_css.trim().is_empty() {
        return;
    }
    if !merged_css.is_empty() && !merged_css.ends_with('\n') {
        merged_css.push('\n');
    }
    merged_css.push_str("/* tailwind */\n");
    merged_css.push_str(tailwind_css);
    if !merged_css.ends_with('\n') {
        merged_css.push('\n');
    }
}
