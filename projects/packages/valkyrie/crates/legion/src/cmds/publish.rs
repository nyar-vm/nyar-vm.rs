use std::{path::PathBuf, process::ExitCode};

use clap::Args;
use miette::{IntoDiagnostic, Result, WrapErr};
use nyar_package_manager::{PackageManager, PublishOptions, VersionBump};

/// `legion publish` arguments.
#[derive(Debug, Clone, Args)]
pub struct PublishArgs {
    /// Project directory (default: current directory).
    #[arg(value_name = "project-dir", default_value = ".")]
    pub project_dir: PathBuf,
    /// Target registry name. When omitted, every registry entry in `legion.von` `publish` is published.
    #[arg(long)]
    pub registry: Option<String>,
    /// Publish tag (`latest`, `beta`, …).
    #[arg(long)]
    pub tag: Option<String>,
    /// Access level (`public` / `restricted`).
    #[arg(long)]
    pub access: Option<String>,
    /// Version bump (`patch` / `minor` / `major`).
    #[arg(long)]
    pub bump: Option<String>,
    /// Validate and pack without uploading.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
    /// Skip unclean git working-tree check.
    #[arg(long, default_value_t = false)]
    pub skip_git_check: bool,
    /// Do not create a git tag after success.
    #[arg(long, default_value_t = false)]
    pub no_git_tag: bool,
    /// Publish every workspace member.
    #[arg(long, default_value_t = false)]
    pub workspace: bool,
    /// Explicit auth token (otherwise uses stored / env token).
    #[arg(long)]
    pub token: Option<String>,
}

/// Run `legion publish`.
pub fn run(args: &PublishArgs) -> Result<ExitCode> {
    let mut legion =
        PackageManager::open(&args.project_dir, crate::LEGION_PROJECT_LAYOUT).into_diagnostic().wrap_err("打开包管理器上下文失败")?;
    let bump = args
        .bump
        .as_deref()
        .map(|value| VersionBump::parse(value).ok_or_else(|| miette::miette!("无效的 --bump 值: {value}（期望 patch/minor/major）")))
        .transpose()?;

    let options = PublishOptions {
        registry_name: args.registry.clone().unwrap_or_default(),
        tag: args.tag.clone(),
        access: args.access.clone(),
        bump,
        dry_run: args.dry_run,
        skip_git_check: args.skip_git_check,
        create_git_tag: !args.no_git_tag && !args.dry_run,
        auth_token: args.token.clone(),
        ..PublishOptions::default()
    };

    let result = legion.publish(options, args.workspace).into_diagnostic().wrap_err("发布失败")?;
    if result.success {
        if result.dry_run {
            println!(
                "dry-run 成功: {}@{} ({} bytes, {} files, {})",
                result.package_name,
                result.version,
                result.size.unwrap_or(0),
                result.file_count.unwrap_or(0),
                result.sha256.as_deref().unwrap_or("-")
            );
        }
        else {
            println!("发布成功: {}@{}", result.package_name, result.version);
            if let Some(url) = &result.published_url {
                println!("URL: {url}");
            }
        }
        Ok(ExitCode::SUCCESS)
    }
    else if result.official_tool_required {
        println!("需要官方工具发布 {}@{}", result.package_name, result.version);
        println!("{}", result.message);
        if let Some(url) = &result.published_url {
            println!("registry: {url}");
        }
        Ok(ExitCode::FAILURE)
    }
    else {
        eprintln!("发布失败: {}", result.message);
        Ok(ExitCode::FAILURE)
    }
}
