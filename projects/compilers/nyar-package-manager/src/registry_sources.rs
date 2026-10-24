use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
};

use nyar_package_registry::{Registry, registries_with_endpoints};
use serde::{Deserialize, Serialize};

use crate::{PackageManagerError, ProjectLayout, Result};

/// Persistent registry endpoint configuration.
///
/// Built-in endpoint defaults come from `nyar-package-registry`; this layer does not
/// interpret registry product names.
#[derive(Debug, Clone)]
pub struct RegistrySourceManager {
    sources: BTreeMap<String, String>,
    path: PathBuf,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SourcesFile {
    #[serde(default)]
    sources: BTreeMap<String, String>,
}

impl RegistrySourceManager {
    /// Open using paths from the product layout.
    pub fn open_with_layout(layout: ProjectLayout) -> Result<Self> {
        Self::open(layout.registry_sources_path())
    }

    /// Open with neutral layout paths (tests / tools without a product CLI).
    pub fn open_default() -> Result<Self> {
        Self::open_with_layout(ProjectLayout::neutral())
    }

    /// 打开 `registry-sources.von`；文件缺失时使用内置默认 endpoint 表。
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut manager = Self { sources: default_sources()?, path };
        manager.load()?;
        Ok(manager)
    }

    /// 返回产品布局下的注册表 endpoint 配置文件路径。
    pub fn path_for(layout: ProjectLayout) -> PathBuf {
        layout.registry_sources_path()
    }

    /// 当前打开的 `registry-sources.von` 路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 注册表名称到 endpoint URL 的只读映射。
    pub fn sources(&self) -> &BTreeMap<String, String> {
        &self.sources
    }

    /// 查询单个注册表的 endpoint；名称不区分大小写。
    pub fn get_endpoint(&self, registry_name: &str) -> Option<&str> {
        self.sources.get(&registry_name.to_ascii_lowercase()).map(String::as_str)
    }

    /// 列出所有 `(registry, endpoint)` 配置对。
    pub fn list(&self) -> Vec<(String, String)> {
        self.sources.iter().map(|(name, endpoint)| (name.clone(), endpoint.clone())).collect()
    }

    /// 新增或覆盖注册表 endpoint 并持久化。
    pub fn add(&mut self, registry_name: &str, endpoint: &str) -> Result<()> {
        let name = registry_name.to_ascii_lowercase();
        if name.trim().is_empty() {
            return Err(PackageManagerError::message("注册表名称不能为空"));
        }
        let endpoint = endpoint.trim().trim_end_matches('/');
        if endpoint.is_empty() {
            return Err(PackageManagerError::message("注册表 endpoint 不能为空"));
        }
        self.sources.insert(name, endpoint.to_string());
        self.save()
    }

    /// 删除自定义 endpoint；内置注册表会恢复为默认值。
    pub fn remove(&mut self, registry_name: &str) -> Result<bool> {
        let name = registry_name.to_ascii_lowercase();
        let defaults = default_sources()?;
        if defaults.contains_key(&name) {
            if let Some(default) = defaults.get(&name).cloned() {
                self.sources.insert(name, default);
                self.save()?;
                return Ok(true);
            }
        }
        let removed = self.sources.remove(&name).is_some();
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    /// 返回 `(名称, endpoint, 是否为内置默认值)`。
    pub fn info(&self, registry_name: &str) -> Result<(String, String, bool)> {
        let name = registry_name.to_ascii_lowercase();
        let endpoint = self.get_endpoint(&name).ok_or_else(|| PackageManagerError::message(format!("注册表 {name} 未配置")))?.to_string();
        let is_default = default_sources()?.get(&name).map(String::as_str) == Some(endpoint.as_str());
        Ok((name, endpoint, is_default))
    }

    /// 根据当前 endpoint 表实例化所有注册表适配器。
    pub fn build_registries(&self) -> Result<HashMap<String, Arc<dyn Registry>>> {
        registries_with_endpoints(&self.sources).map_err(PackageManagerError::from)
    }

    fn load(&mut self) -> Result<()> {
        if !self.path.is_file() {
            self.sources = default_sources()?;
            return Ok(());
        }
        let source = std::fs::read_to_string(&self.path)?;
        if source.trim().is_empty() {
            self.sources = default_sources()?;
            return Ok(());
        }
        let file: SourcesFile = oak_von::from_str(&source).unwrap_or_else(|_| {
            oak_von::from_str::<BTreeMap<String, String>>(&source).map(|sources| SourcesFile { sources }).unwrap_or_default()
        });
        self.sources = default_sources()?;
        for (name, endpoint) in file.sources {
            self.sources.insert(name.to_ascii_lowercase(), endpoint.trim_end_matches('/').to_string());
        }
        Ok(())
    }

    /// 将 endpoint 表写回 `registry-sources.von`。
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = SourcesFile { sources: self.sources.clone() };
        let rendered = nyar_format::printer::to_string_indented(&file)?;
        std::fs::write(&self.path, rendered)?;
        Ok(())
    }
}

fn default_sources() -> Result<BTreeMap<String, String>> {
    let registries = nyar_package_registry::default_registries().map_err(PackageManagerError::from)?;
    Ok(registries.into_iter().map(|(name, registry)| (name, registry.endpoint().to_string())).collect())
}
