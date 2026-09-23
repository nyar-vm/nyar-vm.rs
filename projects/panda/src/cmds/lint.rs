//! `panda lint` — `nyar_language::python` lint.

use std::{path::PathBuf, process::ExitCode};

use clap::Args;
use miette::{IntoDiagnostic, Result, miette};
use nyar_language::python::{PythonLintOptions, collect_python_files, lint_python_path};

use crate::project::PandaProject;

/// `panda lint`.
#[derive(Debug, Clone, Args)]
pub struct LintArgs {
    /// Optional paths; default is the discovered project root.
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,
    /// Reserved for future autofixes (`PythonLintOptions::fix`).
    #[arg(long, default_value_t = false)]
    pub fix: bool,
    /// Project directory.
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
}

/// Run lint.
pub fn run(args: &LintArgs) -> Result<ExitCode> {
    let project = PandaProject::discover(&args.project_dir)?;
    let roots = if args.paths.is_empty() { vec![project.root.clone()] } else { args.paths.clone() };
    let files = collect_python_files(&roots).into_diagnostic()?;
    if files.is_empty() {
        return Err(miette!("没有可 lint 的 .py 文件"));
    }
    let options = PythonLintOptions { fix: args.fix };
    let mut count = 0usize;
    for file in &files {
        let issues = lint_python_path(file, &options).into_diagnostic()?;
        for issue in &issues {
            eprintln!("{}:{}: {}", file.display(), issue.line, issue.message);
            count += 1;
        }
    }
    if count == 0 {
        println!("lint ok");
        Ok(ExitCode::SUCCESS)
    }
    else {
        Err(miette!("lint failed: {count} issue(s)"))
    }
}
