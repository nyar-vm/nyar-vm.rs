use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
};
use tracing::{debug, info, warn};

/// legion.json 配置文件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    pub version: String,
    pub dependencies: Option<HashMap<String, DependencyConfig>>,
}

/// 依赖配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DependencyConfig {
    Simple(String),
    Detailed { version: Option<String>, path: Option<String>, vendor: Option<String>, workspace: Option<bool> },
}

impl DependencyConfig {
    /// 获取依赖版本
    pub fn version(&self) -> Option<&str> {
        match self {
            DependencyConfig::Simple(v) => Some(v),
            DependencyConfig::Detailed { version, .. } => version.as_deref(),
        }
    }

    /// 获取依赖路径
    pub fn path(&self) -> Option<&str> {
        match self {
            DependencyConfig::Simple(_) => None,
            DependencyConfig::Detailed { path, .. } => path.as_deref(),
        }
    }

    /// 是否为 workspace 依赖
    pub fn is_workspace(&self) -> bool {
        match self {
            DependencyConfig::Simple(_) => false,
            DependencyConfig::Detailed { workspace, .. } => workspace.unwrap_or(false),
        }
    }
}

/// legions.json 工作空间配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub packages: Vec<String>,
    pub dependencies: Option<HashMap<String, DependencyConfig>>,
}

/// 依赖版本冲突信息
#[derive(Debug, Clone)]
pub struct VersionConflict {
    /// 依赖名称
    pub name: String,
    /// 冲突的版本需求
    pub conflicting_versions: Vec<(String, String)>,
}

/// 依赖图节点
#[derive(Debug, Clone)]
pub struct DependencyNode {
    /// 包名称
    pub name: String,
    /// 包版本
    pub version: String,
    /// 包路径
    pub path: PathBuf,
    /// 直接依赖
    pub dependencies: Vec<String>,
}

/// 循环依赖检测结果
#[derive(Debug, Clone)]
pub struct CycleDetectionResult {
    /// 是否存在循环依赖
    pub has_cycle: bool,
    /// 循环依赖路径
    pub cycle_path: Vec<String>,
}

/// 依赖提升结果
#[derive(Debug, Clone)]
pub struct HoistedDependency {
    /// 依赖名称
    pub name: String,
    /// 提升后的版本
    pub version: String,
    /// 引用此依赖的成员数量
    pub ref_count: usize,
    /// 依赖路径
    pub path: PathBuf,
}

/// Legion 包管理器
pub struct LegionManager {
    /// %LEGION_ROOT%
    root: Option<PathBuf>,
    /// 工作区根目录
    workspace_root: Option<PathBuf>,
    /// 工作区配置 (legions.json)
    workspace_config: Option<WorkspaceConfig>,
    /// 项目配置缓存 (目录 -> 配置)
    projects: HashMap<PathBuf, ProjectConfig>,
    /// 项目名称到路径的映射
    project_names: HashMap<String, PathBuf>,
    /// 依赖图缓存
    dependency_graph: HashMap<String, DependencyNode>,
    /// 提升后的依赖缓存
    hoisted_dependencies: HashMap<String, HoistedDependency>,
    /// 版本冲突缓存
    version_conflicts: Vec<VersionConflict>,
}

impl Default for LegionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LegionManager {
    pub fn new() -> Self {
        let root = std::env::var("LEGION_ROOT").ok().map(PathBuf::from);
        Self {
            root,
            workspace_root: None,
            workspace_config: None,
            projects: HashMap::new(),
            project_names: HashMap::new(),
            dependency_graph: HashMap::new(),
            hoisted_dependencies: HashMap::new(),
            version_conflicts: Vec::new(),
        }
    }

    /// 设置工作区根目录并扫描配置
    pub fn set_workspace_root(&mut self, root: PathBuf) {
        self.workspace_root = Some(root.clone());
        self.scan_workspace(&root);
        self.scan_vendor();
        self.build_project_name_map();
        self.build_dependency_graph();
        self.hoist_dependencies();
    }

    /// 构建项目名称到路径的映射
    fn build_project_name_map(&mut self) {
        self.project_names.clear();
        for (path, config) in &self.projects {
            self.project_names.insert(config.name.clone(), path.clone());
        }
    }

    /// 构建依赖图
    fn build_dependency_graph(&mut self) {
        self.dependency_graph.clear();
        for (path, config) in &self.projects {
            let mut deps = Vec::new();
            if let Some(dependencies) = &config.dependencies {
                for (dep_name, _) in dependencies {
                    deps.push(dep_name.clone());
                }
            }
            let node = DependencyNode {
                name: config.name.clone(),
                version: config.version.clone(),
                path: path.clone(),
                dependencies: deps,
            };
            self.dependency_graph.insert(config.name.clone(), node);
        }
    }

    /// 依赖提升：将共享依赖提升到 workspace 级别
    fn hoist_dependencies(&mut self) {
        self.hoisted_dependencies.clear();
        self.version_conflicts.clear();

        let mut dep_usage: HashMap<String, Vec<(String, String, PathBuf)>> = HashMap::new();

        for (path, config) in &self.projects {
            if let Some(deps) = &config.dependencies {
                for (dep_name, dep_config) in deps {
                    if dep_config.is_workspace() {
                        continue;
                    }

                    let version = dep_config.version().unwrap_or("*").to_string();
                    let resolved_path =
                        self.resolve_dep_path(dep_name, dep_config, path).unwrap_or_else(|| PathBuf::from(dep_name));

                    dep_usage.entry(dep_name.clone()).or_insert_with(Vec::new).push((
                        config.name.clone(),
                        version,
                        resolved_path,
                    ));
                }
            }
        }

        for (dep_name, usages) in dep_usage {
            if usages.len() > 1 {
                let versions: HashSet<String> = usages.iter().map(|(_, v, _)| v.clone()).collect();

                if versions.len() > 1 {
                    let conflicting_versions: Vec<(String, String)> =
                        usages.iter().map(|(pkg, v, _)| (pkg.clone(), v.clone())).collect();
                    self.version_conflicts.push(VersionConflict { name: dep_name.clone(), conflicting_versions });
                    warn!("Version conflict detected for dependency '{}': {:?}", dep_name, versions);
                }

                let (first_pkg, first_version, first_path) = &usages[0];
                self.hoisted_dependencies.insert(
                    dep_name.clone(),
                    HoistedDependency {
                        name: dep_name,
                        version: first_version.clone(),
                        ref_count: usages.len(),
                        path: first_path.clone(),
                    },
                );
                debug!("Hoisted dependency '{}' (version: {}, used by {} packages)", first_pkg, first_version, usages.len());
            }
        }
    }

    /// 扫描供应商目录
    fn scan_vendor(&mut self) {
        if let Some(root) = &self.root {
            let vendor_dir = root.join("vendor");
            if vendor_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&vendor_dir) {
                    for entry in entries.flatten() {
                        let vendor_path = entry.path();
                        if vendor_path.is_dir() {
                            if let Ok(pkg_entries) = std::fs::read_dir(&vendor_path) {
                                for pkg_entry in pkg_entries.flatten() {
                                    let pkg_path = pkg_entry.path();
                                    if pkg_path.is_dir() {
                                        self.scan_project_dir(&pkg_path);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// 扫描工作区中的 legion.json/toml 和 legions.json/toml
    fn scan_workspace(&mut self, root: &Path) {
        // 1. 尝试加载 legions.json 或 legions.toml
        let legions_json = root.join("legions.json");
        let legions_toml = root.join("legions.toml");

        if legions_json.exists() {
            if let Ok(content) = std::fs::read_to_string(&legions_json) {
                if let Ok(config) = serde_json::from_str::<WorkspaceConfig>(&content) {
                    info!("Found Legion workspace at {:?}", root);
                    self.workspace_config = Some(config);
                }
            }
        }
        else if legions_toml.exists() {
            if let Ok(content) = std::fs::read_to_string(&legions_toml) {
                if let Ok(config) = toml::from_str::<WorkspaceConfig>(&content) {
                    info!("Found Legion workspace (TOML) at {:?}", root);
                    self.workspace_config = Some(config);
                }
            }
        }

        // 2. 递归扫描子目录中的 legion.json/toml
        // 如果有 legions.json/toml，按 packages 列表扫描
        let packages = self.workspace_config.as_ref().map(|c| c.packages.clone());
        if let Some(packages) = packages {
            for pkg_pattern in &packages {
                if pkg_pattern.contains('*') {
                    // 简单的通配符支持: "dir/*"
                    if let Some(base) = pkg_pattern.strip_suffix("/*") {
                        let base_path = root.join(base);
                        if let Ok(entries) = std::fs::read_dir(base_path) {
                            for entry in entries.flatten() {
                                if entry.path().is_dir() {
                                    self.scan_project_dir(&entry.path());
                                }
                            }
                        }
                    }
                }
                else {
                    let pkg_path = root.join(pkg_pattern);
                    if pkg_path.is_dir() {
                        self.scan_project_dir(&pkg_path);
                    }
                }
            }
        }
        else {
            // 没有工作区配置，扫描当前目录
            self.scan_project_dir(root);
        }
    }

    /// 扫描单个项目目录
    fn scan_project_dir(&mut self, path: &Path) {
        let legion_json = path.join("legion.json");
        let legion_toml = path.join("legion.toml");

        if legion_json.exists() {
            if let Ok(content) = std::fs::read_to_string(&legion_json) {
                if let Ok(config) = serde_json::from_str::<ProjectConfig>(&content) {
                    debug!("Found project at {:?}", path);
                    self.projects.insert(path.to_path_buf(), config);
                }
            }
        }
        else if legion_toml.exists() {
            if let Ok(content) = std::fs::read_to_string(&legion_toml) {
                if let Ok(config) = toml::from_str::<ProjectConfig>(&content) {
                    debug!("Found project (TOML) at {:?}", path);
                    self.projects.insert(path.to_path_buf(), config);
                }
            }
        }
    }

    /// 解析包路径
    pub fn resolve_package(&self, name: &str, current_file: &str) -> Option<PathBuf> {
        let current_path = Path::new(current_file);
        let current_dir = current_path.parent()?;

        // 1. 检查是否引用自身 (using package)
        if name == "package" {
            if let Some(config) = self.find_project_config(current_dir) {
                // 返回项目根目录，或者 library 目录
                let project_dir =
                    self.projects.keys().find(|p| self.projects.get(*p).map(|c| c.name == config.name).unwrap_or(false))?;
                let lib_dir = project_dir.join("library");
                return if lib_dir.exists() { Some(lib_dir) } else { Some(project_dir.to_path_buf()) };
            }
        }

        // 2. 检查当前项目的依赖
        if let Some(config) = self.find_project_config(current_dir) {
            if let Some(deps) = &config.dependencies {
                if let Some(dep) = deps.get(name) {
                    if let Some(path) = self.resolve_dep_path(name, dep, current_dir) {
                        return Some(path);
                    }
                }
            }
            // 如果包名就是项目名，返回自身
            if config.name == name {
                return self.resolve_package("package", current_file);
            }
        }

        // 3. 检查工作空间级别的共享依赖
        if let Some(config) = &self.workspace_config {
            if let Some(deps) = &config.dependencies {
                if let Some(dep) = deps.get(name) {
                    if let Some(path) = self.resolve_dep_path(name, dep, self.workspace_root.as_ref()?) {
                        return Some(path);
                    }
                }
            }
        }

        // 4. 检查本地供应商缓存 %LEGION_ROOT%/vendor/
        if let Some(root) = &self.root {
            let vendor_dir = root.join("vendor");
            if vendor_dir.exists() {
                // 搜索所有供应商目录
                if let Ok(entries) = std::fs::read_dir(&vendor_dir) {
                    for entry in entries.flatten() {
                        let vendor_path = entry.path();
                        if vendor_path.is_dir() {
                            let pkg_path = vendor_path.join(name);
                            if pkg_path.exists() {
                                return Some(pkg_path);
                            }
                            // 检查带版本的目录 name@version
                            if let Ok(pkg_entries) = std::fs::read_dir(&vendor_path) {
                                for pkg_entry in pkg_entries.flatten() {
                                    let p = pkg_entry.path();
                                    if let Some(n) = p.file_name().and_then(|s| s.to_str()) {
                                        if n.starts_with(&format!("{}@", name)) {
                                            return Some(p);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        None
    }

    fn resolve_dep_path(&self, name: &str, dep: &DependencyConfig, base_dir: &Path) -> Option<PathBuf> {
        match dep {
            DependencyConfig::Simple(version) => {
                // 仅版本号，去 vendor 找
                self.find_in_vendor(name, Some(version))
            }
            DependencyConfig::Detailed { path, version, .. } => {
                if let Some(p) = path {
                    let full_path = base_dir.join(p);
                    if full_path.exists() {
                        return Some(full_path);
                    }
                }

                self.find_in_vendor(name, version.as_deref())
            }
        }
    }

    fn find_in_vendor(&self, name: &str, version: Option<&str>) -> Option<PathBuf> {
        let root = self.root.as_ref()?;
        let vendor_dir = root.join("vendor");
        if !vendor_dir.exists() {
            return None;
        }

        if let Ok(entries) = std::fs::read_dir(&vendor_dir) {
            for entry in entries.flatten() {
                let vendor_path = entry.path();
                if vendor_path.is_dir() {
                    if let Some(v) = version {
                        let pkg_v = vendor_path.join(format!("{}@{}", name, v));
                        if pkg_v.exists() {
                            return Some(pkg_v);
                        }
                    }
                    let pkg = vendor_path.join(name);
                    if pkg.exists() {
                        return Some(pkg);
                    }
                }
            }
        }
        None
    }

    fn find_project_config(&self, dir: &Path) -> Option<&ProjectConfig> {
        let mut curr = Some(dir);
        while let Some(d) = curr {
            if let Some(config) = self.projects.get(d) {
                return Some(config);
            }
            curr = d.parent();
        }
        None
    }

    /// 获取包的所有源文件
    pub fn get_package_sources(&self, package_dir: &Path) -> Vec<PathBuf> {
        let mut sources = Vec::new();
        // 优先检查 library 目录
        let lib_dir = package_dir.join("library");
        let search_dir = if lib_dir.exists() { lib_dir } else { package_dir.to_path_buf() };

        self.collect_valkyrie_source_files(&search_dir, &mut sources);
        sources
    }

    fn collect_valkyrie_source_files(&self, dir: &Path, sources: &mut Vec<PathBuf>) {
        use crate::formatter::valkyrie_family_extensions;
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    self.collect_valkyrie_source_files(&path, sources);
                }
                else if path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| valkyrie_family_extensions().any(|e| e == ext))
                {
                    sources.push(path);
                }
            }
        }
    }

    /// 获取所有可发现的包根目录
    pub fn get_all_packages(&self) -> Vec<PathBuf> {
        let mut packages = Vec::new();
        // 1. 工作区中的项目
        for path in self.projects.keys() {
            packages.push(path.clone());
        }

        // 2. 检查本地供应商缓存 %LEGION_ROOT%/vendor/
        if let Some(root) = &self.root {
            let vendor_dir = root.join("vendor");
            if vendor_dir.exists() {
                self.scan_vendor_recursive(&vendor_dir, &mut packages);
            }
        }

        packages.sort();
        packages.dedup();
        packages
    }

    fn scan_vendor_recursive(&self, vendor_dir: &Path, packages: &mut Vec<PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(vendor_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if path.join("legion.json").exists() {
                        packages.push(path);
                    }
                    else {
                        self.scan_vendor_recursive(&path, packages);
                    }
                }
            }
        }
    }

    /// 解析 workspace 成员间的依赖关系
    pub fn resolve_workspace_member(&self, name: &str) -> Option<PathBuf> {
        self.project_names.get(name).cloned()
    }

    /// 检测循环依赖
    pub fn detect_cycles(&self) -> CycleDetectionResult {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut path = Vec::new();

        for node_name in self.dependency_graph.keys() {
            if !visited.contains(node_name) {
                if let Some(cycle) = self.dfs_detect_cycle(node_name, &mut visited, &mut rec_stack, &mut path) {
                    return CycleDetectionResult { has_cycle: true, cycle_path: cycle };
                }
            }
        }

        CycleDetectionResult { has_cycle: false, cycle_path: Vec::new() }
    }

    fn dfs_detect_cycle(
        &self,
        node_name: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        visited.insert(node_name.to_string());
        rec_stack.insert(node_name.to_string());
        path.push(node_name.to_string());

        if let Some(node) = self.dependency_graph.get(node_name) {
            for dep_name in &node.dependencies {
                if !visited.contains(dep_name) {
                    if let Some(cycle) = self.dfs_detect_cycle(dep_name, visited, rec_stack, path) {
                        return Some(cycle);
                    }
                }
                else if rec_stack.contains(dep_name) {
                    let cycle_start = path.iter().position(|n| n == dep_name).unwrap_or(0);
                    let mut cycle = path[cycle_start..].to_vec();
                    cycle.push(dep_name.to_string());
                    return Some(cycle);
                }
            }
        }

        path.pop();
        rec_stack.remove(node_name);
        None
    }

    /// 获取拓扑排序后的构建顺序
    pub fn get_build_order(&self) -> Result<Vec<String>, CycleDetectionResult> {
        let cycle_result = self.detect_cycles();
        if cycle_result.has_cycle {
            return Err(cycle_result);
        }

        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut graph: HashMap<String, Vec<String>> = HashMap::new();

        for (name, node) in &self.dependency_graph {
            in_degree.entry(name.clone()).or_insert(0);
            for dep in &node.dependencies {
                graph.entry(dep.clone()).or_insert_with(Vec::new).push(name.clone());
                *in_degree.entry(name.clone()).or_insert(0) += 1;
            }
        }

        let mut queue: VecDeque<String> = VecDeque::new();
        for (name, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(name.clone());
            }
        }

        let mut result = Vec::new();
        while let Some(node) = queue.pop_front() {
            result.push(node.clone());
            if let Some(neighbors) = graph.get(&node) {
                for neighbor in neighbors {
                    if let Some(degree) = in_degree.get_mut(neighbor) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(neighbor.clone());
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    /// 获取提升后的依赖列表
    pub fn get_hoisted_dependencies(&self) -> &HashMap<String, HoistedDependency> {
        &self.hoisted_dependencies
    }

    /// 获取版本冲突列表
    pub fn get_version_conflicts(&self) -> &[VersionConflict] {
        &self.version_conflicts
    }

    /// 获取依赖图
    pub fn get_dependency_graph(&self) -> &HashMap<String, DependencyNode> {
        &self.dependency_graph
    }

    /// 获取指定包的传递依赖
    pub fn get_transitive_dependencies(&self, package_name: &str) -> Vec<String> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        self.collect_transitive_deps(package_name, &mut result, &mut visited);
        result
    }

    fn collect_transitive_deps(&self, name: &str, result: &mut Vec<String>, visited: &mut HashSet<String>) {
        if visited.contains(name) {
            return;
        }
        visited.insert(name.to_string());

        if let Some(node) = self.dependency_graph.get(name) {
            for dep in &node.dependencies {
                if !visited.contains(dep) {
                    result.push(dep.clone());
                    self.collect_transitive_deps(dep, result, visited);
                }
            }
        }
    }

    /// 解析成员间依赖路径
    pub fn resolve_member_dependency(&self, from_package: &str, to_package: &str) -> Option<PathBuf> {
        if let Some(from_path) = self.project_names.get(from_package) {
            if let Some(from_config) = self.projects.get(from_path) {
                if let Some(deps) = &from_config.dependencies {
                    if let Some(dep_config) = deps.get(to_package) {
                        return self.resolve_dep_path(to_package, dep_config, from_path);
                    }
                }
            }
        }

        self.project_names.get(to_package).cloned()
    }

    /// 检查是否存在成员间依赖
    pub fn has_member_dependency(&self, from_package: &str, to_package: &str) -> bool {
        if let Some(from_path) = self.project_names.get(from_package) {
            if let Some(from_config) = self.projects.get(from_path) {
                if let Some(deps) = &from_config.dependencies {
                    return deps.contains_key(to_package);
                }
            }
        }
        false
    }

    /// 获取 workspace 成员列表
    pub fn get_workspace_members(&self) -> Vec<String> {
        self.project_names.keys().cloned().collect()
    }

    /// 获取指定包的配置信息
    pub fn get_package_config(&self, name: &str) -> Option<&ProjectConfig> {
        self.project_names.get(name).and_then(|path| self.projects.get(path))
    }

    /// 获取指定包的路径
    pub fn get_package_path(&self, name: &str) -> Option<&PathBuf> {
        self.project_names.get(name)
    }
}
