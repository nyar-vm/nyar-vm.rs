//! `legion doc` — 用户文档静态站点生成（Asgard static renderer）。

mod assets;
mod discover;
mod markdown;
mod nav;
mod render;

use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::Args;
use miette::{IntoDiagnostic, Result, WrapErr};
use serde_json::json;

use crate::cmds::project_input::resolve_project_path;
use legion_workspace::{planner::WorkspaceResolver, script};

pub use discover::discover_sections;

/// `legion doc` 命令参数。
#[derive(Debug, Clone, Args)]
pub struct DocArgs {
    /// 项目或 workspace 目录。
    #[arg(value_name = "project-dir", default_value = ".")]
    pub project_dir: PathBuf,
    /// 输出目录（默认 `{project}/dist/legion-document`）。
    #[arg(short = 'o', long = "output")]
    pub output_dir: Option<PathBuf>,
    /// 按 workspace 成员聚合文档。
    #[arg(long, default_value_t = false)]
    pub workspace: bool,
    /// 详细输出。
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,
}

/// 执行 `legion doc`。
pub fn run(args: &DocArgs) -> Result<ExitCode> {
    let output_dir = if args.workspace {
        let workspace = WorkspaceResolver::discover(&args.project_dir)?;
        let output_dir = args.output_dir.clone().unwrap_or_else(|| workspace.root_dir.join("dist/legion-document"));
        run_workspace_doc(&workspace, &output_dir, args.verbose)?;
        output_dir
    }
    else {
        let project_input = resolve_project_path(&args.project_dir)?;
        let scan_root = if script::is_script_path(&project_input) {
            project_input.parent().map(Path::to_path_buf).unwrap_or_else(|| project_input.clone())
        }
        else {
            project_input.clone()
        };
        let output_dir = args.output_dir.clone().unwrap_or_else(|| scan_root.join("dist/legion-document"));
        run_project_doc(&scan_root, &output_dir, args.verbose)?;
        output_dir
    };

    println!("文档已生成到 {}", output_dir.display());
    Ok(ExitCode::SUCCESS)
}

fn run_workspace_doc(workspace: &WorkspaceResolver, output_dir: &Path, verbose: bool) -> Result<()> {
    let members = workspace.member_manifest_dirs();
    if members.is_empty() {
        return Err(miette::miette!("workspace 中无成员项目"));
    }

    if verbose {
        println!("发现 Workspace，共 {} 个成员项目", members.len());
    }

    let mut all_sections = discover::discover_sections(&workspace.root_dir);
    if verbose && !all_sections.is_empty() {
        println!("  扫描 workspace 根目录文档（{} 个分区）...", all_sections.len());
    }

    for member_dir in members {
        let name = member_dir.file_name().and_then(|s| s.to_str()).unwrap_or("member");
        if verbose {
            println!("  扫描成员 {name}...");
        }
        let mut sections = discover::discover_sections(&member_dir);
        for section in &mut sections {
            section.name = format!("{name} / {}", section.name);
            section.sidebar_title = format!("{name}");
        }
        all_sections.extend(sections);
    }

    if all_sections.is_empty() {
        return Err(miette::miette!("未发现任何 documentation/pages 内容"));
    }

    emit_documentation(&mut all_sections, output_dir, verbose)
}

fn run_project_doc(project_dir: &Path, output_dir: &Path, verbose: bool) -> Result<()> {
    let mut sections = discover::discover_sections(project_dir);
    if sections.is_empty() {
        return Err(miette::miette!("未发现用户文档目录：{}", project_dir.join("documentation/pages").display()));
    }

    if verbose {
        println!("项目目录：{}", project_dir.display());
        println!("发现 {} 个文档分区", sections.len());
    }

    emit_documentation(&mut sections, output_dir, verbose)
}

fn emit_documentation(sections: &mut [nav::DocSection], output_dir: &Path, verbose: bool) -> Result<()> {
    fs::create_dir_all(output_dir).into_diagnostic()?;
    let doc_dir = output_dir.join("doc");

    if verbose {
        println!("正在渲染用户文档到 {}...", doc_dir.display());
    }

    let component_css = render::render_sections(sections, &doc_dir, verbose)?;
    assets::write_document_css(output_dir, assets::embedded_base_css(), &component_css)
        .into_diagnostic()
        .wrap_err("写出 legion-document.css 失败")?;

    let has_pages = sections.iter().any(|s| !s.pages.is_empty());
    if has_pages {
        render::render_doc_index(sections, &doc_dir)?;
        if verbose {
            let total: usize = sections.iter().map(|s| s.pages.len()).sum();
            println!("用户文档已渲染，共 {total} 个页面");
        }
    }

    render::render_root_hub(output_dir, has_pages)?;
    Ok(())
}

/// 供测试使用的文档生成入口。
pub fn generate_for_project(project_dir: &Path, output_dir: &Path) -> Result<()> {
    let mut sections = discover::discover_sections(project_dir);
    emit_documentation(&mut sections, output_dir, false)
}
#[cfg(test)]
#[path = "../../../tests/path_wired/cmds_doc_mod.rs"]
mod cmds_doc_mod_tests;
