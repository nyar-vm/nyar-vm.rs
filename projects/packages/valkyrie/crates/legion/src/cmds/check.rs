use std::process::ExitCode;

use clap::Args;
use miette::Result;

use crate::cmds::build::{BuildArgs, run as run_build};

/// `legion check` arguments (compile / plan validation, currently reuses build).
#[derive(Debug, Clone, Args)]
pub struct CheckArgs {
    /// Project directory.
    #[arg(value_name = "project-dir", default_value = ".")]
    pub project_dir: std::path::PathBuf,
    /// Target platform.
    #[arg(long, default_value = "clr")]
    pub target: rusty_valkyrie::CanonicalTarget,
    /// Force workspace resolution.
    #[arg(long, default_value_t = false)]
    pub workspace: bool,
    /// Verbose progress (maps to build debug artifacts currently unused).
    #[arg(long, default_value_t = false)]
    pub verbose: bool,
}

/// Run `legion check`.
pub fn run(args: &CheckArgs) -> Result<ExitCode> {
    println!("check：验证项目可编译性（复用 build）");
    let build_args = BuildArgs {
        project_dir: args.project_dir.clone(),
        target: args.target.clone(),
        output_dir: None,
        workspace: args.workspace,
        debug_artifacts: args.verbose,
    };
    run_build(&build_args)
}
