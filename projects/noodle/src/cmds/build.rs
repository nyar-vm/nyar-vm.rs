//! `noodle build` — run `scripts.build` via package-manager ScriptRunner (no vite shell).

use std::{path::PathBuf, process::ExitCode};

use clap::Args;
use miette::{IntoDiagnostic, Result, miette};
use nyar_package_manager::ScriptRunner;

use crate::{manifest::load_package_manifest, project::NoodleProject};

/// `noodle build` arguments.
#[derive(Debug, Clone, Args)]
pub struct BuildArgs {
    /// Project directory (walks up for `package.json`).
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
}

/// Run build script from package.json.
pub fn run(args: &BuildArgs) -> Result<ExitCode> {
    let project = NoodleProject::discover(&args.project_dir)?;
    let manifest = load_package_manifest(&project.root)?;
    let command = manifest.scripts.get("build").ok_or_else(|| miette!("package.json 没有 scripts.build"))?;
    let runner = ScriptRunner::new(&project.root);
    let result = runner.run("build", command).into_diagnostic()?;
    print!("{}", result.stdout);
    eprint!("{}", result.stderr);
    if result.success { Ok(ExitCode::SUCCESS) } else { Ok(ExitCode::from(result.exit_code.unwrap_or(1) as u8)) }
}
