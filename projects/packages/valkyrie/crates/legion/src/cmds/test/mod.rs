//! `legion test` — 运行项目测试并生成 HTML 报告。

use legion_workspace::planner::WorkspaceResolver;
use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::Args;
use miette::{Result, miette};

use crate::cmds::{
    project_input::resolve_project_path,
    report::{TestResultEntry, finish_standalone_report, render_test_report},
    test_engine::{resolve_test_targets, run_tests_for_project},
};

/// `legion test` 命令参数。
#[derive(Debug, Clone, Args)]
pub struct TestArgs {
    /// 项目或 workspace 目录。
    #[arg(value_name = "project-dir", default_value = ".")]
    pub project_dir: PathBuf,
    /// 测试名称过滤器。
    #[arg(short = 'f', long = "filter")]
    pub filter: Option<String>,
    /// 编译目标（逗号分隔，或 `all`）。
    #[arg(short = 't', long = "target")]
    pub target: Option<String>,
    /// 显式运行器，格式 `target=command`。
    #[arg(short = 'r', long = "runner", value_name = "target=command")]
    pub runner: Vec<String>,
    /// 详细输出。
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,
    /// 额外产出单文件 HTML（内联 css/js/wasm，便于离线打开）。
    #[arg(long, default_value_t = false)]
    pub standalone: bool,
}

/// 执行 `legion test`。
pub fn run(args: &TestArgs) -> Result<ExitCode> {
    let project_dir = resolve_project_path(&args.project_dir)?;
    let targets = resolve_test_targets(args.target.as_deref());
    let workspace = WorkspaceResolver::discover_for_project(&project_dir)?;

    let (passed, failed, skipped, results) = if is_workspace_root(&project_dir, &workspace) {
        run_workspace_tests(&workspace, &project_dir, args.filter.as_deref(), &targets, &args.runner, args.verbose)
    }
    else {
        let member_name = project_dir.file_name().and_then(|n| n.to_str()).unwrap_or("project").to_string();
        println!("--- {member_name} ---");
        run_tests_for_project(&workspace, &project_dir, args.filter.as_deref(), &targets, &args.runner, args.verbose)
    };

    println!();
    println!("测试报告：{passed} 通过, {failed} 失败, {skipped} 跳过");

    let report_dir = if args.project_dir.extension().and_then(|e| e.to_str()) == Some("v") {
        args.project_dir.parent().unwrap_or(&args.project_dir).join("dist").join("legion-test")
    }
    else {
        project_dir.join("dist").join("legion-test")
    };
    let project_name = project_dir.file_stem().or_else(|| project_dir.file_name()).and_then(|n| n.to_str()).unwrap_or("project");
    render_test_report(&report_dir, project_name, &results)?;
    println!("HTML 测试报告已生成：{}", report_dir.join("index.html").display());
    finish_standalone_report(&report_dir, args.standalone)?;

    Ok(if failed > 0 { ExitCode::FAILURE } else { ExitCode::SUCCESS })
}

fn run_workspace_tests(
    workspace: &WorkspaceResolver,
    workspace_dir: &Path,
    filter: Option<&str>,
    targets: &[String],
    runners: &[String],
    verbose: bool,
) -> (usize, usize, usize, Vec<TestResultEntry>) {
    let members = workspace.member_manifest_dirs();
    if members.is_empty() {
        println!("错误：legions.von 中无 members");
        return (0, 0, 0, Vec::new());
    }

    let mut total_passed = 0usize;
    let mut total_failed = 0usize;
    let mut total_skipped = 0usize;
    let mut all_results = Vec::new();

    for member_dir in members {
        let member_name = member_dir.file_name().and_then(|n| n.to_str()).unwrap_or("member").to_string();
        println!("--- {member_name} ---");
        let (passed, failed, skipped, results) = run_tests_for_project(workspace, &member_dir, filter, targets, runners, verbose);
        total_passed += passed;
        total_failed += failed;
        total_skipped += skipped;
        for result in results {
            all_results.push(TestResultEntry {
                name: format!("{member_name}::{}", result.name),
                status: result.status,
                error: result.error,
                target: result.target,
            });
        }
        println!();
    }

    let _ = workspace_dir;
    (total_passed, total_failed, total_skipped, all_results)
}

fn is_workspace_root(project_dir: &Path, workspace: &WorkspaceResolver) -> bool {
    workspace.workspace_manifest.is_some() && same_path(project_dir, &workspace.root_dir)
}

fn same_path(left: &Path, right: &Path) -> bool {
    let left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    left == right
}
