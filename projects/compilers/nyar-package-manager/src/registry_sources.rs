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

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut manager = Self { sources: default_sources()?, path };
        manager.load()?;
        Ok(manager)
    }

    pub fn path_for(layout: ProjectLayout) -> PathBuf {
        layout.registry_sources_path()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn sources(&self) -> &BTreeMap<String, String> {
        &self.sources
    }

    pub fn get_endpoint(&self, registry_name: &str) -> Option<&str> {
        self.sources.get(&registry_name.to_ascii_lowercase()).map(String::as_str)
    }

    pub fn list(&self) -> Vec<(String, String)> {
        self.sources.iter().map(|(name, endpoint)| (name.clone(), endpoint.clone())).collect()
    }

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

    pub fn info(&self, registry_name: &str) -> Result<(String, String, bool)> {
        let name = registry_name.to_ascii_lowercase();
        let endpoint = self.get_endpoint(&name).ok_or_else(|| PackageManagerError::message(format!("注册表 {name} 未配置")))?.to_string();
        let is_default = default_sources()?.get(&name).map(String::as_str) == Some(endpoint.as_str());
        Ok((name, endpoint, is_default))
    }

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
        let file: SourcesFile = std_data::text::von::from_str(&source).unwrap_or_else(|_| {
            std_data::text::von::from_str::<BTreeMap<String, String>>(&source).map(|sources| SourcesFile { sources }).unwrap_or_default()
        });
        self.sources = default_sources()?;
        for (name, endpoint) in file.sources {
            self.sources.insert(name.to_ascii_lowercase(), endpoint.trim_end_matches('/').to_string());
        }
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = SourcesFile { sources: self.sources.clone() };
        let rendered = nyar_language::formatter::to_string_indented(&file)?;
        std::fs::write(&self.path, rendered)?;
        Ok(())
    }
}

fn default_sources() -> Result<BTreeMap<String, String>> {
    let registries = nyar_package_registry::default_registries().map_err(PackageManagerError::from)?;
    Ok(registries.into_iter().map(|(name, registry)| (name, registry.endpoint().to_string())).collect())
}
