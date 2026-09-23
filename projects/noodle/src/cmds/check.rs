//! `noodle check` — fmt + lint（借鉴 Vite+ `vp check` 的聚合形态）.

use std::{path::PathBuf, process::ExitCode};

use clap::Args;
use miette::{Result, miette};

use crate::cmds::{
    fmt::{FmtArgs, run as run_fmt},
    lint::{LintArgs, run as run_lint},
};

/// `noodle check` arguments.
#[derive(Debug, Clone, Args)]
pub struct CheckArgs {
    /// Project directory.
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
    /// Skip formatting.
    #[arg(long = "no-fmt", default_value_t = false)]
    pub no_fmt: bool,
    /// Skip lint.
    #[arg(long = "no-lint", default_value_t = false)]
    pub no_lint: bool,
    /// Write fmt + apply lint fixes.
    #[arg(long, default_value_t = false)]
    pub fix: bool,
}

/// Run `noodle check`.
pub fn run(args: &CheckArgs) -> Result<ExitCode> {
    println!("noodle check · {}", args.project_dir.display());

    if !args.no_fmt {
        let code = run_fmt(&FmtArgs { paths: Vec::new(), check: !args.fix, project_dir: args.project_dir.clone(), verbose: false })?;
        if code != ExitCode::SUCCESS {
            return Err(miette!("check failed at fmt"));
        }
    }

    if !args.no_lint {
        let code = run_lint(&LintArgs { paths: Vec::new(), fix: args.fix, project_dir: args.project_dir.clone() })?;
        if code != ExitCode::SUCCESS {
            return Err(miette!("check failed at lint"));
        }
    }

    println!("check ok");
    Ok(ExitCode::SUCCESS)
}
