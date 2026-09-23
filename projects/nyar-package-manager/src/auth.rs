use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use nyar_package_registry::{PublisherKey, Registry, TokenVerifyResult};
use serde::{Deserialize, Serialize};

use crate::{DiscoveredCredential, PackageManagerError, ProjectLayout, Result, credentials};

/// Stored vendor authentication tokens (path from [`ProjectLayout`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorAuthStore {
    #[serde(default)]
    tokens: BTreeMap<String, String>,
    #[serde(skip)]
    path: PathBuf,
    #[serde(skip)]
    token_env_vars: &'static [&'static str],
}

impl Default for VendorAuthStore {
    fn default() -> Self {
        Self { tokens: BTreeMap::new(), path: PathBuf::new(), token_env_vars: &[] }
    }
}

impl VendorAuthStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_token_envs(path, &[])
    }

    pub fn open_with_layout(layout: ProjectLayout) -> Result<Self> {
        Self::open_with_token_envs(layout.auth_store_path(), layout.token_env_vars)
    }

    pub fn open_with_token_envs(path: impl AsRef<Path>, token_env_vars: &'static [&'static str]) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if path.is_file() {
            let source = std::fs::read_to_string(&path)?;
            let mut store: VendorAuthStore = std_data::text::von::from_str(&source).unwrap_or_default();
            store.path = path;
            store.token_env_vars = token_env_vars;
            Ok(store)
        }
        else {
            Ok(Self { tokens: BTreeMap::new(), path, token_env_vars })
        }
    }

    pub fn path_for(layout: ProjectLayout) -> PathBuf {
        layout.auth_store_path()
    }

    pub fn get_token(&self, registry: &str) -> Option<String> {
        self.get_token_for(registry, None, None)
    }

    /// Resolve token: vendor store / generic env, then registry credential discovery.
    ///
    /// External registries skip `auth.von`; Valhalla and other first-party ids read `auth.von` first.
    pub fn get_token_for(&self, registry: &str, endpoint: Option<&str>, project_dir: Option<&Path>) -> Option<String> {
        self.discover_credential(registry, endpoint, project_dir).map(|item| item.token)
    }

    pub fn discover_credential(&self, registry: &str, endpoint: Option<&str>, project_dir: Option<&Path>) -> Option<DiscoveredCredential> {
        if !credentials::is_external_registry(registry) {
            if let Some(token) = self.tokens.get(registry).cloned().filter(|token| !token.trim().is_empty()) {
                return Some(DiscoveredCredential { token, source: format!("auth.von ({})", self.path.display()) });
            }
        }
        if let Ok(token) = std::env::var(format!("{}_TOKEN", registry.to_ascii_uppercase())) {
            let token = token.trim().to_string();
            if !token.is_empty() {
                return Some(DiscoveredCredential { token, source: format!("env:{}_TOKEN", registry.to_ascii_uppercase()) });
            }
        }
        for var in self.token_env_vars {
            if let Ok(token) = std::env::var(var) {
                let token = token.trim().to_string();
                if !token.is_empty() {
                    return Some(DiscoveredCredential { token, source: format!("env:{var}") });
                }
            }
        }
        credentials::discover_token(registry, endpoint, project_dir)
    }

    pub fn set_token(&mut self, registry: impl Into<String>, token: impl Into<String>) -> Result<()> {
        self.tokens.insert(registry.into(), token.into());
        self.save()
    }

    pub fn remove_token(&mut self, registry: &str) -> Result<bool> {
        let removed = self.tokens.remove(registry).is_some();
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    pub fn list(&self) -> Vec<String> {
        self.tokens.keys().cloned().collect()
    }

    fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = nyar_language::formatter::to_string_indented(self)?;
        std::fs::write(&self.path, content)?;
        Ok(())
    }
}

/// Result of product CLI login.
#[derive(Debug, Clone)]
pub struct LoginResult {
    pub verify: TokenVerifyResult,
    pub credential_source: String,
    pub synced_to: Option<String>,
    pub stored_in_auth_von: bool,
}

/// Vendor / registry auth operations.
#[derive(Debug, Clone)]
pub struct VendorManager {
    pub auth: VendorAuthStore,
}

impl VendorManager {
    pub fn open_with_layout(layout: ProjectLayout) -> Result<Self> {
        Ok(Self { auth: VendorAuthStore::open_with_layout(layout)? })
    }

    /// Open with neutral layout paths (tests / tools without a product CLI).
    pub fn open_default() -> Result<Self> {
        Self::open_with_layout(ProjectLayout::neutral())
    }

    /// Login to a registry. When `token` is empty, discover from registry credential stores first.
    pub fn login(
        &mut self,
        registry: &dyn Registry,
        token: Option<&str>,
        project_dir: Option<&Path>,
        sync_official: bool,
    ) -> Result<LoginResult> {
        let manual = token.filter(|value| !value.trim().is_empty());
        let (token, source) = if let Some(token) = manual {
            (token.to_string(), "手动输入".to_string())
        }
        else if let Some(credential) = registry.discover_credential(project_dir) {
            (credential.token, credential.source)
        }
        else if !registry.uses_external_credentials() {
            if let Some(credential) = self.auth.discover_credential(registry.name(), Some(registry.endpoint()), project_dir) {
                (credential.token, credential.source)
            }
            else {
                return Err(PackageManagerError::message(format!(
                    "未提供认证令牌，且无法从官方 CLI 配置或 auth.von 发现 {} 凭据；请使用 `login --token <token>`",
                    registry.name()
                )));
            }
        }
        else {
            return Err(PackageManagerError::message(format!(
                "未提供认证令牌，且无法从官方 CLI 配置发现 {} 凭据；请先完成该注册表的官方登录，或使用 `login --token <token>`",
                registry.name()
            )));
        };

        let result = registry.verify_token(&token)?;
        if !result.valid {
            return Ok(LoginResult { verify: result, credential_source: source, synced_to: None, stored_in_auth_von: false });
        }

        let mut stored_in_auth_von = false;
        if registry.name() == "valhalla" {
            let stored = PublisherKey::parse(&token).map(|key| key.to_auth_material()).unwrap_or_else(|_| token.clone());
            self.auth.set_token(registry.name(), stored)?;
            stored_in_auth_von = true;
        }
        else if registry.uses_external_credentials() {
            // Legacy vendor auth entries — remove without touching official CLI stores.
            let _ = self.auth.remove_token(registry.name());
        }
        else {
            self.auth.set_token(registry.name(), token.clone())?;
            stored_in_auth_von = true;
        }

        let synced_to =
            if manual.is_some() && sync_official { registry.sync_credential(&token)?.map(|path| path.display().to_string()) } else { None };

        Ok(LoginResult { verify: result, credential_source: source, synced_to, stored_in_auth_von })
    }

    pub fn logout(&mut self, registry_name: &str) -> Result<bool> {
        self.auth.remove_token(registry_name)
    }

    pub fn whoami(&self, registry: &dyn Registry) -> Result<TokenVerifyResult> {
        let Some(credential) = self.auth.discover_credential(registry.name(), Some(registry.endpoint()), None)
        else {
            return Ok(TokenVerifyResult::failure(format!(
                "未找到注册表 {} 的凭据（环境变量 / 官方 CLI 配置{}）",
                registry.name(),
                if registry.uses_external_credentials() { "" } else { " / auth.von" }
            )));
        };
        let mut result = registry.verify_token(&credential.token)?;
        if result.valid {
            if let Some(username) = result.username.as_mut() {
                *username = format!("{username} ← {}", credential.source);
            }
        }
        Ok(result)
    }

    pub fn list(&self) -> Vec<String> {
        self.auth.list()
    }

    pub fn require_token(&self, registry_name: &str) -> Result<String> {
        self.auth
            .get_token(registry_name)
            .ok_or_else(|| PackageManagerError::message(format!("未找到注册表 {registry_name} 的认证令牌，请先使用产品 CLI 登录")))
    }
}
