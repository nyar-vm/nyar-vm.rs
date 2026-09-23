//! `noodle run` / `exec` — package.json scripts via ScriptRunner (no npm/pnpm shell).

use std::{path::PathBuf, process::ExitCode};

use clap::Args;
use miette::{IntoDiagnostic, Result, miette};
use nyar_package_manager::ScriptRunner;

use crate::{manifest::load_package_manifest, project::NoodleProject};

/// `noodle run`.
#[derive(Debug, Clone, Args)]
pub struct RunArgs {
    /// Script name from `package.json` `scripts`.
    pub script: String,
    /// Extra args appended to the script command.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
    /// Project directory (walks up for `package.json`).
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
}

/// Run a named script.
pub fn run(args: &RunArgs) -> Result<ExitCode> {
    let project = NoodleProject::discover(&args.project_dir)?;
    let manifest = load_package_manifest(&project.root)?;
    let command = manifest.scripts.get(&args.script).ok_or_else(|| miette!("package.json 中不存在 scripts.{}", args.script))?;
    let mut full = command.clone();
    if !args.args.is_empty() {
        full.push(' ');
        full.push_str(&args.args.join(" "));
    }
    let runner = ScriptRunner::new(&project.root);
    let result = runner.run(&args.script, &full).into_diagnostic()?;
    print!("{}", result.stdout);
    eprint!("{}", result.stderr);
    if result.success { Ok(ExitCode::SUCCESS) } else { Ok(ExitCode::from(result.exit_code.unwrap_or(1) as u8)) }
}

/// `noodle exec` — run an arbitrary command string in the project directory.
#[derive(Debug, Clone, Args)]
pub struct ExecArgs {
    /// Command / binary to run in the project directory.
    pub bin: String,
    /// Extra args appended to the command.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
    /// Project directory (walks up for `package.json`).
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
}

/// Run exec.
pub fn run_exec(args: &ExecArgs) -> Result<ExitCode> {
    let project = NoodleProject::discover(&args.project_dir)?;
    let mut command = args.bin.clone();
    if !args.args.is_empty() {
        command.push(' ');
        command.push_str(&args.args.join(" "));
    }
    let runner = ScriptRunner::new(&project.root);
    let result = runner.run("exec", &command).into_diagnostic()?;
    print!("{}", result.stdout);
    eprint!("{}", result.stderr);
    if result.success { Ok(ExitCode::SUCCESS) } else { Ok(ExitCode::from(result.exit_code.unwrap_or(1) as u8)) }
}
