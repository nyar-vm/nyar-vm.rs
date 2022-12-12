use std::{io::Write, path::Path, thread, time::Duration};

use flate2::read::GzDecoder;
use reqwest::{
    Method, StatusCode,
    blocking::{Client, Response},
    header::{AUTHORIZATION, CONTENT_TYPE},
};
use sha2::{Digest, Sha256, Sha512};
use tar::Archive;

use crate::{RegistryError, RetryConfig};

/// Shared HTTP helpers used by registry adapters.
#[derive(Debug, Clone)]
pub struct HttpClient {
    client: Client,
    retry: RetryConfig,
}

enum AuthHeader<'a> {
    None,
    Bearer(&'a str),
    Raw(&'a str),
}

impl HttpClient {
    pub fn new(retry: RetryConfig) -> Result<Self, RegistryError> {
        let client = Client::builder()
            .user_agent("nyar-package-registry/0.1")
            .tcp_keepalive(Some(Duration::from_secs(60)))
            .connect_timeout(Duration::from_secs(retry.connect_timeout_secs))
            .timeout(Duration::from_secs(retry.request_timeout_secs))
            .build()?;
        Ok(Self { client, retry })
    }

    /// Human-readable hint when standard proxy env vars are set (`HTTPS_PROXY`, etc.).
    pub fn proxy_env_hint() -> Option<String> {
        for (name, value) in [
            ("HTTPS_PROXY", std::env::var("HTTPS_PROXY").ok()),
            ("https_proxy", std::env::var("https_proxy").ok()),
            ("HTTP_PROXY", std::env::var("HTTP_PROXY").ok()),
            ("http_proxy", std::env::var("http_proxy").ok()),
            ("ALL_PROXY", std::env::var("ALL_PROXY").ok()),
            ("all_proxy", std::env::var("all_proxy").ok()),
        ] {
            if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
                return Some(format!("{name}={value}"));
            }
        }
        None
    }

    pub fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T, RegistryError> {
        let response = self.send_get(url, None)?;
        if response.status() == StatusCode::NOT_FOUND {
            return Err(RegistryError::NotFound(url.to_string()));
        }
        if !response.status().is_success() {
            return Err(RegistryError::status(response.status().as_u16(), response.text().unwrap_or_default()));
        }
        Ok(response.json()?)
    }

    pub fn get_bytes(&self, url: &str) -> Result<Vec<u8>, RegistryError> {
        let response = self.send_get(url, None)?;
        if !response.status().is_success() {
            return Err(RegistryError::status(response.status().as_u16(), response.text().unwrap_or_default()));
        }
        Ok(response.bytes()?.to_vec())
    }

    pub fn get_authenticated_json<T: serde::de::DeserializeOwned>(&self, url: &str, token: &str) -> Result<T, RegistryError> {
        let response = self.send_get(url, Some(token))?;
        if !response.status().is_success() {
            return Err(RegistryError::status(response.status().as_u16(), response.text().unwrap_or_default()));
        }
        Ok(response.json()?)
    }

    /// GET with arbitrary extra headers (e.g. `X-NuGet-ApiKey`).
    pub fn get_with_headers(&self, url: &str, headers: &[(&str, &str)]) -> Result<Response, RegistryError> {
        self.send_get_with_headers(url, None, headers)
    }

    /// GET with Bearer token and extra headers (e.g. Maven Basic auth).
    pub fn get_authenticated_with_headers(&self, url: &str, authorization: &str, headers: &[(&str, &str)]) -> Result<Response, RegistryError> {
        self.send_get_with_headers(url, Some(authorization), headers)
    }

    pub fn put_bytes(&self, url: &str, body: &[u8], token: Option<&str>, content_type: &str) -> Result<Response, RegistryError> {
        let auth = token.map(AuthHeader::Bearer).unwrap_or(AuthHeader::None);
        self.send_with_body(Method::PUT, url, Some(body), auth, content_type, true)
    }

    /// POST a body with an explicit Authorization header value (e.g. Ed25519 signature scheme).
    pub fn post_bytes_with_authorization(
        &self,
        url: &str,
        body: &[u8],
        authorization: &str,
        content_type: &str,
    ) -> Result<Response, RegistryError> {
        self.send_with_body(Method::POST, url, Some(body), AuthHeader::Raw(authorization), content_type, true)
    }

    /// POST with Bearer token (e.g. JSR management API).
    pub fn post_bytes(&self, url: &str, body: &[u8], token: &str, content_type: &str) -> Result<Response, RegistryError> {
        self.send_with_body(Method::POST, url, Some(body), AuthHeader::Bearer(token), content_type, true)
    }

    fn send_get(&self, url: &str, token: Option<&str>) -> Result<Response, RegistryError> {
        let auth = token.map(AuthHeader::Bearer).unwrap_or(AuthHeader::None);
        self.send_with_body(Method::GET, url, None, auth, "application/json", false)
    }

    fn send_get_with_headers(&self, url: &str, authorization: Option<&str>, headers: &[(&str, &str)]) -> Result<Response, RegistryError> {
        let mut delay = self.retry.initial_delay_ms;
        let mut last_error = None;
        let timeout = Duration::from_secs(self.retry.request_timeout_secs);

        for attempt in 0..=self.retry.max_retries {
            let mut request = self.client.request(Method::GET, url).timeout(timeout);
            if let Some(value) = authorization {
                request = request.header(AUTHORIZATION, value);
            }
            for (name, value) in headers {
                request = request.header(*name, *value);
            }
            match request.send() {
                Ok(response) => {
                    let status = response.status();
                    let should_retry = status.as_u16() == 429 || status.is_server_error();
                    if should_retry && attempt < self.retry.max_retries {
                        thread::sleep(Duration::from_millis(delay));
                        delay = ((delay as f64) * self.retry.backoff_multiplier).min(self.retry.max_delay_ms as f64) as u64;
                        last_error = Some(RegistryError::status(status.as_u16(), "retryable status"));
                        continue;
                    }
                    return Ok(response);
                }
                Err(error) => {
                    if attempt < self.retry.max_retries {
                        thread::sleep(Duration::from_millis(delay));
                        delay = ((delay as f64) * self.retry.backoff_multiplier).min(self.retry.max_delay_ms as f64) as u64;
                        last_error = Some(RegistryError::Http(error));
                        continue;
                    }
                    return Err(RegistryError::Http(error));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| RegistryError::message("retry exhausted")))
    }

    fn send_with_body(
        &self,
        method: Method,
        url: &str,
        body: Option<&[u8]>,
        auth: AuthHeader<'_>,
        content_type: &str,
        upload: bool,
    ) -> Result<Response, RegistryError> {
        let mut delay = self.retry.initial_delay_ms;
        let mut last_error = None;
        let timeout =
            if upload { Duration::from_secs(self.retry.upload_timeout_secs) } else { Duration::from_secs(self.retry.request_timeout_secs) };

        for attempt in 0..=self.retry.max_retries {
            let mut request = self.client.request(method.clone(), url).timeout(timeout);
            if method != Method::GET {
                request = request.header(CONTENT_TYPE, content_type);
            }
            match auth {
                AuthHeader::None => {}
                AuthHeader::Bearer(token) => {
                    request = request.header(AUTHORIZATION, format!("Bearer {token}"));
                }
                AuthHeader::Raw(value) => {
                    request = request.header(AUTHORIZATION, value);
                }
            }
            if let Some(body) = body {
                request = request.body(body.to_vec());
            }
            match request.send() {
                Ok(response) => {
                    let status = response.status();
                    let should_retry = status.as_u16() == 429 || status.is_server_error();
                    if should_retry && attempt < self.retry.max_retries {
                        thread::sleep(Duration::from_millis(delay));
                        delay = ((delay as f64) * self.retry.backoff_multiplier).min(self.retry.max_delay_ms as f64) as u64;
                        last_error = Some(RegistryError::status(status.as_u16(), "retryable status"));
                        continue;
                    }
                    return Ok(response);
                }
                Err(error) => {
                    if attempt < self.retry.max_retries {
                        thread::sleep(Duration::from_millis(delay));
                        delay = ((delay as f64) * self.retry.backoff_multiplier).min(self.retry.max_delay_ms as f64) as u64;
                        last_error = Some(RegistryError::Http(error));
                        continue;
                    }
                    return Err(RegistryError::Http(error));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| RegistryError::message("retry exhausted")))
    }
}

/// Build `{endpoint}/{path}` with no trailing slash on endpoint.
pub fn build_url(endpoint: &str, path: &str) -> String {
    let endpoint = endpoint.trim_end_matches('/');
    let path = path.trim_start_matches('/');
    format!("{endpoint}/{path}")
}

/// Minimal percent-encoding for query values.
pub fn url_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Verify SRI integrity (`sha512-…` / `sha256-…`).
pub fn verify_sri(data: &[u8], integrity: Option<&str>) -> Result<(), RegistryError> {
    let Some(integrity) = integrity.filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    let (algo, expected_b64) =
        integrity.split_once('-').ok_or_else(|| RegistryError::message(format!("invalid integrity format: {integrity}")))?;
    let digest = match algo {
        "sha256" => Sha256::digest(data).to_vec(),
        "sha512" => Sha512::digest(data).to_vec(),
        other => return Err(RegistryError::message(format!("unsupported integrity algorithm: {other}"))),
    };
    let actual = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, digest);
    if actual != expected_b64 {
        return Err(RegistryError::message(format!("integrity mismatch: expected {expected_b64}, got {actual}")));
    }
    Ok(())
}

/// Extract a gzip-compressed tarball, strip optional `package/` prefix.
pub fn extract_tarball(data: &[u8], target_directory: &Path) -> Result<(), RegistryError> {
    std::fs::create_dir_all(target_directory)?;
    let decoder = GzDecoder::new(data);
    let mut archive = Archive::new(decoder);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let mut relative = path.as_os_str().to_string_lossy().replace('\\', "/");
        if let Some(stripped) = relative.strip_prefix("package/") {
            relative = stripped.to_string();
        }
        if relative.is_empty() || relative == "." {
            continue;
        }
        let dest = target_directory.join(&relative);
        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&dest)?;
            continue;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(&dest)?;
        std::io::copy(&mut entry, &mut file)?;
        file.flush()?;
    }
    Ok(())
}

/// SHA-256 digest in `sha256-<hex>` form.
pub fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    format!("sha256-{}", hex_encode(&digest))
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Returns a short description when `HTTPS_PROXY` / `HTTP_PROXY` / `ALL_PROXY` is set.
pub fn proxy_env_hint() -> Option<String> {
    HttpClient::proxy_env_hint()
}

#[cfg(test)]
mod proxy_tests {
    use super::*;

    #[test]
    fn proxy_hint_reads_https_proxy() {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().expect("lock");
        let previous = std::env::var_os("HTTPS_PROXY");
        unsafe {
            std::env::set_var("HTTPS_PROXY", "http://127.0.0.1:7890");
        }
        let hint = proxy_env_hint().expect("hint");
        assert!(hint.contains("7890"));
        unsafe {
            match previous {
                Some(value) => std::env::set_var("HTTPS_PROXY", value),
                None => std::env::remove_var("HTTPS_PROXY"),
            }
        }
    }
}
