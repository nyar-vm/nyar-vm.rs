//! `noodle install` / `add` / `remove` / `update` via `nyar-package-manager`.

use std::{collections::HashMap, path::PathBuf, process::ExitCode, sync::Arc};

use clap::Args;
use miette::{IntoDiagnostic, Result, WrapErr, miette};
use nyar_package_manager::PackageManager;
use nyar_package_registry::{Registry, default_registries};

use crate::{
    layout::project_layout,
    manifest::{load_package_manifest, save_dependencies},
    node_modules::{materialize_for_compat, note_foreign_lockfiles},
    project::NoodleProject,
};

/// After PM vendors install, adapt layout for Node resolve (flat or pnpm-like).
fn sync_node_modules(project: &NoodleProject) -> Result<()> {
    note_foreign_lockfiles(&project.root);
    let report = materialize_for_compat(&project.root, project.compat)?;
    if report.linked == 0 {
        println!("node_modules · {} · (empty)", report.layout.label());
    }
    else if report.copied > 0 {
        println!("node_modules · {} · linked {} package(s) ({} via copy fallback)", report.layout.label(), report.linked, report.copied);
    }
    else {
        println!("node_modules · {} · linked {} package(s)", report.layout.label(), report.linked);
    }
    if matches!(project.compat, crate::project::PmCompat::Pnpm) {
        println!("note: pnpm-like layout is noodle's 自研 adapter (≠ pnpm CLI)");
    }
    Ok(())
}

/// Default registry id chosen by the **noodle product** (not by the PM core).
fn default_registry_for(project: &NoodleProject) -> &'static str {
    // Opaque registry adapter name from nyar-package-registry; product picks it.
    match project.compat {
        crate::project::PmCompat::Npm | crate::project::PmCompat::Pnpm | crate::project::PmCompat::Yarn | crate::project::PmCompat::Bun => {
            "npm"
        }
    }
}

/// Open PM with product-translated manifest, registries, and noodle [`project_layout`].
pub(crate) fn open_pm_with_registries(project: &NoodleProject, registries: HashMap<String, Arc<dyn Registry>>) -> Result<PackageManager> {
    let manifest = load_package_manifest(&project.root)?;
    PackageManager::open_with_manifest_layout(&project.root, manifest, registries, project_layout())
        .into_diagnostic()
        .wrap_err("打开包管理器失败")
}

fn open_pm(project: &NoodleProject) -> Result<PackageManager> {
    let registries: HashMap<String, Arc<dyn Registry>> = default_registries().into_diagnostic()?;
    open_pm_with_registries(project, registries)
}

fn save_pm_manifest(project: &NoodleProject, pm: &PackageManager) -> Result<()> {
    let manifest = pm.mode.package_manifest().ok_or_else(|| miette!("包管理器未持有 package manifest"))?;
    save_dependencies(&project.root, manifest)
}

/// `noodle install`.
#[derive(Debug, Clone, Args)]
pub struct InstallArgs {
    /// Project directory (walks up for `package.json`).
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
    /// Fail when lockfile would change.
    #[arg(long, default_value_t = false)]
    pub frozen_lockfile: bool,
    /// Install from lock/cache only (no network).
    #[arg(long, default_value_t = false)]
    pub offline: bool,
    /// Skip `devDependencies` (production-only install).
    #[arg(long, default_value_t = false)]
    pub prod: bool,
}

/// Run install through the package manager (noodle layout).
pub fn run_install(args: &InstallArgs) -> Result<ExitCode> {
    let project = NoodleProject::discover(&args.project_dir)?;
    let registry = default_registry_for(&project);
    let include_dev = !args.prod;
    println!("noodle install · compat={} · registry={registry} · prod={} · {}", project.compat.label(), args.prod, project.root.display());
    let mut pm = open_pm(&project)?;
    pm.frozen_lockfile = args.frozen_lockfile;
    pm.offline = args.offline;
    // Install does not mutate the in-memory manifest; product owns package.json — no rewrite.
    let installed = pm.install_dependencies(include_dev, registry).into_diagnostic()?;
    println!("已安装 {} 个依赖", installed.len());
    for info in installed {
        println!("  - {}@{} ({})", info.name, info.version, info.registry);
    }
    // PM keeps vendors/{registry}/…; product materializes flat or pnpm-like node_modules.
    sync_node_modules(&project)?;
    Ok(ExitCode::SUCCESS)
}

/// `noodle add`.
#[derive(Debug, Clone, Args)]
pub struct AddArgs {
    /// Package name to add.
    pub package: String,
    /// Version constraint / tag (default: `latest`).
    #[arg(long, default_value = "latest")]
    pub version: String,
    /// Save under `devDependencies`.
    #[arg(long, default_value_t = false)]
    pub dev: bool,
    /// Project directory (walks up for `package.json`).
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
}

/// Run add.
pub fn run_add(args: &AddArgs) -> Result<ExitCode> {
    let project = NoodleProject::discover(&args.project_dir)?;
    let registry = default_registry_for(&project);
    let mut pm = open_pm(&project)?;
    // write_manifest=false: avoid VON save; product owns package.json.
    // Prefer PackageManifest::{add_dependency,add_dev_dependency} over manual bucket edits.
    let info = pm.install_one(&args.package, &args.version, registry, false).into_diagnostic()?;
    let mut manifest = pm.mode.package_manifest().cloned().ok_or_else(|| miette!("包管理器未持有 package manifest"))?;
    if args.dev {
        manifest.add_dev_dependency(&info.name, &info.version);
    }
    else {
        manifest.add_dependency(&info.name, &info.version);
    }
    save_dependencies(&project.root, &manifest)?;
    sync_node_modules(&project)?;
    println!("已添加 {}@{}", info.name, info.version);
    Ok(ExitCode::SUCCESS)
}

/// `noodle remove`.
#[derive(Debug, Clone, Args)]
pub struct RemoveArgs {
    /// Package name to remove.
    pub package: String,
    /// Project directory (walks up for `package.json`).
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
}

/// Run remove.
pub fn run_remove(args: &RemoveArgs) -> Result<ExitCode> {
    let project = NoodleProject::discover(&args.project_dir)?;
    let mut pm = open_pm(&project)?;
    // PM `remove_vendor_installs` clears nested `vendors/{registry}/{name}@{ver}`.
    let info = pm.remove_dependency_with_manifest(&args.package, false).into_diagnostic()?;
    save_pm_manifest(&project, &pm)?;
    sync_node_modules(&project)?;
    println!("已移除 {}", info.name);
    Ok(ExitCode::SUCCESS)
}

/// `noodle update`.
#[derive(Debug, Clone, Args)]
pub struct UpdateArgs {
    /// Package to update (all locked deps when omitted).
    pub package: Option<String>,
    /// Target version / tag (default: `latest`).
    #[arg(long, default_value = "latest")]
    pub version: String,
    /// Project directory (walks up for `package.json`).
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
}

/// Run update (PM preserves runtime vs dev bucket).
pub fn run_update(args: &UpdateArgs) -> Result<ExitCode> {
    let project = NoodleProject::discover(&args.project_dir)?;
    let registry = default_registry_for(&project);
    let mut pm = open_pm(&project)?;
    if let Some(package) = &args.package {
        let info = pm.update_one_with_manifest(package, &args.version, registry, false).into_diagnostic()?;
        save_pm_manifest(&project, &pm)?;
        sync_node_modules(&project)?;
        println!("已更新 {}@{}", info.name, info.version);
    }
    else {
        let updated = pm.update_all_with_manifest(registry, false).into_diagnostic()?;
        save_pm_manifest(&project, &pm)?;
        sync_node_modules(&project)?;
        println!("已更新 {} 个依赖", updated.len());
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::{Mutex, OnceLock},
    };

    use nyar_package_registry::{MockRegistry, Package};
    use serde_json::json;
    use tempfile::TempDir;

    use crate::node_modules::materialize_node_modules;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn with_isolated_home<T>(f: impl FnOnce() -> T) -> T {
        let _guard = env_lock().lock().expect("env lock");
        let home = TempDir::new().expect("temp home");
        let prev_noodle = std::env::var_os("NOODLE_HOME");
        unsafe {
            std::env::set_var("NOODLE_HOME", home.path());
        }
        let result = f();
        unsafe {
            match prev_noodle {
                Some(v) => std::env::set_var("NOODLE_HOME", v),
                None => std::env::remove_var("NOODLE_HOME"),
            }
        }
        result
    }

    #[test]
    fn mock_add_writes_package_json_vendors_and_noodle_lock() {
        with_isolated_home(|| {
            let dir = TempDir::new().unwrap();
            let pkg = json!({
                "name": "noodle-smoke",
                "version": "0.1.0",
                "noodle": { "compat": "npm" }
            });
            fs::write(dir.path().join("package.json"), format!("{}\n", serde_json::to_string_pretty(&pkg).unwrap())).unwrap();

            let mock = Arc::new(MockRegistry::new("npm", "https://mock.npm.local"));
            mock.insert_package(Package {
                name: "left-pad".into(),
                version: "1.3.0".into(),
                description: "smoke".into(),
                ..Package::default()
            });
            let mut registries: HashMap<String, Arc<dyn Registry>> = HashMap::new();
            registries.insert("npm".into(), mock);

            let project = NoodleProject::open(dir.path()).unwrap();
            let mut pm = open_pm_with_registries(&project, registries).unwrap();
            let info = pm.install_one("left-pad", "1.3.0", "npm", false).unwrap();
            let mut manifest = pm.mode.package_manifest().cloned().unwrap();
            manifest.add_dependency(&info.name, &info.version);
            save_dependencies(&project.root, &manifest).unwrap();

            let saved: serde_json::Value = serde_json::from_str(&fs::read_to_string(dir.path().join("package.json")).unwrap()).unwrap();
            assert_eq!(saved["dependencies"]["left-pad"], "1.3.0");
            assert!(dir.path().join("vendors").join("npm").join("left-pad@1.3.0").is_dir());
            assert!(dir.path().join("noodle-lock.von").is_file());

            let nm = materialize_node_modules(&project.root).unwrap();
            assert_eq!(nm.linked, 1);
            assert!(project.root.join("node_modules").join("left-pad").exists());

            pm.remove_dependency_with_manifest("left-pad", false).unwrap();
            assert!(!dir.path().join("vendors").join("npm").join("left-pad@1.3.0").exists());
            materialize_node_modules(&project.root).unwrap();
            assert!(!project.root.join("node_modules").join("left-pad").exists());
        });
    }

    #[test]
    fn mock_add_dev_uses_manifest_bucket_api() {
        with_isolated_home(|| {
            let dir = TempDir::new().unwrap();
            let pkg = json!({ "name": "noodle-dev", "version": "0.1.0" });
            fs::write(dir.path().join("package.json"), format!("{}\n", serde_json::to_string_pretty(&pkg).unwrap())).unwrap();

            let mock = Arc::new(MockRegistry::new("npm", "https://mock.npm.local"));
            mock.insert_package(Package {
                name: "typescript".into(),
                version: "5.4.0".into(),
                description: "dev".into(),
                ..Package::default()
            });
            let mut registries: HashMap<String, Arc<dyn Registry>> = HashMap::new();
            registries.insert("npm".into(), mock);

            let project = NoodleProject::open(dir.path()).unwrap();
            let mut pm = open_pm_with_registries(&project, registries).unwrap();
            let info = pm.install_one("typescript", "5.4.0", "npm", false).unwrap();
            let mut manifest = pm.mode.package_manifest().cloned().unwrap();
            manifest.add_dev_dependency(&info.name, &info.version);
            save_dependencies(&project.root, &manifest).unwrap();

            let saved: serde_json::Value = serde_json::from_str(&fs::read_to_string(dir.path().join("package.json")).unwrap()).unwrap();
            assert_eq!(saved["devDependencies"]["typescript"], "5.4.0");
            assert!(saved.get("dependencies").is_none());
        });
    }

    #[test]
    fn mock_install_dependencies_from_package_json() {
        with_isolated_home(|| {
            let dir = TempDir::new().unwrap();
            let pkg = json!({
                "name": "noodle-smoke-install",
                "version": "0.1.0",
                "dependencies": { "demo-dep": "2.0.0" }
            });
            fs::write(dir.path().join("package.json"), format!("{}\n", serde_json::to_string_pretty(&pkg).unwrap())).unwrap();

            let mock = Arc::new(MockRegistry::new("npm", "https://mock.npm.local"));
            mock.insert_package(Package { name: "demo-dep".into(), version: "2.0.0".into(), description: "dep".into(), ..Package::default() });
            let mut registries: HashMap<String, Arc<dyn Registry>> = HashMap::new();
            registries.insert("npm".into(), mock);

            let project = NoodleProject::open(dir.path()).unwrap();
            let mut pm = open_pm_with_registries(&project, registries).unwrap();
            let installed = pm.install_dependencies(false, "npm").unwrap();
            assert_eq!(installed.len(), 1);
            assert_eq!(installed[0].name, "demo-dep");
            assert!(dir.path().join("vendors").join("npm").join("demo-dep@2.0.0").is_dir());
            assert!(dir.path().join("noodle-lock.von").is_file());

            sync_node_modules(&project).unwrap();
            assert!(dir.path().join("node_modules").join("demo-dep").exists());
        });
    }

    #[test]
    fn mock_install_pnpm_compat_materializes_pnpm_like() {
        with_isolated_home(|| {
            let dir = TempDir::new().unwrap();
            let pkg = json!({
                "name": "noodle-pnpm-install",
                "version": "0.1.0",
                "packageManager": "pnpm@9.15.0",
                "dependencies": { "demo-dep": "2.0.0" }
            });
            fs::write(dir.path().join("package.json"), format!("{}\n", serde_json::to_string_pretty(&pkg).unwrap())).unwrap();

            let mock = Arc::new(MockRegistry::new("npm", "https://mock.npm.local"));
            mock.insert_package(Package { name: "demo-dep".into(), version: "2.0.0".into(), description: "dep".into(), ..Package::default() });
            let mut registries: HashMap<String, Arc<dyn Registry>> = HashMap::new();
            registries.insert("npm".into(), mock);

            let project = NoodleProject::open(dir.path()).unwrap();
            assert_eq!(project.compat, crate::project::PmCompat::Pnpm);
            let mut pm = open_pm_with_registries(&project, registries).unwrap();
            let installed = pm.install_dependencies(false, "npm").unwrap();
            assert_eq!(installed.len(), 1);

            sync_node_modules(&project).unwrap();
            let store = dir.path().join("node_modules").join(".pnpm").join("demo-dep@2.0.0").join("node_modules").join("demo-dep");
            assert!(store.exists());
            assert!(dir.path().join("node_modules").join("demo-dep").exists());
        });
    }
}
