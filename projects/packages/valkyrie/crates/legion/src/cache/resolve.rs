//! Cache root resolution conventions (`legions.von` → workspace `.cache`).

use std::path::{Path, PathBuf};

use nyar_workspace::resolve_marker_root;

/// Walk upward looking for `legions.von`.
pub fn resolve_workspace_root(project_dir: impl AsRef<Path>) -> Option<PathBuf> {
    resolve_marker_root(project_dir, "legions.von")
}

/// Project or workspace directory that owns the shared `.cache` folder.
pub fn resolve_cache_root(project_dir: impl AsRef<Path>) -> PathBuf {
    resolve_workspace_root(project_dir.as_ref()).unwrap_or_else(|| project_dir.as_ref().to_path_buf())
}

/// `{resolve_cache_root}/.cache`.
pub fn cache_root_for(project_dir: impl AsRef<Path>) -> PathBuf {
    resolve_cache_root(project_dir).join(".cache")
}
#[cfg(test)]
#[path = "../../tests/path_wired/cache_resolve.rs"]
mod cache_resolve_tests;
