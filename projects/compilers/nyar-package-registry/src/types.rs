use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Package metadata returned by a registry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub dependency_versions: BTreeMap<String, String>,
    #[serde(default)]
    pub target_conditions: BTreeMap<String, Vec<String>>,
    pub dist_tarball: Option<String>,
    pub dist_integrity: Option<String>,
    #[serde(default)]
    pub is_sdk_package: bool,
    pub sdk_module_name: Option<String>,
    pub peer_dependencies: Option<BTreeMap<String, String>>,
}

/// Version bump kind for publish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionBump {
    Patch,
    Minor,
    Major,
}

impl VersionBump {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "patch" => Some(Self::Patch),
            "minor" => Some(Self::Minor),
            "major" => Some(Self::Major),
            _ => None,
        }
    }
}

/// Options controlling package publish.
#[derive(Debug, Clone)]
pub struct PublishOptions {
    pub package_name: String,
    pub version: String,
    pub description: String,
    pub homepage: Option<String>,
    pub license: Option<String>,
    pub package_path: String,
    pub registry_name: String,
    pub auth_token: Option<String>,
    pub tag: Option<String>,
    pub access: Option<String>,
    pub bump: Option<VersionBump>,
    pub create_git_tag: bool,
    pub git_tag_prefix: Option<String>,
    pub skip_git_check: bool,
    pub run_pre_publish_script: bool,
    pub dry_run: bool,
    /// When set, pack from this directory (registry artifact layout) instead of the project root.
    pub artifact_dir: Option<String>,
    /// Glob patterns from package-manifest `files` merged into the publish tarball.
    pub include_files: Vec<String>,
    /// JSR tarballs use a flat root layout (no `package/` prefix).
    pub flat_layout: bool,
}

impl Default for PublishOptions {
    fn default() -> Self {
        Self {
            package_name: String::new(),
            version: String::new(),
            description: String::new(),
            homepage: None,
            license: None,
            package_path: String::new(),
            registry_name: String::new(),
            auth_token: None,
            tag: Some("latest".to_string()),
            access: Some("public".to_string()),
            bump: None,
            create_git_tag: true,
            git_tag_prefix: Some("v".to_string()),
            skip_git_check: false,
            run_pre_publish_script: true,
            dry_run: false,
            artifact_dir: None,
            include_files: Vec::new(),
            flat_layout: false,
        }
    }
}

/// Result of a publish attempt.
#[derive(Debug, Clone, Default)]
pub struct PublishResult {
    pub success: bool,
    pub package_name: String,
    pub version: String,
    pub message: String,
    pub published_url: Option<String>,
    pub dry_run: bool,
    pub sha256: Option<String>,
    pub size: Option<usize>,
    pub file_count: Option<usize>,
    /// True when the registry expects users to publish via an official CLI.
    pub official_tool_required: bool,
}

impl PublishResult {
    /// Build a structured result for registries that require official tooling.
    pub fn official_tool(
        package_name: impl Into<String>,
        version: impl Into<String>,
        message: impl Into<String>,
        published_url: Option<String>,
        size: usize,
    ) -> Self {
        Self {
            success: false,
            package_name: package_name.into(),
            version: version.into(),
            message: message.into(),
            published_url,
            dry_run: false,
            sha256: None,
            size: Some(size),
            file_count: None,
            official_tool_required: true,
        }
    }
}

/// Token verification outcome.
#[derive(Debug, Clone)]
pub struct TokenVerifyResult {
    pub valid: bool,
    pub username: Option<String>,
    pub error_message: Option<String>,
}

impl TokenVerifyResult {
    pub fn success(username: impl Into<String>) -> Self {
        Self { valid: true, username: Some(username.into()), error_message: None }
    }

    pub fn failure(message: impl Into<String>) -> Self {
        Self { valid: false, username: None, error_message: Some(message.into()) }
    }
}

/// Retry configuration for registry HTTP calls.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub initial_delay_ms: u64,
    pub backoff_multiplier: f64,
    pub max_delay_ms: u64,
    pub connect_timeout_secs: u64,
    pub request_timeout_secs: u64,
    pub upload_timeout_secs: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 500,
            backoff_multiplier: 2.0,
            max_delay_ms: 10_000,
            connect_timeout_secs: 30,
            request_timeout_secs: 60,
            upload_timeout_secs: 600,
        }
    }
}

impl RetryConfig {
    pub fn npm() -> Self {
        Self { max_retries: 5, initial_delay_ms: 500, backoff_multiplier: 2.0, max_delay_ms: 15_000, ..Self::default() }
    }
}

#[cfg(test)]
mod retry_config_tests {
    use super::RetryConfig;

    #[test]
    fn npm_upload_timeout_is_generous() {
        let config = RetryConfig::npm();
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.upload_timeout_secs, 600);
        assert_eq!(config.connect_timeout_secs, 30);
    }
}
