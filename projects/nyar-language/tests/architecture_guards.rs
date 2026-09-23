//! Architecture guards for language crate boundaries.
//!
//! Expected crate layering: `nyar-language` → `emitter` → `std-data`.
//!
//! Concrete language/framework frontends (guests) may live here; shared
//! `host_script` / `HostScript*` trait layers do **not** belong in this crate.
//! Runtime PE substrate lives in `legacy-vm` — see `../host-script-languages.md`.

use std::{
    fs,
    path::{Path, PathBuf},
};

/// Concrete frontend dirs that must stay free of Valkyrie MIR / driver imports.
const CONCRETE_FRONTEND_DIRS: &[&str] = &["bash", "lua", "tcl", "powershell", "c", "javascript", "python"];
const FORBIDDEN_TOKENS: &[&str] = &["valkyrie::mir", "MirFunction", "MirInstruction", "nyar_emitter", "use emitter", "MirModule", "MirLowerer"];

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

#[test]
fn concrete_frontends_do_not_reference_valkyrie_ir() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    for dir in CONCRETE_FRONTEND_DIRS {
        let mut files = Vec::new();
        collect_rs_files(&root.join(dir), &mut files);
        for path in files {
            let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            for token in FORBIDDEN_TOKENS {
                assert!(!source.contains(token), "{} must not contain '{token}'", path.display());
            }
        }
    }
}

#[test]
fn concrete_frontends_do_not_import_valkyrie() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    for dir in CONCRETE_FRONTEND_DIRS {
        let mut files = Vec::new();
        collect_rs_files(&root.join(dir), &mut files);
        for path in files {
            let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            for line in source.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("use crate::valkyrie") || trimmed.starts_with("use super::valkyrie") {
                    panic!("{} must not import valkyrie: {trimmed}", path.display());
                }
            }
        }
    }
}

#[test]
fn emitter_is_a_direct_dependency() {
    let cargo_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let cargo = fs::read_to_string(&cargo_path).unwrap_or_else(|error| panic!("failed to read Cargo.toml: {error}"));
    let mut in_dependencies = false;
    for line in cargo.lines() {
        let trimmed = line.trim();
        if trimmed == "[dependencies]" {
            in_dependencies = true;
            continue;
        }
        if trimmed.starts_with('[') {
            in_dependencies = false;
            continue;
        }
        if in_dependencies && trimmed.starts_with("emitter") {
            return;
        }
    }
    panic!("emitter must be listed under [dependencies] (language → driver → std-data)");
}

#[test]
fn emitter_crate_does_not_depend_on_language() {
    // Package lives in projects/emitter; crate name is emitter.
    let cargo_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../emitter/Cargo.toml");
    let cargo = fs::read_to_string(&cargo_path).unwrap_or_else(|error| panic!("failed to read {}: {error}", cargo_path.display()));
    let mut in_dependencies = false;
    for line in cargo.lines() {
        let trimmed = line.trim();
        if trimmed == "[dependencies]" {
            in_dependencies = true;
            continue;
        }
        if trimmed.starts_with('[') {
            in_dependencies = false;
            continue;
        }
        if in_dependencies && trimmed.starts_with("nyar-language") {
            panic!("emitter [dependencies] must not include nyar-language (layering: language → driver → std-data)");
        }
    }
}

#[test]
fn host_script_shared_module_must_be_absent() {
    let host_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/host_script");
    assert!(!host_dir.exists(), "shared host_script abstraction must not live in nyar-language; use concrete src/<lang>/ modules");
}

#[test]
fn concrete_frontends_must_not_reintroduce_host_script_traits() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let forbidden = ["HostScriptModule", "HostScriptBridge", "mod host_script", "crate::host_script"];
    for dir in CONCRETE_FRONTEND_DIRS {
        let mut files = Vec::new();
        collect_rs_files(&root.join(dir), &mut files);
        for path in files {
            let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            for token in forbidden {
                assert!(
                    !source.contains(token),
                    "{} must not contain '{token}'; guests use inherent APIs (language_id / source_path / exported_symbols)",
                    path.display()
                );
            }
        }
    }
}
