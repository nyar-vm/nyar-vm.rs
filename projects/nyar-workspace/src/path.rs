//! Path helpers for cache buckets and workspace discovery.

use std::path::{Path, PathBuf};

/// Sanitize a bucket name for filesystem use (`-` and spaces become `_`).
pub fn sanitize_bucket_name(name: &str) -> String {
    name.chars()
        .map(|ch| match ch {
            '-' | ' ' => '_',
            other => other,
        })
        .collect()
}

/// Walk upward from `start` looking for a file named `marker`.
///
/// Returns the directory containing the marker when found.
pub fn resolve_marker_root(start: impl AsRef<Path>, marker: &str) -> Option<PathBuf> {
    let mut current = start.as_ref().to_path_buf();
    if current.is_file() {
        current = current.parent()?.to_path_buf();
    }
    loop {
        if current.join(marker).is_file() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn sanitize_replaces_dash_and_space() {
        assert_eq!(sanitize_bucket_name("wasm32-unknown browser"), "wasm32_unknown_browser");
    }

    #[test]
    fn resolve_marker_finds_ancestor() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("legions.von"), "members: []\n").unwrap();
        let nested = root.join("packages").join("app");
        std::fs::create_dir_all(&nested).unwrap();
        assert_eq!(resolve_marker_root(&nested, "legions.von").as_deref(), Some(root));
    }
}
