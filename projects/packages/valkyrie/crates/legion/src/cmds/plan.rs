//! `legion plan`：只做 workspace 解析与 build plan，**不**进入 Compiler。

use std::{path::Path, process::ExitCode};

use clap::Args;
use miette::{Result, miette};
use rusty_valkyrie::CanonicalTarget;

use crate::{api::plan_project, cmds::project_input::resolve_project_path};

/// `legion plan` 参数。
#[derive(Debug, Clone, Args)]
pub struct PlanArgs {
    /// 项目目录。
    #[arg(value_name = "project-dir", default_value = ".")]
    pub project_dir: std::path::PathBuf,
    /// 目标平台。
    #[arg(long, default_value = "clr")]
    pub target: CanonicalTarget,
}

/// 执行 `legion plan`（安全：不调用 `compile_plan`）。
pub fn run(args: &PlanArgs) -> Result<ExitCode> {
    let project_input = resolve_project_path(&args.project_dir)?;
    let summary = plan_project(&project_input, args.target.clone())?;

    println!("workspace: {}", crate::cmds::path_for_cli_log(Path::new(&summary.workspace_root)));
    println!("project: {}", summary.project);
    println!("target: {}", summary.target);
    println!("sources: {}", summary.source_count);
    println!("semantic groups: {}", summary.semantic_group_count);
    println!("dependencies: {}", summary.dependency_count);

    for group in &summary.groups {
        println!("  group {}: {} file(s)", group.dependency_key, group.file_count);
    }

    println!("plan: ok (未进入 Compiler；完整 build 请用 `legion build`)");
    Ok(ExitCode::SUCCESS)
}
