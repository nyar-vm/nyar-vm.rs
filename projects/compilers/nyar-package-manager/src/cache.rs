use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{ProjectLayout, Result};

/// Content-addressable package cache under the product layout cache dir.
#[derive(Debug, Clone)]
pub struct PackageCache {
    root: PathBuf,
    index: CacheIndex,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CacheIndex {
    packages: BTreeMap<String, String>,
}

impl PackageCache {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(&root)?;
        let index_path = root.join("cache-index.von");
        let index = if index_path.is_file() {
            let source = std::fs::read_to_string(&index_path)?;
            std_data::text::von::from_str(&source).unwrap_or_default()
        }
        else {
            CacheIndex::default()
        };
        Ok(Self { root, index })
    }

    /// Default cache root for the given product layout.
    pub fn root_for(layout: ProjectLayout) -> PathBuf {
        layout.cache_dir()
    }

    pub fn has_package(&self, name: &str, version: &str) -> bool {
        self.index.packages.contains_key(&format!("{name}@{version}"))
    }

    pub fn add_package(&mut self, name: &str, version: &str, package_path: impl AsRef<Path>) -> Result<()> {
        let key = format!("{name}@{version}");
        let target = self.root.join(&key);
        if target.exists() {
            let _ = std::fs::remove_dir_all(&target);
        }
        copy_dir(package_path.as_ref(), &target)?;
        self.index.packages.insert(key, target.display().to_string());
        self.save_index()
    }

    pub fn package_path(&self, name: &str, version: &str) -> Option<PathBuf> {
        self.index.packages.get(&format!("{name}@{version}")).map(PathBuf::from)
    }

    pub fn clear(&mut self) -> Result<()> {
        if self.root.exists() {
            std::fs::remove_dir_all(&self.root)?;
        }
        std::fs::create_dir_all(&self.root)?;
        self.index = CacheIndex::default();
        self.save_index()
    }

    fn save_index(&self) -> Result<()> {
        let path = self.root.join("cache-index.von");
        let content = nyar_language::formatter::to_string_indented(&self.index)?;
        std::fs::write(path, content)?;
        Ok(())
    }
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
