use std::process::ExitCode;

use clap::{Args, Subcommand};
use miette::{IntoDiagnostic, Result, WrapErr};
use nyar_package_manager::PackageManager;

/// `legion vendor` arguments.
#[derive(Debug, Clone, Args)]
pub struct VendorArgs {
    /// vendor 子命令。
    #[command(subcommand)]
    pub command: VendorCommand,
}

/// Vendor auth subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum VendorCommand {
    /// Login to a registry.
    Login(VendorLoginArgs),
    /// Logout from a registry.
    Logout(VendorLogoutArgs),
    /// List stored registry credentials.
    List,
    /// Show the authenticated user for a registry.
    Whoami(VendorWhoamiArgs),
}

/// `legion vendor login` arguments.
#[derive(Debug, Clone, Args)]
pub struct VendorLoginArgs {
    /// Registry name.
    pub registry: String,
    /// Auth token. When omitted, discovers from official CLI stores.
    #[arg(long)]
    pub token: Option<String>,
}

/// `legion vendor logout` arguments.
#[derive(Debug, Clone, Args)]
pub struct VendorLogoutArgs {
    /// Registry name.
    pub registry: String,
}

/// `legion vendor whoami` arguments.
#[derive(Debug, Clone, Args)]
pub struct VendorWhoamiArgs {
    /// Registry name.
    #[arg(default_value = "npm")]
    pub registry: String,
}

/// Run `legion vendor`.
pub fn run(args: &VendorArgs) -> Result<ExitCode> {
    let mut legion = PackageManager::open(".", crate::LEGION_PROJECT_LAYOUT).into_diagnostic().wrap_err("打开包管理器上下文失败")?;
    match &args.command {
        VendorCommand::Login(login) => {
            let result = legion.vendor_login(&login.registry, login.token.as_deref()).into_diagnostic()?;
            if result.verify.valid {
                println!(
                    "已登录 {} 为 {}（{}）",
                    login.registry,
                    result.verify.username.unwrap_or_else(|| "unknown".to_string()),
                    result.credential_source
                );
                if result.stored_in_auth_von {
                    println!("凭据已写入 ~/.valkyrie/auth.von");
                }
                else if nyar_package_manager::is_external_registry(&login.registry) {
                    println!("凭据来自官方 CLI 配置，未写入 auth.von");
                }
                if let Some(target) = result.synced_to {
                    println!("已同步到官方 CLI 存储：{target}");
                }
                Ok(ExitCode::SUCCESS)
            }
            else {
                eprintln!("登录失败: {}", result.verify.error_message.unwrap_or_default());
                Ok(ExitCode::FAILURE)
            }
        }
        VendorCommand::Logout(logout) => {
            if legion.vendor_logout(&logout.registry).into_diagnostic()? {
                println!("已登出 {}", logout.registry);
            }
            else {
                println!("未找到 {} 的凭证", logout.registry);
            }
            Ok(ExitCode::SUCCESS)
        }
        VendorCommand::List => {
            let list = legion.vendor_list();
            if list.is_empty() {
                println!("没有已存储的注册表凭证");
            }
            else {
                for name in list {
                    println!("{name}");
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        VendorCommand::Whoami(whoami) => {
            let result = legion.vendor_whoami(&whoami.registry).into_diagnostic()?;
            if result.valid {
                println!("{}", result.username.unwrap_or_else(|| "unknown".to_string()));
                Ok(ExitCode::SUCCESS)
            }
            else {
                eprintln!("{}", result.error_message.unwrap_or_else(|| "未登录".to_string()));
                Ok(ExitCode::FAILURE)
            }
        }
    }
}
