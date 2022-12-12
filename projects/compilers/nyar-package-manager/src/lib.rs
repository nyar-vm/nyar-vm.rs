//! Nyar package manager core library.

mod auth;
mod cache;
mod credentials;
mod error;
mod layout;
mod lock;
mod manager;
mod manifest;
mod pack;
mod publish;
mod publish_target;
mod registry_sources;
mod resolve;
mod scripts;
mod security;
mod version;
mod workspace;

pub use auth::{LoginResult, VendorAuthStore, VendorManager};
pub use cache::PackageCache;
pub use credentials::{
    DiscoveredCredential, RegistryCredentialId, TokenDiscovery, discover_token, is_external_registry, sync_token_to_official_store,
};
pub use error::{PackageManagerError, Result};
pub use layout::{EntryAliasFn, ProjectLayout};
pub use lock::{LockEntry, LockFile};
pub use manager::{PackageInfo, PackageManager};
pub use manifest::{DependencyBucket, DependencySpec, PackageManifest, PublishConfig, PublishTarget, is_registry_publish_type};
pub use pack::{PackMeta, PackResult, PackageIgnore, RegistryBinEntry, RegistryPackOptions, pack, pack_registry_artifact, unpack};
pub use publish::PackagePublisher;
pub use publish_target::{ResolvedPublishArtifact, resolve_publish_artifact};
pub use registry_sources::RegistrySourceManager;
pub use resolve::{DependencyNode, DependencyResolver};
pub use scripts::{ScriptResult, ScriptRunner};
pub use security::{LicenseInfo, SecurityAudit, SecurityAuditResult, VulnerabilityReport};
pub use version::{SemanticVersion, YearlyVersion};
pub use workspace::{ProjectMode, WorkspaceManifest};

pub use nyar_package_registry::{
    MockRegistry, Package, PublishOptions, PublishResult, PublisherKey, Registry, RegistryError, TokenVerifyResult, VersionBump,
};
