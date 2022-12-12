use std::{collections::BTreeMap, path::Path};

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use serde::Deserialize;
use sha1::{Digest as Sha1Digest, Sha1};
use sha2::{Digest as Sha2Digest, Sha512};

use crate::{
    Package, PublishOptions, PublishResult, Registry, RegistryError, RetryConfig, TokenVerifyResult,
    http::{HttpClient, build_url, extract_tarball, verify_sri},
};

/// npm registry adapter.
#[derive(Debug, Clone)]
pub struct NpmRegistry {
    endpoint: String,
    http: HttpClient,
}

impl NpmRegistry {
    pub const DEFAULT_ENDPOINT: &'static str = "https://registry.npmjs.org";

    pub fn new(endpoint: impl Into<String>) -> Result<Self, RegistryError> {
        Ok(Self { endpoint: endpoint.into().trim_end_matches('/').to_string(), http: HttpClient::new(RetryConfig::npm())? })
    }

    pub fn default_registry() -> Result<Self, RegistryError> {
        Self::new(Self::DEFAULT_ENDPOINT)
    }
}

impl Registry for NpmRegistry {
    fn name(&self) -> &str {
        "npm"
    }

    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn set_endpoint(&mut self, endpoint: &str) {
        self.endpoint = endpoint.trim_end_matches('/').to_string();
    }

    fn get_package(&self, package_name: &str, version: &str) -> Result<Package, RegistryError> {
        let encoded = npm_package_path(package_name);
        let path = if version == "latest" { format!("{encoded}/latest") } else { format!("{encoded}/{version}") };
        let url = build_url(&self.endpoint, &path);
        let data: NpmVersionData = self.http.get_json(&url)?;
        Ok(convert_to_package(package_name, data))
    }

    fn search_packages(&self, query: &str) -> Result<Vec<Package>, RegistryError> {
        let encoded = crate::http::url_encode(query);
        let url = build_url(&self.endpoint, &format!("-/v1/search?text={encoded}&size=20"));
        let response: NpmSearchResponse = self.http.get_json(&url)?;
        Ok(response
            .objects
            .unwrap_or_default()
            .into_iter()
            .filter_map(|object| object.package)
            .map(|package| Package {
                name: package.name,
                version: package.version,
                description: package.description.unwrap_or_default(),
                author: package.author.and_then(|author| author.name).unwrap_or_default(),
                ..Package::default()
            })
            .collect())
    }

    fn publish_package(&self, options: &PublishOptions, tarball_data: &[u8]) -> Result<PublishResult, RegistryError> {
        let Some(token) = options.auth_token.as_deref().filter(|value| !value.is_empty())
        else {
            return Ok(PublishResult {
                success: false,
                package_name: options.package_name.clone(),
                version: options.version.clone(),
                message:
                    "缺少 npm token：请配置 `~/.npmrc`（npm login）、环境变量 NPM_TOKEN / NODE_AUTH_TOKEN，或使用产品 CLI `login --token <token>`"
                        .to_string(),
                published_url: None,
                dry_run: false,
                sha256: None,
                size: Some(tarball_data.len()),
                file_count: None,
                official_tool_required: false,
            });
        };

        let body = build_npm_publish_document(options, tarball_data, &self.endpoint);
        let url = build_url(&self.endpoint, &npm_package_path(&options.package_name));
        let response = self.http.put_bytes(&url, body.as_bytes(), Some(token), "application/json")?;
        let status = response.status();
        let response_text = response.text().unwrap_or_default();
        if status.is_success() {
            Ok(PublishResult {
                success: true,
                package_name: options.package_name.clone(),
                version: options.version.clone(),
                message: "发布成功".to_string(),
                published_url: Some(format!("https://www.npmjs.com/package/{}/v/{}", options.package_name, options.version)),
                dry_run: false,
                sha256: None,
                size: Some(tarball_data.len()),
                file_count: None,
                official_tool_required: false,
            })
        }
        else {
            let detail = response_text.trim();
            let detail = if detail.is_empty() { "(empty body)" } else { detail };
            Ok(PublishResult {
                success: false,
                package_name: options.package_name.clone(),
                version: options.version.clone(),
                message: format!("发布失败，HTTP {}: {detail}", status.as_u16()),
                published_url: Some(url),
                dry_run: false,
                sha256: None,
                size: Some(tarball_data.len()),
                file_count: None,
                official_tool_required: false,
            })
        }
    }

    fn download_package(&self, package: &Package, target_directory: &Path) -> Result<String, RegistryError> {
        let tarball_url = if let Some(url) = &package.dist_tarball {
            url.clone()
        }
        else {
            let url = build_url(&self.endpoint, &format!("{}/{}", npm_package_path(&package.name), package.version));
            let data: NpmVersionData = self.http.get_json(&url)?;
            data.dist.and_then(|dist| dist.tarball).ok_or_else(|| RegistryError::NotFound(format!("{}@{}", package.name, package.version)))?
        };

        let bytes = self.http.get_bytes(&tarball_url)?;
        verify_sri(&bytes, package.dist_integrity.as_deref())?;
        extract_tarball(&bytes, target_directory)?;
        Ok(target_directory.display().to_string())
    }

    fn get_package_versions(&self, package_name: &str) -> Result<Vec<String>, RegistryError> {
        let url = build_url(&self.endpoint, &npm_package_path(package_name));
        let data: NpmPackageMetadata = self.http.get_json(&url)?;
        Ok(data.versions.unwrap_or_default().into_keys().collect())
    }

    fn verify_token(&self, token: &str) -> Result<TokenVerifyResult, RegistryError> {
        let url = build_url(&self.endpoint, "-/whoami");
        match self.http.get_authenticated_json::<NpmWhoAmIResponse>(&url, token) {
            Ok(whoami) => Ok(TokenVerifyResult::success(whoami.username.unwrap_or_else(|| "unknown".to_string()))),
            Err(RegistryError::Status { status, message }) => Ok(TokenVerifyResult::failure(format!("令牌验证失败，HTTP {status}: {message}"))),
            Err(error) => Ok(TokenVerifyResult::failure(error.to_string())),
        }
    }
}

fn convert_to_package(package_name: &str, data: NpmVersionData) -> Package {
    let mut dependencies = Vec::new();
    let mut dependency_versions = BTreeMap::new();
    if let Some(deps) = data.dependencies {
        for (name, version) in deps {
            dependencies.push(format!("{name}@{version}"));
            dependency_versions.insert(name, version);
        }
    }
    Package {
        name: data.name.unwrap_or_else(|| package_name.to_string()),
        version: data.version.unwrap_or_default(),
        description: data.description.unwrap_or_default(),
        author: data.author.and_then(|author| author.name).unwrap_or_default(),
        license: data.license.unwrap_or_default(),
        dependencies,
        dependency_versions,
        dist_tarball: data.dist.as_ref().and_then(|dist| dist.tarball.clone()),
        dist_integrity: data.dist.and_then(|dist| dist.integrity),
        peer_dependencies: data.peer_dependencies,
        ..Package::default()
    }
}

/// Encode package names the way npm registry paths expect (`@scope/name` → `@scope%2fname`).
fn npm_package_path(package_name: &str) -> String {
    if let Some((scope, name)) = package_name.split_once('/') { format!("{scope}%2f{name}") } else { package_name.to_string() }
}

fn build_npm_publish_document(options: &PublishOptions, tarball_data: &[u8], endpoint: &str) -> String {
    let name = &options.package_name;
    let version = &options.version;
    let tag = options.tag.as_deref().unwrap_or("latest");
    let access = options.access.as_deref().unwrap_or("public");
    let attachment = format!("{name}-{version}.tgz");
    let integrity = {
        let digest = <Sha512 as Sha2Digest>::digest(tarball_data);
        format!("sha512-{}", BASE64.encode(digest))
    };
    let shasum = {
        let digest = <Sha1 as Sha1Digest>::digest(tarball_data);
        digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
    };
    let tarball_url = format!("{}/{}/-/{}", endpoint.trim_end_matches('/'), npm_package_path(name), attachment);

    let mut version_doc = serde_json::json!({
        "name": name,
        "version": version,
        "description": options.description,
        "dist": {
            "shasum": shasum,
            "integrity": integrity,
            "tarball": tarball_url
        }
    });
    if name.starts_with('@') {
        version_doc["publishConfig"] = serde_json::json!({ "access": access });
    }
    if let Some(license) = &options.license {
        version_doc["license"] = serde_json::json!(license);
    }

    let mut dist_tags = serde_json::Map::new();
    dist_tags.insert(tag.to_string(), serde_json::json!(version));

    let mut versions = serde_json::Map::new();
    versions.insert(version.to_string(), version_doc);

    let mut attachments = serde_json::Map::new();
    attachments.insert(
        attachment,
        serde_json::json!({
            "content_type": "application/octet-stream",
            "data": BASE64.encode(tarball_data),
            "length": tarball_data.len()
        }),
    );

    serde_json::json!({
        "_id": name,
        "name": name,
        "description": options.description,
        "dist-tags": dist_tags,
        "versions": versions,
        "access": access,
        "_attachments": attachments
    })
    .to_string()
}

#[derive(Debug, Deserialize)]
struct NpmPackageMetadata {
    versions: Option<BTreeMap<String, NpmVersionData>>,
}

#[derive(Debug, Deserialize)]
struct NpmVersionData {
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    author: Option<NpmAuthor>,
    license: Option<String>,
    dist: Option<NpmDist>,
    dependencies: Option<BTreeMap<String, String>>,
    #[serde(rename = "peerDependencies")]
    peer_dependencies: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct NpmAuthor {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NpmDist {
    tarball: Option<String>,
    integrity: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NpmSearchResponse {
    objects: Option<Vec<NpmSearchObject>>,
}

#[derive(Debug, Deserialize)]
struct NpmSearchObject {
    package: Option<NpmSearchPackage>,
}

#[derive(Debug, Deserialize)]
struct NpmSearchPackage {
    name: String,
    version: String,
    description: Option<String>,
    author: Option<NpmAuthor>,
}

#[derive(Debug, Deserialize)]
struct NpmWhoAmIResponse {
    username: Option<String>,
}
