//! `noodle fmt` — `nyar_language::javascript` SourceFormatter.

use std::{path::PathBuf, process::ExitCode};

use clap::Args;
use miette::{IntoDiagnostic, Result, miette};
use nyar_analyzer::format::FormatOptions;
use nyar_language::javascript::format::{format_javascript_source, language_id_from_extension};

use crate::project::NoodleProject;

/// `noodle fmt` arguments.
#[derive(Debug, Clone, Args)]
pub struct FmtArgs {
    /// Paths to format (default: project root).
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,
    /// Check only; non-zero exit when dirty.
    #[arg(long, default_value_t = false)]
    pub check: bool,
    /// Project directory.
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
    /// Print each rewritten file.
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,
}

/// Run `noodle fmt`.
pub fn run(args: &FmtArgs) -> Result<ExitCode> {
    let project = NoodleProject::discover(&args.project_dir)?;
    let roots = if args.paths.is_empty() { vec![project.root.clone()] } else { args.paths.clone() };
    let mut files = Vec::new();
    for root in &roots {
        collect(root, &mut files)?;
    }
    if files.is_empty() {
        return Err(miette!("没有可格式化的 JS/TS/JSON 文件"));
    }

    let options = FormatOptions::default();
    let mut dirty = 0usize;
    for file in files {
        let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
        let lang = language_id_from_extension(ext).ok_or_else(|| miette!("不支持的扩展名 {}", file.display()))?;
        let raw = std::fs::read_to_string(&file).into_diagnostic()?;
        let formatted = format_javascript_source(&raw, lang, &options).map_err(|e| miette!("{e}"))?;
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

fn collect(path: &std::path::Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if path.is_file() {
        if path.extension().and_then(|e| e.to_str()).and_then(language_id_from_extension).is_some() {
            out.push(path.to_path_buf());
        }
        return Ok(());
    }
    if !path.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(path).into_diagnostic()? {
        let entry = entry.into_diagnostic()?;
        let p = entry.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if matches!(name, "node_modules" | "dist" | ".git" | "coverage" | ".next" | "build") {
            continue;
        }
        collect(&p, out)?;
    }
    Ok(())
}
