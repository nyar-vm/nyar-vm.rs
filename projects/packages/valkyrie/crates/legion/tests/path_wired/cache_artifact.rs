//! Migrated from `src/cache/artifact.rs` (`tests`).

use super::*;

use tempfile::tempdir;

#[test]
fn artifact_hash_changes_when_manifest_changes() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("main.v");
    let manifest = dir.path().join("legion.von");
    fs::write(&source, "micro main() -> i64 { return 0 }\n").unwrap();
    fs::write(&manifest, "target: clr\n").unwrap();
    let first = compute_artifact_hash(&[source.clone()], &[manifest.clone()], "clr", false, false).unwrap();
    fs::write(&manifest, "target: wasm\n").unwrap();
    let second = compute_artifact_hash(&[source], &[manifest], "clr", false, false).unwrap();
    assert_ne!(first, second);
}

#[test]
fn cache_ignores_unreported_output_files() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("old.mjs"), "stale").unwrap();
    let bundle = collect_build_bundle(dir.path(), &DriverCompileReport::default()).unwrap();
    assert!(bundle.files.is_empty());
}
