use std::{path::PathBuf, process::ExitCode};

use clap::Args;
use miette::{IntoDiagnostic, Result, WrapErr};
use nyar_package_manager::PackageManager;

/// `legion install` arguments.
#[derive(Debug, Clone, Args)]
pub struct InstallArgs {
    /// Optional package name. When omitted, installs manifest dependencies.
    pub package: Option<String>,
    /// Package version (default: latest).
    #[arg(long, default_value = "latest")]
    pub version: String,
    /// Registry name.
    #[arg(long, default_value = "npm")]
    pub registry: String,
    /// Project directory.
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
    /// Fail if lockfile would change.
    #[arg(long, default_value_t = false)]
    pub frozen_lockfile: bool,
    /// Install only from local cache/vendors; fail immediately when a package is missing.
    #[arg(long, default_value_t = false)]
    pub offline: bool,
    /// Include devDependencies.
    #[arg(long, default_value_t = false)]
    pub dev: bool,
}

/// Run `legion install`.
pub fn run(args: &InstallArgs) -> Result<ExitCode> {
    let mut pm = PackageManager::open(&args.project_dir, crate::LEGION_PROJECT_LAYOUT).into_diagnostic().wrap_err("打开包管理器上下文失败")?;
    pm.frozen_lockfile = args.frozen_lockfile;
    pm.offline = args.offline;

    if let Some(package) = &args.package {
        let info = pm.install_one(package, &args.version, &args.registry, true).into_diagnostic().wrap_err("安装包失败")?;
        println!("已安装 {}@{} ({})", info.name, info.version, info.registry);
    }
    else {
        let installed = pm.install_dependencies(args.dev, &args.registry).into_diagnostic().wrap_err("安装依赖失败")?;
        println!("已安装 {} 个依赖", installed.len());
        for info in installed {
            println!("  - {}@{}", info.name, info.version);
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// `legion add` arguments.
#[derive(Debug, Clone, Args)]
pub struct AddArgs {
    /// Package name.
    pub package: String,
    /// Version constraint.
    #[arg(long, default_value = "latest")]
    pub version: String,
    /// Registry name.
    #[arg(long, default_value = "npm")]
    pub registry: String,
    /// Project directory.
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
}

/// Run `legion add`.
pub fn run_add(args: &AddArgs) -> Result<ExitCode> {
    let mut pm = PackageManager::open(&args.project_dir, crate::LEGION_PROJECT_LAYOUT).into_diagnostic()?;
    let info = pm.add_dependency(&args.package, &args.version, &args.registry).into_diagnostic()?;
    println!("已添加依赖 {}@{}", info.name, info.version);
    Ok(ExitCode::SUCCESS)
}

/// `legion remove` arguments.
#[derive(Debug, Clone, Args)]
pub struct RemoveArgs {
    /// Package name.
    pub package: String,
    /// Project directory.
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
}

/// Run `legion remove`.
pub fn run_remove(args: &RemoveArgs) -> Result<ExitCode> {
    let mut pm = PackageManager::open(&args.project_dir, crate::LEGION_PROJECT_LAYOUT).into_diagnostic()?;
    let info = pm.remove_dependency(&args.package).into_diagnostic()?;
    println!("已移除依赖 {}", info.name);
    Ok(ExitCode::SUCCESS)
}

/// `legion update` arguments.
#[derive(Debug, Clone, Args)]
pub struct UpdateArgs {
    /// Optional package name. When omitted, updates all locked packages.
    pub package: Option<String>,
    /// Version to update to.
    #[arg(long, default_value = "latest")]
    pub version: String,
    /// Registry name.
    #[arg(long, default_value = "npm")]
    pub registry: String,
    /// Project directory.
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
}

/// Run `legion update`.
pub fn run_update(args: &UpdateArgs) -> Result<ExitCode> {
    let mut pm = PackageManager::open(&args.project_dir, crate::LEGION_PROJECT_LAYOUT).into_diagnostic()?;
    if let Some(package) = &args.package {
        let info = pm.update_one(package, &args.version, &args.registry).into_diagnostic()?;
        println!("已更新 {}@{}", info.name, info.version);
    }
    else {
        let updated = pm.update_all(&args.registry).into_diagnostic()?;
        println!("已更新 {} 个依赖", updated.len());
        for info in updated {
            println!("  - {}@{}", info.name, info.version);
        }
    }
    Ok(ExitCode::SUCCESS)
}
