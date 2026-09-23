//! `panda fmt` — `nyar_language::python` SourceFormatter.

use std::{path::PathBuf, process::ExitCode};

use clap::Args;
use miette::{IntoDiagnostic, Result, miette};
use nyar_analyzer::format::FormatOptions;
use nyar_language::python::{collect_python_files, format_python_source};

use crate::project::PandaProject;

/// `panda fmt`.
#[derive(Debug, Clone, Args)]
pub struct FmtArgs {
    /// Optional paths; default is the discovered project root.
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,
    /// Check only; do not rewrite files.
    #[arg(long, default_value_t = false)]
    pub check: bool,
    /// Project directory.
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
    /// Print each rewritten path.
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,
}

/// Run fmt.
pub fn run(args: &FmtArgs) -> Result<ExitCode> {
    let project = PandaProject::discover(&args.project_dir)?;
    let roots = if args.paths.is_empty() { vec![project.root.clone()] } else { args.paths.clone() };
    let files = collect_python_files(&roots).into_diagnostic()?;
    if files.is_empty() {
        return Err(miette!("没有可格式化的 .py 文件"));
    }
    let options = FormatOptions::default();
    let mut dirty = 0usize;
    for file in files {
        let raw = std::fs::read_to_string(&file).into_diagnostic()?;
        let formatted = format_python_source(&raw, &options).map_err(|e| miette!("{e}"))?;
        if formatted == raw {
            continue;
        }
        dirty += 1;
        if args.check {
            eprintln!("would reformat: {}", file.display());
        }
        else {
            std::fs::write(&file, &formatted).into_diagnostic()?;
            if args.verbose {
                println!("formatted: {}", file.display());
            }
        }
    }
    if args.check && dirty > 0 {
        return Err(miette!("fmt check failed: {dirty} file(s)"));
    }
    if args.check {
        println!("fmt check ok");
    }
    else {
        println!("fmt ok ({dirty} rewritten)");
    }
    Ok(ExitCode::SUCCESS)
}
