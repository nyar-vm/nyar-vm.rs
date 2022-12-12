use std::path::Path;

use crate::{
    Package, PublishOptions, PublishResult, Registry, RegistryError, RetryConfig, TokenVerifyResult,
    http::{HttpClient, build_url, url_encode},
};

/// Maven Central registry adapter (read-oriented).
#[derive(Debug, Clone)]
pub struct MavenRegistry {
    endpoint: String,
    http: HttpClient,
}

impl MavenRegistry {
    pub const DEFAULT_ENDPOINT: &'static str = "https://search.maven.org";

    pub fn new(endpoint: impl Into<String>) -> Result<Self, RegistryError> {
        Ok(Self { endpoint: endpoint.into().trim_end_matches('/').to_string(), http: HttpClient::new(RetryConfig::default())? })
    }

    pub fn default_registry() -> Result<Self, RegistryError> {
        Self::new(Self::DEFAULT_ENDPOINT)
    }
}

impl Registry for MavenRegistry {
    fn name(&self) -> &str {
        "maven"
    }

    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn set_endpoint(&mut self, endpoint: &str) {
        self.endpoint = endpoint.trim_end_matches('/').to_string();
    }

    fn get_package(&self, package_name: &str, version: &str) -> Result<Package, RegistryError> {
        let encoded = url_encode(package_name);
        let url = build_url(&self.endpoint, &format!("solrsearch/select?q={encoded}&rows=1&wt=json"));
        let response: serde_json::Value = self.http.get_json(&url)?;
        let docs = response.pointer("/response/docs").and_then(|value| value.as_array()).cloned().unwrap_or_default();
        let doc = docs.first().ok_or_else(|| RegistryError::NotFound(package_name.to_string()))?;
        let resolved = if version == "latest" {
            doc.get("latestVersion").or_else(|| doc.get("v")).and_then(|value| value.as_str()).unwrap_or(version).to_string()
        }
        else {
            version.to_string()
        };
        let group = doc.get("g").and_then(|value| value.as_str()).unwrap_or_default();
        let artifact = doc.get("a").and_then(|value| value.as_str()).unwrap_or(package_name);
        Ok(Package {
            name: format!("{group}:{artifact}"),
            version: resolved,
            description: doc.get("text").and_then(|value| value.as_str()).unwrap_or_default().to_string(),
            ..Package::default()
        })
    }

    fn search_packages(&self, query: &str) -> Result<Vec<Package>, RegistryError> {
        let encoded = url_encode(query);
        let url = build_url(&self.endpoint, &format!("solrsearch/select?q={encoded}&rows=20&wt=json"));
        let response: serde_json::Value = self.http.get_json(&url)?;
        let docs = response.pointer("/response/docs").and_then(|value| value.as_array()).cloned().unwrap_or_default();
        Ok(docs
            .into_iter()
            .map(|doc| {
                let group = doc.get("g").and_then(|value| value.as_str()).unwrap_or_default();
                let artifact = doc.get("a").and_then(|value| value.as_str()).unwrap_or_default();
                Package {
                    name: format!("{group}:{artifact}"),
                    version: doc.get("latestVersion").or_else(|| doc.get("v")).and_then(|value| value.as_str()).unwrap_or("0.0.0").to_string(),
                    ..Package::default()
                }
            })
            .collect())
    }

    fn publish_package(&self, options: &PublishOptions, tarball_data: &[u8]) -> Result<PublishResult, RegistryError> {
        let url = build_url(&self.endpoint, &options.package_name.replace(':', "/"));
        let response = self.http.put_bytes(&url, tarball_data, options.auth_token.as_deref(), "application/java-archive")?;
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
            .ok_or_else(|| RegistryError::message(format!("missing jar for {}@{}", package.name, package.version)))?;
        let bytes = self.http.get_bytes(&url)?;
        std::fs::create_dir_all(target_directory)?;
        std::fs::write(target_directory.join(format!("{}-{}.jar", package.name.replace(':', "-"), package.version)), bytes)?;
        Ok(target_directory.display().to_string())
    }

    fn get_package_versions(&self, package_name: &str) -> Result<Vec<String>, RegistryError> {
        Ok(vec![self.get_package(package_name, "latest")?.version])
    }

    fn verify_token(&self, token: &str) -> Result<TokenVerifyResult, RegistryError> {
        let url = build_url(&self.endpoint, "solrsearch/select?q=g:com&rows=0&wt=json");
        let authorization = if token.starts_with("Basic ") { token.to_string() } else { format!("Basic {token}") };
        let response = self.http.get_authenticated_with_headers(&url, &authorization, &[])?;
        if response.status().as_u16() == 401 || response.status().as_u16() == 403 {
            return Ok(TokenVerifyResult::failure("认证令牌无效或已过期".to_string()));
        }
        if response.status().is_success() {
            Ok(TokenVerifyResult::success("maven-user".to_string()))
        }
        else {
            Ok(TokenVerifyResult::failure(format!("验证失败，HTTP {}", response.status().as_u16())))
        }
    }
}
