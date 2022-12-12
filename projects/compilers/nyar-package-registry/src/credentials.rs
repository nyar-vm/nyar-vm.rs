//! Concrete credential discovery/sync for named registry ecosystems.
//!
//! Reads and writes official CLI stores (`.npmrc`, Deno config, NuGet.Config, Maven settings,
//! Anaconda nucleus, …). The package-manager crate only holds credential *concepts* and
//! delegates here by registry adapter id.
//!
//! Priority per registry: config files → env vars.

use std::path::{Path, PathBuf};

use crate::RegistryError;

type Result<T> = std::result::Result<T, RegistryError>;

/// Token discovered from an official tooling store.
#[derive(Debug, Clone)]
pub struct DiscoveredCredential {
    pub token: String,
    pub source: String,
}

/// Whether `registry` uses third-party CLI credential stores (not persisted in vendor `auth.von`).
pub fn uses_external_credential_store(registry: &str) -> bool {
    matches!(registry.trim().to_ascii_lowercase().as_str(), "npm" | "jsr" | "conda" | "nuget" | "maven")
}

/// Discover a token for `registry` using official CLI configs.
pub fn discover_token(registry: &str, endpoint: Option<&str>, project_dir: Option<&Path>) -> Option<DiscoveredCredential> {
    match registry.to_ascii_lowercase().as_str() {
        "npm" => discover_npm_token(endpoint, project_dir),
        "jsr" => discover_jsr_token(),
        "nuget" => discover_nuget_token(),
        "maven" => discover_maven_token(),
        "conda" => discover_conda_token(),
        _ => None,
    }
}

/// Write a verified token into the official CLI store so other tools can reuse it.
pub fn sync_token_to_official_store(registry: &str, endpoint: Option<&str>, token: &str) -> Result<Option<PathBuf>> {
    match registry.to_ascii_lowercase().as_str() {
        "npm" => sync_npm_npmrc(endpoint, token).map(Some),
        "jsr" => sync_jsr_deno_config(token).map(Some),
        "nuget" => sync_nuget_config(endpoint, token).map(Some),
        "maven" => sync_maven_settings(token).map(Some),
        "conda" => sync_conda_nucleus(token).map(Some),
        _ => Ok(None),
    }
}

fn discover_jsr_token() -> Option<DiscoveredCredential> {
    if let Some(path) = deno_config_path() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Some(token) = parse_deno_config_jsr_token(&content) {
                return Some(DiscoveredCredential { token, source: format!("Deno 配置文件 ({})", path.display()) });
            }
        }
    }

    if let Ok(deno_auth_tokens) = std::env::var("DENO_AUTH_TOKENS") {
        for part in deno_auth_tokens.split(';') {
            let part = part.trim();
            if let Some(token) = part.strip_prefix("jsr@") {
                let token = token.trim();
                if !token.is_empty() {
                    return Some(DiscoveredCredential { token: token.to_string(), source: "env:DENO_AUTH_TOKENS".to_string() });
                }
            }
        }
    }

    if let Ok(token) = std::env::var("JSR_TOKEN") {
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Some(DiscoveredCredential { token, source: "env:JSR_TOKEN".to_string() });
        }
    }

    None
}

fn deno_config_path() -> Option<PathBuf> {
    deno_config_write_path().filter(|path| path.is_file())
}

fn deno_config_write_path() -> Option<PathBuf> {
    if let Ok(app_data) = std::env::var("APPDATA") {
        return Some(PathBuf::from(app_data).join("deno").join("config.json"));
    }
    if let Ok(config_home) = std::env::var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(config_home).join("deno").join("config.json"));
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)?;
    let existing =
        [home.join(".config").join("deno").join("config.json"), home.join(".deno").join("config.json")].into_iter().find(|path| path.is_file());
    existing.or(Some(home.join(".config").join("deno").join("config.json")))
}

fn parse_deno_config_jsr_token(content: &str) -> Option<String> {
    extract_json_string_value(content, r#""net.jsr""#).or_else(|| extract_json_string_value(content, r#""https://jsr.io""#))
}

/// Read `"key": "value"` string from a loosely-formatted JSON file.
fn extract_json_string_value(content: &str, key: &str) -> Option<String> {
    let start = content.find(key)?;
    let after_key = &content[start + key.len()..];
    let colon = after_key.find(':')?;
    let after_colon = after_key[colon + 1..].trim_start();
    let rest = after_colon.strip_prefix('"')?;
    let end = rest.find('"')?;
    let token = rest[..end].trim();
    (!token.is_empty()).then(|| token.to_string())
}

fn discover_npm_token(endpoint: Option<&str>, project_dir: Option<&Path>) -> Option<DiscoveredCredential> {
    let prefixes = npm_registry_prefixes(endpoint);
    if let Some(project_dir) = project_dir {
        let path = project_dir.join(".npmrc");
        if let Some(token) = read_npmrc_token(&path, &prefixes) {
            return Some(DiscoveredCredential { token, source: format!("project .npmrc ({})", path.display()) });
        }
    }

    if let Some(path) = user_npmrc_path() {
        if let Some(token) = read_npmrc_token(&path, &prefixes) {
            return Some(DiscoveredCredential { token, source: format!("user .npmrc ({})", path.display()) });
        }
    }

    for (var, source) in [("NPM_TOKEN", "env:NPM_TOKEN"), ("NODE_AUTH_TOKEN", "env:NODE_AUTH_TOKEN")] {
        if let Ok(token) = std::env::var(var) {
            let token = token.trim().to_string();
            if !token.is_empty() {
                return Some(DiscoveredCredential { token, source: source.to_string() });
            }
        }
    }

    None
}

fn discover_nuget_token() -> Option<DiscoveredCredential> {
    if let Some(path) = nuget_config_path() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Some((token, detail)) = parse_nuget_config_token(&content) {
                return Some(DiscoveredCredential { token, source: format!("NuGet.Config ({detail}, {})", path.display()) });
            }
        }
    }

    if let Ok(token) = std::env::var("NUGET_API_KEY") {
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Some(DiscoveredCredential { token, source: "env:NUGET_API_KEY".to_string() });
        }
    }

    None
}

fn discover_maven_token() -> Option<DiscoveredCredential> {
    let path = user_home_dir()?.join(".m2").join("settings.xml");
    if path.is_file() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Some((token, server_id)) = parse_maven_settings_token(&content) {
                return Some(DiscoveredCredential { token, source: format!("~/.m2/settings.xml [server id={server_id}]") });
            }
        }
    }

    if let Ok(token) = std::env::var("MAVEN_TOKEN") {
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Some(DiscoveredCredential { token, source: "env:MAVEN_TOKEN".to_string() });
        }
    }

    let username = std::env::var("SONATYPE_USERNAME").ok().filter(|value| !value.trim().is_empty());
    let password = std::env::var("SONATYPE_PASSWORD").ok().filter(|value| !value.trim().is_empty());
    if let (Some(username), Some(password)) = (username, password) {
        use base64::{Engine, engine::general_purpose::STANDARD};
        let token = STANDARD.encode(format!("{username}:{password}"));
        return Some(DiscoveredCredential { token, source: "env:SONATYPE_USERNAME/SONATYPE_PASSWORD".to_string() });
    }

    None
}

fn discover_conda_token() -> Option<DiscoveredCredential> {
    if let Some((path, token, username)) = find_conda_nucleus_token() {
        let source = if let Some(user) = username {
            format!("Anaconda Nucleus 令牌 ({user}, {})", path.display())
        }
        else {
            format!("Anaconda Nucleus 令牌 ({})", path.display())
        };
        return Some(DiscoveredCredential { token, source });
    }

    for (var, source) in [("ANACONDA_API_TOKEN", "env:ANACONDA_API_TOKEN"), ("CONDA_TOKEN", "env:CONDA_TOKEN")] {
        if let Ok(token) = std::env::var(var) {
            let token = token.trim().to_string();
            if !token.is_empty() {
                return Some(DiscoveredCredential { token, source: source.to_string() });
            }
        }
    }

    None
}

fn nuget_config_path() -> Option<PathBuf> {
    if let Ok(app_data) = std::env::var("APPDATA") {
        let path = PathBuf::from(app_data).join("NuGet").join("NuGet.Config");
        if path.is_file() {
            return Some(path);
        }
    }
    if let Ok(config_home) = std::env::var("XDG_CONFIG_HOME") {
        let path = PathBuf::from(config_home).join("NuGet").join("NuGet.Config");
        if path.is_file() {
            return Some(path);
        }
    }
    let path = user_home_dir()?.join(".nuget").join("NuGet").join("NuGet.Config");
    path.is_file().then_some(path)
}

fn nuget_config_write_path() -> Option<PathBuf> {
    if let Ok(app_data) = std::env::var("APPDATA") {
        return Some(PathBuf::from(app_data).join("NuGet").join("NuGet.Config"));
    }
    if let Ok(config_home) = std::env::var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(config_home).join("NuGet").join("NuGet.Config"));
    }
    user_home_dir().map(|home| home.join(".nuget").join("NuGet").join("NuGet.Config"))
}

fn parse_nuget_config_token(content: &str) -> Option<(String, String)> {
    if let Some(token) = extract_xml_add_attribute_value(content, "apikeys", "value") {
        return Some((token, "apikeys".to_string()));
    }
    for key in ["ClearTextPassword", "Password"] {
        if let Some(token) = extract_xml_add_attribute_value(content, key, "value") {
            return Some((token, format!("packageSourceCredentials/{key}")));
        }
    }
    None
}

fn extract_xml_add_attribute_value(content: &str, context_hint: &str, attribute: &str) -> Option<String> {
    let lower = content.to_ascii_lowercase();
    let hint_pos = lower.find(&context_hint.to_ascii_lowercase())?;
    let slice = &content[hint_pos..];
    for line in slice.lines().take(40) {
        let line_lower = line.to_ascii_lowercase();
        if !line_lower.contains("<add") {
            continue;
        }
        if let Some(value) = extract_xml_attribute(line, attribute) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn extract_xml_attribute(line: &str, attribute: &str) -> Option<String> {
    let pattern = format!("{attribute}=\"");
    let start = line.find(&pattern)? + pattern.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn parse_maven_settings_token(content: &str) -> Option<(String, String)> {
    let normalized = content.replace('\r', "");
    for block in normalized.split("<server>").skip(1) {
        let block = block.split("</server>").next().unwrap_or(block);
        let id = extract_xml_element_text(block, "id")?;
        let id_lower = id.to_ascii_lowercase();
        if !id_lower.contains("ossrh") && !id_lower.contains("central") && !id_lower.contains("sonatype") {
            continue;
        }
        if let (Some(username), Some(password)) = (extract_xml_element_text(block, "username"), extract_xml_element_text(block, "password")) {
            use base64::{Engine, engine::general_purpose::STANDARD};
            let token = STANDARD.encode(format!("{username}:{password}"));
            return Some((token, id));
        }
        if let Some(private_key) = extract_xml_element_text(block, "privateKey") {
            return Some((private_key, id));
        }
    }
    None
}

fn extract_xml_element_text(block: &str, element: &str) -> Option<String> {
    let open = format!("<{element}>");
    let close = format!("</{element}>");
    let start = block.find(&open)? + open.len();
    let rest = &block[start..];
    let end = rest.find(&close)?;
    let value = rest[..end].trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn find_conda_nucleus_token() -> Option<(PathBuf, String, Option<String>)> {
    let nucleus_dir = user_home_dir()?.join(".anaconda").join("nucleus");
    if !nucleus_dir.is_dir() {
        return None;
    }
    let entries = std::fs::read_dir(&nucleus_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path().join("tokens.json");
        if !path.is_file() {
            continue;
        }
        let content = std::fs::read_to_string(&path).ok()?;
        let token = extract_json_string_value(&content, "\"token\"").or_else(|| extract_json_string_value(&content, "\"access_token\""))?;
        let username = extract_json_string_value(&content, "\"login\"");
        return Some((path, token, username));
    }
    None
}

fn maven_settings_write_path() -> Option<PathBuf> {
    user_home_dir().map(|home| home.join(".m2").join("settings.xml"))
}

fn conda_nucleus_write_path() -> Option<PathBuf> {
    let nucleus_dir = user_home_dir()?.join(".anaconda").join("nucleus");
    if !nucleus_dir.is_dir() {
        return None;
    }
    let entries = std::fs::read_dir(&nucleus_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path().join("tokens.json");
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn user_home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from)
}

fn user_npmrc_path() -> Option<PathBuf> {
    let path = user_home_dir()?.join(".npmrc");
    path.is_file().then_some(path)
}

fn npm_registry_prefixes(endpoint: Option<&str>) -> Vec<String> {
    let mut prefixes = vec!["//registry.npmjs.org/".to_string()];
    if let Some(endpoint) = endpoint {
        if let Some(prefix) = registry_auth_prefix(endpoint) {
            if !prefixes.iter().any(|item| item == &prefix) {
                prefixes.insert(0, prefix);
            }
        }
    }
    prefixes
}

/// Convert `https://registry.npmjs.org` → `//registry.npmjs.org/`.
fn registry_auth_prefix(endpoint: &str) -> Option<String> {
    let trimmed = endpoint.trim().trim_end_matches('/');
    let without_scheme = trimmed.strip_prefix("https://").or_else(|| trimmed.strip_prefix("http://")).unwrap_or(trimmed);
    if without_scheme.is_empty() {
        return None;
    }
    Some(format!("//{without_scheme}/"))
}

fn read_npmrc_token(path: &Path, prefixes: &[String]) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let content = expand_npmrc_env(&content);

    for prefix in prefixes {
        if let Some(token) = parse_npmrc_token(&content, prefix) {
            return Some(token);
        }
    }

    // Fallback: registry= line may point at a private host.
    if let Some(registry) = parse_npmrc_registry(&content) {
        if let Some(prefix) = registry_auth_prefix(&registry) {
            if let Some(token) = parse_npmrc_token(&content, &prefix) {
                return Some(token);
            }
        }
    }

    // Last resort: any `:_authToken=` line for npm-like hosts.
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            if key.ends_with(":_authToken") || key.ends_with(":_authToken\"") {
                let token = expand_npmrc_value(value.trim());
                if !token.is_empty() {
                    return Some(token);
                }
            }
        }
    }
    None
}

fn parse_npmrc_token(content: &str, registry_prefix: &str) -> Option<String> {
    let auth_token_key = format!("{registry_prefix}:_authToken");
    let auth_key = format!("{registry_prefix}:_auth");
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = expand_npmrc_value(value.trim());
            if key.eq_ignore_ascii_case(&auth_token_key) && !value.is_empty() {
                return Some(value);
            }
            if key.eq_ignore_ascii_case(&auth_key) && !value.is_empty() {
                if let Some(token) = decode_basic_auth_password(&value) {
                    return Some(token);
                }
            }
        }
    }
    None
}

fn parse_npmrc_registry(content: &str) -> Option<String> {
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            if key.trim().eq_ignore_ascii_case("registry") {
                let mut registry = expand_npmrc_value(value.trim());
                if !registry.ends_with('/') {
                    registry.push('/');
                }
                return Some(registry);
            }
        }
    }
    None
}

fn decode_basic_auth_password(value: &str) -> Option<String> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let bytes = STANDARD.decode(value).ok()?;
    let auth = String::from_utf8(bytes).ok()?;
    let (_user, password) = auth.split_once(':')?;
    let password = password.trim();
    (!password.is_empty()).then(|| password.to_string())
}

fn expand_npmrc_env(content: &str) -> String {
    content
        .lines()
        .map(|line| {
            if let Some((key, value)) = line.split_once('=') { format!("{key}={}", expand_npmrc_value(value.trim())) } else { line.to_string() }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Expand `${VAR}` / `$VAR` in npmrc values (common CI pattern).
fn expand_npmrc_value(value: &str) -> String {
    let value = value.trim().trim_matches('"').trim_matches('\'');
    if let Some(name) = value.strip_prefix("${").and_then(|value| value.strip_suffix('}')) {
        return std::env::var(name).unwrap_or_default();
    }
    if let Some(name) = value.strip_prefix('$') {
        if name.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
            return std::env::var(name).unwrap_or_default();
        }
    }
    value.to_string()
}

fn sync_npm_npmrc(endpoint: Option<&str>, token: &str) -> Result<PathBuf> {
    let prefixes = npm_registry_prefixes(endpoint);
    let prefix = prefixes.first().cloned().unwrap_or_else(|| "//registry.npmjs.org/".to_string());
    let path = user_home_dir().map(|home| home.join(".npmrc")).ok_or_else(|| RegistryError::message("无法定位用户 home 目录以写入 .npmrc"))?;
    let content = if path.is_file() { std::fs::read_to_string(&path)? } else { String::new() };
    let updated = upsert_npmrc_auth_token(&content, &prefix, token);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, updated)?;
    Ok(path)
}

fn upsert_npmrc_auth_token(content: &str, prefix: &str, token: &str) -> String {
    let line_key = format!("{prefix}:_authToken");
    let mut replaced = false;
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    for line in &mut lines {
        let trimmed = line.trim();
        if trimmed.starts_with(&format!("{line_key}=")) {
            *line = format!("{line_key}={token}");
            replaced = true;
        }
    }
    if !replaced {
        if !content.is_empty() && !content.ends_with('\n') {
            lines.push(String::new());
        }
        lines.push(format!("{line_key}={token}"));
    }
    let mut out = lines.join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn sync_jsr_deno_config(token: &str) -> Result<PathBuf> {
    let path = deno_config_write_path().ok_or_else(|| RegistryError::message("无法定位 Deno 配置文件路径"))?;
    let content =
        if path.is_file() { upsert_deno_jsr_token(&std::fs::read_to_string(&path)?, token) } else { format!(r#"{{"net.jsr": "{token}"}}"#) };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, format!("{content}\n"))?;
    Ok(path)
}

fn upsert_deno_jsr_token(content: &str, token: &str) -> String {
    if content.contains("\"net.jsr\"") {
        return replace_json_string_value(content, "\"net.jsr\"", token);
    }
    if content.contains("\"https://jsr.io\"") {
        return replace_json_string_value(content, "\"https://jsr.io\"", token);
    }
    let trimmed = content.trim();
    if trimmed.is_empty() || trimmed == "{}" {
        return format!(r#"{{"net.jsr": "{token}"}}"#);
    }
    if trimmed.ends_with('}') {
        let body = trimmed.trim_end_matches('}').trim_end_matches(',');
        return format!("{body},\n  \"net.jsr\": \"{token}\"\n}}");
    }
    format!(r#"{{"net.jsr": "{token}"}}"#)
}

fn replace_json_string_value(content: &str, key: &str, token: &str) -> String {
    let Some(start) = content.find(key)
    else {
        return content.to_string();
    };
    let after_key = &content[start + key.len()..];
    let Some(colon) = after_key.find(':')
    else {
        return content.to_string();
    };
    let before = &content[..start + key.len() + colon + 1];
    format!("{before} \"{token}\"")
}

fn sync_nuget_config(endpoint: Option<&str>, token: &str) -> Result<PathBuf> {
    let path = nuget_config_write_path().ok_or_else(|| RegistryError::message("无法定位 NuGet.Config 路径"))?;
    let api_key_source = endpoint
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("{}/v3/index.json", value.trim_end_matches('/')))
        .unwrap_or_else(|| "https://api.nuget.org/v3/index.json".to_string());
    let content = if path.is_file() {
        std::fs::read_to_string(&path)?
    }
    else {
        r#"<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <config />
  <packageSources />
  <apikeys />
</configuration>
"#
        .to_string()
    };
    let updated = upsert_nuget_api_key(&content, &api_key_source, token);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, updated)?;
    Ok(path)
}

fn upsert_nuget_api_key(content: &str, source_key: &str, token: &str) -> String {
    let escaped_key = xml_escape(source_key);
    let add_line = format!(r#"    <add key="{escaped_key}" value="{}" />"#, xml_escape(token));
    if content.contains("<apikeys>") {
        if content.contains(&format!("key=\"{escaped_key}\"")) {
            let mut lines: Vec<String> = Vec::new();
            let mut replaced = false;
            for line in content.lines() {
                if line.contains(&format!("key=\"{escaped_key}\"")) && line.contains("<add") {
                    lines.push(add_line.clone());
                    replaced = true;
                }
                else {
                    lines.push(line.to_string());
                }
            }
            if replaced {
                return format!("{}\n", lines.join("\n"));
            }
        }
        return content.replacen("<apikeys>", &format!("<apikeys>\n{add_line}"), 1);
    }
    if content.contains("</configuration>") {
        return content.replacen("</configuration>", &format!("  <apikeys>\n{add_line}\n  </apikeys>\n</configuration>"), 1);
    }
    format!("{content}\n<apikeys>\n{add_line}\n</apikeys>\n")
}

fn sync_maven_settings(token: &str) -> Result<PathBuf> {
    let path = maven_settings_write_path().ok_or_else(|| RegistryError::message("无法定位 ~/.m2/settings.xml 路径"))?;
    let content = if path.is_file() {
        std::fs::read_to_string(&path)?
    }
    else {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<settings xmlns="http://maven.apache.org/SETTINGS/1.0.0">
  <servers />
</settings>
"#
        .to_string()
    };
    let updated = upsert_maven_server_token(&content, "ossrh", token);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, updated)?;
    Ok(path)
}

fn upsert_maven_server_token(content: &str, server_id: &str, token: &str) -> String {
    let block = format!(
        "    <server>\n      <id>{server_id}</id>\n      <username>token</username>\n      <password>{}</password>\n    </server>",
        xml_escape(token)
    );
    if content.contains(&format!("<id>{server_id}</id>")) {
        let mut out = String::new();
        let normalized = content.replace('\r', "");
        let mut in_target = false;
        for line in normalized.lines() {
            if line.contains("<server>") && normalized[normalized.find(line).unwrap_or(0)..].contains(&format!("<id>{server_id}</id>")) {
                if !in_target {
                    out.push_str(&block);
                    out.push('\n');
                    in_target = true;
                }
                continue;
            }
            if in_target {
                if line.contains("</server>") {
                    in_target = false;
                }
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
        return out;
    }
    if content.contains("<servers>") {
        return content.replacen("<servers>", &format!("<servers>\n{block}"), 1);
    }
    if content.contains("</settings>") {
        return content.replacen("</settings>", &format!("  <servers>\n{block}\n  </servers>\n</settings>"), 1);
    }
    format!("{content}\n<servers>\n{block}\n</servers>\n")
}

fn sync_conda_nucleus(token: &str) -> Result<PathBuf> {
    let path = conda_nucleus_write_path().ok_or_else(|| RegistryError::message("未找到 Anaconda nucleus 目录，请先运行 `anaconda login`"))?;
    let content = std::fs::read_to_string(&path)?;
    let updated = if content.contains("\"token\"") {
        replace_json_string_value(&content, "\"token\"", token)
    }
    else if content.contains("\"access_token\"") {
        replace_json_string_value(&content, "\"access_token\"", token)
    }
    else {
        let trimmed = content.trim();
        if trimmed.ends_with('}') {
            let body = trimmed.trim_end_matches('}').trim_end_matches(',');
            format!("{body},\n  \"token\": \"{token}\"\n}}")
        }
        else {
            format!(r#"{{"token": "{token}"}}"#)
        }
    };
    std::fs::write(&path, format!("{updated}\n"))?;
    Ok(path)
}

fn xml_escape(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use super::*;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn parses_auth_token_line() {
        let content = "//registry.npmjs.org/:_authToken=npm_test_token\n";
        assert_eq!(parse_npmrc_token(content, "//registry.npmjs.org/").as_deref(), Some("npm_test_token"));
    }

    #[test]
    fn expands_env_placeholder() {
        // SAFETY: unit-test only.
        unsafe {
            std::env::set_var("REGISTRY_TEST_NPM_TOKEN", "from-env");
        }
        assert_eq!(expand_npmrc_value("${REGISTRY_TEST_NPM_TOKEN}"), "from-env");
        unsafe {
            std::env::remove_var("REGISTRY_TEST_NPM_TOKEN");
        }
    }

    #[test]
    fn registry_prefix_from_endpoint() {
        assert_eq!(registry_auth_prefix("https://registry.npmjs.org"), Some("//registry.npmjs.org/".to_string()));
        assert_eq!(registry_auth_prefix("https://npm.pkg.github.com/"), Some("//npm.pkg.github.com/".to_string()));
    }

    #[test]
    fn discovers_jsr_token_from_env() {
        let _guard = env_lock().lock().expect("lock");
        let previous = std::env::var_os("JSR_TOKEN");
        let previous_deno = std::env::var_os("DENO_AUTH_TOKENS");
        unsafe {
            std::env::set_var("JSR_TOKEN", "jsr-from-env");
            std::env::remove_var("DENO_AUTH_TOKENS");
        }
        let credential = discover_jsr_token().expect("token");
        assert_eq!(credential.token, "jsr-from-env");
        unsafe {
            match previous {
                Some(value) => std::env::set_var("JSR_TOKEN", value),
                None => std::env::remove_var("JSR_TOKEN"),
            }
            match previous_deno {
                Some(value) => std::env::set_var("DENO_AUTH_TOKENS", value),
                None => std::env::remove_var("DENO_AUTH_TOKENS"),
            }
        }
    }

    #[test]
    fn discovers_jsr_token_from_deno_auth_tokens() {
        let _guard = env_lock().lock().expect("lock");
        let previous = std::env::var_os("JSR_TOKEN");
        let previous_deno = std::env::var_os("DENO_AUTH_TOKENS");
        unsafe {
            std::env::remove_var("JSR_TOKEN");
            std::env::set_var("DENO_AUTH_TOKENS", "npm@token1;jsr@jsr-from-deno;other@token3");
        }
        let credential = discover_jsr_token().expect("token");
        assert_eq!(credential.token, "jsr-from-deno");
        assert!(credential.source.contains("DENO_AUTH_TOKENS"));
        unsafe {
            match previous {
                Some(value) => std::env::set_var("JSR_TOKEN", value),
                None => std::env::remove_var("JSR_TOKEN"),
            }
            match previous_deno {
                Some(value) => std::env::set_var("DENO_AUTH_TOKENS", value),
                None => std::env::remove_var("DENO_AUTH_TOKENS"),
            }
        }
    }

    #[test]
    fn extracts_deno_config_token_value() {
        let content = r#"{ "net.jsr": "token-from-deno-config" }"#;
        assert_eq!(extract_json_string_value(content, r#""net.jsr""#).as_deref(), Some("token-from-deno-config"));
        let alt = r#"{ "https://jsr.io": "alt-token" }"#;
        assert_eq!(extract_json_string_value(alt, r#""https://jsr.io""#).as_deref(), Some("alt-token"));
    }

    #[test]
    fn sync_npm_token_writes_user_npmrc() {
        let _guard = env_lock().lock().expect("lock");
        let home = tempfile::tempdir().expect("temp home");
        let previous_home = std::env::var_os("HOME");
        let previous_profile = std::env::var_os("USERPROFILE");
        unsafe {
            std::env::set_var("HOME", home.path());
            std::env::set_var("USERPROFILE", home.path());
        }

        let path = sync_token_to_official_store("npm", Some("https://registry.npmjs.org"), "npm-sync-token").expect("sync").expect("path");
        let content = std::fs::read_to_string(&path).expect("read npmrc");
        assert!(content.contains("npm-sync-token"));

        unsafe {
            match previous_home {
                Some(value) => std::env::set_var("HOME", value),
                None => std::env::remove_var("HOME"),
            }
            match previous_profile {
                Some(value) => std::env::set_var("USERPROFILE", value),
                None => std::env::remove_var("USERPROFILE"),
            }
        }
    }

    #[test]
    fn discovers_nuget_token_from_config() {
        let _guard = env_lock().lock().expect("lock");
        let home = tempfile::tempdir().expect("temp home");
        let nuget_dir = home.path().join("NuGet");
        std::fs::create_dir_all(&nuget_dir).expect("mkdir");
        std::fs::write(
            nuget_dir.join("NuGet.Config"),
            r#"<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <apikeys>
    <add key="https://api.nuget.org/v3/index.json" value="nuget-from-config" />
  </apikeys>
</configuration>
"#,
        )
        .expect("write config");
        let previous = std::env::var_os("APPDATA");
        unsafe {
            std::env::set_var("APPDATA", home.path());
        }
        let credential = discover_nuget_token().expect("token");
        assert_eq!(credential.token, "nuget-from-config");
        unsafe {
            match previous {
                Some(value) => std::env::set_var("APPDATA", value),
                None => std::env::remove_var("APPDATA"),
            }
        }
    }

    #[test]
    fn discovers_maven_token_from_settings_xml() {
        let _guard = env_lock().lock().expect("lock");
        let home = tempfile::tempdir().expect("temp home");
        let m2 = home.path().join(".m2");
        std::fs::create_dir_all(&m2).expect("mkdir");
        std::fs::write(
            m2.join("settings.xml"),
            r#"<settings>
  <servers>
    <server>
      <id>ossrh</id>
      <username>user</username>
      <password>pass</password>
    </server>
  </servers>
</settings>
"#,
        )
        .expect("write settings");
        let previous_home = std::env::var_os("HOME");
        let previous_profile = std::env::var_os("USERPROFILE");
        unsafe {
            std::env::set_var("HOME", home.path());
            std::env::set_var("USERPROFILE", home.path());
        }
        let credential = discover_maven_token().expect("token");
        use base64::{Engine, engine::general_purpose::STANDARD};
        assert_eq!(credential.token, STANDARD.encode("user:pass"));
        unsafe {
            match previous_home {
                Some(value) => std::env::set_var("HOME", value),
                None => std::env::remove_var("HOME"),
            }
            match previous_profile {
                Some(value) => std::env::set_var("USERPROFILE", value),
                None => std::env::remove_var("USERPROFILE"),
            }
        }
    }

    #[test]
    fn discovers_conda_token_from_nucleus() {
        let _guard = env_lock().lock().expect("lock");
        let home = tempfile::tempdir().expect("temp home");
        let token_path = home.path().join(".anaconda").join("nucleus").join("user-1").join("tokens.json");
        std::fs::create_dir_all(token_path.parent().unwrap()).expect("mkdir");
        std::fs::write(token_path, r#"{"token":"conda-from-nucleus","login":"alice"}"#).expect("write token");
        let previous_home = std::env::var_os("HOME");
        let previous_profile = std::env::var_os("USERPROFILE");
        unsafe {
            std::env::set_var("HOME", home.path());
            std::env::set_var("USERPROFILE", home.path());
        }
        let credential = discover_conda_token().expect("token");
        assert_eq!(credential.token, "conda-from-nucleus");
        unsafe {
            match previous_home {
                Some(value) => std::env::set_var("HOME", value),
                None => std::env::remove_var("HOME"),
            }
            match previous_profile {
                Some(value) => std::env::set_var("USERPROFILE", value),
                None => std::env::remove_var("USERPROFILE"),
            }
        }
    }

    #[test]
    fn sync_nuget_token_writes_config() {
        let _guard = env_lock().lock().expect("lock");
        let home = tempfile::tempdir().expect("temp home");
        let previous = std::env::var_os("APPDATA");
        unsafe {
            std::env::set_var("APPDATA", home.path());
        }
        let path = sync_token_to_official_store("nuget", Some("https://api.nuget.org/v3"), "nuget-sync-token").expect("sync").expect("path");
        let content = std::fs::read_to_string(path).expect("read config");
        assert!(content.contains("nuget-sync-token"));
        unsafe {
            match previous {
                Some(value) => std::env::set_var("APPDATA", value),
                None => std::env::remove_var("APPDATA"),
            }
        }
    }

    #[test]
    fn uses_external_credential_store_classifies_third_party_registries() {
        assert!(uses_external_credential_store("npm"));
        assert!(uses_external_credential_store("nuget"));
        assert!(!uses_external_credential_store("valhalla"));
    }
}
