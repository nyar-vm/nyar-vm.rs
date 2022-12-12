use std::path::Path;

use serde::Deserialize;

use crate::{
    Package, PublishOptions, PublishResult, Registry, RegistryError, RetryConfig, TokenVerifyResult,
    http::{HttpClient, build_url, url_encode},
};

/// Conda / Anaconda registry adapter (read-oriented).
#[derive(Debug, Clone)]
pub struct CondaRegistry {
    endpoint: String,
    http: HttpClient,
}

impl CondaRegistry {
    pub const DEFAULT_ENDPOINT: &'static str = "https://api.anaconda.org";

    pub fn new(endpoint: impl Into<String>) -> Result<Self, RegistryError> {
        Ok(Self { endpoint: endpoint.into().trim_end_matches('/').to_string(), http: HttpClient::new(RetryConfig::default())? })
    }

    pub fn default_registry() -> Result<Self, RegistryError> {
        Self::new(Self::DEFAULT_ENDPOINT)
    }
}

impl Registry for CondaRegistry {
    fn name(&self) -> &str {
        "conda"
    }

    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn set_endpoint(&mut self, endpoint: &str) {
        self.endpoint = endpoint.trim_end_matches('/').to_string();
    }

    fn get_package(&self, package_name: &str, version: &str) -> Result<Package, RegistryError> {
        let url = build_url(&self.endpoint, &format!("package/conda-forge/{package_name}"));
        let data: CondaPackageResponse = self.http.get_json(&url)?;
        let resolved = if version == "latest" { data.latest_version.unwrap_or_else(|| "0.0.0".to_string()) } else { version.to_string() };
        Ok(Package {
            name: package_name.to_string(),
            version: resolved,
            description: data.summary.or(data.description).unwrap_or_default(),
            homepage: data.home.unwrap_or_default(),
            license: data.license.unwrap_or_default(),
            ..Package::default()
        })
    }

    fn search_packages(&self, query: &str) -> Result<Vec<Package>, RegistryError> {
        let encoded = url_encode(query);
        let url = build_url(&self.endpoint, &format!("search?name={encoded}&type=conda&offset=0&limit=20"));
        let response: Vec<CondaSearchItem> = self.http.get_json(&url).unwrap_or_default();
        Ok(response
            .into_iter()
            .map(|item| Package {
                name: item.name.unwrap_or_default(),
                version: item.latest_version.unwrap_or_else(|| "0.0.0".to_string()),
                description: item.summary.unwrap_or_default(),
                ..Package::default()
            })
            .collect())
    }

    fn publish_package(&self, options: &PublishOptions, tarball_data: &[u8]) -> Result<PublishResult, RegistryError> {
        let url = build_url(&self.endpoint, &format!("package/{}/files", options.package_name));
        let response = self.http.put_bytes(&url, tarball_data, options.auth_token.as_deref(), "multipart/form-data")?;
        Ok(PublishResult {
            success: response.status().is_success(),
            package_name: options.package_name.clone(),
            version: options.version.clone(),
            message: if response.status().is_success() {
                "发布成功".to_string()
            }
            else {
                format!("发布失败，HTTP {}", response.status().as_u16())
            },
            published_url: Some(url),
            ..PublishResult::default()
        })
    }

    fn download_package(&self, package: &Package, target_directory: &Path) -> Result<String, RegistryError> {
        let url = package
            .dist_tarball
            .clone()
            .ok_or_else(|| RegistryError::message(format!("missing artifact for {}@{}", package.name, package.version)))?;
        let bytes = self.http.get_bytes(&url)?;
        std::fs::create_dir_all(target_directory)?;
        std::fs::write(target_directory.join(format!("{}-{}.tar.bz2", package.name, package.version)), bytes)?;
        Ok(target_directory.display().to_string())
    }

    fn get_package_versions(&self, package_name: &str) -> Result<Vec<String>, RegistryError> {
        Ok(vec![self.get_package(package_name, "latest")?.version])
    }

    fn verify_token(&self, token: &str) -> Result<TokenVerifyResult, RegistryError> {
        let url = build_url(&self.endpoint, "user");
        let response = self.http.get_with_headers(&url, &[("Authorization", &format!("Bearer {token}"))])?;
        if response.status().as_u16() == 401 || response.status().as_u16() == 403 {
            return Ok(TokenVerifyResult::failure("认证令牌无效或已过期".to_string()));
        }
        if !response.status().is_success() {
            return Ok(TokenVerifyResult::failure(format!("验证失败，HTTP {}", response.status().as_u16())));
        }
        let body = response.text().unwrap_or_default();
        let username = extract_json_string_value(&body, "\"login\"")
            .or_else(|| extract_json_string_value(&body, "\"user\""))
            .unwrap_or_else(|| "未知用户".to_string());
        Ok(TokenVerifyResult::success(username))
    }
}

fn extract_json_string_value(content: &str, key: &str) -> Option<String> {
    let start = content.find(key)?;
    let after_key = &content[start + key.len()..];
    let colon = after_key.find(':')?;
    let after_colon = after_key[colon + 1..].trim_start();
    let rest = after_colon.strip_prefix('"')?;
    let end = rest.find('"')?;
    let value = rest[..end].trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[derive(Debug, Deserialize)]
struct CondaPackageResponse {
    latest_version: Option<String>,
    summary: Option<String>,
    description: Option<String>,
    home: Option<String>,
    license: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CondaSearchItem {
    name: Option<String>,
    latest_version: Option<String>,
    summary: Option<String>,
}
