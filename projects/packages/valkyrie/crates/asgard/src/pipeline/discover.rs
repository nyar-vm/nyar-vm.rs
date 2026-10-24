//! 项目源码发现。

use std::path::{Path, PathBuf};

use miette::{IntoDiagnostic, Result, WrapErr};

/// 发现的 AWSL 组件文件。
#[derive(Debug, Clone)]
pub struct DiscoveredAwslFile {
    /// 绝对路径。
    pub path: PathBuf,
    /// 路由/组件名（来自文件名）。
    pub component_name: String,
    /// 相对 source 目录的路径。
    pub relative_path: String,
}

/// 发现的 AWS（Tailwind）样式入口。
#[derive(Debug, Clone)]
pub struct DiscoveredAwsFile {
    /// 绝对路径。
    pub path: PathBuf,
    /// 相对 source 目录的路径。
    pub relative_path: String,
}

/// 项目源码扫描结果。
#[derive(Debug, Clone)]
pub struct DiscoveredSources {
    /// AWSL 文件。
    pub awsl_files: Vec<DiscoveredAwslFile>,
    /// AWS Tailwind 入口文件。
    pub aws_files: Vec<DiscoveredAwsFile>,
}

/// 扫描项目 `source/` 目录。
pub fn discover_sources(project_dir: &Path) -> Result<DiscoveredSources> {
    let source_dir = project_dir.join("source");
    if !source_dir.exists() {
        return Err(miette::miette!("未找到 source/ 目录: {}", source_dir.display()));
    }

    let mut awsl_files = Vec::new();
    let mut aws_files = Vec::new();
    collect_files(&source_dir, &source_dir, &mut awsl_files, &mut aws_files)?;
    awsl_files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    aws_files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(DiscoveredSources { awsl_files, aws_files })
}

fn collect_files(root: &Path, current: &Path, awsl_files: &mut Vec<DiscoveredAwslFile>, aws_files: &mut Vec<DiscoveredAwsFile>) -> Result<()> {
    for entry in std::fs::read_dir(current).into_diagnostic().wrap_err_with(|| format!("读取目录失败: {}", current.display()))? {
        let entry = entry.into_diagnostic().wrap_err("读取目录项失败")?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, awsl_files, aws_files)?;
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str())
        else {
            continue;
        };
        match ext {
            "awsl" => {
                let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
                let component_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("component").to_string();
                awsl_files.push(DiscoveredAwslFile { path, component_name, relative_path: relative });
            }
            "aws" => {
                let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
                aws_files.push(DiscoveredAwsFile { path, relative_path: relative });
            }
            _ => {}
        }
    }
    Ok(())
}
