use std::{path::Path, thread, time::Duration};

use serde::Deserialize;

use crate::{
    Package, PublishOptions, PublishResult, Registry, RegistryError, RetryConfig, TokenVerifyResult,
    http::{HttpClient, build_url, extract_tarball, url_encode},
};

/// JSR registry adapter.
#[derive(Debug, Clone)]
pub struct JsrRegistry {
    endpoint: String,
    management_endpoint: String,
    http: HttpClient,
}

impl JsrRegistry {
    pub const DEFAULT_ENDPOINT: &'static str = "https://jsr.io";
    pub const DEFAULT_MANAGEMENT_ENDPOINT: &'static str = "https://api.jsr.io";

    pub fn new(endpoint: impl Into<String>) -> Result<Self, RegistryError> {
        Ok(Self {
            endpoint: endpoint.into().trim_end_matches('/').to_string(),
            management_endpoint: Self::DEFAULT_MANAGEMENT_ENDPOINT.to_string(),
            http: HttpClient::new(RetryConfig::npm())?,
        })
    }

    pub fn default_registry() -> Result<Self, RegistryError> {
        Self::new(Self::DEFAULT_ENDPOINT)
    }

    fn management_url(&self, path: &str) -> String {
        build_url(&self.management_endpoint, path)
    }
}

impl Registry for JsrRegistry {
    fn name(&self) -> &str {
        "jsr"
    }

    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn set_endpoint(&mut self, endpoint: &str) {
        self.endpoint = endpoint.trim_end_matches('/').to_string();
    }

    fn get_package(&self, package_name: &str, version: &str) -> Result<Package, RegistryError> {
        let (scope, name) = parse_scoped_name(package_name)?;
        let target_version = if version == "latest" {
            let url = build_url(&self.endpoint, &format!("api/scopes/{scope}/packages/{name}/versions"));
            let response: JsrVersionListResponse = self.http.get_json(&url)?;
            response
                .versions
                .unwrap_or_default()
                .into_iter()
                .map(|item| item.version)
                .max()
                .ok_or_else(|| RegistryError::NotFound(package_name.to_string()))?
        }
        else {
            version.to_string()
        };
        let url = build_url(&self.endpoint, &format!("api/scopes/{scope}/packages/{name}/versions/{target_version}"));
        let data: JsrVersionData = self.http.get_json(&url)?;
        Ok(Package {
            name: package_name.to_string(),
            version: data.version.unwrap_or(target_version),
            description: data.description.unwrap_or_default(),
            ..Package::default()
        })
    }

    fn search_packages(&self, query: &str) -> Result<Vec<Package>, RegistryError> {
        let encoded = url_encode(query);
        let url = build_url(&self.endpoint, &format!("api/packages?query={encoded}&limit=20"));
        let response: JsrSearchResponse = self.http.get_json(&url)?;
        Ok(response
            .items
            .unwrap_or_default()
            .into_iter()
            .map(|item| Package {
                name: format!("@{}/{}", item.scope, item.name),
                version: item.latest_stable_version.or(item.latest_version).unwrap_or_else(|| "0.0.0".to_string()),
                description: item.description.unwrap_or_default(),
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
                message: "缺少 JSR token：请配置 JSR_TOKEN、DENO_AUTH_TOKENS、Deno 配置文件，或 `legion login jsr --token <token>`".to_string(),
                published_url: None,
                dry_run: false,
                sha256: None,
                size: Some(tarball_data.len()),
                file_count: None,
                official_tool_required: false,
            });
        };

        let (scope, name) = parse_scoped_name(&options.package_name)?;
        let config = url_encode("/jsr.json");
        let url = self.management_url(&format!("scopes/{scope}/packages/{name}/versions/{}?config={config}", options.version));
        let response = self.http.post_bytes(&url, tarball_data, token, "application/octet-stream")?;
        let status = response.status();
        let response_text = response.text().unwrap_or_default();
        if !status.is_success() {
            let detail = response_text.trim();
            let detail = if detail.is_empty() { "(empty body)" } else { detail };
            return Ok(PublishResult {
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
            });
        }

        let task: PublishingTask = serde_json::from_str(&response_text)
            .map_err(|error| RegistryError::message(format!("无效的 PublishingTask 响应: {error}; body={response_text}")))?;
        let final_task = self.poll_publishing_task(&task.id, token)?;
        let published_url = format!("https://jsr.io/@{scope}/{name}@{}", options.version);

        match final_task.status.as_str() {
            "success" => Ok(PublishResult {
                success: true,
                package_name: options.package_name.clone(),
                version: options.version.clone(),
                message: "发布成功".to_string(),
                published_url: Some(published_url),
                dry_run: false,
                sha256: None,
                size: Some(tarball_data.len()),
                file_count: None,
                official_tool_required: false,
            }),
            other => {
                let error_message = final_task
                    .error
                    .as_ref()
                    .map(|error| format!("{}: {}", error.code.as_deref().unwrap_or("error"), error.message.as_deref().unwrap_or("")))
                    .unwrap_or_else(|| format!("publishing task status: {other}"));
                Ok(PublishResult {
                    success: false,
                    package_name: options.package_name.clone(),
                    version: options.version.clone(),
                    message: format!("JSR 发布任务失败：{error_message}"),
                    published_url: Some(published_url),
                    dry_run: false,
                    sha256: None,
                    size: Some(tarball_data.len()),
                    file_count: None,
                    official_tool_required: false,
                })
            }
        }
    }

    fn download_package(&self, package: &Package, target_directory: &Path) -> Result<String, RegistryError> {
        let url = package
            .dist_tarball
            .clone()
            .ok_or_else(|| RegistryError::message(format!("missing tarball for {}@{}", package.name, package.version)))?;
        let bytes = self.http.get_bytes(&url)?;
        extract_tarball(&bytes, target_directory)?;
        Ok(target_directory.display().to_string())
    }

    fn get_package_versions(&self, package_name: &str) -> Result<Vec<String>, RegistryError> {
        let (scope, name) = parse_scoped_name(package_name)?;
        let url = build_url(&self.endpoint, &format!("api/scopes/{scope}/packages/{name}/versions"));
        let response: JsrVersionListResponse = self.http.get_json(&url)?;
        Ok(response.versions.unwrap_or_default().into_iter().map(|item| item.version).collect())
    }

    fn verify_token(&self, token: &str) -> Result<TokenVerifyResult, RegistryError> {
        let url = self.management_url("user");
        match self.http.get_authenticated_json::<JsrUserResponse>(&url, token) {
            Ok(user) => {
                let username = user.name.or(user.user.and_then(|inner| inner.name)).unwrap_or_else(|| "unknown".to_string());
                Ok(TokenVerifyResult::success(username))
            }
            Err(RegistryError::Status { status, message }) => Ok(TokenVerifyResult::failure(format!("令牌验证失败，HTTP {status}: {message}"))),
            Err(error) => Ok(TokenVerifyResult::failure(error.to_string())),
        }
    }
}

impl JsrRegistry {
    fn poll_publishing_task(&self, task_id: &str, token: &str) -> Result<PublishingTask, RegistryError> {
        let url = self.management_url(&format!("publishing_tasks/{task_id}"));
        for _ in 0..120 {
            let task: PublishingTask = self.http.get_authenticated_json(&url, token)?;
            match task.status.as_str() {
                "success" | "failure" => return Ok(task),
                _ => thread::sleep(Duration::from_secs(1)),
            }
        }
        Err(RegistryError::message(format!("JSR publishing task timed out: {task_id}")))
    }
}

fn parse_scoped_name(package_name: &str) -> Result<(&str, &str), RegistryError> {
    let trimmed = package_name.trim_start_matches('@');
    let (scope, name) = trimmed.split_once('/').ok_or_else(|| RegistryError::message(format!("JSR package must be scoped: {package_name}")))?;
    Ok((scope, name))
}

#[derive(Debug, Deserialize)]
struct JsrVersionListResponse {
    versions: Option<Vec<JsrVersionItem>>,
}

#[derive(Debug, Deserialize)]
struct JsrVersionItem {
    version: String,
}

#[derive(Debug, Deserialize)]
struct JsrVersionData {
    version: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JsrSearchResponse {
    items: Option<Vec<JsrSearchItem>>,
}

#[derive(Debug, Deserialize)]
struct JsrSearchItem {
    scope: String,
    name: String,
    description: Option<String>,
    latest_stable_version: Option<String>,
    latest_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JsrUserResponse {
    name: Option<String>,
    user: Option<JsrUserInner>,
}

#[derive(Debug, Deserialize)]
struct JsrUserInner {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PublishingTask {
    id: String,
    status: String,
    error: Option<PublishingTaskError>,
}

#[derive(Debug, Deserialize)]
struct PublishingTaskError {
    code: Option<String>,
    message: Option<String>,
}
