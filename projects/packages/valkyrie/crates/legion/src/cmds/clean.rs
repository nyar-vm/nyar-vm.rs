use std::{fs, path::PathBuf, process::ExitCode};

use clap::Args;
use miette::{IntoDiagnostic, Result};

use legion_workspace::planner::WorkspaceResolver;

/// `legion clean` arguments.
#[derive(Debug, Clone, Args)]
pub struct CleanArgs {
    /// Project directory.
    #[arg(value_name = "project-dir", default_value = ".")]
    pub project_dir: PathBuf,
    /// Clean all workspace members.
    #[arg(long, default_value_t = false)]
    pub workspace: bool,
}

/// Run `legion clean`.
pub fn run(args: &CleanArgs) -> Result<ExitCode> {
    if args.workspace {
        let workspace = WorkspaceResolver::discover(&args.project_dir)?;
        let mut cleaned = 0usize;
        for member in workspace.member_manifest_dirs() {
            cleaned += clean_project_dirs(&member)?;
        }
        println!("已清理 workspace 成员 {} 个目录", cleaned);
        return Ok(ExitCode::SUCCESS);
    }

    let cleaned = clean_project_dirs(&args.project_dir)?;
    println!("已清理 {} 个目录", cleaned);
    Ok(ExitCode::SUCCESS)
}

fn clean_project_dirs(project_dir: &PathBuf) -> Result<usize> {
    let mut cleaned = 0usize;
    for relative in ["dist", ".cache"] {
        let path = project_dir.join(relative);
        if path.is_dir() {
            fs::remove_dir_all(&path).into_diagnostic()?;
            println!("clean: {}", path.display());
            cleaned += 1;
        }
    }
    Ok(cleaned)
}
