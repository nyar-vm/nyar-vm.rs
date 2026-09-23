//! Credential *concepts* and delegation to package-registry adapters.
//!
//! This crate must not read or write ecosystem-specific stores (`.npmrc`, Deno config,
//! NuGet.Config, Maven settings, Anaconda nucleus, …). Concrete I/O lives in
//! [`nyar_package_registry`] (see `discover_token` / `sync_token_to_official_store`).

use std::path::{Path, PathBuf};

use crate::Result;

pub use nyar_package_registry::DiscoveredCredential;

/// Whether `registry` uses third-party CLI credential stores (not persisted in `auth.von`).
#[inline]
pub fn is_external_registry(registry: &str) -> bool {
    nyar_package_registry::uses_external_credential_store(registry)
}

/// Discover a token by delegating to the registry credential facade.
#[inline]
pub fn discover_token(registry: &str, endpoint: Option<&str>, project_dir: Option<&Path>) -> Option<DiscoveredCredential> {
    nyar_package_registry::discover_token(registry, endpoint, project_dir)
}

/// Sync a verified token into the official CLI store via the registry facade.
#[inline]
pub fn sync_token_to_official_store(registry: &str, endpoint: Option<&str>, token: &str) -> Result<Option<PathBuf>> {
    Ok(nyar_package_registry::sync_token_to_official_store(registry, endpoint, token)?)
}

/// Concept-level discovery/sync surface (ids + optional endpoint).
///
/// Prefer [`nyar_package_registry::Registry::discover_credential`] when a registry
/// adapter is already in hand; this trait covers string-keyed orchestration hooks.
pub trait TokenDiscovery {
    fn registry_id(&self) -> &str;

    fn registry_endpoint(&self) -> Option<&str> {
        None
    }

    fn discover_credential(&self, project_dir: Option<&Path>) -> Option<DiscoveredCredential> {
        discover_token(self.registry_id(), self.registry_endpoint(), project_dir)
    }

    fn sync_credential(&self, token: &str) -> Result<Option<PathBuf>> {
        sync_token_to_official_store(self.registry_id(), self.registry_endpoint(), token)
    }

    fn uses_external_store(&self) -> bool {
        is_external_registry(self.registry_id())
    }
}

/// String-keyed credential provider used by auth orchestration.
#[derive(Debug, Clone, Copy)]
pub struct RegistryCredentialId<'a> {
    pub id: &'a str,
    pub endpoint: Option<&'a str>,
}

impl<'a> TokenDiscovery for RegistryCredentialId<'a> {
    fn registry_id(&self) -> &str {
        self.id
    }

    fn registry_endpoint(&self) -> Option<&str> {
        self.endpoint
    }
}
