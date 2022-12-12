use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
};

use nyar_package_registry::{Package, PublishOptions, PublishResult, Registry, TokenVerifyResult};

use crate::{
    DependencyBucket, DependencyResolver, LockEntry, LockFile, PackageCache, PackageManagerError, PackageManifest, PackagePublisher,
    ProjectLayout, ProjectMode, RegistrySourceManager, Result, ScriptResult, ScriptRunner, SecurityAudit, SecurityAuditResult, VendorManager,
    WorkspaceManifest, is_registry_publish_type, resolve_publish_artifact, version::satisfies_version_constraint,
};

/// Installed / queried package summary.
#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub registry: String,
    pub install_path: Option<String>,
}

impl PackageInfo {
    pub fn from_package(package: &Package, registry: &str, install_path: Option<String>) -> Self {
        Self {
            name: package.name.clone(),
            version: package.version.clone(),
            description: package.description.clone(),
            registry: registry.to_string(),
            install_path,
        }
    }
}

/// Package manager facade (install / publish / auth).
pub struct PackageManager {
    pub mode: ProjectMode,
    /// Product-supplied on-disk layout (manifest / ignore filenames).
    pub layout: ProjectLayout,
    registries: HashMap<String, Arc<dyn Registry>>,
    lock_file: LockFile,
    cache: PackageCache,
    publisher: PackagePublisher,
    vendor: VendorManager,
    source_manager: RegistrySourceManager,
    pub frozen_lockfile: bool,
    pub offline: bool,
}

impl PackageManager {
    /// Open a project using the product's [`ProjectLayout`].
    pub fn open(directory: impl AsRef<Path>, layout: ProjectLayout) -> Result<Self> {
        let mode = ProjectMode::discover(directory, layout)?;
        let source_manager = RegistrySourceManager::open_with_layout(layout)?;
        let registries = source_manager.build_registries()?;
        let lock_file = LockFile::load(mode.root(), layout)?;
        let cache = PackageCache::open(PackageCache::root_for(layout))?;
        let publisher = PackagePublisher::new(registries.clone(), layout);
        let vendor = VendorManager::open_with_layout(layout)?;
        Ok(Self { mode, layout, registries, lock_file, cache, publisher, vendor, source_manager, frozen_lockfile: false, offline: false })
    }

    pub fn open_with_registries(
        directory: impl AsRef<Path>,
        registries: HashMap<String, Arc<dyn Registry>>,
        layout: ProjectLayout,
    ) -> Result<Self> {
        let mode = ProjectMode::discover(directory, layout)?;
        let lock_file = LockFile::load(mode.root(), layout)?;
        let cache = PackageCache::open(PackageCache::root_for(layout))?;
        let publisher = PackagePublisher::new(registries.clone(), layout);
        let vendor = VendorManager::open_with_layout(layout)?;
        let source_manager = RegistrySourceManager::open_with_layout(layout)?;
        Ok(Self { mode, layout, registries, lock_file, cache, publisher, vendor, source_manager, frozen_lockfile: false, offline: false })
    }

    /// Open with an in-memory package manifest (product CLIs translate their native manifests).
    ///
    /// Uses [`ProjectLayout::neutral`] for lockfile / home paths. Prefer
    /// [`Self::open_with_manifest_layout`] when the product has its own layout.
    pub fn open_with_manifest(
        directory: impl AsRef<Path>,
        manifest: PackageManifest,
        registries: HashMap<String, Arc<dyn Registry>>,
    ) -> Result<Self> {
        Self::open_with_manifest_layout(directory, manifest, registries, ProjectLayout::neutral())
    }

    /// Open with an in-memory manifest and a product-supplied [`ProjectLayout`].
    pub fn open_with_manifest_layout(
        directory: impl AsRef<Path>,
        manifest: PackageManifest,
        registries: HashMap<String, Arc<dyn Registry>>,
        layout: ProjectLayout,
    ) -> Result<Self> {
        let root = directory.as_ref().to_path_buf();
        let mode = ProjectMode::Package { root, manifest };
        let lock_file = LockFile::load(mode.root(), layout)?;
        let cache = PackageCache::open(PackageCache::root_for(layout))?;
        let publisher = PackagePublisher::new(registries.clone(), layout);
        let vendor = VendorManager::open_with_layout(layout)?;
        let source_manager = RegistrySourceManager::open_with_layout(layout)?;
        Ok(Self { mode, layout, registries, lock_file, cache, publisher, vendor, source_manager, frozen_lockfile: false, offline: false })
    }

    pub fn register_registry(&mut self, registry: Arc<dyn Registry>) {
        self.registries.insert(registry.name().to_string(), registry.clone());
        self.publisher = PackagePublisher::new(self.registries.clone(), self.layout);
    }

    fn rebuild_registries(&mut self) -> Result<()> {
        self.registries = self.source_manager.build_registries()?;
        self.publisher = PackagePublisher::new(self.registries.clone(), self.layout);
        Ok(())
    }

    pub fn base_directory(&self) -> &Path {
        self.mode.root()
    }

    pub fn vendors_directory(&self) -> PathBuf {
        self.mode.root().join("vendors")
    }

    pub fn is_workspace(&self) -> bool {
        matches!(self.mode, ProjectMode::Workspace { .. })
    }

    pub fn has_manifest(&self) -> bool {
        self.mode.package_manifest().is_some()
    }

    fn registry(&self, name: &str) -> Result<&Arc<dyn Registry>> {
        self.registries.get(name).ok_or_else(|| PackageManagerError::message(format!("注册器 {name} 未找到")))
    }

    pub fn search(&self, query: &str, registry_name: &str) -> Result<Vec<PackageInfo>> {
        let registry = self.registry(registry_name)?;
        Ok(registry.search_packages(query)?.into_iter().map(|package| PackageInfo::from_package(&package, registry_name, None)).collect())
    }

    pub fn get_package_info(&self, package_name: &str, registry_name: &str) -> Result<PackageInfo> {
        let registry = self.registry(registry_name)?;
        let package = registry.get_package(package_name, "latest")?;
        Ok(PackageInfo::from_package(&package, registry_name, None))
    }

    pub fn install_one(&mut self, package_name: &str, version: &str, registry_name: &str, write_manifest: bool) -> Result<PackageInfo> {
        if version.starts_with("workspace:") || version == "workspace" {
            return self.install_workspace_package(package_name);
        }
        if self.offline {
            return self.install_one_offline(package_name, version, registry_name, write_manifest);
        }

        if self.frozen_lockfile {
            let key = LockFile::package_key(package_name, version);
            if version != "latest" && !self.lock_file.packages.contains_key(&key) {
                return Err(PackageManagerError::message(format!(
                    "frozen lockfile: missing {package_name}@{version}. run install without --frozen-lockfile"
                )));
            }
        }

        let registry = self.registry(registry_name)?.clone();
        let package = registry.get_package(package_name, version)?;
        println!("正在安装 {}@{}（来源: {}）", package.name, package.version, registry_name);

        let package_path = self.package_install_path(registry_name, &package.name, &package.version);
        std::fs::create_dir_all(&package_path)?;
        match registry.download_package(&package, &package_path) {
            Ok(extracted) => println!("下载完成：{extracted}"),
            Err(error) => println!("下载失败：{error}，仅记录元数据"),
        }

        self.cache.add_package(&package.name, &package.version, &package_path)?;
        self.lock_file.add_package(&package, registry_name, registry.endpoint());
        self.lock_file.save()?;

        if write_manifest {
            self.write_manifest_dependency(&package.name, &package.version, DependencyBucket::Runtime)?;
        }

        println!("已安装到: {}", package_path.display());
        Ok(PackageInfo::from_package(&package, registry_name, Some(package_path.display().to_string())))
    }

    pub fn install_dependencies(&mut self, include_dev: bool, default_registry: &str) -> Result<Vec<PackageInfo>> {
        match &self.mode {
            ProjectMode::Workspace { workspace, root, .. } => {
                println!("[Workspace 模式] 安装所有工作区成员的依赖");
                let mut installed = Vec::new();
                for member in workspace.enumerate_member_dirs(root, self.layout)? {
                    let mut member_pm = PackageManager::open_with_registries(&member, self.registries.clone(), self.layout)?;
                    member_pm.frozen_lockfile = self.frozen_lockfile;
                    member_pm.offline = self.offline;
                    installed.extend(member_pm.install_current_manifest(include_dev, default_registry)?);
                }
                Ok(installed)
            }
            ProjectMode::Package { .. } => self.install_current_manifest(include_dev, default_registry),
            ProjectMode::Standalone { .. } => Err(PackageManagerError::message("当前目录不是包或工作区，无法安装依赖")),
        }
    }

    fn install_current_manifest(&mut self, include_dev: bool, default_registry: &str) -> Result<Vec<PackageInfo>> {
        let manifest = self.mode.expect_package_manifest()?.clone();
        if self.offline {
            return self.install_current_manifest_offline(manifest, include_dev);
        }
        let mut deps = manifest.dependencies.clone();
        if include_dev {
            deps.extend(manifest.dev_dependencies.clone());
        }
        let resolver = DependencyResolver::new(self.registries.clone());
        let nodes = resolver.resolve_all(&deps, default_registry)?;
        let mut installed = Vec::new();
        for node in nodes {
            installed.push(self.install_one(&node.name, &node.version, &node.registry, false)?);
        }
        for (name, spec) in &manifest.dependencies {
            if spec.is_workspace() {
                installed.push(self.install_workspace_package(name)?);
            }
        }
        self.validate_peer_dependencies(&manifest, &installed);
        Ok(installed)
    }

    fn install_current_manifest_offline(&mut self, manifest: PackageManifest, include_dev: bool) -> Result<Vec<PackageInfo>> {
        let mut deps = manifest.dependencies.clone();
        if include_dev {
            deps.extend(manifest.dev_dependencies.clone());
        }

        let mut installed = Vec::new();
        for (name, spec) in deps {
            if spec.is_workspace() {
                installed.push(self.install_workspace_package(&name)?);
                continue;
            }

            let requested = spec.version_constraint().unwrap_or("latest");
            let lock = self.find_locked_entry(&name, requested).ok_or_else(|| {
                PackageManagerError::message(format!(
                    "offline install failed: missing lock entry for {name}@{requested}; run online install first"
                ))
            })?;
            installed.push(self.install_one_offline(&name, &lock.version, &lock.registry, false)?);
        }

        self.validate_peer_dependencies(&manifest, &installed);
        Ok(installed)
    }

    fn install_workspace_package(&mut self, package_name: &str) -> Result<PackageInfo> {
        let members = match &self.mode {
            ProjectMode::Workspace { workspace, root, .. } => workspace.member_packages(root, self.layout)?,
            _ => BTreeMap::new(),
        };
        let member_path =
            members.get(package_name).cloned().ok_or_else(|| PackageManagerError::message(format!("工作区中未找到成员包 '{package_name}'")))?;
        let member_manifest = PackageManifest::load(&member_path, self.layout)?;
        let link_path = self.vendors_directory().join(package_name);
        if link_path.exists() {
            let _ = std::fs::remove_dir_all(&link_path);
        }
        if let Some(parent) = link_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        copy_dir(&member_path, &link_path)?;
        println!("已复制工作区成员：{} → {}", member_path.display(), link_path.display());

        let key = LockFile::package_key(package_name, &member_manifest.version);
        self.lock_file.packages.insert(
            key,
            crate::LockEntry {
                name: package_name.to_string(),
                version: member_manifest.version.clone(),
                registry: "workspace".to_string(),
                resolved: member_path.display().to_string(),
                integrity: String::new(),
                license: member_manifest.license.clone().unwrap_or_default(),
                is_dev: false,
                is_workspace: true,
                install_path: package_name.to_string(),
                dependencies: member_manifest.dependencies.keys().cloned().collect(),
            },
        );
        self.lock_file.save()?;
        Ok(PackageInfo {
            name: member_manifest.name,
            version: member_manifest.version,
            description: member_manifest.description.unwrap_or_default(),
            registry: "workspace".to_string(),
            install_path: Some(link_path.display().to_string()),
        })
    }

    pub fn add_dependency(&mut self, package_name: &str, version: &str, registry_name: &str) -> Result<PackageInfo> {
        self.install_one(package_name, version, registry_name, true)
    }

    /// Install and record under `dev_dependencies` (product-agnostic).
    pub fn add_dev_dependency(&mut self, package_name: &str, version: &str, registry_name: &str) -> Result<PackageInfo> {
        let info = self.install_one(package_name, version, registry_name, false)?;
        self.write_manifest_dependency(&info.name, &info.version, DependencyBucket::Dev)?;
        Ok(info)
    }

    pub fn remove_dependency(&mut self, package_name: &str) -> Result<PackageInfo> {
        self.remove_dependency_with_manifest(package_name, true)
    }

    /// Remove a dependency while allowing a product CLI to own native-manifest persistence.
    pub fn remove_dependency_with_manifest(&mut self, package_name: &str, write_manifest: bool) -> Result<PackageInfo> {
        self.remove_vendor_installs(package_name);
        self.lock_file.remove_package(package_name);
        self.lock_file.save()?;
        let root = self.mode.root().to_path_buf();
        let layout = self.layout;
        let version = if let Some(manifest) = self.mutable_manifest()? {
            let version = manifest
                .dependencies
                .get(package_name)
                .or_else(|| manifest.dev_dependencies.get(package_name))
                .and_then(|spec| spec.version_constraint())
                .unwrap_or("removed")
                .to_string();
            manifest.remove_dependency(package_name);
            if write_manifest {
                manifest.save(&root, layout)?;
                self.reload_mode()?;
            }
            version
        }
        else {
            "removed".to_string()
        };
        Ok(PackageInfo { name: package_name.to_string(), version, description: String::new(), registry: String::new(), install_path: None })
    }

    pub fn update_one(&mut self, package_name: &str, version: &str, registry_name: &str) -> Result<PackageInfo> {
        self.update_one_with_manifest(package_name, version, registry_name, true)
    }

    /// Update one dependency without assuming a package-manifest file format.
    ///
    /// Writes the new version back into the same dependency bucket it already occupied
    /// (`dev_dependencies` vs `dependencies`); defaults to runtime when the name is new.
    pub fn update_one_with_manifest(
        &mut self,
        package_name: &str,
        version: &str,
        registry_name: &str,
        write_manifest: bool,
    ) -> Result<PackageInfo> {
        let prior_bucket = self.mode.package_manifest().and_then(|manifest| manifest.dependency_bucket(package_name));
        self.remove_vendor_installs(package_name);
        self.lock_file.remove_package(package_name);
        let info = self.install_one(package_name, version, registry_name, false)?;
        let root = self.mode.root().to_path_buf();
        let layout = self.layout;
        if let Some(manifest) = self.mutable_manifest()? {
            let bucket = prior_bucket
                .or_else(|| manifest.dependency_bucket(&info.name))
                .or_else(|| manifest.dependency_bucket(package_name))
                .unwrap_or(DependencyBucket::Runtime);
            manifest.set_dependency(&info.name, &info.version, bucket);
            if write_manifest {
                manifest.save(&root, layout)?;
                self.reload_mode()?;
            }
        }
        Ok(info)
    }

    pub fn update_all(&mut self, default_registry: &str) -> Result<Vec<PackageInfo>> {
        self.update_all_with_manifest(default_registry, true)
    }

    /// Update all locked dependencies without assuming a package-manifest file format.
    pub fn update_all_with_manifest(&mut self, default_registry: &str, write_manifest: bool) -> Result<Vec<PackageInfo>> {
        let jobs: Vec<(String, String)> = self
            .lock_file
            .packages
            .values()
            .map(|entry| {
                let registry = if entry.registry.is_empty() { default_registry.to_string() } else { entry.registry.clone() };
                (entry.name.clone(), registry)
            })
            .collect();
        let mut updated = Vec::new();
        for (name, registry) in jobs {
            updated.push(self.update_one_with_manifest(&name, "latest", &registry, write_manifest)?);
        }
        Ok(updated)
    }

    pub fn publish(&mut self, options: PublishOptions, workspace: bool) -> Result<PublishResult> {
        let jobs = self.expand_publish_jobs(&options, workspace)?;
        let mut last_success = None;
        let mut failures = Vec::new();

        for job in jobs {
            let mut package_options = options.clone();
            package_options.package_name = job.package_id.clone();
            package_options.version = job.version.clone();
            package_options.registry_name = job.registry.clone();
            if package_options.description.is_empty() {
                package_options.description = job.description.clone();
            }
            package_options.homepage = package_options.homepage.or(job.homepage.clone());
            package_options.license = package_options.license.or(job.license.clone());
            package_options.package_path = job.package_path.display().to_string();
            if package_options.tag.is_none() {
                package_options.tag = Some(job.tag.clone());
            }
            if package_options.access.is_none() {
                package_options.access = Some(job.access.clone());
            }

            let artifact = if !job.build_target.is_empty() && is_registry_publish_type(&job.registry) {
                Some(resolve_publish_artifact(job.package_path.as_path(), &job.build_target, &job.registry)?)
            }
            else {
                None
            };
            if let Some(artifact) = &artifact {
                package_options.artifact_dir = Some(artifact.artifact_dir.display().to_string());
            }
            package_options.include_files = job.manifest.files.clone();
            package_options.flat_layout = job.registry.eq_ignore_ascii_case("jsr");

            if package_options.auth_token.is_none() {
                let endpoint = self.registries.get(&package_options.registry_name).map(|registry| registry.endpoint().to_string());
                if let Some(credential) =
                    self.vendor.auth.discover_credential(&package_options.registry_name, endpoint.as_deref(), Some(job.package_path.as_path()))
                {
                    println!("使用凭据：{}", credential.source);
                    package_options.auth_token = Some(credential.token);
                }
            }

            let target_label = artifact
                .as_ref()
                .map(|value| format!(" ({})", value.canonical_label))
                .or_else(|| if job.build_target.is_empty() { None } else { Some(format!(" ({})", job.build_target)) })
                .unwrap_or_default();
            println!("发布 {}@{} → {}{target_label}", package_options.package_name, package_options.version, package_options.registry_name);

            self.run_hook_if_present(&job.manifest, job.package_path.as_path(), "pre_publish")?;
            let result = self.publisher.publish(package_options)?;
            self.run_hook_if_present(&job.manifest, job.package_path.as_path(), "post_publish")?;

            if result.success || result.dry_run {
                last_success = Some(result);
            }
            else if result.official_tool_required {
                failures.push(format!("{}@{} → {}：需要官方工具\n{}", job.package_id, job.version, job.registry, result.message));
            }
            else {
                failures.push(format!("{}@{} → {}：{}", job.package_id, job.version, job.registry, result.message));
            }
        }

        if !failures.is_empty() {
            return Err(PackageManagerError::message(format!("部分发布失败：\n{}", failures.join("\n"))));
        }

        last_success.ok_or_else(|| PackageManagerError::message("没有可发布的包"))
    }

    /// Expand manifest `publish: [...]` registry targets (or fall back to package identity).
    fn expand_publish_jobs(&self, options: &PublishOptions, workspace: bool) -> Result<Vec<PublishJob>> {
        let packages = self.publish_targets(workspace)?;
        let workspace_version = self.workspace_version();
        let mut jobs = Vec::new();

        for (path, manifest) in packages {
            let registry_targets: Vec<_> = manifest.publish.iter().filter(|target| target.is_registry_target()).cloned().collect();

            if registry_targets.is_empty() {
                let registry = resolve_legacy_registry(options, &manifest);
                let version = resolve_publish_version("", &manifest.version, workspace_version.as_deref())?;
                jobs.push(PublishJob {
                    package_path: path,
                    manifest: manifest.clone(),
                    package_id: if options.package_name.is_empty() { manifest.name.clone() } else { options.package_name.clone() },
                    version: if options.version.is_empty() { version } else { options.version.clone() },
                    registry,
                    build_target: String::new(),
                    description: manifest.description.clone().unwrap_or_default(),
                    homepage: manifest.homepage.clone(),
                    license: manifest.license.clone(),
                    tag: manifest.publish_config.tag.clone(),
                    access: manifest.publish_config.access.clone(),
                });
                continue;
            }

            for target in registry_targets {
                if !options.registry_name.is_empty() && !options.registry_name.eq_ignore_ascii_case(target.format_type.trim()) {
                    continue;
                }
                let package_id = target.package_id.trim();
                if package_id.is_empty() {
                    return Err(PackageManagerError::message(format!(
                        "publish 目标 type={} target={} 缺少 package_id",
                        target.format_type, target.target
                    )));
                }
                let version = resolve_publish_version(&target.version, &manifest.version, workspace_version.as_deref())?;
                jobs.push(PublishJob {
                    package_path: path.clone(),
                    manifest: manifest.clone(),
                    package_id: package_id.to_string(),
                    version,
                    registry: target.format_type.trim().to_ascii_lowercase(),
                    build_target: target.target.clone(),
                    description: manifest.description.clone().unwrap_or_default(),
                    homepage: manifest.homepage.clone(),
                    license: manifest.license.clone(),
                    tag: manifest.publish_config.tag.clone(),
                    access: manifest.publish_config.access.clone(),
                });
            }
        }

        if jobs.is_empty() {
            return Err(PackageManagerError::message(
                "没有匹配的 publish 目标；请在清单的 publish 数组中配置 type/package_id/version，或移除 --registry 过滤",
            ));
        }
        Ok(jobs)
    }

    fn workspace_version(&self) -> Option<String> {
        match &self.mode {
            ProjectMode::Workspace { workspace, .. } => workspace.version.clone().filter(|value| !value.is_empty() && value != "workspace"),
            _ => None,
        }
    }

    fn publish_targets(&self, workspace_all: bool) -> Result<Vec<(PathBuf, PackageManifest)>> {
        match &self.mode {
            ProjectMode::Workspace { workspace, root, manifest } => {
                if workspace_all {
                    println!("[Workspace 模式] 发布所有工作区成员");
                    let mut targets = Vec::new();
                    for member in workspace.enumerate_member_dirs(root, self.layout)? {
                        targets.push((member.clone(), PackageManifest::load(&member, self.layout)?));
                    }
                    if targets.is_empty() {
                        return Err(PackageManagerError::message("工作区没有可发布的成员包"));
                    }
                    return Ok(targets);
                }
                if let Some(manifest) = manifest {
                    println!("[Workspace 模式] 发布当前包");
                    return Ok(vec![(root.clone(), manifest.clone())]);
                }
                Err(PackageManagerError::message("工作区根目录没有包清单；请进入成员目录或使用 --workspace"))
            }
            ProjectMode::Package { root, manifest } => {
                println!("[Package 模式] 发布当前包");
                Ok(vec![(root.clone(), manifest.clone())])
            }
            ProjectMode::Standalone { .. } => Err(PackageManagerError::message("当前目录不是包或工作区，无法发布")),
        }
    }

    pub fn login(&mut self, registry_name: Option<&str>, token: Option<&str>) -> Result<crate::LoginResult> {
        let registry_name = self.resolve_login_registry(registry_name);
        let registry = self.registry(&registry_name)?.clone();
        self.vendor.login(registry.as_ref(), token, Some(self.mode.root()), true)
    }

    pub fn logout(&mut self, registry_name: Option<&str>) -> Result<bool> {
        let registry_name = self.resolve_login_registry(registry_name);
        self.vendor.logout(&registry_name)
    }

    pub fn whoami(&self, registry_name: Option<&str>) -> Result<TokenVerifyResult> {
        let registry_name = self.resolve_login_registry(registry_name);
        let registry = self.registry(&registry_name)?;
        self.vendor.whoami(registry.as_ref())
    }

    /// Resolved registry name for display (login default).
    pub fn login_registry_name(&self, registry_name: Option<&str>) -> String {
        self.resolve_login_registry(registry_name)
    }

    pub fn vendor_login(&mut self, registry_name: &str, token: Option<&str>) -> Result<crate::LoginResult> {
        let registry = self.registry(registry_name)?.clone();
        self.vendor.login(registry.as_ref(), token, Some(self.mode.root()), true)
    }

    pub fn vendor_logout(&mut self, registry_name: &str) -> Result<bool> {
        self.vendor.logout(registry_name)
    }

    pub fn vendor_whoami(&self, registry_name: &str) -> Result<TokenVerifyResult> {
        let registry = self.registry(registry_name)?;
        self.vendor.whoami(registry.as_ref())
    }

    pub fn vendor_list(&self) -> Vec<String> {
        self.vendor.list()
    }

    /// Audit lockfile packages (OSV vulnerabilities + license policy).
    pub fn audit(&self, offline: bool) -> Result<SecurityAuditResult> {
        let mut audit = SecurityAudit::open_with_layout(self.layout)?;
        if offline {
            audit = audit.offline();
        }
        let packages: Vec<(Package, String)> = self
            .lock_file
            .packages
            .values()
            .map(|entry| {
                (
                    Package { name: entry.name.clone(), version: entry.version.clone(), license: entry.license.clone(), ..Package::default() },
                    entry.registry.clone(),
                )
            })
            .collect();
        audit.audit_dependencies(&packages)
    }

    pub fn registry_list(&self) -> Vec<(String, String)> {
        self.source_manager.list()
    }

    pub fn registry_add(&mut self, name: &str, endpoint: &str) -> Result<()> {
        self.source_manager.add(name, endpoint)?;
        self.rebuild_registries()
    }

    pub fn registry_remove(&mut self, name: &str) -> Result<bool> {
        let removed = self.source_manager.remove(name)?;
        if removed {
            self.rebuild_registries()?;
        }
        Ok(removed)
    }

    pub fn registry_info(&self, name: &str) -> Result<(String, String, bool)> {
        self.source_manager.info(name)
    }

    pub fn registry_sources_path(&self) -> &Path {
        self.source_manager.path()
    }

    pub fn run_script(&self, script_name: &str) -> Result<ScriptResult> {
        let manifest = self.mode.expect_package_manifest()?;
        let command = manifest.scripts.get(script_name).ok_or_else(|| PackageManagerError::message(format!("脚本 `{script_name}` 不存在")))?;
        ScriptRunner::new(self.mode.root()).run(script_name, command)
    }

    fn run_hook_if_present(&self, manifest: &PackageManifest, path: &Path, hook: &str) -> Result<()> {
        let command = manifest.hooks.get(hook).cloned().or_else(|| manifest.hooks.get(&hook.replace('_', "")).cloned()).or_else(|| {
            let camel = hook
                .split('_')
                .enumerate()
                .map(|(index, part)| {
                    if index == 0 {
                        part.to_string()
                    }
                    else {
                        let mut chars = part.chars();
                        match chars.next() {
                            Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                            None => String::new(),
                        }
                    }
                })
                .collect::<String>();
            manifest.hooks.get(&camel).cloned().or_else(|| manifest.scripts.get(&camel).cloned())
        });
        let Some(command) = command
        else {
            return Ok(());
        };
        let result = ScriptRunner::new(path).run(hook, &command)?;
        if !result.success {
            return Err(PackageManagerError::message(format!("hook `{hook}` failed: {}", result.stderr)));
        }
        Ok(())
    }

    pub fn clear_cache(&mut self) -> Result<()> {
        self.cache.clear()
    }

    fn find_locked_entry(&self, name: &str, requested: &str) -> Option<LockEntry> {
        if requested != "latest" {
            if let Some(entry) = self.lock_file.packages.get(&LockFile::package_key(name, requested)) {
                return Some(entry.clone());
            }
        }
        self.lock_file.packages.values().filter(|entry| entry.name == name).max_by(|left, right| left.version.cmp(&right.version)).cloned()
    }

    fn install_one_offline(&mut self, package_name: &str, version: &str, registry_name: &str, write_manifest: bool) -> Result<PackageInfo> {
        if version.starts_with("workspace:") || version == "workspace" {
            return self.install_workspace_package(package_name);
        }

        let package_path = self.package_install_path(registry_name, package_name, version);
        if !package_path.is_dir() {
            if let Some(cached_path) = self.cache.package_path(package_name, version) {
                if let Some(parent) = package_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                copy_dir(&cached_path, &package_path)?;
            }
            else if let Some(entry) = self.find_locked_entry(package_name, version) {
                if entry.is_workspace {
                    return self.install_workspace_package(package_name);
                }
                let resolved = PathBuf::from(&entry.resolved);
                if resolved.is_dir() {
                    if let Some(parent) = package_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    copy_dir(&resolved, &package_path)?;
                }
                else {
                    return Err(PackageManagerError::message(format!(
                        "offline install failed: missing cached package {package_name}@{version}"
                    )));
                }
            }
            else {
                return Err(PackageManagerError::message(format!("offline install failed: missing cached package {package_name}@{version}")));
            }
        }
        println!("offline install: {package_name}@{version} -> {}", package_path.display());
        if write_manifest {
            self.write_manifest_dependency(package_name, version, DependencyBucket::Runtime)?;
        }
        Ok(PackageInfo {
            name: package_name.to_string(),
            version: version.to_string(),
            description: String::new(),
            registry: registry_name.to_string(),
            install_path: Some(package_path.display().to_string()),
        })
    }

    fn validate_peer_dependencies(&self, manifest: &PackageManifest, installed: &[PackageInfo]) {
        if manifest.peer_dependencies.is_empty() {
            return;
        }

        let mut present: BTreeMap<String, String> = BTreeMap::new();
        for info in installed {
            present.insert(info.name.clone(), info.version.clone());
        }
        for entry in self.lock_file.packages.values() {
            present.entry(entry.name.clone()).or_insert_with(|| entry.version.clone());
        }

        for (peer_name, spec) in &manifest.peer_dependencies {
            if spec.is_workspace() {
                continue;
            }
            let constraint = spec.version_constraint().unwrap_or("*");
            if let Some(version) = present.get(peer_name) {
                if satisfies_version_constraint(version, constraint) {
                    println!("peer dependency ok: {peer_name}@{version} satisfies {constraint}");
                }
                else {
                    eprintln!("peer dependency warning: {peer_name}@{version} does not satisfy {constraint}");
                }
                continue;
            }

            let vendor_path = self.vendors_directory().join(peer_name);
            if vendor_path.is_dir() {
                println!("peer dependency ok: {peer_name} (vendored)");
            }
            else {
                eprintln!("peer dependency warning: missing peer {peer_name} ({constraint})");
            }
        }
    }

    fn package_install_path(&self, registry_name: &str, name: &str, version: &str) -> PathBuf {
        self.vendors_directory().join(format!("{registry_name}/{name}@{version}"))
    }

    /// Delete vendor dirs for `package_name`: registry layout `vendors/{registry}/{name}@{version}`,
    /// plus legacy / workspace flat `vendors/{name}` so remove matches install without breaking older trees.
    fn remove_vendor_installs(&self, package_name: &str) {
        let vendors = self.vendors_directory();
        let mut paths: Vec<PathBuf> = self
            .lock_file
            .packages
            .values()
            .filter(|entry| entry.name == package_name)
            .map(|entry| {
                if entry.is_workspace || entry.registry == "workspace" {
                    vendors.join(package_name)
                }
                else if !entry.install_path.is_empty()
                    && (entry.install_path.contains('/') || entry.install_path.contains('\\') || entry.install_path.contains('@'))
                {
                    vendors.join(&entry.install_path)
                }
                else if !entry.registry.is_empty() {
                    self.package_install_path(&entry.registry, &entry.name, &entry.version)
                }
                else {
                    vendors.join(package_name)
                }
            })
            .collect();
        // Always try the flat name (workspace members + pre-layout installs).
        paths.push(vendors.join(package_name));
        paths.sort();
        paths.dedup();
        for path in paths {
            if path.exists() {
                let _ = std::fs::remove_dir_all(&path);
            }
        }
    }

    fn write_manifest_dependency(&mut self, name: &str, version: &str, bucket: DependencyBucket) -> Result<()> {
        let root = self.mode.root().to_path_buf();
        let layout = self.layout;
        if let Some(manifest) = self.mutable_manifest()? {
            manifest.set_dependency(name, version, bucket);
            manifest.save(&root, layout)?;
            self.reload_mode()?;
        }
        Ok(())
    }

    fn mutable_manifest(&mut self) -> Result<Option<&mut PackageManifest>> {
        Ok(match &mut self.mode {
            ProjectMode::Workspace { manifest, .. } => manifest.as_mut(),
            ProjectMode::Package { manifest, .. } => Some(manifest),
            ProjectMode::Standalone { .. } => None,
        })
    }

    fn reload_mode(&mut self) -> Result<()> {
        self.mode = ProjectMode::discover(self.mode.root(), self.layout)?;
        Ok(())
    }

    fn resolve_login_registry(&self, registry_name: Option<&str>) -> String {
        if let Some(name) = registry_name.map(str::trim).filter(|name| !name.is_empty()) {
            return name.to_string();
        }
        if let Some(manifest) = self.mode.package_manifest() {
            if let Some(registry) = manifest.publish_config.registry.as_deref().filter(|value| !value.is_empty()) {
                return registry.to_string();
            }
        }
        String::new()
    }
}

struct PublishJob {
    package_path: PathBuf,
    manifest: PackageManifest,
    package_id: String,
    version: String,
    registry: String,
    build_target: String,
    description: String,
    homepage: Option<String>,
    license: Option<String>,
    tag: String,
    access: String,
}

fn resolve_legacy_registry(options: &PublishOptions, manifest: &PackageManifest) -> String {
    if !options.registry_name.is_empty() {
        return options.registry_name.clone();
    }
    manifest.publish_config.registry.clone().filter(|value| !value.is_empty()).unwrap_or_else(|| options.registry_name.clone())
}

fn resolve_publish_version(target_version: &str, manifest_version: &str, workspace_version: Option<&str>) -> Result<String> {
    for candidate in [target_version, manifest_version] {
        let value = candidate.trim();
        if !value.is_empty() && value != "workspace" {
            return Ok(value.to_string());
        }
    }
    if let Some(value) = workspace_version.map(str::trim).filter(|value| !value.is_empty() && *value != "workspace") {
        return Ok(value.to_string());
    }
    Err(PackageManagerError::message("发布版本无效：请在 publish.version、包清单 version 或工作区 version 中提供具体版本号"))
}

fn copy_dir(source: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir(&source_path, &dest_path)?;
        }
        else {
            std::fs::copy(&source_path, &dest_path)?;
        }
    }
    Ok(())
}

// Keep WorkspaceManifest reachable for callers.
#[allow(dead_code)]
fn _workspace_manifest_path(root: &Path) -> PathBuf {
    WorkspaceManifest::path_in(root, ProjectLayout::neutral())
}
