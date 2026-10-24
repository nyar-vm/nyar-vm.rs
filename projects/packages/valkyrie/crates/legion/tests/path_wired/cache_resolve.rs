//! Migrated from `src/cache/resolve.rs` (`tests`).

use super::*;

use tempfile::tempdir;

#[test]
fn cache_root_prefers_workspace() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("legions.von"), "members: []\n").unwrap();
    let member = root.join("app");
    std::fs::create_dir_all(&member).unwrap();
    assert_eq!(resolve_cache_root(&member), root);
    assert_eq!(cache_root_for(&member), root.join(".cache"));
}

#[test]
fn falls_back_to_project_dir() {
    let dir = tempdir().unwrap();
    let project = dir.path().join("solo");
    std::fs::create_dir_all(&project).unwrap();
    assert_eq!(resolve_cache_root(&project), project);
}
