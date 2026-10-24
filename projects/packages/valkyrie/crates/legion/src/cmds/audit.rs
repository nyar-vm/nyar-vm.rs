use std::process::ExitCode;

use clap::Args;
use miette::{IntoDiagnostic, Result, WrapErr};
use nyar_package_manager::PackageManager;

/// `legion audit` arguments.
#[derive(Debug, Clone, Args)]
pub struct AuditArgs {
    /// Project directory (default: current directory).
    #[arg(value_name = "project-dir", default_value = ".")]
    pub project_dir: std::path::PathBuf,
    /// Skip OSV network queries (license checks only).
    #[arg(long, default_value_t = false)]
    pub offline: bool,
    /// Fail with non-zero exit if vulnerabilities are found.
    #[arg(long, default_value_t = false)]
    pub fail_on_vuln: bool,
    /// Fail with non-zero exit if license policy issues are found.
    #[arg(long, default_value_t = false)]
    pub fail_on_license: bool,
}

/// Run `legion audit`.
pub fn run(args: &AuditArgs) -> Result<ExitCode> {
    let legion = PackageManager::open(&args.project_dir, crate::LEGION_PROJECT_LAYOUT).into_diagnostic().wrap_err("打开包管理器上下文失败")?;
    let result = legion.audit(args.offline).into_diagnostic().wrap_err("安全审计失败")?;

    if result.vulnerabilities.is_empty() {
        println!("vulnerabilities: none");
    }
    else {
        println!("vulnerabilities ({}):", result.vulnerabilities.len());
        for report in &result.vulnerabilities {
            let id = report.vulnerability_id.as_deref().unwrap_or("unknown");
            println!("  - {}@{}  {}  [{}] {}", report.package_name, report.version, id, report.severity, report.title);
            if let Some(fixed) = &report.fixed_version {
                println!("      fixed in: {fixed}");
            }
            if let Some(url) = &report.url {
                println!("      url: {url}");
            }
        }
    }

    let issues: Vec<_> = result.licenses.iter().filter(|license| !license.is_compatible || license.is_restricted).collect();
    if issues.is_empty() {
        println!("licenses: ok ({} packages)", result.licenses.len());
    }
    else {
        println!("licenses issues ({}):", issues.len());
        for license in issues {
            let status = if license.is_restricted { "restricted" } else { "incompatible" };
            println!("  - {}@{}  {} ({status})", license.package_name, license.version, license.license);
        }
    }

    let suggestions = result.fix_suggestions();
    if !suggestions.is_empty() {
        println!("suggested upgrades:");
        for (package, version) in suggestions {
            println!("  - {package} -> {version}");
        }
    }

    let fail_vuln = args.fail_on_vuln && result.has_vulnerabilities();
    let fail_license = args.fail_on_license && result.has_license_issues();
    if fail_vuln || fail_license { Ok(ExitCode::FAILURE) } else { Ok(ExitCode::SUCCESS) }
}
