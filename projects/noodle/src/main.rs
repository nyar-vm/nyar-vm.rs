//! Noodle CLI — clap + nyar-language javascript + nyar-package-manager.

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use miette::Report;
use noodle::cmds::{
    build::{BuildArgs, run as run_build},
    check::{CheckArgs, run as run_check},
    create::{CreateArgs, run as run_create},
    fmt::{FmtArgs, run as run_fmt},
    install::{AddArgs, InstallArgs, RemoveArgs, UpdateArgs, run_add, run_install, run_remove, run_update},
    lint::{LintArgs, run as run_lint},
    run::{ExecArgs, RunArgs, run as run_run, run_exec},
    test::{TestArgs, run as run_test},
};

#[derive(Debug, Parser)]
#[command(
    name = "noodle",
    version,
    about = "Noodle — Node.js 统一工具链（clap + nyar-language javascript + nyar-package-manager；形态借鉴 Vite+，非逐命令对齐）"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// 创建脚手架。
    Create(CreateArgs),
    /// 安装依赖（nyar-package-manager）。
    Install(InstallArgs),
    Add(AddArgs),
    Remove(RemoveArgs),
    Update(UpdateArgs),
    /// 自带格式化（nyar-language javascript）。
    #[command(visible_alias = "format")]
    Fmt(FmtArgs),
    /// 自带 lint（nyar-language javascript）。
    Lint(LintArgs),
    /// 聚合 fmt + lint。
    Check(CheckArgs),
    Build(BuildArgs),
    Run(RunArgs),
    Exec(ExecArgs),
    Test(TestArgs),
}

fn main() -> Result<ExitCode, Report> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Create(args) => run_create(&args),
        Commands::Install(args) => run_install(&args),
        Commands::Add(args) => run_add(&args),
        Commands::Remove(args) => run_remove(&args),
        Commands::Update(args) => run_update(&args),
        Commands::Fmt(args) => run_fmt(&args),
        Commands::Lint(args) => run_lint(&args),
        Commands::Check(args) => run_check(&args),
        Commands::Build(args) => run_build(&args),
        Commands::Run(args) => run_run(&args),
        Commands::Exec(args) => run_exec(&args),
        Commands::Test(args) => run_test(&args),
    }
}
