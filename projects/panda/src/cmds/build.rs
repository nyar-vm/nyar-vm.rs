//! `panda build` / `run` / `test` — ScriptRunner only (no uv/poetry/pip/pytest shells).

use std::{path::PathBuf, process::ExitCode};

use clap::Args;
use miette::{IntoDiagnostic, Result, miette};
use nyar_package_manager::ScriptRunner;

use crate::{project::PandaProject, python_path::python_runtime_env};

fn run_named(project: &PandaProject, name: &str, command: &str) -> Result<ExitCode> {
    let runner = ScriptRunner::new(&project.root);
    let env_owned = python_runtime_env(&project.root);
    let env_refs: Vec<(&str, &str)> = env_owned.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let result = runner.run_with_env(name, command, &env_refs).into_diagnostic()?;
    print!("{}", result.stdout);
    eprint!("{}", result.stderr);
    if result.success { Ok(ExitCode::SUCCESS) } else { Ok(ExitCode::from(result.exit_code.unwrap_or(1) as u8)) }
}

/// `panda build`.
#[derive(Debug, Clone, Args)]
pub struct BuildArgs {
    /// Project directory.
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
}

/// Run build via `scripts/build.py` (no hatch/uv/poetry/`python -m build` shell).
pub fn run(args: &BuildArgs) -> Result<ExitCode> {
    let project = PandaProject::discover(&args.project_dir)?;
    let script = project.root.join("scripts").join("build.py");
    if script.is_file() {
        // Relative path avoids Windows backslash issues inside `cmd /C`.
        return run_named(&project, "build", "python scripts/build.py");
    }
    Err(miette!("未找到 scripts/build.py；panda 不再套壳 hatch/uv/poetry build"))
}

/// `panda run`.
#[derive(Debug, Clone, Args)]
pub struct RunArgs {
    /// Script path or module name (`-m`).
    pub target: String,
    /// Extra args forwarded to Python.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
    /// Treat `target` as a module (`python -m`).
    #[arg(short = 'm', long, default_value_t = false)]
    pub module: bool,
    /// Project directory.
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
}

/// Run a python module or script path via ScriptRunner.
pub fn run_cmd(args: &RunArgs) -> Result<ExitCode> {
    let project = PandaProject::discover(&args.project_dir)?;
    let mut command = if args.module { format!("python -m {}", args.target) } else { format!("python {}", args.target) };
    if !args.args.is_empty() {
        command.push(' ');
        command.push_str(&args.args.join(" "));
    }
    run_named(&project, "run", &command)
}

/// `panda exec`.
#[derive(Debug, Clone, Args)]
pub struct ExecArgs {
    /// Executable / script path.
    pub bin: String,
    /// Extra args.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
    /// Project directory.
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
}

/// Exec a script path via ScriptRunner (same path as `panda run` without `-m`).
pub fn run_exec(args: &ExecArgs) -> Result<ExitCode> {
    run_cmd(&RunArgs { target: args.bin.clone(), args: args.args.clone(), module: false, project_dir: args.project_dir.clone() })
}

/// `panda test`.
#[derive(Debug, Clone, Args)]
pub struct TestArgs {
    /// Extra args for `unittest discover`.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
    /// Project directory.
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
}

/// Run stdlib unittest discovery — no pytest shell.
pub fn run_test(args: &TestArgs) -> Result<ExitCode> {
    let project = PandaProject::discover(&args.project_dir)?;
    let mut command = if args.args.is_empty() && project.root.join("tests").is_dir() {
        "python -m unittest discover -s tests -p test_*.py".to_string()
    }
    else {
        "python -m unittest discover".to_string()
    };
    if !args.args.is_empty() {
        command.push(' ');
        command.push_str(&args.args.join(" "));
    }
    run_named(&project, "test", &command)
}
