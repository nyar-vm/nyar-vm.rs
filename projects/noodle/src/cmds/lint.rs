//! `noodle lint` — `nyar_language::javascript` lint.

use std::{path::PathBuf, process::ExitCode};

use clap::Args;
use miette::{IntoDiagnostic, Result, miette};
use nyar_language::javascript::lint::{JavascriptLintOptions, collect_javascript_files, lint_javascript_path};

use crate::project::NoodleProject;

/// `noodle lint` arguments.
#[derive(Debug, Clone, Args)]
pub struct LintArgs {
    /// Paths to lint.
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,
    /// Apply safe autofixes.
    #[arg(long, default_value_t = false)]
    pub fix: bool,
    /// Project directory.
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
}

/// Run `noodle lint`.
pub fn run(args: &LintArgs) -> Result<ExitCode> {
    let project = NoodleProject::discover(&args.project_dir)?;
    let roots = if args.paths.is_empty() { vec![project.root.clone()] } else { args.paths.clone() };
    let files = collect_javascript_files(&roots).into_diagnostic()?;
    if files.is_empty() {
        return Err(miette!("没有可 lint 的 JS/TS 文件"));
    }

    let options = JavascriptLintOptions { fix: args.fix };
    let mut issues = Vec::new();
    for file in &files {
        let (_, file_issues) = lint_javascript_path(file, &options).into_diagnostic()?;
        for issue in file_issues {
            if args.fix && issue.fixed {
                continue;
            }
            eprintln!("{}:{}: {}", file.display(), issue.line, issue.message);
            issues.push(issue);
        }
    }

    if issues.is_empty() {
        println!("lint ok");
        Ok(ExitCode::SUCCESS)
    }
    else {
        Err(miette!("lint failed: {} issue(s)", issues.len()))
    }
}
