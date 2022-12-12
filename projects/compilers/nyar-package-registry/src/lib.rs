//! Multi-registry adapters for the Nyar package ecosystem.

mod conda;
pub mod credentials;
mod error;
mod http;
mod jsr;
mod maven;
mod npm;
mod nuget;
mod publisher_key;
mod registry;
mod types;
mod valhalla;

pub use conda::CondaRegistry;
pub use credentials::{DiscoveredCredential, discover_token, sync_token_to_official_store, uses_external_credential_store};
pub use error::RegistryError;
pub use http::{extract_tarball, proxy_env_hint, sha256_hex, verify_sri};
pub use jsr::JsrRegistry;
pub use maven::MavenRegistry;
pub use npm::NpmRegistry;
pub use nuget::NugetRegistry;
pub use publisher_key::{PublisherKey, canonicalize_package_name, normalize_fingerprint};
pub use registry::Registry;
pub use types::{Package, PublishOptions, PublishResult, RetryConfig, TokenVerifyResult, VersionBump};
pub use valhalla::ValhallaRegistry;

use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    sync::Arc,
};

/// Build the default set of registry adapters.
pub fn default_registries() -> Result<HashMap<String, Arc<dyn Registry>>, RegistryError> {
    registries_with_endpoints(&BTreeMap::new())
}

/// Build registries, applying custom endpoints when present.
pub fn registries_with_endpoints(endpoints: &BTreeMap<String, String>) -> Result<HashMap<String, Arc<dyn Registry>>, RegistryError> {
    let mut registries: HashMap<String, Arc<dyn Registry>> = HashMap::new();
    registries.insert("npm".to_string(), Arc::new(NpmRegistry::new(endpoint_or(endpoints, "npm", NpmRegistry::DEFAULT_ENDPOINT))?));
    registries.insert("jsr".to_string(), Arc::new(JsrRegistry::new(endpoint_or(endpoints, "jsr", JsrRegistry::DEFAULT_ENDPOINT))?));
    registries.insert("nuget".to_string(), Arc::new(NugetRegistry::new(endpoint_or(endpoints, "nuget", NugetRegistry::DEFAULT_ENDPOINT))?));
    registries.insert("maven".to_string(), Arc::new(MavenRegistry::new(endpoint_or(endpoints, "maven", MavenRegistry::DEFAULT_ENDPOINT))?));
    registries.insert("conda".to_string(), Arc::new(CondaRegistry::new(endpoint_or(endpoints, "conda", CondaRegistry::DEFAULT_ENDPOINT))?));
    registries.insert(
        "valhalla".to_string(),
        Arc::new(ValhallaRegistry::new(endpoint_or(endpoints, "valhalla", ValhallaRegistry::DEFAULT_ENDPOINT))?),
    );
    Ok(registries)
}

fn endpoint_or<'a>(endpoints: &'a BTreeMap<String, String>, name: &str, default: &'a str) -> &'a str {
    endpoints.get(name).map(String::as_str).unwrap_or(default)
}

/// In-memory registry used by unit / smoke tests.
#[derive(Debug, Default)]
pub struct MockRegistry {
    name: String,
    endpoint: String,
    packages: std::sync::Mutex<HashMap<String, Package>>,
    published: std::sync::Mutex<Vec<(String, String, Vec<u8>)>>,
    tokens: std::sync::Mutex<HashMap<String, String>>,
}

impl MockRegistry {
    pub fn new(name: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            endpoint: endpoint.into().trim_end_matches('/').to_string(),
            packages: std::sync::Mutex::new(HashMap::new()),
            published: std::sync::Mutex::new(Vec::new()),
            tokens: std::sync::Mutex::new(HashMap::new()),
        }
    }

    pub fn insert_package(&self, package: Package) {
        let key = format!("{}@{}", package.name, package.version);
        self.packages.lock().expect("packages lock").insert(key, package);
    }

    pub fn insert_token(&self, token: impl Into<String>, username: impl Into<String>) {
        self.tokens.lock().expect("tokens lock").insert(token.into(), username.into());
    }

    pub fn published(&self) -> Vec<(String, String, Vec<u8>)> {
        self.published.lock().expect("published lock").clone()
    }
}

impl Registry for MockRegistry {
    fn name(&self) -> &str {
        &self.name
    }

    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn set_endpoint(&mut self, endpoint: &str) {
        self.endpoint = endpoint.trim_end_matches('/').to_string();
    }

    fn get_package(&self, package_name: &str, version: &str) -> Result<Package, RegistryError> {
        let packages = self.packages.lock().expect("packages lock");
        if version == "latest" {
            return packages
                .values()
                .filter(|package| package.name == package_name)
                .max_by(|left, right| left.version.cmp(&right.version))
                .cloned()
                .ok_or_else(|| RegistryError::NotFound(package_name.to_string()));
        }
        packages.get(&format!("{package_name}@{version}")).cloned().ok_or_else(|| RegistryError::NotFound(format!("{package_name}@{version}")))
    }

    fn search_packages(&self, query: &str) -> Result<Vec<Package>, RegistryError> {
        let packages = self.packages.lock().expect("packages lock");
        Ok(packages.values().filter(|package| package.name.contains(query) || package.description.contains(query)).cloned().collect())
    }

    fn publish_package(&self, options: &PublishOptions, tarball_data: &[u8]) -> Result<PublishResult, RegistryError> {
        self.published.lock().expect("published lock").push((options.package_name.clone(), options.version.clone(), tarball_data.to_vec()));
        let package = Package {
            name: options.package_name.clone(),
            version: options.version.clone(),
            description: options.description.clone(),
            homepage: options.homepage.clone().unwrap_or_default(),
            license: options.license.clone().unwrap_or_default(),
            ..Package::default()
        };
        self.insert_package(package);
        Ok(PublishResult {
            success: true,
            package_name: options.package_name.clone(),
            version: options.version.clone(),
            message: "发布成功".to_string(),
            published_url: Some(format!("{}/{}/{}", self.endpoint, options.package_name, options.version)),
            dry_run: false,
            sha256: Some(sha256_hex(tarball_data)),
            size: Some(tarball_data.len()),
            file_count: None,
            official_tool_required: false,
        })
    }

    fn download_package(&self, package: &Package, target_directory: &Path) -> Result<String, RegistryError> {
        std::fs::create_dir_all(target_directory)?;
        let manifest = format!("{{\n    name: \"{}\",\n    version: \"{}\",\n}}\n", package.name, package.version);
        std::fs::write(target_directory.join("package.von"), manifest)?;
        Ok(target_directory.display().to_string())
    }

    fn get_package_versions(&self, package_name: &str) -> Result<Vec<String>, RegistryError> {
        let packages = self.packages.lock().expect("packages lock");
        Ok(packages.values().filter(|package| package.name == package_name).map(|package| package.version.clone()).collect())
    }

    fn verify_token(&self, token: &str) -> Result<TokenVerifyResult, RegistryError> {
        let tokens = self.tokens.lock().expect("tokens lock");
        match tokens.get(token) {
            Some(username) => Ok(TokenVerifyResult::success(username.clone())),
            None => Ok(TokenVerifyResult::failure("invalid token")),
        }
    }
}
