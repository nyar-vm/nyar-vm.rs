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
fn nyar_emitter_must_not_import_vcc_data() {
    let src_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rs_files(&src_root, &mut files);
    for path in files {
        let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        assert!(
            !source.contains("vcc_data::"),
            "{} must not import `vcc_data` (`nyar-emitter` binary formats route through `acorn-*`)",
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

#[test]
fn wasm_mir_lowering_must_not_emit_raw_leaf_opcodes() {
    let mir_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/lowering/backends/wasm/mir");
    let mut files = Vec::new();
    collect_rs_files(&mir_dir, &mut files);
    let forbidden = ["push(0x04)", "push(0x05)", "push(0x0B)", "push(0x88)", "push(0xAC)", "leaf_opcodes"];
    for path in files {
        let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        for pattern in forbidden {
            assert!(
                !source.contains(pattern),
                "{} must emit Wasm leaf opcodes through `acorn_wasm::WasmOpcode`, not `{pattern}`",
                path.display()
            );
        }
    }
}
