//! Architecture guards for `nyar-emitter` crate boundaries.

use std::{fs, path::PathBuf};

fn collect_rs_files(dir: &PathBuf, out: &mut Vec<PathBuf>) {
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

#[test]
fn vcc_data_imports_are_confined_to_transitional_modules() {
    let src_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rs_files(&src_root, &mut files);
    for path in files {
        let rel = path.strip_prefix(&src_root).unwrap().to_string_lossy();
        if rel.starts_with("transitional") {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        assert!(
            !source.contains("vcc_data::"),
            "{} must not import `vcc_data` directly (use `crate::transitional`)",
            path.display()
        );
    }
}

#[test]
fn workspace_must_not_alias_vcc_data_as_std_data() {
    let cargo_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../Cargo.toml");
    let cargo = fs::read_to_string(&cargo_path).unwrap_or_else(|error| panic!("failed to read {}: {error}", cargo_path.display()));
    for line in cargo.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("std-data") {
            panic!("workspace must not alias transitional `vcc-data` as `std-data` (ADR-0018)");
        }
    }
}
