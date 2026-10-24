//! `legion bench` — 性能基准测试并生成 HTML 报告。

use legion_workspace::planner::WorkspaceResolver;
use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::Args;
use miette::{Result, miette};

use crate::cmds::{
    project_input::resolve_project_path,
    report::{BenchReport, BenchResultRow, finish_standalone_report, render_bench_report},
    test_engine::{bench_project, resolve_test_targets},
};

/// `legion bench` 命令参数。
#[derive(Debug, Clone, Args)]
pub struct BenchArgs {
    /// 项目或 workspace 目录。
    #[arg(value_name = "project-dir", default_value = ".")]
    pub project_dir: PathBuf,
    /// 运行次数。
    #[arg(short = 'n', long = "runs", default_value_t = 3)]
    pub runs: usize,
    /// 编译目标（逗号分隔，或 `all`）。
    #[arg(short = 't', long = "target")]
    pub target: Option<String>,
    /// 详细输出。
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,
    /// 额外产出单文件 HTML（内联 css/js/wasm，便于离线打开）。
    #[arg(long, default_value_t = false)]
    pub standalone: bool,
}

/// 执行 `legion bench`。
pub fn run(args: &BenchArgs) -> Result<ExitCode> {
    let project_dir = resolve_project_path(&args.project_dir)?;
    let targets = resolve_test_targets(args.target.as_deref());
    let workspace = WorkspaceResolver::discover_for_project(&project_dir)?;

    let results = if is_workspace_root(&project_dir, &workspace) {
        run_workspace_bench(&workspace, args.runs, &targets, args.verbose)
    }
    else {
        let project_name = project_dir.file_name().and_then(|n| n.to_str()).unwrap_or("project");
        println!("--- {project_name} ---");
        bench_project(&workspace, &project_dir, project_name, args.runs, &targets, args.verbose)
    };

    print_bench_report(&results, args.runs);

    let report = BenchReport { runs: args.runs, rows: results };
    let report_dir = if args.project_dir.extension().and_then(|e| e.to_str()) == Some("v") {
        args.project_dir.parent().unwrap_or(&args.project_dir).join("dist").join("legion-benchmark")
    }
    else {
        project_dir.join("dist").join("legion-benchmark")
    };
    render_bench_report(&report_dir, &report)?;
    if report_dir.join("index.html").exists() {
        println!("  HTML 报告已生成: {}", report_dir.join("index.html").display());
    }
    finish_standalone_report(&report_dir, args.standalone)?;

    Ok(ExitCode::SUCCESS)
}

fn run_workspace_bench(workspace: &WorkspaceResolver, runs: usize, targets: &[String], verbose: bool) -> Vec<BenchResultRow> {
    let mut all = Vec::new();
    for member_dir in workspace.member_manifest_dirs() {
        let project_name = member_dir.file_name().and_then(|n| n.to_str()).unwrap_or("member");
        println!("--- {project_name} ---");
        all.extend(bench_project(workspace, &member_dir, project_name, runs, targets, verbose));
    }
    all
}

fn print_bench_report(results: &[BenchResultRow], runs: usize) {
    println!();
    println!("基准结果（{runs} 次运行）：");
    println!("{}", "-".repeat(72));
    println!("{:<20} {:<14} {:<8} {:>10} {:>10}", "项目", "测试", "目标", "编译(ms)", "运行(ms)");
    println!("{}", "-".repeat(72));
    for row in results {
        println!("{:<20} {:<14} {:<8} {:8.1}   {:8.1}", row.project, row.test, row.target, row.compile_ms, row.runtime_ms);
    }
    println!("{}", "-".repeat(72));
}

fn is_workspace_root(project_dir: &Path, workspace: &WorkspaceResolver) -> bool {
    workspace.workspace_manifest.is_some() && same_path(project_dir, &workspace.root_dir)
}

fn same_path(left: &Path, right: &Path) -> bool {
    let left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    left == right
}
