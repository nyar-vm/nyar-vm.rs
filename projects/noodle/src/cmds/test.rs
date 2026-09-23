//! `noodle test` — `scripts.test` via ScriptRunner.

use std::{path::PathBuf, process::ExitCode};

use clap::Args;
use miette::{IntoDiagnostic, Result, miette};
use nyar_package_manager::ScriptRunner;

use crate::{manifest::load_package_manifest, project::NoodleProject};

/// `noodle test`.
#[derive(Debug, Clone, Args)]
pub struct TestArgs {
    /// Extra args appended to `scripts.test`.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
    /// Project directory (walks up for `package.json`).
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
}

/// Run tests.
pub fn run(args: &TestArgs) -> Result<ExitCode> {
    let project = NoodleProject::discover(&args.project_dir)?;
    let manifest = load_package_manifest(&project.root)?;
    let command = manifest.scripts.get("test").ok_or_else(|| miette!("package.json 没有 scripts.test"))?;
    let mut full = command.clone();
    if !args.args.is_empty() {
        full.push(' ');
        full.push_str(&args.args.join(" "));
    }
    let runner = ScriptRunner::new(&project.root);
    let result = runner.run("test", &full).into_diagnostic()?;
    print!("{}", result.stdout);
    eprint!("{}", result.stderr);
    if result.success { Ok(ExitCode::SUCCESS) } else { Ok(ExitCode::from(result.exit_code.unwrap_or(1) as u8)) }
}
