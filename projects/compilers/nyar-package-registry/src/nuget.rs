use std::path::Path;

use serde::Deserialize;

use crate::{
    Package, PublishOptions, PublishResult, Registry, RegistryError, RetryConfig, TokenVerifyResult,
    http::{HttpClient, build_url, url_encode},
};

/// NuGet registry adapter (read-oriented).
#[derive(Debug, Clone)]
pub struct NugetRegistry {
    endpoint: String,
    http: HttpClient,
}

impl NugetRegistry {
    pub const DEFAULT_ENDPOINT: &'static str = "https://api.nuget.org/v3";

    pub fn new(endpoint: impl Into<String>) -> Result<Self, RegistryError> {
        Ok(Self { endpoint: endpoint.into().trim_end_matches('/').to_string(), http: HttpClient::new(RetryConfig::default())? })
    }

    pub fn default_registry() -> Result<Self, RegistryError> {
        Self::new(Self::DEFAULT_ENDPOINT)
    }
}

impl Registry for NugetRegistry {
    fn name(&self) -> &str {
        "nuget"
    }

    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn set_endpoint(&mut self, endpoint: &str) {
        self.endpoint = endpoint.trim_end_matches('/').to_string();
    }

    fn get_package(&self, package_name: &str, version: &str) -> Result<Package, RegistryError> {
        let versions = self.get_package_versions(package_name)?;
        let resolved = if version == "latest" {
            versions.last().cloned().ok_or_else(|| RegistryError::NotFound(package_name.to_string()))?
        }
        else if versions.iter().any(|item| item == version) {
            version.to_string()
        }
        else {
            return Err(RegistryError::NotFound(format!("{package_name}@{version}")));
        };
        Ok(Package {
            name: package_name.to_string(),
            version: resolved.clone(),
            dist_tarball: Some(format!(
                "https://api.nuget.org/v3-flatcontainer/{}/{}/{}.{}.nupkg",
                package_name.to_ascii_lowercase(),
                resolved,
                package_name.to_ascii_lowercase(),
                resolved
            )),
            ..Package::default()
        })
    }

    fn search_packages(&self, query: &str) -> Result<Vec<Package>, RegistryError> {
        let encoded = url_encode(query);
        let url = format!("https://azuresearch-usnc.nuget.org/query?q={encoded}&take=20");
        let response: NugetSearchResponse = self.http.get_json(&url)?;
        Ok(response
            .data
            .unwrap_or_default()
            .into_iter()
            .map(|item| Package {
                name: item.id.unwrap_or_default(),
                version: item.version.unwrap_or_default(),
                description: item.description.unwrap_or_default(),
                ..Package::default()
            })
            .collect())
    }

    fn publish_package(&self, options: &PublishOptions, tarball_data: &[u8]) -> Result<PublishResult, RegistryError> {
        Ok(PublishResult::official_tool(
            &options.package_name,
            &options.version,
            format!(
                "NuGet 不支持从 legion 直接上传。请使用官方工具发布 {}@{}:\n\
                 1. `dotnet pack` 生成 .nupkg\n\
                 2. `dotnet nuget push <package>.nupkg --source {} --api-key <TOKEN>`",
                options.package_name, options.version, self.endpoint
            ),
            Some(self.endpoint.clone()),
            tarball_data.len(),
        ))
    }

    fn download_package(&self, package: &Package, target_directory: &Path) -> Result<String, RegistryError> {
        let url = package
            .dist_tarball
            .clone()
            .ok_or_else(|| RegistryError::message(format!("missing nupkg for {}@{}", package.name, package.version)))?;
        let bytes = self.http.get_bytes(&url)?;
        std::fs::create_dir_all(target_directory)?;
        let file_path = target_directory.join(format!("{}.{}.nupkg", package.name, package.version));
        std::fs::write(&file_path, bytes)?;
        Ok(target_directory.display().to_string())
    }

    fn get_package_versions(&self, package_name: &str) -> Result<Vec<String>, RegistryError> {
        let url = build_url("https://api.nuget.org/v3-flatcontainer", &format!("{}/index.json", package_name.to_ascii_lowercase()));
        let response: NugetVersionsResponse = self.http.get_json(&url)?;
        Ok(response.versions.unwrap_or_default())
    }

    fn verify_token(&self, token: &str) -> Result<TokenVerifyResult, RegistryError> {
        let index_url = build_url(&self.endpoint, "index.json");
        let index: NugetServiceIndex = self.http.get_json(&index_url)?;
        let search_url = index
            .resources
            .as_ref()
            .and_then(|resources| {
                resources.iter().find_map(|resource| {
                    let resource_type = resource.resource_type.as_deref()?;
                    resource_type.eq_ignore_ascii_case("SearchQueryService").then(|| resource.id.clone())
                })
            })
            .unwrap_or_else(|| "https://azuresearch-usnc.nuget.org/query".to_string());
        let probe_url = format!("{}?q=test&take=1", search_url.trim_end_matches('/'));
        let response = self.http.get_with_headers(&probe_url, &[("X-NuGet-ApiKey", token)])?;
        if response.status().is_success() {
            Ok(TokenVerifyResult::success("authenticated".to_string()))
        }
        else {
            Ok(TokenVerifyResult::failure(format!("令牌验证失败，HTTP {}", response.status().as_u16())))
        }
    }
}

#[derive(Debug, Deserialize)]
struct NugetServiceIndex {
    resources: Option<Vec<NugetServiceResource>>,
}

#[derive(Debug, Deserialize)]
struct NugetServiceResource {
    #[serde(rename = "@id")]
    id: String,
    #[serde(rename = "@type")]
    resource_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NugetVersionsResponse {
    versions: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct NugetSearchResponse {
    data: Option<Vec<NugetSearchItem>>,
}

#[derive(Debug, Deserialize)]
struct NugetSearchItem {
    id: Option<String>,
    version: Option<String>,
    description: Option<String>,
}
