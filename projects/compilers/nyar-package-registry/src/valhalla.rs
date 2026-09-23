use std::path::Path;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    Package, PublishOptions, PublishResult, PublisherKey, Registry, RegistryError, RetryConfig, TokenVerifyResult, canonicalize_package_name,
    http::{HttpClient, build_url, extract_tarball, url_encode},
    sha256_hex,
};

/// Valhalla registry adapter (Ed25519 publisher-key auth).
#[derive(Debug, Clone)]
pub struct ValhallaRegistry {
    endpoint: String,
    http: HttpClient,
}

impl ValhallaRegistry {
    pub const DEFAULT_ENDPOINT: &'static str = "https://valhalla.nyar.dev";

    pub fn new(endpoint: impl Into<String>) -> Result<Self, RegistryError> {
        Ok(Self { endpoint: endpoint.into().trim_end_matches('/').to_string(), http: HttpClient::new(RetryConfig::default())? })
    }

    pub fn default_registry() -> Result<Self, RegistryError> {
        Self::new(Self::DEFAULT_ENDPOINT)
    }
}

impl Registry for ValhallaRegistry {
    fn name(&self) -> &str {
        "valhalla"
    }

    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn set_endpoint(&mut self, endpoint: &str) {
        self.endpoint = endpoint.trim_end_matches('/').to_string();
    }

    fn get_package(&self, package_name: &str, version: &str) -> Result<Package, RegistryError> {
        let package_name = canonicalize_package_name(package_name)?;
        let manifest = self.fetch_manifest(&package_name)?;
        let versions = manifest.versions.unwrap_or_default();
        let resolved = if version == "latest" {
            versions.keys().max().cloned().ok_or_else(|| RegistryError::NotFound(package_name.clone()))?
        }
        else if versions.contains_key(version) {
            version.to_string()
        }
        else {
            return Err(RegistryError::NotFound(format!("{package_name}@{version}")));
        };

        let entry = versions.get(&resolved).ok_or_else(|| RegistryError::NotFound(format!("{package_name}@{resolved}")))?;

        Ok(Package {
            name: manifest.name.unwrap_or_else(|| package_name.clone()),
            version: entry.version.clone().unwrap_or(resolved.clone()),
            author: manifest.publisher.unwrap_or_default(),
            description: manifest.description.unwrap_or_default(),
            license: manifest.license.unwrap_or_default(),
            dist_tarball: Some(build_url(
                &self.endpoint,
                &format!("api/packages/{}/versions/{}/download", url_encode(&package_name), url_encode(&resolved)),
            )),
            dist_integrity: entry.package_digest.clone().map(
                |digest| {
                    if digest.starts_with("sha256-") { digest } else { format!("sha256-{digest}") }
                },
            ),
            ..Package::default()
        })
    }

    fn search_packages(&self, query: &str) -> Result<Vec<Package>, RegistryError> {
        let encoded = url_encode(query);
        let url = build_url(&self.endpoint, &format!("api/search?q={encoded}&page=1&size=20"));
        let response: ValhallaSearchResponse = self.http.get_json(&url)?;
        Ok(response
            .packages
            .unwrap_or_default()
            .into_iter()
            .map(|item| Package {
                name: item.name.unwrap_or_default(),
                version: item.latest_version.unwrap_or_else(|| "0.0.0".to_string()),
                description: item.description.unwrap_or_default(),
                author: item.author.or(item.publisher).unwrap_or_default(),
                ..Package::default()
            })
            .collect())
    }

    fn publish_package(&self, options: &PublishOptions, tarball_data: &[u8]) -> Result<PublishResult, RegistryError> {
        let key_material = options.auth_token.as_deref().ok_or_else(|| {
            RegistryError::message(
                "Valhalla publish requires a publisher key. Provide --token or run \
                 `legion vendor login valhalla --token <ed25519-seed|key-file>`",
            )
        })?;
        let key = PublisherKey::parse(key_material)?;
        if !key.can_sign() {
            return Err(RegistryError::message(format!(
                "publisher key {} is fingerprint-only; need ed25519-seed / private key to publish",
                key.fingerprint()
            )));
        }

        let package_name = canonicalize_package_name(&options.package_name)?;
        let manifest_json = serde_json::json!({
            "name": package_name,
            "version": options.version,
            "description": options.description,
            "publisher": key.fingerprint(),
        })
        .to_string();

        let (body, content_type) = build_publish_multipart(&manifest_json, tarball_data);
        let authorization = key.authorization_header(&body)?;
        let url = build_url(&self.endpoint, &format!("api/packages/{}/versions", url_encode(&package_name)));
        let response = self.http.post_bytes_with_authorization(&url, &body, &authorization, &content_type)?;
        let status = response.status();
        let response_text = response.text().unwrap_or_default();
        if status.is_success() {
            Ok(PublishResult {
                success: true,
                package_name,
                version: options.version.clone(),
                message: format!("发布成功（publisher {}）", key.fingerprint()),
                published_url: Some(url),
                dry_run: false,
                sha256: Some(sha256_hex(tarball_data)),
                size: Some(tarball_data.len()),
                file_count: None,
                official_tool_required: false,
            })
        }
        else {
            Ok(PublishResult {
                success: false,
                package_name,
                version: options.version.clone(),
                message: format!("Valhalla 发布失败 HTTP {}: {}", status.as_u16(), response_text.trim()),
                published_url: Some(url),
                dry_run: false,
                sha256: Some(sha256_hex(tarball_data)),
                size: Some(tarball_data.len()),
                file_count: None,
                official_tool_required: false,
            })
        }
    }

    fn download_package(&self, package: &Package, target_directory: &Path) -> Result<String, RegistryError> {
        let url = package
            .dist_tarball
            .clone()
            .ok_or_else(|| RegistryError::message(format!("missing download URL for {}@{}", package.name, package.version)))?;
        let bytes = self.http.get_bytes(&url)?;
        std::fs::create_dir_all(target_directory)?;

        // Valhalla bundles package.nyar (+ optional source tar.gz). Prefer extracting source when present.
        if let Some((nyar, source)) = split_valhalla_blob(&bytes) {
            std::fs::write(target_directory.join("package.nyar"), nyar)?;
            if let Some(source) = source {
                if extract_tarball(&source, target_directory).is_err() {
                    std::fs::write(target_directory.join("source.tar.gz"), source)?;
                }
            }
        }
        else if extract_tarball(&bytes, target_directory).is_err() {
            std::fs::write(target_directory.join("package.bin"), bytes)?;
        }

        let manifest = format!("{{\n    name: \"{}\",\n    version: \"{}\",\n}}\n", package.name, package.version);
        std::fs::write(target_directory.join("package.von"), manifest)?;
        Ok(target_directory.display().to_string())
    }

    fn get_package_versions(&self, package_name: &str) -> Result<Vec<String>, RegistryError> {
        let package_name = canonicalize_package_name(package_name)?;
        let manifest = self.fetch_manifest(&package_name)?;
        let mut versions: Vec<String> = manifest.versions.unwrap_or_default().into_keys().collect();
        versions.sort();
        Ok(versions)
    }

    fn verify_token(&self, token: &str) -> Result<TokenVerifyResult, RegistryError> {
        match PublisherKey::parse(token) {
            Ok(key) => {
                let username = if key.can_sign() { key.fingerprint().to_string() } else { format!("{} (fingerprint-only)", key.fingerprint()) };
                Ok(TokenVerifyResult::success(username))
            }
            Err(error) => Ok(TokenVerifyResult::failure(format!("invalid Valhalla publisher key: {error}"))),
        }
    }
}

impl ValhallaRegistry {
    fn fetch_manifest(&self, package_name: &str) -> Result<ValhallaManifest, RegistryError> {
        let url = build_url(&self.endpoint, &format!("api/packages/{}/manifest", url_encode(package_name)));
        self.http.get_json(&url)
    }
}

fn build_publish_multipart(manifest_json: &str, package_bytes: &[u8]) -> (Vec<u8>, String) {
    let boundary = format!("----LegionValhalla{}", hex_boundary(package_bytes));
    let mut body = Vec::with_capacity(manifest_json.len() + package_bytes.len() + 256);
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"manifest\"\r\n\r\n");
    body.extend_from_slice(manifest_json.as_bytes());
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"package\"; filename=\"package.nyar\"\r\n");
    body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
    body.extend_from_slice(package_bytes);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    let content_type = format!("multipart/form-data; boundary={boundary}");
    (body, content_type)
}

fn hex_boundary(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().take(8).map(|byte| format!("{byte:02x}")).collect()
}

fn split_valhalla_blob(bytes: &[u8]) -> Option<(Vec<u8>, Option<Vec<u8>>)> {
    if bytes.len() < 4 {
        return None;
    }
    let package_size = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    if package_size == 0 || 4 + package_size > bytes.len() {
        return None;
    }
    let nyar = bytes[4..4 + package_size].to_vec();
    let mut offset = 4 + package_size;
    let source = if offset + 4 <= bytes.len() {
        let source_size = u32::from_le_bytes([bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3]]) as usize;
        offset += 4;
        if source_size > 0 && offset + source_size <= bytes.len() { Some(bytes[offset..offset + source_size].to_vec()) } else { None }
    }
    else {
        None
    };
    Some((nyar, source))
}

#[derive(Debug, Deserialize)]
struct ValhallaManifest {
    name: Option<String>,
    publisher: Option<String>,
    description: Option<String>,
    license: Option<String>,
    versions: Option<std::collections::BTreeMap<String, ValhallaVersionEntry>>,
}

#[derive(Debug, Deserialize)]
struct ValhallaVersionEntry {
    version: Option<String>,
    #[serde(alias = "packageDigest", alias = "package_digest")]
    package_digest: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ValhallaSearchResponse {
    packages: Option<Vec<ValhallaSearchItem>>,
}

#[derive(Debug, Deserialize)]
struct ValhallaSearchItem {
    name: Option<String>,
    description: Option<String>,
    author: Option<String>,
    publisher: Option<String>,
    #[serde(alias = "latestVersion", alias = "latest_version")]
    latest_version: Option<String>,
}
