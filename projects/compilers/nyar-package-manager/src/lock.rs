use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use nyar_language::formatter::to_string_indented;
use nyar_package_registry::Package;
use serde::{Deserialize, Serialize};
use std_data::text::von::from_str;

use crate::{ProjectLayout, Result};

/// One locked package entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LockEntry {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub registry: String,
    #[serde(default)]
    pub resolved: String,
    #[serde(default)]
    pub integrity: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub is_dev: bool,
    #[serde(default)]
    pub is_workspace: bool,
    #[serde(default)]
    pub install_path: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// Project lockfile (filename supplied by [`ProjectLayout`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LockFile {
    #[serde(default = "default_lock_version")]
    pub version: String,
    #[serde(default)]
    pub packages: BTreeMap<String, LockEntry>,
    #[serde(skip)]
    path: PathBuf,
}

fn default_lock_version() -> String {
    "1".to_string()
}

impl LockFile {
    pub fn new(directory: impl AsRef<Path>, layout: ProjectLayout) -> Self {
        Self { version: default_lock_version(), packages: BTreeMap::new(), path: layout.lockfile_path(directory) }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn exists(&self) -> bool {
        self.path.is_file()
    }

    pub fn load(directory: impl AsRef<Path>, layout: ProjectLayout) -> Result<Self> {
        let path = layout.lockfile_path(&directory);
        if !path.is_file() {
            return Ok(Self::new(directory, layout));
        }
        let source = std::fs::read_to_string(&path)?;
        let mut lock: LockFile = from_str(&source)?;
        lock.path = path;
        Ok(lock)
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = to_string_indented(self)?;
        std::fs::write(&self.path, content)?;
        Ok(())
    }

    pub fn package_key(name: &str, version: &str) -> String {
        format!("{name}@{version}")
    }

    pub fn add_package(&mut self, package: &Package, registry_name: &str, registry_endpoint: &str) {
        let key = Self::package_key(&package.name, &package.version);
        self.packages.insert(
            key,
            LockEntry {
                name: package.name.clone(),
                version: package.version.clone(),
                registry: registry_name.to_string(),
                resolved: package.dist_tarball.clone().unwrap_or_else(|| format!("{registry_endpoint}/{}", package.name)),
                integrity: package.dist_integrity.clone().unwrap_or_default(),
                license: package.license.clone(),
                is_dev: false,
                is_workspace: false,
                // Relative to `vendors/`; matches `PackageManager::package_install_path`.
                install_path: format!("{registry_name}/{}@{}", package.name, package.version),
                dependencies: package.dependencies.clone(),
            },
        );
    }

    pub fn remove_package(&mut self, name: &str) {
        self.packages.retain(|_, entry| entry.name != name);
    }

    pub fn detect_drift(&self, expected: &[(String, String)]) -> Vec<String> {
        let mut drift = Vec::new();
        for (name, version) in expected {
            let key = Self::package_key(name, version);
            if !self.packages.contains_key(&key) {
                drift.push(format!("missing locked package {key}"));
            }
        }
        drift
    }
}
