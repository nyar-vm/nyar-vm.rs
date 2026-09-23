use std::path::{Path, PathBuf};

use crate::{DiscoveredCredential, Package, PublishOptions, PublishResult, RegistryError, TokenVerifyResult};

/// Registry adapter contract.
pub trait Registry: Send + Sync {
    /// Registry name identifier (`npm`, `jsr`, …).
    fn name(&self) -> &str;

    /// API endpoint URL (no trailing slash).
    fn endpoint(&self) -> &str;

    /// Override the endpoint.
    fn set_endpoint(&mut self, endpoint: &str);

    /// Fetch package metadata (`version = "latest"` resolves the newest release).
    fn get_package(&self, package_name: &str, version: &str) -> Result<Package, RegistryError>;

    /// Search packages by keyword.
    fn search_packages(&self, query: &str) -> Result<Vec<Package>, RegistryError>;

    /// Publish packed bytes to the registry.
    fn publish_package(&self, options: &PublishOptions, tarball_data: &[u8]) -> Result<PublishResult, RegistryError>;

    /// Download and extract a package into `target_directory`.
    fn download_package(&self, package: &Package, target_directory: &Path) -> Result<String, RegistryError>;

    /// List available versions for a package.
    fn get_package_versions(&self, package_name: &str) -> Result<Vec<String>, RegistryError>;

    /// Verify an auth token.
    fn verify_token(&self, token: &str) -> Result<TokenVerifyResult, RegistryError>;

    /// Whether this registry keeps tokens in official CLI stores (not vendor `auth.von`).
    fn uses_external_credentials(&self) -> bool {
        crate::credentials::uses_external_credential_store(self.name())
    }

    /// Discover a token from official CLI / env stores for this registry.
    fn discover_credential(&self, project_dir: Option<&Path>) -> Option<DiscoveredCredential> {
        crate::credentials::discover_token(self.name(), Some(self.endpoint()), project_dir)
    }

    /// Write a verified token into the official CLI store, when applicable.
    fn sync_credential(&self, token: &str) -> Result<Option<PathBuf>, RegistryError> {
        crate::credentials::sync_token_to_official_store(self.name(), Some(self.endpoint()), token)
    }

    /// Whether `package_name@version` exists.
    fn package_exists(&self, package_name: &str, version: &str) -> Result<bool, RegistryError> {
        Ok(self.get_package_versions(package_name)?.iter().any(|v| v == version))
    }

    /// Resolve the latest stable version, if any.
    fn get_latest_version(&self, package_name: &str) -> Result<Option<String>, RegistryError> {
        match self.get_package(package_name, "latest") {
            Ok(package) => Ok(Some(package.version)),
            Err(RegistryError::NotFound(_)) => Ok(None),
            Err(RegistryError::Status { status: 404, .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }
}
