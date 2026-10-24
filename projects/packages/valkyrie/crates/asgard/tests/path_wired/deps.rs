//! Migrated from `src/deps.rs` (`tests`).

use super::*;

use tempfile::tempdir;

#[test]
fn add_and_remove_dep() {
    let dir = tempdir().unwrap();
    add_dependency(dir.path(), "asgard.ui", "0.1.0").unwrap();
    let text = fs::read_to_string(dir.path().join("legion.von")).unwrap();
    assert!(text.contains("asgard.ui"));
    remove_dependency(dir.path(), "asgard.ui").unwrap();
    let text2 = fs::read_to_string(dir.path().join("legion.von")).unwrap();
    assert!(!text2.contains("asgard.ui"));
}
