use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use std_data::text::von::from_str;

use crate::{PackageManagerError, PackageManifest, ProjectLayout, Result};

/// Workspace manifest (members list). Filename comes from [`ProjectLayout`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkspaceManifest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub members: Vec<String>,
}

impl WorkspaceManifest {
    pub fn path_in(directory: impl AsRef<Path>, layout: ProjectLayout) -> PathBuf {
        layout.workspace_manifest_path(directory)
    }

    pub fn exists(directory: impl AsRef<Path>, layout: ProjectLayout) -> bool {
        Self::path_in(directory, layout).is_file()
    }

    pub fn load(directory: impl AsRef<Path>, layout: ProjectLayout) -> Result<Self> {
        let source = std::fs::read_to_string(Self::path_in(directory, layout))?;
        Ok(from_str(&source)?)
    }

    /// Resolve member paths relative to the workspace root (non-recursive).
    pub fn member_paths(&self, root: impl AsRef<Path>) -> Vec<PathBuf> {
        let root = root.as_ref();
        self.members.iter().map(|member| root.join(member)).collect()
    }

    /// Recursively enumerate member directories that contain a package manifest.
    pub fn enumerate_member_dirs(&self, root: impl AsRef<Path>, layout: ProjectLayout) -> Result<Vec<PathBuf>> {
        let mut members = Vec::new();
        self.collect_member_dirs(root.as_ref(), layout, &mut members)?;
        Ok(members)
    }

    fn collect_member_dirs(&self, workspace_root: &Path, layout: ProjectLayout, members: &mut Vec<PathBuf>) -> Result<()> {
        for member in &self.members {
            let member_dir = workspace_root.join(member);
            if Self::exists(&member_dir, layout) {
                let nested_workspace_manifest = Self::load(&member_dir, layout)?;
                nested_workspace_manifest.collect_member_dirs(&member_dir, layout, members)?;
            }
            if PackageManifest::exists(&member_dir, layout) {
                members.push(member_dir);
            }
        }
        Ok(())
    }

    /// Map package name -> member directory with a package manifest (recursive).
    pub fn member_packages(&self, root: impl AsRef<Path>, layout: ProjectLayout) -> Result<BTreeMap<String, PathBuf>> {
        let mut map = BTreeMap::new();
        for path in self.enumerate_member_dirs(root, layout)? {
            let manifest = PackageManifest::load(&path, layout)?;
            map.insert(manifest.name.clone(), path.clone());
            if let Some(basename) = path.file_name().and_then(|name| name.to_str()) {
                map.entry(basename.to_string()).or_insert(path);
            }
        }
        Ok(map)
    }
}

/// Discover package-manager project mode from a directory.
#[derive(Debug, Clone)]
pub enum ProjectMode {
    Workspace { root: PathBuf, workspace: WorkspaceManifest, manifest: Option<PackageManifest> },
    Package { root: PathBuf, manifest: PackageManifest },
    Standalone { root: PathBuf },
}

impl ProjectMode {
    pub fn discover(directory: impl AsRef<Path>, layout: ProjectLayout) -> Result<Self> {
        let root = directory.as_ref().to_path_buf();
        if WorkspaceManifest::exists(&root, layout) {
            let workspace = WorkspaceManifest::load(&root, layout)?;
            let manifest = if PackageManifest::exists(&root, layout) { Some(PackageManifest::load(&root, layout)?) } else { None };
            return Ok(Self::Workspace { root, workspace, manifest });
        }
        if PackageManifest::exists(&root, layout) {
            return Ok(Self::Package { manifest: PackageManifest::load(&root, layout)?, root });
        }
        Ok(Self::Standalone { root })
    }

    pub fn root(&self) -> &Path {
        match self {
            Self::Workspace { root, .. } | Self::Package { root, .. } | Self::Standalone { root } => root,
        }
    }

    pub fn package_manifest(&self) -> Option<&PackageManifest> {
        match self {
            Self::Workspace { manifest, .. } => manifest.as_ref(),
            Self::Package { manifest, .. } => Some(manifest),
            Self::Standalone { .. } => None,
        }
    }

    pub fn expect_package_manifest(&self) -> Result<&PackageManifest> {
        self.package_manifest().ok_or_else(|| PackageManagerError::message("当前目录不是包（缺少包清单）"))
    }
}
