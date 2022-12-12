use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use nyar_language::formatter::to_string_indented;
use serde::{Deserialize, Serialize};
use std_data::text::von::from_str;

use crate::{ProjectLayout, Result};

/// Registry publish configuration from a package manifest (`publishConfig`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishConfig {
    #[serde(default)]
    pub registry: Option<String>,
    #[serde(default = "default_access")]
    pub access: String,
    #[serde(default = "default_tag")]
    pub tag: String,
}

impl Default for PublishConfig {
    fn default() -> Self {
        Self { registry: None, access: default_access(), tag: default_tag() }
    }
}

fn default_access() -> String {
    "public".to_string()
}

fn default_tag() -> String {
    "latest".to_string()
}

/// One entry from package-manifest `publish: [ { target, type, package_id, version } ]`.
///
/// `type` is the registry destination id from `nyar-package-registry`. Artifact-only formats
/// such as `web-app` / `apk` are ignored by the package-manager publish path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PublishTarget {
    /// Build target this publication is associated with (`wasm`, `clr`, …).
    #[serde(default)]
    pub target: String,
    /// Registry adapter name (opaque id from the registry crate).
    #[serde(rename = "type", default)]
    pub format_type: String,
    /// Published package identity on the registry.
    #[serde(default)]
    pub package_id: String,
    /// Published version; falls back to manifest / workspace version when empty or `workspace`.
    #[serde(default)]
    pub version: String,
}

impl PublishTarget {
    /// True when `type` names a package registry rather than an artifact format.
    pub fn is_registry_target(&self) -> bool {
        is_registry_publish_type(&self.format_type)
    }
}

/// In-memory package-manager view of a product package manifest.
///
/// On-disk filename is supplied by [`ProjectLayout`]; this type is format-agnostic
/// once loaded (VON today via `parse` / `save`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageManifest {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub dependencies: BTreeMap<String, DependencySpec>,
    #[serde(default)]
    pub dev_dependencies: BTreeMap<String, DependencySpec>,
    #[serde(rename = "peerDependencies", default)]
    pub peer_dependencies: BTreeMap<String, DependencySpec>,
    #[serde(default)]
    pub scripts: BTreeMap<String, String>,
    #[serde(default)]
    pub hooks: BTreeMap<String, String>,
    #[serde(rename = "publishConfig", default)]
    pub publish_config: PublishConfig,
    /// Registry / channel publish targets (`publish: [...]`).
    #[serde(default)]
    pub publish: Vec<PublishTarget>,
    /// Extra files to include in registry tarballs (glob patterns relative to project root).
    #[serde(default)]
    pub files: Vec<String>,
}

/// Whether `type` on a publish target refers to a package registry.
pub fn is_registry_publish_type(format_type: &str) -> bool {
    matches!(format_type.trim().to_ascii_lowercase().as_str(), "npm" | "jsr" | "nuget" | "maven" | "conda" | "valhalla" | "local" | "mock")
}

fn default_version() -> String {
    "0.1.0".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencySpec {
    Workspace,
    Version(String),
    Detailed { version: Option<String>, path: Option<String>, registry: Option<String> },
}

impl DependencySpec {
    pub fn version_constraint(&self) -> Option<&str> {
        match self {
            Self::Workspace => Some("workspace:*"),
            Self::Version(version) => Some(version.as_str()),
            Self::Detailed { version, .. } => version.as_deref(),
        }
    }

    pub fn is_workspace(&self) -> bool {
        matches!(self, Self::Workspace) || matches!(self, Self::Version(value) if value.starts_with("workspace:"))
    }
}

impl Serialize for DependencySpec {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Workspace => true.serialize(serializer),
            Self::Version(version) => version.serialize(serializer),
            Self::Detailed { version, path, registry } => {
                DetailedDependency { version: version.clone(), path: path.clone(), registry: registry.clone() }.serialize(serializer)
            }
        }
    }
}

impl<'de> Deserialize<'de> for DependencySpec {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        match DependencySpecDef::deserialize(deserializer)? {
            DependencySpecDef::Bool(true) => Ok(Self::Workspace),
            DependencySpecDef::Bool(false) => Ok(Self::Version("false".to_string())),
            DependencySpecDef::String(version) if version == "workspace:*" || version.starts_with("workspace:") => Ok(Self::Workspace),
            DependencySpecDef::String(version) => Ok(Self::Version(version)),
            DependencySpecDef::Detailed(value) => Ok(Self::Detailed { version: value.version, path: value.path, registry: value.registry }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DetailedDependency {
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    registry: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
enum DependencySpecDef {
    Bool(bool),
    String(String),
    Detailed(DetailedDependency),
}

/// Which dependency map a package belongs to (runtime vs development).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DependencyBucket {
    /// `dependencies` (runtime).
    Runtime,
    /// `dev_dependencies`.
    Dev,
}

impl PackageManifest {
    pub fn path_in(directory: impl AsRef<Path>, layout: ProjectLayout) -> PathBuf {
        layout.package_manifest_path(directory)
    }

    pub fn load(directory: impl AsRef<Path>, layout: ProjectLayout) -> Result<Self> {
        let path = Self::path_in(directory, layout);
        let source = std::fs::read_to_string(&path)?;
        Self::parse(&source)
    }

    pub fn parse(source: &str) -> Result<Self> {
        Ok(from_str(source)?)
    }

    pub fn exists(directory: impl AsRef<Path>, layout: ProjectLayout) -> bool {
        Self::path_in(directory, layout).is_file()
    }

    /// Persist as indented VON using the layout's package-manifest filename.
    ///
    /// Product CLIs with native manifests (package.json / pyproject.toml) should keep
    /// `write_manifest=false` on PM mutators and save through their own bridges instead.
    pub fn save(&self, directory: impl AsRef<Path>, layout: ProjectLayout) -> Result<()> {
        let path = Self::path_in(directory, layout);
        let content = to_string_indented(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Which bucket currently lists `name`, preferring runtime when present in both.
    pub fn dependency_bucket(&self, name: &str) -> Option<DependencyBucket> {
        if self.dependencies.contains_key(name) {
            Some(DependencyBucket::Runtime)
        }
        else if self.dev_dependencies.contains_key(name) {
            Some(DependencyBucket::Dev)
        }
        else {
            None
        }
    }

    /// Record under `dependencies` (and drop from `dev_dependencies` if present).
    pub fn add_dependency(&mut self, name: impl Into<String>, version: impl Into<String>) {
        self.set_dependency(name, version, DependencyBucket::Runtime);
    }

    /// Record under `dev_dependencies` (and drop from `dependencies` if present).
    ///
    /// Product CLIs that own native manifests can call this (or [`Self::set_dependency`])
    /// after `install_one(..., write_manifest=false)` instead of re-homing buckets by hand.
    pub fn add_dev_dependency(&mut self, name: impl Into<String>, version: impl Into<String>) {
        self.set_dependency(name, version, DependencyBucket::Dev);
    }

    /// Write `name@version` into the given bucket, removing it from the other map.
    pub fn set_dependency(&mut self, name: impl Into<String>, version: impl Into<String>, bucket: DependencyBucket) {
        let name = name.into();
        let spec = DependencySpec::Version(version.into());
        match bucket {
            DependencyBucket::Runtime => {
                self.dev_dependencies.remove(&name);
                self.dependencies.insert(name, spec);
            }
            DependencyBucket::Dev => {
                self.dependencies.remove(&name);
                self.dev_dependencies.insert(name, spec);
            }
        }
    }

    /// Update version in whatever bucket already lists `name`; defaults to runtime when absent.
    pub fn upsert_dependency_preserving_bucket(&mut self, name: impl Into<String>, version: impl Into<String>) {
        let name = name.into();
        let bucket = self.dependency_bucket(&name).unwrap_or(DependencyBucket::Runtime);
        self.set_dependency(name, version, bucket);
    }

    pub fn remove_dependency(&mut self, name: &str) -> bool {
        self.dependencies.remove(name).is_some() || self.dev_dependencies.remove(name).is_some()
    }
}
