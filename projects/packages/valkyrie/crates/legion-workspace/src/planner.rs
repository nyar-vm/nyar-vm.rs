use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fmt::{Display, Formatter},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use miette::{Diagnostic, Severity};
use rusty_valkyrie::{CanonicalTarget, DeclarationKey, DeclarationKind};
use nyar_package_manager::PackageManager as PackageLegion;
use serde::{Deserialize, Serialize};

use crate::{
    LOCAL_LEGION_CONFIG,
    manifest::{
        BuildTargetSpec, DependencySourcePreference, DependencySpec, LocalLegionConfig, ManifestError, ProjectManifest, WorkspaceManifest,
    },
};

/// 单次构建请求：工程目录、目标平台与可选输出目录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildRequest {
    /// 请求的工程根目录。
    pub project_dir: PathBuf,
    /// 目标平台。
    pub target: CanonicalTarget,
    /// 自定义输出目录；缺省时按目标推导默认 dist 目录。
    pub output_dir: Option<PathBuf>,
}

/// 构建计划中记录的一项已解析依赖。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedDependency {
    /// 依赖名称。
    pub name: String,
    /// 依赖 manifest 所在目录。
    pub manifest_dir: PathBuf,
}

/// One isolated frontend compilation unit. Dependencies are represented by
/// their exported semantic contracts, never by concatenating their source
/// text into this group's parser input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedSemanticSourceGroup {
    /// Resolver-assigned dependency identity used for semantic export lookup.
    pub dependency_key: String,
    /// 源码组名称（依赖包名或入口工程名）。
    pub name: String,
    /// 该源码组 manifest 所在目录。
    pub manifest_dir: PathBuf,
    /// 属于该源码组的源码文件列表。
    pub source_files: Vec<PathBuf>,
    /// 该源码组直接依赖的依赖键列表。
    pub direct_dependencies: Vec<String>,
    /// 当前构建入口工程组；依赖包 inventory 可容忍不可达文件的 parse 失败。
    pub is_entry: bool,
}

/// 计划中的宿主契约：源码声明的宿主接口、冻结声明身份及其位置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedHostContract {
    /// 契约限定名（`namespace` + 拥有者 + 声明名，统一 `::` 分隔）。
    pub id: String,
    /// 契约的冻结声明身份；Compiler 只接受结构化身份，不接受限定名字符串。
    pub key: DeclarationKey,
    /// 声明该契约的源码文件。
    pub source_file: PathBuf,
    /// 声明所在行号。
    pub line: usize,
}

/// 计划中的宿主契约提供者候选：把契约绑定到宿主符号。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedHostProvider {
    /// 被实现的宿主契约标识（保留源码属性中的书写形式）。
    pub contract: String,
    /// 被引用契约的冻结声明身份；候选被选中时由 Resolver 填充。
    pub contract_key: Option<DeclarationKey>,
    /// 宿主侧绑定的符号名（提供者声明限定名）。
    pub symbol: String,
    /// 提供者声明的冻结身份。
    pub key: DeclarationKey,
    /// 声明该提供者的源码文件。
    pub source_file: PathBuf,
    /// 声明所在行号。
    pub line: usize,
}

/// 单个工程的构建计划：清单位置、源码闭包、语义源码组与宿主契约。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedProject {
    /// 工程名称。
    pub name: String,
    /// 工程 manifest 所在目录。
    pub manifest_dir: PathBuf,
    /// 工程 manifest 文件路径。
    pub manifest_path: PathBuf,
    /// 该工程完整的源码输入闭包（用于哈希、宿主 inventory 与运行摘要）。
    pub source_files: Vec<PathBuf>,
    /// Dependency-first semantic compilation groups. `source_files` remains
    /// the complete input closure for hashes, host inventory and run digests.
    pub semantic_source_groups: Vec<PlannedSemanticSourceGroup>,
    /// 工程声明的宿主契约列表。
    pub host_contracts: Vec<PlannedHostContract>,
    /// 宿主契约提供者候选列表。
    pub host_provider_candidates: Vec<PlannedHostProvider>,
    /// 已选定的宿主契约提供者。
    pub selected_host_providers: Vec<PlannedHostProvider>,
    /// 匹配到的构建目标配置。
    pub build_target: BuildTargetSpec,
    /// 产物类型（binary / library）。
    pub artifact_kind: crate::manifest::ProjectArtifactKind,
    /// 已解析的依赖列表。
    pub dependencies: Vec<PlannedDependency>,
}

/// 完整构建计划：workspace 根、目标工程与输出目录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildPlan {
    /// workspace 根目录。
    pub workspace_root: PathBuf,
    /// 目标工程的构建计划。
    pub project: PlannedProject,
    /// 产物输出目录。
    pub output_dir: PathBuf,
}

/// 工程解析模式：workspace 成员、独立包或单脚本。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectResolutionMode {
    /// 通过 workspace `legions.von` 解析。
    Workspace,
    /// 独立包（仅有 `legion.von`）。
    Package,
    /// 内嵌清单的单脚本 `.v`。
    Script,
}

/// workspace 解析器：缓存已发现的工程清单与单脚本上下文。
#[derive(Debug)]
pub struct WorkspaceResolver {
    /// workspace 根目录。
    pub root_dir: PathBuf,
    /// workspace 根清单；独立包模式下为 `None`。
    pub workspace_manifest: Option<WorkspaceManifest>,
    projects: BTreeMap<PathBuf, ProjectManifest>,
    projects_by_name: BTreeMap<String, PathBuf>,
    /// `legion build solution.v` — 内嵌清单的单脚本模式。
    pub single_script: Option<crate::script::SingleScriptContext>,
}

/// planner 在解析 workspace / 依赖并生成构建计划时产生的错误。
#[derive(Debug)]
pub enum PlannerError {
    /// 底层 I/O 错误。
    Io(std::io::Error),
    /// 清单解析错误。
    Manifest(ManifestError),
    /// 未能从起始路径定位 `legions.von`。
    MissingWorkspace(PathBuf),
    /// 未能定位 `legion.von`。
    MissingProjectManifest(PathBuf),
    /// 工程未声明请求的构建目标。
    MissingBuildTarget {
        /// 工程名称。
        project: String,
        /// 请求的目标平台。
        target: CanonicalTarget,
    },
    /// 无法解析某依赖。
    MissingDependency {
        /// 工程名称。
        project: String,
        /// 依赖名称。
        dependency: String,
    },
    /// 依赖要求使用 workspace 成员，但未找到对应成员。
    ForcedWorkspaceDependencyMissing {
        /// 工程名称。
        project: String,
        /// 依赖名称。
        dependency: String,
    },
    /// registry 依赖缺少版本号。
    RegistryDependencyMissingVersion {
        /// 工程名称。
        project: String,
        /// 依赖名称。
        dependency: String,
    },
    /// registry 依赖安装失败。
    RegistryDependencyInstallFailed {
        /// 工程名称。
        project: String,
        /// 依赖名称。
        dependency: String,
        /// 依赖版本。
        version: String,
        /// registry 名称。
        registry: String,
        /// 失败原因。
        reason: String,
    },
    /// path 依赖缺少路径字段。
    PathDependencyMissingPath {
        /// 工程名称。
        project: String,
        /// 依赖名称。
        dependency: String,
    },
    /// path 依赖目录下找不到 manifest。
    PathDependencyMissingManifest {
        /// 工程名称。
        project: String,
        /// 依赖名称。
        dependency: String,
        /// 依赖 manifest 目录。
        manifest_dir: PathBuf,
    },
    /// git 依赖缺少仓库地址。
    GitDependencyMissingUrl {
        /// 工程名称。
        project: String,
        /// 依赖名称。
        dependency: String,
    },
    /// git 依赖目录下找不到 manifest。
    GitDependencyMissingManifest {
        /// 工程名称。
        project: String,
        /// 依赖名称。
        dependency: String,
        /// git 仓库地址。
        git_url: String,
        /// git 引用。
        git_ref: String,
        /// 检出后的 manifest 目录。
        manifest_dir: PathBuf,
    },
    /// git 依赖检出失败。
    GitDependencyCheckoutFailed {
        /// 工程名称。
        project: String,
        /// 依赖名称。
        dependency: String,
        /// git 仓库地址。
        git_url: String,
        /// 失败原因。
        reason: String,
    },
    /// 宿主契约提供者声明了未知的契约。
    UnknownHostProviderContract {
        /// 提供者名称。
        provider: String,
        /// 契约标识。
        contract: String,
        /// 声明该提供者的源码文件。
        source_file: PathBuf,
        /// 声明所在行号。
        line: usize,
    },
    /// 多个提供者实现同一宿主契约，产生冲突。
    ConflictingHostProviders {
        /// 冲突的契约标识。
        contract: String,
        /// 冲突的提供者名称列表。
        providers: Vec<String>,
    },
    /// 单脚本模式错误。
    Script(crate::script::ScriptError),
    /// 防止依赖图或源码闭包失控（死循环 / 指数膨胀 / OOM）。
    ResourceLimitExceeded {
        /// 触发的上限类别。
        kind: &'static str,
        /// 允许的上限值。
        limit: usize,
        /// 实际达到的值。
        actual: usize,
    },
}

impl Display for PlannerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => Display::fmt(error, f),
            Self::Manifest(error) => Display::fmt(error, f),
            Self::MissingWorkspace(path) => write!(f, "cannot locate `legions.von` from {}", path.display()),
            Self::MissingProjectManifest(path) => write!(f, "cannot locate `legion.von` in {}", path.display()),
            Self::MissingBuildTarget { project, target } => {
                write!(f, "project '{}' does not declare build target '{}'", project, target)
            }
            Self::MissingDependency { project, dependency } => {
                write!(f, "project '{}' cannot resolve dependency '{}'", project, dependency)
            }
            Self::ForcedWorkspaceDependencyMissing { project, dependency } => {
                write!(f, "project '{}' requires workspace dependency '{}', but no workspace member was found", project, dependency)
            }
            Self::RegistryDependencyMissingVersion { project, dependency } => {
                write!(f, "project '{}' sets dependency '{}' source=registry but no version is provided", project, dependency)
            }
            Self::RegistryDependencyInstallFailed { project, dependency, version, registry, reason } => {
                write!(
                    f,
                    "project '{}' dependency '{}' failed to install from registry '{}' version '{}': {}",
                    project, dependency, registry, version, reason
                )
            }
            Self::PathDependencyMissingPath { project, dependency } => {
                write!(f, "project '{}' dependency '{}' uses source=path but no path is provided", project, dependency)
            }
            Self::PathDependencyMissingManifest { project, dependency, manifest_dir } => {
                write!(f, "project '{}' dependency '{}' path does not contain `legion.von` at {}", project, dependency, manifest_dir.display())
            }
            Self::GitDependencyMissingUrl { project, dependency } => {
                write!(f, "project '{}' dependency '{}' uses source=git but no git url is provided", project, dependency)
            }
            Self::GitDependencyMissingManifest { project, dependency, git_url, git_ref, manifest_dir } => {
                write!(
                    f,
                    "project '{}' dependency '{}' git checkout `{}`@{} has no `legion.von` at {}",
                    project,
                    dependency,
                    git_url,
                    git_ref,
                    manifest_dir.display()
                )
            }
            Self::GitDependencyCheckoutFailed { project, dependency, git_url, reason } => {
                write!(f, "project '{}' dependency '{}' failed to checkout git `{}`: {}", project, dependency, git_url, reason)
            }
            Self::UnknownHostProviderContract { provider, contract, source_file, line } => {
                write!(f, "host provider '{}' references unknown host contract '{}' at {}:{}", provider, contract, source_file.display(), line)
            }
            Self::ConflictingHostProviders { contract, providers } => {
                write!(f, "host contract '{}' has multiple visible providers: {}", contract, providers.join(", "))
            }
            Self::Script(error) => Display::fmt(error, f),
            Self::ResourceLimitExceeded { kind, limit, actual } => {
                write!(f, "planner {kind} exceeded limit {limit} (actual {actual})")
            }
        }
    }
}

impl std::error::Error for PlannerError {}

impl Diagnostic for PlannerError {
    fn code<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        Some(Box::new(match self {
            PlannerError::Io(_) => "legion::planner::io",
            PlannerError::Manifest(_) => "legion::planner::manifest",
            PlannerError::MissingWorkspace(_) => "legion::planner::missing_workspace",
            PlannerError::MissingProjectManifest(_) => "legion::planner::missing_project_manifest",
            PlannerError::MissingBuildTarget { .. } => "legion::planner::missing_build_target",
            PlannerError::MissingDependency { .. } => "legion::planner::missing_dependency",
            PlannerError::ForcedWorkspaceDependencyMissing { .. } => "legion::planner::forced_workspace_dependency_missing",
            PlannerError::RegistryDependencyMissingVersion { .. } => "legion::planner::registry_dependency_missing_version",
            PlannerError::RegistryDependencyInstallFailed { .. } => "legion::planner::registry_dependency_install_failed",
            PlannerError::PathDependencyMissingPath { .. } => "legion::planner::path_dependency_missing_path",
            PlannerError::PathDependencyMissingManifest { .. } => "legion::planner::path_dependency_missing_manifest",
            PlannerError::GitDependencyMissingUrl { .. } => "legion::planner::git_dependency_missing_url",
            PlannerError::GitDependencyMissingManifest { .. } => "legion::planner::git_dependency_missing_manifest",
            PlannerError::GitDependencyCheckoutFailed { .. } => "legion::planner::git_dependency_checkout_failed",
            PlannerError::UnknownHostProviderContract { .. } => "legion::planner::unknown_host_provider_contract",
            PlannerError::ConflictingHostProviders { .. } => "legion::planner::conflicting_host_providers",
            PlannerError::Script(_) => "legion::script",
            PlannerError::ResourceLimitExceeded { .. } => "legion::planner::resource_limit",
        }))
    }

    fn severity(&self) -> Option<Severity> {
        Some(Severity::Error)
    }

    fn help<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        Some(Box::new(match self {
            PlannerError::Io(_) => "请确认工作区目录存在且当前进程有访问权限",
            PlannerError::Manifest(_) => "请修复 `legion.von` / `legions.von` 中的配置或 `VON` 语法",
            PlannerError::MissingWorkspace(_) => "请在工作区根目录放置 `legions.von`",
            PlannerError::MissingProjectManifest(_) => "请在项目目录放置 `legion.von`",
            PlannerError::MissingBuildTarget { .. } => "请在 `build` 段中声明对应 target",
            PlannerError::MissingDependency { .. } => "请确认依赖名称存在于 workspace 成员或项目依赖中",
            PlannerError::ForcedWorkspaceDependencyMissing { .. } => {
                "依赖已声明 `source: workspace`，请确认该依赖项目在当前 `legions.von`（含嵌套 members）中"
            }
            PlannerError::RegistryDependencyMissingVersion { .. } => "依赖已声明 `source: registry`，请同时提供 `version`",
            PlannerError::RegistryDependencyInstallFailed { .. } => {
                "请检查 registry 名称、网络连接、包名与版本是否可用，必要时先执行 `legion install` 验证"
            }
            PlannerError::PathDependencyMissingPath { .. } => "请为 path 依赖提供相对或绝对 `path` 字段",
            PlannerError::PathDependencyMissingManifest { .. } => "请确认 path 指向包含 `legion.von` 的 Valkyrie 包目录",
            PlannerError::GitDependencyMissingUrl { .. } => "请为 git 依赖提供 `git` 仓库 URL",
            PlannerError::GitDependencyMissingManifest { .. } => "请确认 git 仓库内 `path` 子目录包含 `legion.von`",
            PlannerError::GitDependencyCheckoutFailed { .. } => "请确认本机已安装 git 且仓库 URL / ref 可访问",
            PlannerError::UnknownHostProviderContract { .. } => {
                "请确认 `[host_provider(...)]` 指向的 contract 标识与可见源码中的 `[host_contract]` 完全一致"
            }
            PlannerError::ConflictingHostProviders { .. } => {
                "请收窄有效依赖闭包，或移除重复的 provider，保证每个 `host_contract` 最多只有一个可见实现"
            }
            PlannerError::Script(_) => "请在 `.v` 单脚本头部使用 `# ```legion` 内嵌清单，或改用 sidecar `legion.von`",
            PlannerError::ResourceLimitExceeded { .. } => "请检查 workspace 依赖是否成环、路径别名是否导致重复展开，或缩小 build 闭包",
        }))
    }

    fn diagnostic_source(&self) -> Option<&dyn Diagnostic> {
        match self {
            PlannerError::Manifest(error) => Some(error),
            PlannerError::Script(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for PlannerError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<ManifestError> for PlannerError {
    fn from(value: ManifestError) -> Self {
        Self::Manifest(value)
    }
}

impl From<crate::script::ScriptError> for PlannerError {
    fn from(value: crate::script::ScriptError) -> Self {
        Self::Script(value)
    }
}

impl WorkspaceResolver {
    /// 从 `start` 向上查找 workspace 根，并加载其全部成员工程清单。
    pub fn discover(start: impl AsRef<Path>) -> Result<Self, PlannerError> {
        let start = start.as_ref();
        let workspace_root = find_workspace_root(start).ok_or_else(|| PlannerError::MissingWorkspace(start.to_path_buf()))?;
        let workspace_manifest_path = workspace_root.join("legions.von");
        let workspace_manifest = WorkspaceManifest::parse(&fs::read_to_string(&workspace_manifest_path)?)?;

        let mut projects = BTreeMap::new();
        let mut projects_by_name = BTreeMap::new();
        register_workspace_members(&workspace_root, &workspace_manifest, &mut projects, &mut projects_by_name)?;

        Ok(Self { root_dir: workspace_root, workspace_manifest: Some(workspace_manifest), projects, projects_by_name, single_script: None })
    }

    fn workspace_from_single_script(ctx: crate::script::SingleScriptContext) -> Result<Self, PlannerError> {
        let project_dir = ctx.project_dir.clone();
        let mut projects = BTreeMap::new();
        let canonical = canonicalize_lossy(&project_dir);
        projects.insert(canonical.clone(), ctx.manifest.clone());
        let mut projects_by_name = BTreeMap::new();
        projects_by_name.insert(ctx.manifest.name.clone(), canonical);
        Ok(Self { root_dir: project_dir, workspace_manifest: None, projects, projects_by_name, single_script: Some(ctx) })
    }

    /// 解析 `start` 所在工程（workspace 成员、独立包或单脚本）。
    pub fn discover_for_project(start: impl AsRef<Path>) -> Result<Self, PlannerError> {
        let start = start.as_ref();
        if let Some(workspace_root) = find_workspace_root(start) {
            let mut workspace = Self::discover(workspace_root)?;
            let project_dir = resolve_project_root(start).unwrap_or_else(|| search_start_dir(start));
            if workspace.project_manifest(&project_dir).is_some() {
                return Ok(workspace);
            }
            // 题级 / 示例包常有独立 `legion.von` 且不在 `legions.von` members 中，
            // 但仍通过 `core: true` / `std: true` 引用 workspace 成员；保留已发现的 workspace。
            let manifest_path = project_dir.join("legion.von");
            if manifest_path.is_file() {
                let manifest = ProjectManifest::parse(&fs::read_to_string(&manifest_path)?)?;
                let project_name = manifest.name.clone();
                let canonical = canonicalize_lossy(&project_dir);
                workspace.projects.insert(canonical.clone(), manifest);
                workspace.projects_by_name.insert(project_name, canonical);
                return Ok(workspace);
            }
        }

        if crate::script::is_script_path(start) {
            if let Some(ctx) = crate::script::try_load_single_script(start).map_err(PlannerError::Script)? {
                return Self::workspace_from_single_script(ctx);
            }
        }

        let project_dir = resolve_project_root(start).ok_or_else(|| PlannerError::MissingProjectManifest(search_start_dir(start)))?;

        let sidecar = project_dir.join("solution.v");
        if sidecar.is_file() {
            if let Some(ctx) = crate::script::try_load_single_script(&sidecar).map_err(PlannerError::Script)? {
                return Self::workspace_from_single_script(ctx);
            }
        }

        let manifest_path = project_dir.join("legion.von");
        let manifest = ProjectManifest::parse(&fs::read_to_string(&manifest_path)?)?;
        let canonical = canonicalize_lossy(&project_dir);
        let mut projects = BTreeMap::new();
        projects.insert(canonical.clone(), manifest.clone());
        let mut projects_by_name = BTreeMap::new();
        projects_by_name.insert(manifest.name.clone(), canonical);

        Ok(Self { root_dir: project_dir.clone(), workspace_manifest: None, projects, projects_by_name, single_script: None })
    }

    /// 返回指定目录对应的工程清单（按路径等价匹配）。
    pub fn project_manifest(&self, project_dir: &Path) -> Option<&ProjectManifest> {
        self.projects
            .get(project_dir)
            .or_else(|| self.projects.iter().find(|(candidate, _)| same_path(candidate, project_dir)).map(|(_, manifest)| manifest))
    }

    /// 返回 workspace 中所有成员项目的 manifest 目录（有序）。
    pub fn member_manifest_dirs(&self) -> Vec<PathBuf> {
        self.projects.keys().cloned().collect()
    }

    /// 发行版默认携带的 SDK：由 planner 注入当前构建的有效依赖闭包。
    ///
    /// 只识别 `sdk_vendor.kind == "distribution-default"` 元数据；不按包名、
    /// 目录名前缀或 target 名称猜测。`std` 不再反向依赖各 adaptor，adaptor
    /// 自身显式依赖 `std`，因此依赖方向与可见性一致。
    fn distribution_default_sdk_names(&self) -> BTreeSet<String> {
        self.projects
            .values()
            .filter(|manifest| manifest.sdk_vendor.as_ref().and_then(|vendor| vendor.kind.as_deref()) == Some("distribution-default"))
            .map(|manifest| manifest.name.clone())
            .collect()
    }

    /// 当前路径是否为仅有 `legions.von`、没有 `legion.von` 的 workspace 根目录。
    pub fn is_workspace_only_root(&self, project_dir: &Path) -> bool {
        self.workspace_manifest.is_some() && same_path(project_dir, &self.root_dir) && !self.root_dir.join("legion.von").exists()
    }

    /// 按请求生成构建计划：解析构建目标、依赖与源码闭包。
    pub fn build_plan(&self, request: &BuildRequest) -> Result<BuildPlan, PlannerError> {
        if let Some(ctx) = &self.single_script {
            return self.build_plan_for_manifest(ctx.project_dir.clone(), &ctx.manifest, request);
        }

        let project_dir = if self.workspace_manifest.is_none() {
            self.root_dir.clone()
        }
        else {
            resolve_project_root(&request.project_dir).unwrap_or_else(|| search_start_dir(&request.project_dir))
        };
        let manifest = if self.workspace_manifest.is_none() {
            self.projects.values().next().ok_or_else(|| PlannerError::MissingProjectManifest(project_dir.clone()))?
        }
        else {
            self.project_manifest(&project_dir).ok_or_else(|| PlannerError::MissingProjectManifest(project_dir.clone()))?
        };
        let build_target = select_build_target(manifest, &request.target)
            .ok_or_else(|| PlannerError::MissingBuildTarget { project: manifest.name.clone(), target: request.target })?;

        let dependencies = self.collect_dependencies(&project_dir, manifest, &build_target, true)?;
        let output_dir = request.output_dir.clone().unwrap_or_else(|| default_dist_output_dir(&project_dir, &request.target));

        // 收集源码闭包：包含项目自身及其所有依赖（含传递依赖）的源文件。
        // 这是模块系统的核心：让编译器能看到依赖项目定义的 struct/function/type。
        let mut visited = BTreeSet::new();
        let source_files = self.collect_source_closure(&project_dir, manifest, &build_target, &mut visited, true)?;
        let semantic_source_groups = self.collect_semantic_source_groups(&project_dir, manifest, &build_target)?;
        let host_inventory = collect_host_inventory(&semantic_source_groups, &build_target.publish)?;

        Ok(BuildPlan {
            workspace_root: self.root_dir.clone(),
            output_dir,
            project: PlannedProject {
                name: manifest.name.clone(),
                manifest_dir: project_dir.clone(),
                manifest_path: self
                    .single_script
                    .as_ref()
                    .filter(|ctx| same_path(&ctx.project_dir, &project_dir))
                    .map(|ctx| ctx.script_path.clone())
                    .unwrap_or_else(|| project_dir.join("legion.von")),
                source_files,
                semantic_source_groups,
                host_contracts: host_inventory.contracts,
                host_provider_candidates: host_inventory.providers,
                selected_host_providers: host_inventory.selected_providers,
                build_target,
                artifact_kind: manifest.artifact,
                dependencies,
            },
        })
    }

    /// 构造测试/基准编译计划：始终允许指定 target，并合并 `source/` + `test/` 源文件。
    pub fn build_test_plan(&self, request: &BuildRequest) -> Result<(BuildPlan, ProjectResolutionMode), PlannerError> {
        let (project_dir, manifest, mode) = if let Some(ctx) = &self.single_script {
            (ctx.project_dir.clone(), ctx.manifest.clone(), ProjectResolutionMode::Script)
        }
        else {
            let project_dir = resolve_project_root(&request.project_dir).unwrap_or_else(|| search_start_dir(&request.project_dir));
            if let Some(manifest) = self.project_manifest(&project_dir) {
                let mode = if self.workspace_manifest.is_some() { ProjectResolutionMode::Workspace } else { ProjectResolutionMode::Script };
                (project_dir, manifest.clone(), mode)
            }
            else {
                let manifest_path = project_dir.join("legion.von");
                if !manifest_path.exists() {
                    return Err(PlannerError::MissingProjectManifest(project_dir));
                }
                let manifest = ProjectManifest::parse(&fs::read_to_string(&manifest_path)?)?;
                let mode = if self.workspace_manifest.is_some() { ProjectResolutionMode::Package } else { ProjectResolutionMode::Script };
                (project_dir, manifest, mode)
            }
        };

        let mut build_target = select_build_target(&manifest, &request.target)
            .unwrap_or_else(|| BuildTargetSpec { target: request.target.clone(), ..BuildTargetSpec::default() });
        build_target.target = request.target.clone();

        let dependencies = self.collect_dependencies(&project_dir, &manifest, &build_target, true)?;
        let output_dir =
            request.output_dir.clone().unwrap_or_else(|| project_dir.join(".cache").join("test").join(request.target.as_canonical_str()));

        let mut visited = BTreeSet::new();
        let mut source_files = self.collect_source_closure(&project_dir, &manifest, &build_target, &mut visited, true)?;
        let mut test_files = collect_test_v_files(&project_dir)?;
        source_files.append(&mut test_files);
        source_files.sort();
        source_files.dedup();

        let semantic_source_groups = self.collect_semantic_source_groups(&project_dir, &manifest, &build_target)?;
        let host_inventory = collect_host_inventory(&semantic_source_groups, &build_target.publish)?;

        Ok((
            BuildPlan {
                workspace_root: self.root_dir.clone(),
                output_dir,
                project: PlannedProject {
                    name: manifest.name.clone(),
                    manifest_dir: project_dir.clone(),
                    manifest_path: self
                        .single_script
                        .as_ref()
                        .map(|ctx| ctx.script_path.clone())
                        .unwrap_or_else(|| project_dir.join("legion.von")),
                    source_files,
                    semantic_source_groups,
                    host_contracts: host_inventory.contracts,
                    host_provider_candidates: host_inventory.providers,
                    selected_host_providers: host_inventory.selected_providers,
                    build_target,
                    artifact_kind: manifest.artifact,
                    dependencies,
                },
            },
            mode,
        ))
    }

    /// 递归收集源码闭包：项目自身 + 所有依赖（含传递依赖）的源文件。
    ///
    /// 这是模块系统依赖管理的核心实现。通过递归遍历依赖图，
    /// 将所有相关项目的源文件收集到一起，供编译器做合并编译。
    ///
    /// 使用 `visited` 集合防止循环依赖导致的无限递归。
    fn collect_source_closure(
        &self,
        project_dir: &Path,
        manifest: &ProjectManifest,
        build_target: &BuildTargetSpec,
        visited: &mut BTreeSet<String>,
        inject_distribution_defaults: bool,
    ) -> Result<Vec<PathBuf>, PlannerError> {
        let canonical_dir = canonicalize_lossy(project_dir);
        let visit_key = project_visit_key(&canonical_dir);
        if visited.contains(&visit_key) {
            return Ok(Vec::new());
        }
        visited.insert(visit_key);

        if let Some(ctx) = &self.single_script {
            if same_path(project_dir, &ctx.project_dir) {
                return Ok(vec![ctx.script_path.clone()]);
            }
        }

        // 收集当前项目自身的源文件。target 不参与声明是否存在，只选择 adaptor 与 TargetExecutionPlan。
        let mut all_files = collect_source_files(&canonical_dir)?;

        // 收集当前项目的依赖名称（auto_link + 显式声明 + 平台隐式 SDK）。
        let mut dep_names = BTreeSet::new();
        if manifest.auto_link.core {
            dep_names.insert("core".to_string());
        }
        if manifest.auto_link.std {
            dep_names.insert("std".to_string());
        }
        for (dependency_name, dependency_spec) in &manifest.dependencies {
            if !matches!(dependency_spec, DependencySpec::Disabled) {
                dep_names.insert(dependency_name.clone());
            }
        }
        let implicit_names = implicit_sdk_dependencies(&build_target.publish).into_iter().map(|item| item.to_string()).collect::<BTreeSet<_>>();
        for implicit in &implicit_names {
            dep_names.insert(implicit.clone());
        }
        let distribution_defaults = if inject_distribution_defaults { self.distribution_default_sdk_names() } else { BTreeSet::new() };
        add_distribution_default_dependencies(manifest, &distribution_defaults, &mut dep_names);

        // 递归收集每个依赖的源文件。
        for dep_name in dep_names {
            let local_manifest_dir = self.projects_by_name.get(&dep_name).cloned();
            let dependency_spec = manifest.dependencies.get(&dep_name);
            let allow_missing_when_auto = implicit_names.contains(&dep_name) || distribution_defaults.contains(&dep_name);
            let overrides = merged_local_dependency_overrides(project_dir);
            let (effective_spec, path_base) = effective_dependency_spec(&dep_name, dependency_spec, &overrides, project_dir);
            match resolve_dependency_source(
                self,
                project_dir,
                &path_base,
                effective_spec.as_deref(),
                local_manifest_dir,
                allow_missing_when_auto,
                &manifest.name,
                &dep_name,
            )? {
                ResolvedDependencySource::Local(dep_dir) => {
                    if let Some(dep_manifest) = self.load_project_manifest_from_dir(&dep_dir) {
                        if !dependency_supports_source_context(&dep_name, &dep_manifest, &build_target.target, &build_target.publish) {
                            continue;
                        }
                        let dep_files = self.collect_source_closure(&dep_dir, &dep_manifest, build_target, visited, false)?;
                        all_files.extend(dep_files);
                    }
                }
                ResolvedDependencySource::Registry { version, registry } => {
                    let install_dir =
                        self.install_registry_dependency(self.root_dir.as_path(), &dep_name, &version, &registry, &manifest.name)?;
                    if let Some(dep_manifest) = self.load_project_manifest_from_dir(&install_dir) {
                        if dependency_supports_source_context(&dep_name, &dep_manifest, &build_target.target, &build_target.publish) {
                            let dep_files = self.collect_source_closure(&install_dir, &dep_manifest, build_target, visited, false)?;
                            all_files.extend(dep_files);
                        }
                    }
                    else {
                        all_files.extend(collect_source_files(&install_dir)?);
                    }
                }
                ResolvedDependencySource::MissingImplicit => {}
            }
        }

        // 去重并排序，确保编译顺序稳定。
        all_files.sort();
        all_files.dedup();
        if all_files.len() > MAX_SOURCE_CLOSURE_FILES {
            return Err(PlannerError::ResourceLimitExceeded {
                kind: "source_closure_files",
                limit: MAX_SOURCE_CLOSURE_FILES,
                actual: all_files.len(),
            });
        }
        Ok(all_files)
    }

    fn collect_semantic_source_groups(
        &self,
        project_dir: &Path,
        manifest: &ProjectManifest,
        build_target: &BuildTargetSpec,
    ) -> Result<Vec<PlannedSemanticSourceGroup>, PlannerError> {
        let mut groups = Vec::new();
        let mut visited = BTreeSet::new();
        self.collect_semantic_source_groups_inner(project_dir, manifest, &manifest.name, build_target, &mut visited, &mut groups, 0)?;
        Ok(groups)
    }

    fn collect_semantic_source_groups_inner(
        &self,
        project_dir: &Path,
        manifest: &ProjectManifest,
        dependency_key: &str,
        build_target: &BuildTargetSpec,
        visited: &mut BTreeSet<String>,
        groups: &mut Vec<PlannedSemanticSourceGroup>,
        depth: usize,
    ) -> Result<(), PlannerError> {
        if depth > MAX_SEMANTIC_GROUP_DEPTH {
            return Err(PlannerError::ResourceLimitExceeded { kind: "semantic_group_depth", limit: MAX_SEMANTIC_GROUP_DEPTH, actual: depth });
        }
        if groups.len() >= MAX_SEMANTIC_GROUPS {
            return Err(PlannerError::ResourceLimitExceeded { kind: "semantic_groups", limit: MAX_SEMANTIC_GROUPS, actual: groups.len() });
        }
        let manifest_dir = canonicalize_lossy(project_dir);
        let visit_key = project_visit_key(&manifest_dir);
        if !visited.insert(visit_key) {
            return Ok(());
        }
        // 发行版默认 SDK 只注入当前构建根的有效依赖闭包；依赖自身不再携带
        // 反向 adaptor 边，否则 `std` 与其 adaptor 会形成可见性环。
        let dependencies = self.collect_dependencies(project_dir, manifest, build_target, depth == 0)?;
        for dependency in &dependencies {
            let dependency_manifest = self
                .load_project_manifest_from_dir(&dependency.manifest_dir)
                .ok_or_else(|| PlannerError::MissingProjectManifest(dependency.manifest_dir.clone()))?;
            self.collect_semantic_source_groups_inner(
                &dependency.manifest_dir,
                &dependency_manifest,
                &dependency.name,
                build_target,
                visited,
                groups,
                depth + 1,
            )?;
        }
        let source_files = collect_source_files(&manifest_dir)?;
        groups.push(PlannedSemanticSourceGroup {
            dependency_key: dependency_key.to_string(),
            name: manifest.name.clone(),
            manifest_dir,
            source_files,
            direct_dependencies: dependencies.into_iter().map(|dependency| dependency.name).collect(),
            is_entry: depth == 0,
        });
        Ok(())
    }

    fn build_plan_for_manifest(
        &self,
        project_dir: PathBuf,
        manifest: &ProjectManifest,
        request: &BuildRequest,
    ) -> Result<BuildPlan, PlannerError> {
        let build_target = select_build_target(manifest, &request.target)
            .ok_or_else(|| PlannerError::MissingBuildTarget { project: manifest.name.clone(), target: request.target.clone() })?;

        let dependencies = self.collect_dependencies(&project_dir, manifest, &build_target, true)?;
        let output_dir = request.output_dir.clone().unwrap_or_else(|| default_dist_output_dir(&project_dir, &request.target));

        let mut visited = BTreeSet::new();
        let source_files = self.collect_source_closure(&project_dir, manifest, &build_target, &mut visited, true)?;
        let semantic_source_groups = self.collect_semantic_source_groups(&project_dir, manifest, &build_target)?;
        let host_inventory = collect_host_inventory(&semantic_source_groups, &build_target.publish)?;

        Ok(BuildPlan {
            workspace_root: self.root_dir.clone(),
            output_dir,
            project: PlannedProject {
                name: manifest.name.clone(),
                manifest_dir: project_dir.clone(),
                manifest_path: self
                    .single_script
                    .as_ref()
                    .filter(|ctx| same_path(&ctx.project_dir, &project_dir))
                    .map(|ctx| ctx.script_path.clone())
                    .unwrap_or_else(|| project_dir.join("legion.von")),
                source_files,
                semantic_source_groups,
                host_contracts: host_inventory.contracts,
                host_provider_candidates: host_inventory.providers,
                selected_host_providers: host_inventory.selected_providers,
                build_target,
                artifact_kind: manifest.artifact,
                dependencies,
            },
        })
    }

    fn collect_dependencies(
        &self,
        project_manifest_dir: &Path,
        manifest: &ProjectManifest,
        build_target: &BuildTargetSpec,
        inject_distribution_defaults: bool,
    ) -> Result<Vec<PlannedDependency>, PlannerError> {
        let mut planned = Vec::new();
        let mut names = BTreeSet::new();
        let mut declared_specs = BTreeMap::new();
        let implicit_names = implicit_sdk_dependencies(&build_target.publish).into_iter().map(|item| item.to_string()).collect::<BTreeSet<_>>();

        if manifest.auto_link.core {
            names.insert("core".to_string());
        }
        if manifest.auto_link.std {
            names.insert("std".to_string());
        }
        for (dependency_name, dependency_spec) in &manifest.dependencies {
            if !matches!(dependency_spec, DependencySpec::Disabled) {
                names.insert(dependency_name.clone());
                declared_specs.insert(dependency_name.clone(), dependency_spec.clone());
            }
        }
        for implicit in &implicit_names {
            names.insert(implicit.clone());
        }
        let distribution_defaults = if inject_distribution_defaults { self.distribution_default_sdk_names() } else { BTreeSet::new() };
        add_distribution_default_dependencies(manifest, &distribution_defaults, &mut names);

        for dependency_name in names {
            let local_manifest_dir = self.projects_by_name.get(&dependency_name).cloned();
            let dependency_spec = declared_specs.get(&dependency_name);
            let allow_missing_when_auto = implicit_names.contains(&dependency_name) || distribution_defaults.contains(&dependency_name);
            let overrides = merged_local_dependency_overrides(project_manifest_dir);
            let (effective_spec, path_base) = effective_dependency_spec(&dependency_name, dependency_spec, &overrides, project_manifest_dir);

            let manifest_dir = match resolve_dependency_source(
                self,
                project_manifest_dir,
                &path_base,
                effective_spec.as_deref(),
                local_manifest_dir,
                allow_missing_when_auto,
                &manifest.name,
                &dependency_name,
            )? {
                ResolvedDependencySource::Local(manifest_dir) => manifest_dir,
                ResolvedDependencySource::Registry { version, registry } => {
                    self.install_registry_dependency(self.root_dir.as_path(), &dependency_name, &version, &registry, &manifest.name)?
                }
                ResolvedDependencySource::MissingImplicit => continue,
            };
            if let Some(dependency_manifest) = self.project_manifest(&manifest_dir) {
                if !dependency_supports_source_context(&dependency_name, dependency_manifest, &build_target.target, &build_target.publish) {
                    continue;
                }
            }
            planned.push(PlannedDependency { name: dependency_name, manifest_dir });
        }

        planned.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(planned)
    }

    fn registry_vendor_path(&self, dependency_name: &str, version: &str, registry: &str) -> PathBuf {
        self.root_dir.join("vendors").join(registry).join(format!("{dependency_name}@{version}"))
    }

    fn load_project_manifest_from_dir(&self, project_dir: &Path) -> Option<ProjectManifest> {
        self.project_manifest(project_dir).cloned().or_else(|| {
            let manifest_path = project_dir.join("legion.von");
            if !manifest_path.exists() {
                return None;
            }
            let source = fs::read_to_string(&manifest_path).ok()?;
            ProjectManifest::parse(&source).ok()
        })
    }

    fn install_registry_dependency(
        &self,
        project_dir: &Path,
        dependency_name: &str,
        version: &str,
        registry: &str,
        project_name: &str,
    ) -> Result<PathBuf, PlannerError> {
        let vendor_path = self.registry_vendor_path(dependency_name, version, registry);
        if vendor_path.join("legion.von").is_file() {
            return Ok(vendor_path);
        }

        let mut legion =
            PackageLegion::open(project_dir, crate::LEGION_PROJECT_LAYOUT).map_err(|error| PlannerError::RegistryDependencyInstallFailed {
                project: project_name.to_string(),
                dependency: dependency_name.to_string(),
                version: version.to_string(),
                registry: registry.to_string(),
                reason: error.to_string(),
            })?;
        let info =
            legion.install_one(dependency_name, version, registry, false).map_err(|error| PlannerError::RegistryDependencyInstallFailed {
                project: project_name.to_string(),
                dependency: dependency_name.to_string(),
                version: version.to_string(),
                registry: registry.to_string(),
                reason: error.to_string(),
            })?;
        info.install_path.map(PathBuf::from).ok_or_else(|| PlannerError::RegistryDependencyInstallFailed {
            project: project_name.to_string(),
            dependency: dependency_name.to_string(),
            version: version.to_string(),
            registry: registry.to_string(),
            reason: "missing install path".to_string(),
        })
    }
}

/// 合并本地 `.config/legion/legions.von` 依赖覆盖（越靠近项目的配置优先级越高）。
fn merged_local_dependency_overrides(project_dir: &Path) -> BTreeMap<String, (PathBuf, DependencySpec)> {
    let mut merged = BTreeMap::new();

    if let Some((anchor, config)) = load_local_legion_config(user_local_config_path()) {
        for (name, spec) in config.dependencies {
            merged.insert(name, (anchor.clone(), spec));
        }
    }

    let mut chain = Vec::new();
    let mut current = Some(canonicalize_lossy(project_dir));
    while let Some(dir) = current {
        let config_path = dir.join(LOCAL_LEGION_CONFIG);
        if config_path.is_file() {
            if let Ok(source) = fs::read_to_string(&config_path) {
                if let Ok(config) = LocalLegionConfig::parse(&source) {
                    chain.push((local_config_anchor(&config_path), config.dependencies));
                }
            }
        }
        current = dir.parent().map(Path::to_path_buf);
    }

    for (anchor, deps) in chain.into_iter().rev() {
        for (name, spec) in deps {
            merged.insert(name, (anchor.clone(), spec));
        }
    }

    merged
}

fn effective_dependency_spec<'a>(
    dependency_name: &str,
    declared: Option<&'a DependencySpec>,
    overrides: &BTreeMap<String, (PathBuf, DependencySpec)>,
    project_manifest_dir: &Path,
) -> (Option<Cow<'a, DependencySpec>>, PathBuf) {
    if let Some((anchor, spec)) = overrides.get(dependency_name) {
        return (Some(Cow::Owned(spec.clone())), anchor.clone());
    }
    (declared.map(Cow::Borrowed), project_manifest_dir.to_path_buf())
}

fn user_local_config_path() -> Option<PathBuf> {
    let home = std::env::var("USERPROFILE").ok().or_else(|| std::env::var("HOME").ok()).map(PathBuf::from)?;
    let path = home.join(".config").join("legion").join("legions.von");
    path.is_file().then_some(path)
}

fn local_config_anchor(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .and_then(|legion| legion.parent())
        .and_then(|config| config.parent())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| config_path.to_path_buf())
}

fn load_local_legion_config(config_path: Option<PathBuf>) -> Option<(PathBuf, LocalLegionConfig)> {
    let config_path = config_path?;
    let source = fs::read_to_string(&config_path).ok()?;
    let config = LocalLegionConfig::parse(&source).ok()?;
    Some((local_config_anchor(&config_path), config))
}

enum ResolvedDependencySource {
    Local(PathBuf),
    Registry { version: String, registry: String },
    MissingImplicit,
}

fn resolve_dependency_source(
    workspace: &WorkspaceResolver,
    project_manifest_dir: &Path,
    path_base: &Path,
    dependency_spec: Option<&DependencySpec>,
    local_manifest_dir: Option<PathBuf>,
    allow_missing_when_auto: bool,
    project_name: &str,
    dependency_name: &str,
) -> Result<ResolvedDependencySource, PlannerError> {
    let source_preference = dependency_spec.map(DependencySpec::source_preference).unwrap_or(DependencySourcePreference::Auto);
    let version_hint = dependency_spec.and_then(DependencySpec::version_hint);
    let registry_hint = dependency_spec.and_then(DependencySpec::registry_hint).unwrap_or("npm");

    match source_preference {
        DependencySourcePreference::Path => {
            let raw_path = dependency_spec.and_then(DependencySpec::path_hint).ok_or_else(|| PlannerError::PathDependencyMissingPath {
                project: project_name.to_string(),
                dependency: dependency_name.to_string(),
            })?;
            let manifest_dir = resolve_local_dependency_path(path_base, raw_path);
            if !manifest_dir.join("legion.von").is_file() {
                return Err(PlannerError::PathDependencyMissingManifest {
                    project: project_name.to_string(),
                    dependency: dependency_name.to_string(),
                    manifest_dir,
                });
            }
            Ok(ResolvedDependencySource::Local(manifest_dir))
        }
        DependencySourcePreference::Git => {
            let git_url = dependency_spec.and_then(DependencySpec::git_url).ok_or_else(|| PlannerError::GitDependencyMissingUrl {
                project: project_name.to_string(),
                dependency: dependency_name.to_string(),
            })?;
            let git_ref = dependency_spec.and_then(DependencySpec::git_ref_hint);
            let subpath = dependency_spec.and_then(DependencySpec::path_hint);
            let manifest_dir = workspace.ensure_git_dependency(git_url, git_ref, subpath, project_name, dependency_name)?;
            Ok(ResolvedDependencySource::Local(manifest_dir))
        }
        DependencySourcePreference::Workspace => local_manifest_dir.map(ResolvedDependencySource::Local).ok_or_else(|| {
            PlannerError::ForcedWorkspaceDependencyMissing { project: project_name.to_string(), dependency: dependency_name.to_string() }
        }),
        DependencySourcePreference::Registry => {
            let Some(version) = version_hint
            else {
                return Err(PlannerError::RegistryDependencyMissingVersion {
                    project: project_name.to_string(),
                    dependency: dependency_name.to_string(),
                });
            };
            Ok(ResolvedDependencySource::Registry { version: version.to_string(), registry: registry_hint.to_string() })
        }
        DependencySourcePreference::Auto => {
            if let Some(local_manifest_dir) = local_manifest_dir {
                return Ok(ResolvedDependencySource::Local(local_manifest_dir));
            }
            if let Some(spec) = dependency_spec {
                if spec.git_url().is_some() {
                    return resolve_dependency_source(
                        workspace,
                        project_manifest_dir,
                        path_base,
                        Some(&DependencySpec::Detailed {
                            version: spec.version_hint().map(str::to_string),
                            path: spec.path_hint().map(str::to_string),
                            abi: None,
                            source: Some("git".to_string()),
                            registry: None,
                            git: spec.git_url().map(str::to_string),
                            git_ref: spec.git_ref_hint().map(str::to_string),
                        }),
                        local_manifest_dir,
                        allow_missing_when_auto,
                        project_name,
                        dependency_name,
                    );
                }
                if spec.path_hint().is_some() {
                    return resolve_dependency_source(
                        workspace,
                        project_manifest_dir,
                        path_base,
                        Some(&DependencySpec::Detailed {
                            version: None,
                            path: spec.path_hint().map(str::to_string),
                            abi: None,
                            source: Some("path".to_string()),
                            registry: None,
                            git: None,
                            git_ref: None,
                        }),
                        local_manifest_dir,
                        allow_missing_when_auto,
                        project_name,
                        dependency_name,
                    );
                }
            }
            if let Some(version) = version_hint {
                if version == "workspace" {
                    return local_manifest_dir.map(ResolvedDependencySource::Local).ok_or_else(|| {
                        PlannerError::ForcedWorkspaceDependencyMissing {
                            project: project_name.to_string(),
                            dependency: dependency_name.to_string(),
                        }
                    });
                }
                return Ok(ResolvedDependencySource::Registry { version: version.to_string(), registry: registry_hint.to_string() });
            }
            if allow_missing_when_auto {
                return Ok(ResolvedDependencySource::MissingImplicit);
            }
            Err(PlannerError::MissingDependency { project: project_name.to_string(), dependency: dependency_name.to_string() })
        }
    }
}

fn resolve_local_dependency_path(project_manifest_dir: &Path, raw_path: &str) -> PathBuf {
    let candidate = Path::new(raw_path);
    if candidate.is_absolute() { candidate.to_path_buf() } else { project_manifest_dir.join(candidate) }
}

impl WorkspaceResolver {
    fn git_vendor_root(&self, git_url: &str, git_ref: &str) -> PathBuf {
        let slug = git_cache_slug(git_url, git_ref);
        self.root_dir.join("vendors").join("git").join(slug)
    }

    fn ensure_git_dependency(
        &self,
        git_url: &str,
        git_ref: Option<&str>,
        subpath: Option<&str>,
        project_name: &str,
        dependency_name: &str,
    ) -> Result<PathBuf, PlannerError> {
        let git_ref = git_ref.unwrap_or("main");
        let vendor_root = self.git_vendor_root(git_url, git_ref);
        let manifest_dir = match subpath {
            Some(path) => vendor_root.join(path),
            None => vendor_root.clone(),
        };

        if manifest_dir.join("legion.von").is_file() {
            return Ok(manifest_dir);
        }

        if !vendor_root.join(".git").is_dir() {
            if vendor_root.exists() {
                fs::remove_dir_all(&vendor_root)?;
            }
            if let Some(parent) = vendor_root.parent() {
                fs::create_dir_all(parent)?;
            }
            run_git(
                project_name,
                dependency_name,
                git_url,
                Command::new("git").args(["clone", "--depth", "1", "--branch", git_ref, git_url]).arg(&vendor_root),
            )?;
        }
        else {
            run_git(
                project_name,
                dependency_name,
                git_url,
                Command::new("git").current_dir(&vendor_root).args(["fetch", "origin", git_ref, "--depth", "1"]),
            )?;
            run_git(project_name, dependency_name, git_url, Command::new("git").current_dir(&vendor_root).args(["checkout", "FETCH_HEAD"]))?;
        }

        if !manifest_dir.join("legion.von").is_file() {
            return Err(PlannerError::GitDependencyMissingManifest {
                project: project_name.to_string(),
                dependency: dependency_name.to_string(),
                git_url: git_url.to_string(),
                git_ref: git_ref.to_string(),
                manifest_dir,
            });
        }

        Ok(manifest_dir)
    }
}

fn git_cache_slug(git_url: &str, git_ref: &str) -> String {
    let mut slug = String::new();
    for ch in git_url.chars().chain(git_ref.chars()) {
        slug.push(if ch.is_ascii_alphanumeric() { ch } else { '_' });
    }
    if slug.len() > 96 {
        slug.truncate(96);
    }
    if slug.is_empty() {
        slug.push('x');
    }
    slug
}

fn run_git(project: &str, dependency: &str, git_url: &str, command: &mut Command) -> Result<(), PlannerError> {
    let output = command.output().map_err(PlannerError::Io)?;
    if output.status.success() {
        return Ok(());
    }
    Err(PlannerError::GitDependencyCheckoutFailed {
        project: project.to_string(),
        dependency: dependency.to_string(),
        git_url: git_url.to_string(),
        reason: format_git_failure(command.get_program(), &output.stderr),
    })
}

fn format_git_failure(program: &OsStr, stderr: &[u8]) -> String {
    let stderr_text = String::from_utf8_lossy(stderr);
    let text = stderr_text.trim();
    if text.is_empty() { format!("`{}` exited with failure", program.to_string_lossy()) } else { text.to_string() }
}

/// 将字符串解析为规范化目标平台。
pub fn canonical_target(target: &str) -> Result<CanonicalTarget, rusty_valkyrie::CanonicalTargetParseError> {
    target.parse()
}

fn default_dist_output_dir(project_dir: &Path, target: &CanonicalTarget) -> PathBuf {
    project_dir.join("dist").join(target.as_canonical_str())
}

fn select_build_target(manifest: &ProjectManifest, target: &CanonicalTarget) -> Option<BuildTargetSpec> {
    manifest.build.iter().find(|item| item.target == *target).cloned().or_else(|| synthesize_cli_build_target(manifest, target))
}

/// CLI 显式 `--target` 未写入 `legion.von` 时，为 library 工程合成默认 build 项（单文件库工程等）。
fn synthesize_cli_build_target(manifest: &ProjectManifest, target: &CanonicalTarget) -> Option<BuildTargetSpec> {
    if manifest.artifact != crate::manifest::ProjectArtifactKind::Library {
        return None;
    }
    Some(BuildTargetSpec { target: *target, ..BuildTargetSpec::default() })
}

/// 收集工程源码：存在 `source/` 时取其中全部 `.v`，否则取根目录 `.v`。
pub fn collect_source_files(project_dir: &Path) -> Result<Vec<PathBuf>, PlannerError> {
    let project_dir = path_for_local_fs(project_dir);
    let source_dir = project_dir.join("source");
    // 标准包：`source/` 下全部 `.v`；可执行/导出入口由源码 `[main]` / `[export]` 标记。
    if source_dir.is_dir() {
        let mut files = Vec::new();
        collect_v_files(&source_dir, &mut files)?;
        files.sort();
        return Ok(files);
    }

    let mut files = Vec::new();
    collect_project_root_v_files(&project_dir, &mut files)?;
    files.sort();
    Ok(files)
}

/// 单脚本工程：项目根目录下的 `.v`（无 `source/` / `test/` 目录时使用）。
pub fn collect_project_root_v_files(project_dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), PlannerError> {
    let project_dir = path_for_local_fs(project_dir);
    for entry in fs::read_dir(&project_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "v") {
            files.push(path);
        }
    }
    Ok(())
}

/// 项目是否采用单脚本布局（根目录 `.v`，无 `source/`）。
pub fn project_uses_single_script_layout(project_dir: &Path) -> bool {
    !project_dir.join("source").exists()
}

/// 收集项目 `source/` + `test/` 下的全部 `.v` 文件（测试编译专用）。
pub fn collect_test_build_sources(project_dir: &Path) -> Result<Vec<PathBuf>, PlannerError> {
    let mut files = collect_source_files(project_dir)?;
    let test_dir = project_dir.join("test");
    if test_dir.exists() {
        collect_v_files(&test_dir, &mut files)?;
    }
    files.sort();
    files.dedup();
    Ok(files)
}

/// 仅收集项目 `test/` 下的 `.v` 文件；无 `source/` 布局时包含根目录 `.v`。
pub fn collect_test_v_files(project_dir: &Path) -> Result<Vec<PathBuf>, PlannerError> {
    let mut files = Vec::new();
    let test_dir = project_dir.join("test");
    if test_dir.exists() {
        collect_v_files(&test_dir, &mut files)?;
    }
    else if project_uses_single_script_layout(project_dir) {
        collect_project_root_v_files(project_dir, &mut files)?;
    }
    files.sort();
    Ok(files)
}

/// 收集项目 `source/` 与 `test/` 中全部 `.v` 文件（覆盖率扫描，含 `compile_only/`）。
pub fn collect_project_v_files(project_dir: &Path) -> Result<Vec<PathBuf>, PlannerError> {
    let mut files = collect_source_files(project_dir)?;
    let test_dir = project_dir.join("test");
    if test_dir.exists() {
        collect_v_files_including_compile_only(&test_dir, &mut files)?;
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn collect_v_files_including_compile_only(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), PlannerError> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_v_files_including_compile_only(&path, files)?;
            continue;
        }
        if path.extension().is_some_and(|ext| ext == "v") {
            files.push(path);
        }
    }
    Ok(())
}

fn collect_v_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), PlannerError> {
    let dir = path_for_local_fs(dir);
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "compile_only") {
                continue;
            }
            collect_v_files(&path, files)?;
            continue;
        }
        if path.extension().is_some_and(|ext| ext == "v") {
            files.push(path);
        }
    }
    Ok(())
}

fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    let start_dir = search_start_dir(start);

    // Prefer the outermost `legions.von`. Nested subspaces (e.g. `projects/legion._`)
    // only declare local members; shared deps like `core` / `legion` / `std` live on the
    // parent super-workspace and are registered via nested member recursion.
    let mut outermost = None;
    let mut current = Some(start_dir.as_path());
    while let Some(dir) = current {
        if dir.join("legions.von").exists() {
            outermost = Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    outermost
}

/// 尽力规范化路径；`canonicalize` 失败时原样返回。
pub fn canonicalize_lossy(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Windows `canonicalize` 常返回 `\\?\` 扩展路径；部分 `read_dir` 调用方无法枚举，需还原为常规路径。
pub fn path_for_local_fs(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    if let Some(stripped) = raw.strip_prefix(r"\\?\") { PathBuf::from(stripped) } else { path.to_path_buf() }
}

fn search_start_dir(start: &Path) -> PathBuf {
    if start.is_dir() {
        return canonicalize_lossy(start);
    }

    start.parent().map(canonicalize_lossy).unwrap_or_else(|| start.to_path_buf())
}

fn resolve_project_root(start: &Path) -> Option<PathBuf> {
    let mut current = Some(search_start_dir(start));
    while let Some(dir) = current {
        if dir.join("legion.von").exists() {
            return Some(dir);
        }
        current = dir.parent().map(Path::to_path_buf);
    }
    None
}

fn register_workspace_members(
    workspace_root: &Path,
    workspace_manifest: &WorkspaceManifest,
    projects: &mut BTreeMap<PathBuf, ProjectManifest>,
    projects_by_name: &mut BTreeMap<String, PathBuf>,
) -> Result<(), PlannerError> {
    for member in &workspace_manifest.members {
        let member_dir = canonicalize_lossy(&workspace_root.join(member));
        let nested_workspace_manifest_path = member_dir.join("legions.von");
        if nested_workspace_manifest_path.exists() {
            let nested_workspace_manifest = WorkspaceManifest::parse(&fs::read_to_string(&nested_workspace_manifest_path)?)?;
            register_workspace_members(&member_dir, &nested_workspace_manifest, projects, projects_by_name)?;
        }

        let manifest_path = member_dir.join("legion.von");
        if !manifest_path.exists() {
            continue;
        }

        // 跳过无法解析的 manifest，避免单个测试项目阻塞整个工作区发现。
        // 这类项目通常是语法特性测试，其 manifest 字段不遵循标准格式。
        let manifest = match ProjectManifest::parse(&fs::read_to_string(&manifest_path)?) {
            Ok(manifest) => manifest,
            Err(error) => {
                eprintln!("warning: 跳过无法解析的 manifest {}: {error}", manifest_path.display());
                continue;
            }
        };
        projects_by_name.insert(manifest.name.clone(), member_dir.clone());
        // 同时按 workspace 成员路径的 basename 建立别名索引。
        // 例如 `projects/core` 的 manifest.name 可能是 `valkyrie-core`，
        // 但 auto_link/dependencies 中引用的是逻辑名 `core`（即目录名）。
        if let Some(basename) = Path::new(member).file_name().and_then(|s| s.to_str()) {
            projects_by_name.entry(basename.to_string()).or_insert_with(|| member_dir.clone());
        }
        projects.insert(member_dir, manifest);
    }

    Ok(())
}

fn same_path(lhs: &Path, rhs: &Path) -> bool {
    normalize_path_for_lookup(lhs) == normalize_path_for_lookup(rhs)
}

fn normalize_path_for_lookup(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let stripped = raw.strip_prefix(r"\\?\").unwrap_or(&raw);
    if cfg!(windows) { stripped.to_ascii_lowercase() } else { stripped.to_string() }
}

fn project_visit_key(path: &Path) -> String {
    normalize_path_for_lookup(&path_for_local_fs(path))
}

/// 单次 build 允许纳入编译的最大 `.v` 文件数（含依赖闭包）。
const MAX_SOURCE_CLOSURE_FILES: usize = 512;
/// 语义源码组（依赖图节点）上限。
const MAX_SEMANTIC_GROUPS: usize = 64;
/// 语义依赖展开最大深度。
const MAX_SEMANTIC_GROUP_DEPTH: usize = 32;

fn manifest_supports_build_context(manifest: &ProjectManifest, target: &CanonicalTarget, requested_publish: &[String]) -> bool {
    if !manifest.build.is_empty() && !manifest.build.iter().any(|item| item.target == *target) {
        return false;
    }
    let Some(sdk_vendor) = &manifest.sdk_vendor
    else {
        return true;
    };
    if !sdk_vendor.targets.is_empty() {
        let target_matches = sdk_vendor
            .targets
            .iter()
            .any(|item| item == &target.to_string() || item.parse::<CanonicalTarget>().map(|parsed| parsed == *target).unwrap_or(false));
        if !target_matches {
            return false;
        }
    }
    if !sdk_vendor.publish.is_empty() && !requested_publish.is_empty() {
        let publish_matches = sdk_vendor.publish.iter().any(|item| requested_publish.iter().any(|requested| requested == item));
        if !publish_matches {
            return false;
        }
    }
    true
}

fn dependency_supports_source_context(
    dependency_name: &str,
    manifest: &ProjectManifest,
    target: &CanonicalTarget,
    requested_publish: &[String],
) -> bool {
    // `auto_link.core` 是语言语义依赖，不是一个按目标筛选的产物依赖。
    // core 的 manifest 只描述 seed 语义构建目标；若用同一过滤器匹配
    // Node/Wasm 或其他 backend target，会在进入 Compiler 前静默丢失 core。
    dependency_name == "core" || manifest_supports_build_context(manifest, target, requested_publish)
}

fn implicit_sdk_dependencies(publish: &[String]) -> Vec<&'static str> {
    let mut deps = Vec::new();
    if publish.iter().any(|item| item == "mini-game") {
        deps.push("tencent.wechat.sdk");
    }
    if publish.iter().any(|item| item == "mini-program") {
        deps.push("tencent.wechat.miniprogram.sdk");
    }
    if publish.iter().any(|item| item == "unity-player") {
        deps.push("unity.engine.sdk");
    }
    deps
}

/// 把发行版默认 SDK 并入根工程的候选依赖集合。
///
/// 排除工程自身（默认 SDK 不应依赖自己），并尊重显式 `Disabled`：显式禁用
/// 是装配层的收窄事实，不能被默认注入覆盖。是否真正进入闭包仍由后续
/// target / publish / sdk-vendor 过滤决定。
fn add_distribution_default_dependencies(
    manifest: &ProjectManifest,
    defaults: &BTreeSet<String>,
    names: &mut BTreeSet<String>,
) {
    for default_name in defaults {
        if default_name == &manifest.name {
            continue;
        }
        if matches!(manifest.dependencies.get(default_name), Some(DependencySpec::Disabled)) {
            continue;
        }
        names.insert(default_name.clone());
    }
}

fn manifest_supports_target(manifest: &ProjectManifest, target: &CanonicalTarget) -> bool {
    manifest_supports_build_context(manifest, target, &[])
}

#[derive(Debug, Default)]
struct HostInventory {
    contracts: Vec<PlannedHostContract>,
    providers: Vec<PlannedHostProvider>,
    selected_providers: Vec<PlannedHostProvider>,
}

fn collect_host_inventory(groups: &[PlannedSemanticSourceGroup], publish: &[String]) -> Result<HostInventory, PlannerError> {
    let mut inventory = HostInventory::default();
    for group in groups {
        let manifest_dir = fs::canonicalize(&group.manifest_dir)?;
        for source_file in &group.source_files {
            let resolved = fs::canonicalize(source_file)?;
            // 逻辑路径必须与 `source_snapshot` 送给 Compiler 的路径一致：
            // 声明身份由（依赖组，manifest 相对路径，namespace，种类，拥有者，短名）共同构成。
            let source_path = resolved
                .strip_prefix(&manifest_dir)
                .map_err(|_| {
                    PlannerError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("源码文件 {} 不属于源码组 `{}` 的 manifest 目录", source_file.display(), group.dependency_key),
                    ))
                })?
                .to_string_lossy()
                .replace('\\', "/");
            let source = fs::read_to_string(source_file)?;
            collect_host_attributes_from_file(&group.dependency_key, &source_path, source_file, &source, &mut inventory);
        }
    }

    inventory
        .contracts
        .sort_by(|left, right| left.id.cmp(&right.id).then(left.source_file.cmp(&right.source_file)).then(left.line.cmp(&right.line)));
    // 模板分支可能重复声明同一身份；声明身份不含行号，重复项必须收敛为同一事实。
    inventory.contracts.dedup_by(|left, right| left.key == right.key);
    inventory.providers.sort_by(|left, right| {
        left.contract
            .cmp(&right.contract)
            .then(left.symbol.cmp(&right.symbol))
            .then(left.source_file.cmp(&right.source_file))
            .then(left.line.cmp(&right.line))
    });
    inventory.providers.dedup_by(|left, right| left.key == right.key && left.contract == right.contract);
    inventory.selected_providers = select_host_providers(&inventory.contracts, &inventory.providers, publish)?;
    Ok(inventory)
}

fn collect_host_attributes_from_file(dependency_key: &str, source_path: &str, source_file: &Path, source: &str, inventory: &mut HostInventory) {
    let mut namespace: Vec<String> = Vec::new();
    let mut pending_host_contract: Option<usize> = None;
    let mut pending_host_providers: Vec<(String, usize)> = Vec::new();
    let mut owner_stack: Vec<(String, usize, DeclarationKind)> = Vec::new();
    let mut brace_depth: usize = 0;

    for (index, raw_line) in source.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") || line.starts_with('⍝') {
            continue;
        }

        let open_brace_count = line.chars().filter(|ch| *ch == '{').count();
        let close_brace_count = line.chars().filter(|ch| *ch == '}').count();
        let parsed_owner = parse_owner_declaration(line);

        if let Some(parsed_namespace) = parse_namespace(line) {
            namespace = namespace_segments(parsed_namespace);
            continue;
        }

        if let Some(attributes) = parse_attribute_items(line) {
            for attribute in attributes {
                if attribute == "host_contract" {
                    pending_host_contract = Some(line_number);
                    continue;
                }

                if let Some(contract) = parse_host_provider_attribute(attribute) {
                    pending_host_providers.push((contract.to_string(), line_number));
                }
            }
            continue;
        }

        let Some(symbol_name) = parse_symbol_name(line)
        else {
            let next_brace_depth = brace_depth.saturating_add(open_brace_count).saturating_sub(close_brace_count);
            if let Some((owner_name, owner_kind)) = parsed_owner {
                if next_brace_depth > brace_depth {
                    owner_stack.push((owner_name, next_brace_depth, owner_kind));
                }
            }
            brace_depth = next_brace_depth;
            while owner_stack.last().map(|(_, depth, _)| brace_depth < *depth).unwrap_or(false) {
                owner_stack.pop();
            }
            continue;
        };
        let (owner, kind) = match owner_stack.last() {
            Some((owner_name, _, owner_kind)) => (Some(owner_name.as_str()), *owner_kind),
            None => (None, DeclarationKind::Micro),
        };
        let qualified_symbol = qualified_declaration_name(&namespace, owner, symbol_name);
        let key = DeclarationKey {
            dependency_key: dependency_key.to_string(),
            source_path: source_path.to_string(),
            namespace: namespace.clone(),
            kind,
            owner: owner.map(str::to_string),
            name: symbol_name.to_string(),
        };

        if let Some(contract_line) = pending_host_contract.take() {
            inventory.contracts.push(PlannedHostContract {
                id: qualified_symbol.clone(),
                key: key.clone(),
                source_file: source_file.to_path_buf(),
                line: contract_line,
            });
        }

        for (contract, provider_line) in pending_host_providers.drain(..) {
            inventory.providers.push(PlannedHostProvider {
                contract,
                contract_key: None,
                symbol: qualified_symbol.clone(),
                key: key.clone(),
                source_file: source_file.to_path_buf(),
                line: provider_line,
            });
        }

        let next_brace_depth = brace_depth.saturating_add(open_brace_count).saturating_sub(close_brace_count);
        if let Some((owner_name, owner_kind)) = parsed_owner {
            if next_brace_depth > brace_depth {
                owner_stack.push((owner_name, next_brace_depth, owner_kind));
            }
        }
        brace_depth = next_brace_depth;
        while owner_stack.last().map(|(_, depth, _)| brace_depth < *depth).unwrap_or(false) {
            owner_stack.pop();
        }
    }
}

fn select_host_providers(
    contracts: &[PlannedHostContract],
    providers: &[PlannedHostProvider],
    publish: &[String],
) -> Result<Vec<PlannedHostProvider>, PlannerError> {
    // 只保留声明了对应 contract 的 provider；contract 本身由 language inventory 与 closure 决定，
    // 不由 target 侧裁剪。属性里允许 `.` / `::` 两种书写，匹配前统一为 `::` 分隔的限定名。
    let contracts_by_id: BTreeMap<&str, &PlannedHostContract> = contracts.iter().map(|item| (item.id.as_str(), item)).collect();
    let mut providers_by_contract: BTreeMap<String, Vec<&PlannedHostProvider>> = BTreeMap::new();
    for provider in providers {
        let contract_id = normalized_contract_reference(&provider.contract);
        if !contracts_by_id.contains_key(contract_id.as_str()) {
            continue;
        }
        providers_by_contract.entry(contract_id).or_default().push(provider);
    }

    let mut selected = Vec::new();
    for contract in contracts {
        let Some(candidates) = providers_by_contract.get(&contract.id)
        else {
            continue;
        };

        let chosen = if candidates.len() == 1 {
            candidates[0]
        }
        else {
            candidates.iter().copied().min_by_key(|provider| host_provider_priority(&provider.source_file, publish)).ok_or_else(|| {
                PlannerError::ConflictingHostProviders {
                    contract: contract.id.clone(),
                    providers: candidates
                        .iter()
                        .map(|item| format!("{} at {}:{}", item.symbol, item.source_file.display(), item.line))
                        .collect(),
                }
            })?
        };

        let mut chosen = chosen.clone();
        chosen.contract_key = Some(contract.key.clone());
        selected.push(chosen);
    }

    selected.sort_by(|left, right| {
        left.contract
            .cmp(&right.contract)
            .then(left.symbol.cmp(&right.symbol))
            .then(left.source_file.cmp(&right.source_file))
            .then(left.line.cmp(&right.line))
    });
    Ok(selected)
}

fn host_provider_priority(source_file: &Path, publish: &[String]) -> u8 {
    let path = source_file.to_string_lossy();
    if publish.iter().any(|item| item == "unity-player") && path.contains("unity.engine.sdk") {
        return 0;
    }
    if publish.iter().any(|item| item == "mini-game") && path.contains("tencent.wechat.sdk") {
        return 0;
    }
    if publish.iter().any(|item| item == "mini-program") && path.contains("tencent.wechat.miniprogram.sdk") {
        return 0;
    }
    if path.contains("std.adaptor.") {
        return 2;
    }
    1
}

fn parse_namespace(line: &str) -> Option<&str> {
    line.strip_prefix("namespace ").or_else(|| line.strip_prefix("namespace! ")).and_then(|value| value.strip_suffix(';')).map(str::trim)
}

fn parse_attribute_items(line: &str) -> Option<Vec<&str>> {
    let inner = line.strip_prefix('[')?.strip_suffix(']')?.trim();
    if inner.is_empty() {
        return Some(Vec::new());
    }

    let mut items = Vec::new();
    let mut start = 0;
    let mut paren_depth = 0usize;
    let mut in_string = false;
    let mut previous_was_escape = false;

    for (index, ch) in inner.char_indices() {
        match ch {
            '"' if !previous_was_escape => {
                in_string = !in_string;
            }
            '(' if !in_string => {
                paren_depth += 1;
            }
            ')' if !in_string => {
                paren_depth = paren_depth.saturating_sub(1);
            }
            ',' if !in_string && paren_depth == 0 => {
                items.push(inner[start..index].trim());
                start = index + ch.len_utf8();
            }
            _ => {}
        }

        if ch == '\\' {
            previous_was_escape = !previous_was_escape;
        }
        else {
            previous_was_escape = false;
        }
    }

    items.push(inner[start..].trim());
    Some(items)
}

fn parse_host_provider_attribute(attribute: &str) -> Option<&str> {
    let inner = attribute.strip_prefix("host_provider(")?.strip_suffix(')')?.trim();
    if let Some(value) = inner.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')) {
        return Some(value);
    }

    Some(inner)
}

fn parse_symbol_name(line: &str) -> Option<&str> {
    let micro_offset = line.find("micro ")?;
    let rest = &line[micro_offset + "micro ".len()..];
    let end = rest.find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')).unwrap_or(rest.len());
    if end == 0 {
        return None;
    }
    Some(&rest[..end])
}

/// 解析成员拥有者及其声明种类：`imply` 块产生 `ImplyMethod`，`class`/`structure`/`trait`
/// 成员产生 `Method`。种类是冻结声明身份的一部分，猜错会导致 Compiler 查不到声明。
fn parse_owner_declaration(line: &str) -> Option<(String, DeclarationKind)> {
    let (rest, kind) = if let Some(rest) = line.strip_prefix("imply ") {
        (rest, DeclarationKind::ImplyMethod)
    }
    else if let Some(rest) = line.strip_prefix("class ").or_else(|| line.strip_prefix("structure ")).or_else(|| line.strip_prefix("trait ")) {
        (rest, DeclarationKind::Method)
    }
    else {
        return None;
    };
    let end = rest.find(|ch: char| ch.is_whitespace() || matches!(ch, '{' | '<' | '>' | '(' | ')' | ';' | '=')).unwrap_or(rest.len());
    let owner = rest[..end].rsplit([':', '.']).find(|segment| !segment.is_empty())?;
    Some((owner.to_string(), kind))
}

/// namespace 段：源码允许 `.` / `::` 两种书写，统一拆成段。
fn namespace_segments(namespace: &str) -> Vec<String> {
    namespace.split(['.', ':']).map(str::trim).filter(|segment| !segment.is_empty()).map(str::to_string).collect()
}

fn qualified_declaration_name(namespace: &[String], owner: Option<&str>, symbol_name: &str) -> String {
    namespace.iter().map(String::as_str).chain(owner).chain(std::iter::once(symbol_name)).collect::<Vec<_>>().join("::")
}

/// 属性里的契约引用统一为 `::` 分隔的限定名，只用于匹配，不据此反推声明身份。
fn normalized_contract_reference(reference: &str) -> String {
    namespace_segments(reference).join("::")
}
