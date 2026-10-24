use std::process::ExitCode;

use clap::Args;
use miette::{IntoDiagnostic, Result, WrapErr};
use nyar_package_manager::PackageManager;

/// `legion search` arguments.
#[derive(Debug, Clone, Args)]
pub struct SearchArgs {
    /// Search query.
    pub query: String,
    /// Registry name.
    #[arg(long, default_value = "npm")]
    pub registry: String,
}

/// Run `legion search`.
pub fn run(args: &SearchArgs) -> Result<ExitCode> {
    let legion = PackageManager::open(".", crate::LEGION_PROJECT_LAYOUT).into_diagnostic().wrap_err("打开包管理器上下文失败")?;
    let packages = legion.search(&args.query, &args.registry).into_diagnostic().wrap_err("搜索失败")?;
    if packages.is_empty() {
        println!("未找到匹配包");
        return Ok(ExitCode::SUCCESS);
    }
    for package in packages {
        println!("{}@{}  {}", package.name, package.version, package.description);
    }
    Ok(ExitCode::SUCCESS)
}

/// `legion info` arguments.
#[derive(Debug, Clone, Args)]
pub struct InfoArgs {
    /// Package name.
    pub package: String,
    /// Registry name.
    #[arg(long, default_value = "npm")]
    pub registry: String,
}

/// Run `legion info`.
pub fn run_info(args: &InfoArgs) -> Result<ExitCode> {
    let legion = PackageManager::open(".", crate::LEGION_PROJECT_LAYOUT).into_diagnostic()?;
    let info = legion.get_package_info(&args.package, &args.registry).into_diagnostic()?;
    println!("name:        {}", info.name);
    println!("version:     {}", info.version);
    println!("registry:    {}", info.registry);
    println!("description: {}", info.description);
    Ok(ExitCode::SUCCESS)
}
