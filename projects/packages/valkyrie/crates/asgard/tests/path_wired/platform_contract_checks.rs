//! Migrated from `src/platform_contract/checks.rs` (`tests`).

use super::*;

#[test]
fn terminal_dist_dir_valid() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("manifest.toml"), "platform: terminal\n").unwrap();
    std::fs::write(dir.path().join("asgard-terminal"), b"#!/bin/sh\n").unwrap();
    std::fs::write(dir.path().join("app.v"), b"namespace std.terminal;").unwrap();
    assert!(validate_terminal_dist_dir(dir.path()).is_ok());
}

#[test]
fn terminal_dist_dir_missing_manifest() {
    let dir = tempfile::tempdir().unwrap();
    assert!(validate_terminal_dist_dir(dir.path()).is_err());
}

#[test]
fn terminal_dist_dir_wrong_platform() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("manifest.toml"), "platform: android\n").unwrap();
    std::fs::write(dir.path().join("app.v"), b"namespace std.terminal;").unwrap();
    assert!(validate_terminal_dist_dir(dir.path()).is_err());
}

#[test]
fn terminal_dist_dir_missing_app_v() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("manifest.toml"), "platform: terminal\n").unwrap();
    std::fs::write(dir.path().join("asgard-terminal"), b"#!/bin/sh\n").unwrap();
    assert!(validate_terminal_dist_dir(dir.path()).is_err());
}

#[test]
fn terminal_dist_dir_forbidden_artifact() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("manifest.toml"), "platform: terminal\n").unwrap();
    std::fs::write(dir.path().join("asgard-terminal"), b"#!/bin/sh\n").unwrap();
    std::fs::write(dir.path().join("app.v"), b"namespace std.terminal;").unwrap();
    std::fs::write(dir.path().join("classes.dex"), b"forbidden").unwrap();
    assert!(validate_terminal_dist_dir(dir.path()).is_err());
}
