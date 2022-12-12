//! On-disk project layout names for package / workspace discovery.
//!
//! Product CLIs inject their own filenames and home paths via [`ProjectLayout`].
//! This crate stays product-identity free: no hard-coded product manifest names,
//! lockfiles, or home directories.

use std::path::{Path, PathBuf};

/// Resolves alternate physical entry names when packing / locating run-contract artifacts.
///
/// Products may inject bootstrap aliases (e.g. historical mangled names → canonical).
/// The neutral layout returns no aliases.
pub type EntryAliasFn = fn(&str) -> &'static [&'static str];

/// Filenames and user-data locations used when discovering and persisting
/// package-manager projects.
#[derive(Debug, Clone, Copy)]
pub struct ProjectLayout {
    /// Per-package manifest file name (e.g. `package.von`).
    pub package_manifest: &'static str,
    /// Workspace manifest file name (e.g. `workspace.von`).
    pub workspace_manifest: &'static str,
    /// Optional ignore file for packing (e.g. `.packageignore`).
    pub ignore_file: &'static str,
    /// Lockfile name under the project root (e.g. `package-lock.von`).
    pub lockfile: &'static str,
    /// Product data directory under the user home (e.g. `.nyar`).
    pub home_dirname: &'static str,
    /// Env var that overrides the user home root used with [`Self::home_dirname`].
    pub home_env: &'static str,
    /// Product-specific token env vars consulted after `{REGISTRY}_TOKEN`.
    pub token_env_vars: &'static [&'static str],
    /// Product-supplied physical-entry aliases tried before the contract name itself.
    pub entry_aliases: EntryAliasFn,
}

fn no_entry_aliases(_: &str) -> &'static [&'static str] {
    &[]
}

impl ProjectLayout {
    /// Neutral default layout with no product branding.
    pub const fn neutral() -> Self {
        Self {
            package_manifest: "package.von",
            workspace_manifest: "workspace.von",
            ignore_file: ".packageignore",
            lockfile: "package-lock.von",
            home_dirname: ".nyar",
            home_env: "NYAR_HOME",
            token_env_vars: &[],
            entry_aliases: no_entry_aliases,
        }
    }

    /// Path to the package manifest under `directory`.
    pub fn package_manifest_path(self, directory: impl AsRef<Path>) -> PathBuf {
        directory.as_ref().join(self.package_manifest)
    }

    /// Path to the workspace manifest under `directory`.
    pub fn workspace_manifest_path(self, directory: impl AsRef<Path>) -> PathBuf {
        directory.as_ref().join(self.workspace_manifest)
    }

    /// Path to the pack ignore file under `directory`.
    pub fn ignore_file_path(self, directory: impl AsRef<Path>) -> PathBuf {
        directory.as_ref().join(self.ignore_file)
    }

    /// Path to the lockfile under `directory`.
    pub fn lockfile_path(self, directory: impl AsRef<Path>) -> PathBuf {
        directory.as_ref().join(self.lockfile)
    }

    /// Resolve the user home root (`home_env`, then `HOME` / `USERPROFILE`).
    pub fn resolve_user_home(self) -> PathBuf {
        std::env::var_os(self.home_env)
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Product data directory (`$home/{home_dirname}`).
    pub fn product_data_dir(self) -> PathBuf {
        self.resolve_user_home().join(self.home_dirname)
    }

    /// Content-addressable package / security cache root.
    pub fn cache_dir(self) -> PathBuf {
        self.product_data_dir().join("cache")
    }

    /// Vendor auth store path (`auth.von` under the product data dir).
    pub fn auth_store_path(self) -> PathBuf {
        self.product_data_dir().join("auth.von")
    }

    /// Registry endpoint config path.
    pub fn registry_sources_path(self) -> PathBuf {
        self.product_data_dir().join("registry-sources.von")
    }
}

impl Default for ProjectLayout {
    fn default() -> Self {
        Self::neutral()
    }
}

#[cfg(test)]
mod architecture_guards {
    use super::*;
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    /// Build product-brand needles without embedding the forbidden quoted literals in this file.
    fn forbidden_product_literals() -> Vec<String> {
        let mut out = Vec::new();
        for brand in ["legion", "noodle", "panda"] {
            out.push(format!("{brand}.von"));
            out.push(format!("{brand}-lock.von"));
            out.push(format!("{brand}.ignore"));
            out.push(format!(".{brand}"));
            out.push(format!("{}_HOME", brand.to_ascii_uppercase()));
            out.push(format!("{}_TOKEN", brand.to_ascii_uppercase()));
        }
        // Legion historically used ~/.valkyrie — product default, not PM.
        let home = "valkyrie";
        out.push(format!(".{home}"));
        out.push(format!("{}_HOME", home.to_ascii_uppercase()));
        out
    }

    /// Ecosystem package-manager CLIs must not be spawned from the neutral core.
    /// (`cmd` / `sh` in ScriptRunner and `git` in publish helpers are allowed.)
    const FORBIDDEN_SHELL_TOOLS: &[&str] = &[
        "npm", "pnpm", "yarn", "bun", "npx", "vite", "biome", "eslint", "prettier", "tsc", "webpack", "esbuild", "uv", "pip", "pip3", "poetry",
        "conda", "mamba", "ruff", "black", "flake8", "mypy", "pytest", "hatch", "pdm", "pipenv",
    ];

    fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
        if !dir.is_dir() {
            return;
        }
        for entry in fs::read_dir(dir).unwrap_or_else(|error| panic!("failed to read {}: {error}", dir.display())) {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                collect_rs_files(&path, out);
            }
            else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                out.push(path);
            }
        }
    }

    fn strip_rust_comments(source: &str) -> String {
        let mut out = String::with_capacity(source.len());
        let bytes = source.as_bytes();
        let mut i = 0;
        let mut in_block = false;
        while i < bytes.len() {
            if in_block {
                if bytes[i] == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    in_block = false;
                    i += 2;
                    continue;
                }
                out.push(' ');
                i += 1;
                continue;
            }
            if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                while i < bytes.len() && bytes[i] != b'\n' {
                    out.push(' ');
                    i += 1;
                }
                continue;
            }
            if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                in_block = true;
                out.push(' ');
                out.push(' ');
                i += 2;
                continue;
            }
            out.push(bytes[i] as char);
            i += 1;
        }
        out
    }

    /// Drop this module's body from `layout.rs` so the guard does not scan itself.
    fn without_architecture_guards_mod(source: &str) -> String {
        const MARKER: &str = "mod architecture_guards";
        if let Some(idx) = source.find(MARKER) { source[..idx].to_string() } else { source.to_string() }
    }

    fn scan_source(path: &Path) -> String {
        let raw = fs::read_to_string(path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let stripped = strip_rust_comments(&raw);
        if path.file_name().and_then(|n| n.to_str()) == Some("layout.rs") { without_architecture_guards_mod(&stripped) } else { stripped }
    }

    #[test]
    fn neutral_layout_has_no_product_brand() {
        let layout = ProjectLayout::neutral();
        assert_eq!(layout.package_manifest, "package.von");
        assert_eq!(layout.lockfile, "package-lock.von");
        assert_eq!(layout.home_dirname, ".nyar");
        assert_eq!(layout.home_env, "NYAR_HOME");
        for brand in ["legion", "noodle", "panda", "valkyrie"] {
            assert!(!layout.package_manifest.contains(brand));
            assert!(!layout.lockfile.contains(brand));
            assert!(!layout.home_dirname.contains(brand));
            assert!(!layout.home_env.to_ascii_lowercase().contains(brand));
        }
    }

    #[test]
    fn package_manager_src_has_no_product_brand_literals() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        collect_rs_files(&root, &mut files);
        assert!(!files.is_empty());
        let needles = forbidden_product_literals();
        for path in files {
            let source = scan_source(&path);
            for token in &needles {
                let quoted = format!("\"{token}\"");
                assert!(
                    !source.contains(&quoted),
                    "{} must not hard-code product identity {quoted}; inject via ProjectLayout in the product crate",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn package_manager_src_does_not_shell_foreign_toolchains() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        collect_rs_files(&root, &mut files);
        for path in files {
            let source = scan_source(&path);
            for tool in FORBIDDEN_SHELL_TOOLS {
                for quote in ['"', '\''] {
                    let needle = format!("Command::new({quote}{tool}{quote})");
                    assert!(
                        !source.contains(&needle),
                        "{} must not spawn foreign toolchain via {needle} (ScriptRunner may use cmd/sh; products own ecosystem CLIs)",
                        path.display()
                    );
                }
            }
        }
    }
}
