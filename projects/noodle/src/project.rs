//! Project discovery for Node packages (`package.json` parameters drive PM-compat behavior).

use std::{
    fs,
    path::{Path, PathBuf},
};

use miette::{IntoDiagnostic, Result, miette};
use serde_json::Value;

/// Which Node PM *behaviors* noodle should emulate for this package.
///
/// Chosen from `package.json` parameters — never from lockfile presence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmCompat {
    /// npm-compatible CLI / lock / save semantics.
    Npm,
    /// pnpm-compatible semantics.
    Pnpm,
    /// Yarn Classic / Berry-compatible semantics.
    Yarn,
    /// Bun-compatible semantics.
    Bun,
}

impl PmCompat {
    /// Label used in logs / docs.
    pub fn label(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn => "yarn",
            Self::Bun => "bun",
        }
    }

    /// Parse a Corepack-style `packageManager` value (`"pnpm@9.0.0"`, `"yarn@4.0.0+sha…"`, …).
    pub fn from_package_manager_field(value: &str) -> Option<Self> {
        let name = value.split('@').next().unwrap_or(value).trim().to_ascii_lowercase();
        match name.as_str() {
            "npm" => Some(Self::Npm),
            "pnpm" => Some(Self::Pnpm),
            "yarn" => Some(Self::Yarn),
            "bun" => Some(Self::Bun),
            _ => None,
        }
    }

    /// Parse an explicit compat id (`"npm"` / `"pnpm"` / …).
    pub fn from_compat_id(value: &str) -> Option<Self> {
        Self::from_package_manager_field(value)
    }
}

/// A noodle-managed Node project root.
#[derive(Debug, Clone)]
pub struct NoodleProject {
    /// Absolute project directory.
    pub root: PathBuf,
    /// Parsed `package.json`.
    pub package_json: Value,
    /// PM-compat profile derived from package parameters.
    pub compat: PmCompat,
}

impl NoodleProject {
    /// Discover from `dir` upward until `package.json` is found.
    pub fn discover(dir: impl AsRef<Path>) -> Result<Self> {
        let start = fs::canonicalize(dir.as_ref()).into_diagnostic().unwrap_or_else(|_| dir.as_ref().to_path_buf());
        let mut cursor = start.clone();
        loop {
            let manifest = cursor.join("package.json");
            if manifest.is_file() {
                return Self::open(&cursor);
            }
            if !cursor.pop() {
                break;
            }
        }
        Err(miette!("未找到 package.json（从 {} 向上搜索）", start.display()))
    }

    /// Open a directory that already contains `package.json`.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = fs::canonicalize(root.as_ref()).into_diagnostic().unwrap_or_else(|_| root.as_ref().to_path_buf());
        let manifest_path = root.join("package.json");
        let text =
            fs::read_to_string(&manifest_path).into_diagnostic().map_err(|e| e.wrap_err(format!("读取 {} 失败", manifest_path.display())))?;
        let package_json: Value = serde_json::from_str(&text).into_diagnostic().map_err(|e| e.wrap_err("解析 package.json 失败"))?;
        let compat = resolve_compat(&package_json);
        Ok(Self { root, package_json, compat })
    }

    /// Package name from manifest, if any.
    pub fn name(&self) -> Option<&str> {
        self.package_json.get("name").and_then(|v| v.as_str())
    }
}

/// Resolve compat profile from `package.json` only.
///
/// Priority:
/// 1. `noodle.compat` — explicit noodle override (`"pnpm"` / `"yarn"` / …)
/// 2. `noodle.packageManager` — same shapes as Corepack field
/// 3. `packageManager` — Corepack field (`"pnpm@9.15.0"`)
/// 4. default → npm-compatible behavior
fn resolve_compat(package_json: &Value) -> PmCompat {
    if let Some(noodle) = package_json.get("noodle") {
        if let Some(compat) = noodle.get("compat").and_then(|v| v.as_str()).and_then(PmCompat::from_compat_id) {
            return compat;
        }
        if let Some(pm) = noodle.get("packageManager").and_then(|v| v.as_str()).and_then(PmCompat::from_package_manager_field) {
            return pm;
        }
    }

    if let Some(pm) = package_json.get("packageManager").and_then(|v| v.as_str()).and_then(PmCompat::from_package_manager_field) {
        return pm;
    }

    PmCompat::Npm
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn compat_from_package_manager_field() {
        assert_eq!(resolve_compat(&json!({"packageManager": "pnpm@9.15.0"})), PmCompat::Pnpm);
        assert_eq!(resolve_compat(&json!({"packageManager": "yarn@4.0.0+sha224.abc"})), PmCompat::Yarn);
        assert_eq!(resolve_compat(&json!({"packageManager": "bun@1.1.0"})), PmCompat::Bun);
        assert_eq!(resolve_compat(&json!({"packageManager": "npm@10.9.0"})), PmCompat::Npm);
    }

    #[test]
    fn noodle_compat_overrides_package_manager() {
        let pkg = json!({
            "packageManager": "npm@10.0.0",
            "noodle": { "compat": "pnpm" }
        });
        assert_eq!(resolve_compat(&pkg), PmCompat::Pnpm);
    }

    #[test]
    fn default_is_npm_without_lockfile_sniffing() {
        // No packageManager / noodle fields → npm, regardless of what files might exist on disk.
        assert_eq!(resolve_compat(&json!({"name": "demo"})), PmCompat::Npm);
    }
}
