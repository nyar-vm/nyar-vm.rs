//! `panda install` via `nyar-package-manager` (no uv/poetry/pip shell).

use std::{collections::HashMap, path::PathBuf, process::ExitCode, sync::Arc};

use clap::Args;
use miette::{IntoDiagnostic, Result, WrapErr};
use nyar_package_manager::{DependencyBucket, PackageManager, PackageManifest};
use nyar_package_registry::{Registry, default_registries};

use crate::{
    layout::project_layout,
    manifest::{load_package_manifest, save_dependencies},
    project::PandaProject,
};

fn product_registry(project: &PandaProject) -> &'static str {
    // Product-chosen opaque registry adapter id from nyar-package-registry.
    match project.compat {
        crate::project::PythonCompat::Uv | crate::project::PythonCompat::Poetry | crate::project::PythonCompat::Pip => "conda",
    }
}

fn open_pm(project: &PandaProject) -> Result<PackageManager> {
    let manifest = load_package_manifest(&project.root)?;
    let registries: HashMap<String, Arc<dyn Registry>> = default_registries().into_diagnostic()?;
    PackageManager::open_with_manifest_layout(&project.root, manifest, registries, project_layout())
        .into_diagnostic()
        .wrap_err("打开包管理器失败")
}

/// Persist a product-owned manifest through panda's native bridge (never VON).
fn save_native(project: &PandaProject, manifest: &PackageManifest) -> Result<()> {
    save_dependencies(&project.root, manifest)
}

/// Record a resolved package into the in-memory manifest (`write_manifest=false` path).
fn apply_added_package(manifest: &mut PackageManifest, name: &str, version: &str, dev: bool) {
    if dev {
        manifest.add_dev_dependency(name, version);
    }
    else {
        manifest.add_dependency(name, version);
    }
}

/// `panda install`.
#[derive(Debug, Clone, Args)]
pub struct InstallArgs {
    /// Project directory (walks upward for pyproject / requirements).
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
    /// Also install `[tool.panda] dev-dependencies`.
    #[arg(long, default_value_t = false)]
    pub dev: bool,
}

/// Run install.
pub fn run_install(args: &InstallArgs) -> Result<ExitCode> {
    let project = PandaProject::discover(&args.project_dir)?;
    let registry = product_registry(&project);
    println!("panda install · compat={} · registry={registry} · {}", project.compat.label(), project.root.display());
    let mut pm = open_pm(&project)?;
    let installed = pm.install_dependencies(args.dev, registry).into_diagnostic()?;
    println!("已安装 {} 个依赖", installed.len());
    for info in installed {
        println!("  - {}@{} ({})", info.name, info.version, info.registry);
    }
    // Honest layout: PM materializes under vendors/, not .venv/site-packages.
    println!("install layout: vendors/{{registry}}/{{name}}@{{version}} (not .venv)");
    println!("hint: panda run/test/build set PYTHONPATH for src/ + vendors/");
    Ok(ExitCode::SUCCESS)
}

/// `panda add`.
#[derive(Debug, Clone, Args)]
pub struct AddArgs {
    /// Package name to add.
    pub package: String,
    /// Version constraint or `latest`.
    #[arg(long, default_value = "latest")]
    pub version: String,
    /// Record under `[tool.panda] dev-dependencies` instead of runtime deps.
    #[arg(long, default_value_t = false)]
    pub dev: bool,
    /// Project directory.
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
}

/// Run add.
pub fn run_add(args: &AddArgs) -> Result<ExitCode> {
    let project = PandaProject::discover(&args.project_dir)?;
    let registry = product_registry(&project);
    let mut pm = open_pm(&project)?;
    // write_manifest=false: avoid PM writing package.von; panda owns pyproject/requirements.
    let info = pm.install_one(&args.package, &args.version, registry, false).into_diagnostic()?;
    if let Some(mut manifest) = pm.mode.package_manifest().cloned() {
        apply_added_package(&mut manifest, &info.name, &info.version, args.dev);
        save_native(&project, &manifest)?;
    }
    println!("已添加 {}@{}", info.name, info.version);
    Ok(ExitCode::SUCCESS)
}

/// `panda remove`.
#[derive(Debug, Clone, Args)]
pub struct RemoveArgs {
    /// Package name to remove.
    pub package: String,
    /// Project directory.
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
}

/// Run remove.
pub fn run_remove(args: &RemoveArgs) -> Result<ExitCode> {
    let project = PandaProject::discover(&args.project_dir)?;
    let mut pm = open_pm(&project)?;
    let info = pm.remove_dependency_with_manifest(&args.package, false).into_diagnostic()?;
    if let Some(manifest) = pm.mode.package_manifest() {
        save_native(&project, manifest)?;
    }
    println!("已移除 {}", info.name);
    Ok(ExitCode::SUCCESS)
}

/// `panda update`.
#[derive(Debug, Clone, Args)]
pub struct UpdateArgs {
    /// Optional single package; omit to update all locked deps.
    pub package: Option<String>,
    /// Target version when updating one package.
    #[arg(long, default_value = "latest")]
    pub version: String,
    /// Project directory.
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
}

/// Run update — PM `update_*_with_manifest` preserves [`DependencyBucket`]; panda only saves native.
pub fn run_update(args: &UpdateArgs) -> Result<ExitCode> {
    let project = PandaProject::discover(&args.project_dir)?;
    let registry = product_registry(&project);
    let mut pm = open_pm(&project)?;
    if let Some(package) = &args.package {
        let info = pm.update_one_with_manifest(package, &args.version, registry, false).into_diagnostic()?;
        if let Some(manifest) = pm.mode.package_manifest() {
            debug_assert!(matches!(manifest.dependency_bucket(&info.name), Some(DependencyBucket::Runtime | DependencyBucket::Dev) | None));
            save_native(&project, manifest)?;
        }
        println!("已更新 {}@{}", info.name, info.version);
    }
    else {
        let updated = pm.update_all_with_manifest(registry, false).into_diagnostic()?;
        if let Some(manifest) = pm.mode.package_manifest() {
            save_native(&project, manifest)?;
        }
        println!("已更新 {} 个依赖", updated.len());
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        sync::{Mutex, OnceLock},
    };

    use nyar_package_manager::{MockRegistry, Package, PackageManager, Registry};
    use tempfile::tempdir;

    fn empty_manifest() -> PackageManifest {
        PackageManifest {
            name: "demo".into(),
            version: "0.1.0".into(),
            description: None,
            homepage: None,
            author: None,
            license: None,
            dependencies: Default::default(),
            dev_dependencies: Default::default(),
            peer_dependencies: Default::default(),
            scripts: Default::default(),
            hooks: Default::default(),
            publish_config: Default::default(),
            publish: Vec::new(),
            files: Vec::new(),
        }
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn with_isolated_panda_home<T>(f: impl FnOnce() -> T) -> T {
        let _guard = env_lock().lock().expect("env lock");
        let home = tempdir().expect("temp home");
        let previous = std::env::var_os("PANDA_HOME");
        // SAFETY: guarded by process-wide mutex for these tests.
        unsafe {
            std::env::set_var("PANDA_HOME", home.path());
        }
        let result = f();
        unsafe {
            match previous {
                Some(value) => std::env::set_var("PANDA_HOME", value),
                None => std::env::remove_var("PANDA_HOME"),
            }
        }
        result
    }

    #[test]
    fn apply_added_package_uses_dependency_bucket() {
        let mut manifest = empty_manifest();
        apply_added_package(&mut manifest, "rich", "13.0.0", false);
        assert_eq!(manifest.dependency_bucket("rich"), Some(DependencyBucket::Runtime));
        apply_added_package(&mut manifest, "rich", "13.1.0", true);
        assert_eq!(manifest.dependency_bucket("rich"), Some(DependencyBucket::Dev));
        assert_eq!(manifest.dev_dependencies.get("rich").and_then(|s| s.version_constraint()), Some("13.1.0"));
    }

    #[test]
    fn upsert_preserving_bucket_keeps_dev() {
        let mut manifest = empty_manifest();
        manifest.add_dev_dependency("mypy", "1.0");
        manifest.upsert_dependency_preserving_bucket("mypy", "1.1.0");
        assert_eq!(manifest.dependency_bucket("mypy"), Some(DependencyBucket::Dev));
        assert_eq!(manifest.dev_dependencies.get("mypy").and_then(|s| s.version_constraint()), Some("1.1.0"));
    }

    #[test]
    fn mock_registry_install_uses_panda_layout_and_native_save() {
        with_isolated_panda_home(|| {
            let root = tempdir().expect("project");
            std::fs::write(
                root.path().join("pyproject.toml"),
                "[project]\nname = \"demo\"\nversion = \"0.1.0\"\ndependencies = []\n\n[tool.panda]\nmanager = \"pip\"\ndev-dependencies = []\n",
            )
            .expect("write pyproject");

            let mock = Arc::new(MockRegistry::new("conda", "https://mock.conda.local"));
            mock.insert_package(Package {
                name: "rich".into(),
                version: "13.7.0".into(),
                description: "mock rich".into(),
                ..Package::default()
            });
            let mut registries: HashMap<String, Arc<dyn Registry>> = HashMap::new();
            registries.insert("conda".into(), mock);

            let manifest = load_package_manifest(root.path()).expect("load");
            let mut pm = PackageManager::open_with_manifest_layout(root.path(), manifest, registries, project_layout()).expect("open");
            assert_eq!(pm.layout.lockfile, "panda-lock.von");
            let info = pm.install_one("rich", "latest", "conda", false).expect("install");
            let mut saved = pm.mode.package_manifest().cloned().expect("manifest");
            apply_added_package(&mut saved, &info.name, &info.version, false);
            save_dependencies(root.path(), &saved).expect("save");

            let text = std::fs::read_to_string(root.path().join("pyproject.toml")).expect("read");
            assert!(text.contains("rich==13.7.0") || text.contains("\"rich==13.7.0\""));
            assert!(root.path().join("panda-lock.von").is_file());
            assert!(!root.path().join("package.von").exists());
            assert!(!root.path().join("legion.von").exists());
            assert!(!root.path().join("package-lock.von").exists());
            let vendor = root.path().join("vendors").join("conda").join("rich@13.7.0");
            assert!(vendor.is_dir());
            let paths = crate::python_path::python_import_paths(root.path());
            assert!(paths.iter().any(|p| p == &vendor));
        });
    }
}
