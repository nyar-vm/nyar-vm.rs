//! Canonical Valhalla publisher keys with bidirectional format compatibility.
//!
//! Supported write/read forms (all normalize to the same identity):
//! - fingerprint: `ed25519:<64-hex public key>`
//! - signing seed: `ed25519-seed:<64-hex private seed>`
//! - bare 64-hex / base64 private seed
//! - path to a JSON keypair or bare seed file
//!
//! Fingerprints always render as `ed25519:<lowercase_hex>` (C# / Valhalla server form).

use std::path::Path;

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::RegistryError;

const FINGERPRINT_PREFIX: &str = "ed25519:";
const SEED_PREFIX: &str = "ed25519-seed:";

/// Ed25519 publisher identity and optional signing material.
#[derive(Debug, Clone)]
pub struct PublisherKey {
    fingerprint: String,
    public_key: [u8; 32],
    seed: Option<[u8; 32]>,
}

impl PublisherKey {
    /// Parse any supported key encoding (path, seed, fingerprint, JSON).
    pub fn parse(input: &str) -> Result<Self, RegistryError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(RegistryError::message("publisher key is empty"));
        }

        if Path::new(trimmed).is_file() {
            let content = std::fs::read_to_string(trimmed)?;
            return Self::parse_content(content.trim());
        }

        Self::parse_content(trimmed)
    }

    fn parse_content(content: &str) -> Result<Self, RegistryError> {
        if let Some(hex) = content.strip_prefix(SEED_PREFIX) {
            let seed = decode_32_hex(hex)?;
            return Self::from_seed(seed);
        }

        if let Some(hex) = content.strip_prefix(FINGERPRINT_PREFIX) {
            let public_key = decode_32_hex(hex)?;
            return Self::from_public(public_key);
        }

        if content.starts_with('{') {
            return Self::parse_json(content);
        }

        if let Ok(seed) = decode_32_hex(content) {
            return Self::from_seed(seed);
        }

        if let Ok(bytes) = BASE64.decode(content.trim()) {
            if bytes.len() == 32 {
                let mut seed = [0u8; 32];
                seed.copy_from_slice(&bytes);
                return Self::from_seed(seed);
            }
            if bytes.len() == 64 {
                // seed || public
                let mut seed = [0u8; 32];
                seed.copy_from_slice(&bytes[..32]);
                let mut public = [0u8; 32];
                public.copy_from_slice(&bytes[32..]);
                let key = Self::from_seed(seed)?;
                if key.public_key != public {
                    return Err(RegistryError::message("publisher key public half does not match private seed"));
                }
                return Ok(key);
            }
        }

        Err(RegistryError::message(format!(
            "unrecognized publisher key form (expected {SEED_PREFIX}<hex>, {FINGERPRINT_PREFIX}<hex>, bare seed, or key file)"
        )))
    }

    fn parse_json(content: &str) -> Result<Self, RegistryError> {
        let value: KeyFile = serde_json::from_str(content)?;
        if let Some(seed) = value.private_key.as_ref().and_then(decode_key_bytes) {
            let key = Self::from_seed(seed)?;
            if let Some(fingerprint) = value.public_key_fingerprint.as_deref() {
                let expected = normalize_fingerprint(fingerprint)?;
                if expected != key.fingerprint {
                    return Err(RegistryError::message("publisher key fingerprint does not match private seed"));
                }
            }
            return Ok(key);
        }
        if let Some(public) = value.public_key.as_ref().and_then(decode_key_bytes) {
            return Self::from_public(public);
        }
        if let Some(fingerprint) = value.public_key_fingerprint.as_deref() {
            let public = decode_32_hex(fingerprint.strip_prefix(FINGERPRINT_PREFIX).unwrap_or(fingerprint))?;
            return Self::from_public(public);
        }
        Err(RegistryError::message("publisher key JSON is missing private_key / public_key"))
    }

    pub fn from_seed(seed: [u8; 32]) -> Result<Self, RegistryError> {
        let signing = SigningKey::from_bytes(&seed);
        let public_key = signing.verifying_key().to_bytes();
        Ok(Self { fingerprint: fingerprint_of(&public_key), public_key, seed: Some(seed) })
    }

    pub fn from_public(public_key: [u8; 32]) -> Result<Self, RegistryError> {
        // Validate curve point.
        VerifyingKey::from_bytes(&public_key).map_err(|error| RegistryError::message(error.to_string()))?;
        Ok(Self { fingerprint: fingerprint_of(&public_key), public_key, seed: None })
    }

    /// Canonical public identity: `ed25519:<hex>`.
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    pub fn public_key(&self) -> &[u8; 32] {
        &self.public_key
    }

    pub fn can_sign(&self) -> bool {
        self.seed.is_some()
    }

    /// Canonical signed-storage form. Prefer seed material; fall back to fingerprint-only identity.
    pub fn to_canonical(&self) -> String {
        if let Some(seed) = self.seed { format!("{SEED_PREFIX}{}", hex_encode(&seed)) } else { self.fingerprint.clone() }
    }

    /// Alias of [`to_canonical`] for auth.von compatibility (token slot).
    pub fn to_auth_material(&self) -> String {
        self.to_canonical()
    }

    /// Sign a request body payload (Valhalla signs SHA-256(body)).
    pub fn sign_body(&self, body: &[u8]) -> Result<Vec<u8>, RegistryError> {
        let seed = self.seed.ok_or_else(|| RegistryError::message("publisher key has no private seed; cannot sign publish request"))?;
        let signing = SigningKey::from_bytes(&seed);
        let hash = Sha256::digest(body);
        Ok(signing.sign(&hash).to_bytes().to_vec())
    }

    /// Authorization header value: `ed25519-signature(fingerprint,base64sig)`.
    pub fn authorization_header(&self, body: &[u8]) -> Result<String, RegistryError> {
        let signature = self.sign_body(body)?;
        Ok(format!("ed25519-signature({},{})", self.fingerprint, BASE64.encode(signature)))
    }
}

/// Normalize fingerprints: accept bare hex or prefixed form, always emit `ed25519:<hex>`.
pub fn normalize_fingerprint(input: &str) -> Result<String, RegistryError> {
    let hex = input.trim().strip_prefix(FINGERPRINT_PREFIX).unwrap_or(input.trim());
    let public = decode_32_hex(hex)?;
    Ok(fingerprint_of(&public))
}

/// Canonical Valhalla package name: lowercase, separators → `.`.
pub fn canonicalize_package_name(raw: &str) -> Result<String, RegistryError> {
    let lower = raw.trim().to_ascii_lowercase();
    let mut replaced = String::with_capacity(lower.len());
    let mut prev_dot = false;
    for ch in lower.chars() {
        let mapped = match ch {
            '_' | '-' | ' ' | '\t' => '.',
            other => other,
        };
        if mapped == '.' {
            if prev_dot || replaced.is_empty() {
                continue;
            }
            replaced.push('.');
            prev_dot = true;
        }
        else if mapped.is_ascii_alphanumeric() {
            replaced.push(mapped);
            prev_dot = false;
        }
        else {
            return Err(RegistryError::message(format!("package name '{raw}' contains illegal character '{ch}'")));
        }
    }
    let canonical = replaced.trim_matches('.').to_string();
    if canonical.is_empty() {
        return Err(RegistryError::message(format!("package name '{raw}' normalizes to empty")));
    }
    if !canonical.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '.')
        || canonical.split('.').any(|part| part.is_empty())
    {
        return Err(RegistryError::message(format!("package name '{raw}' normalizes to invalid form '{canonical}'")));
    }
    Ok(canonical)
}

fn fingerprint_of(public_key: &[u8; 32]) -> String {
    format!("{FINGERPRINT_PREFIX}{}", hex_encode(public_key))
}

fn decode_32_hex(hex: &str) -> Result<[u8; 32], RegistryError> {
    let hex = hex.trim();
    if hex.len() != 64 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(RegistryError::message("publisher key hex must be 64 hex characters"));
    }
    let mut out = [0u8; 32];
    for (index, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let text = std::str::from_utf8(chunk).map_err(|error| RegistryError::message(error.to_string()))?;
        out[index] = u8::from_str_radix(text, 16).map_err(|error| RegistryError::message(error.to_string()))?;
    }
    Ok(out)
}

fn decode_key_bytes(value: &KeyBytes) -> Option<[u8; 32]> {
    match value {
        KeyBytes::Array(bytes) if bytes.len() == 32 => {
            let mut out = [0u8; 32];
            out.copy_from_slice(bytes);
            Some(out)
        }
        KeyBytes::Text(text) => {
            if let Ok(seed) = decode_32_hex(text) {
                return Some(seed);
            }
            if let Ok(bytes) = BASE64.decode(text.trim()) {
                if bytes.len() == 32 {
                    let mut out = [0u8; 32];
                    out.copy_from_slice(&bytes);
                    return Some(out);
                }
            }
            None
        }
        _ => None,
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Debug, Deserialize)]
struct KeyFile {
    #[serde(default)]
    private_key: Option<KeyBytes>,
    #[serde(default)]
    public_key: Option<KeyBytes>,
    #[serde(default)]
    public_key_fingerprint: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum KeyBytes {
    Array(Vec<u8>),
    Text(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_and_fingerprint_are_bidirectional() {
        let seed = [7u8; 32];
        let key = PublisherKey::from_seed(seed).expect("seed");
        let fingerprint = key.fingerprint().to_string();
        assert!(fingerprint.starts_with("ed25519:"));
        assert_eq!(fingerprint.len(), "ed25519:".len() + 64);

        let auth = key.to_auth_material();
        assert!(auth.starts_with("ed25519-seed:"));
        let reloaded = PublisherKey::parse(&auth).expect("reload seed");
        assert_eq!(reloaded.fingerprint(), key.fingerprint());
        assert!(reloaded.can_sign());

        let public_only = PublisherKey::parse(&fingerprint).expect("reload fingerprint");
        assert_eq!(public_only.fingerprint(), key.fingerprint());
        assert!(!public_only.can_sign());
    }

    #[test]
    fn bare_hex_seed_normalizes_to_canonical_seed_form() {
        let seed_hex = "11".repeat(32);
        let key = PublisherKey::parse(&seed_hex).expect("bare hex");
        assert_eq!(key.to_canonical(), format!("ed25519-seed:{seed_hex}"));
        assert!(key.authorization_header(b"body").is_ok());
    }

    #[test]
    fn package_name_canonicalization_is_bidirectional_enough() {
        assert_eq!(canonicalize_package_name("Org_Pkg-Name").unwrap(), "org.pkg.name");
        assert_eq!(canonicalize_package_name("org.pkg.name").unwrap(), "org.pkg.name");
    }
}
