//! Architecture guards: bytecode VM must stay language-agnostic.
//!
//! Concrete language product logic belongs in `nyar-language`. This crate only
//! loads/executes Nyar IR (`.nyar`). Docs may discuss boundaries; imports must not.

use std::{
    fs,
    path::{Path, PathBuf},
};

/// Import / API tokens that must not appear in `src/`.
const FORBIDDEN_TOKENS: &[&str] = &[
    "nyar_language::",
    "nyar_language::{",
    "use nyar_language",
    "std_data::text::",
    "ValkyrieCompiler",
    "HostScriptModule",
    "HostScriptBridge",
    "mod host_script",
];

fn crate_src() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
}

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
fn nvm_src_must_not_import_concrete_languages() {
    let mut files = Vec::new();
    collect_rs_files(&crate_src(), &mut files);
    assert!(!files.is_empty(), "nvm src must exist");
    for file in files {
        let source = fs::read_to_string(&file).unwrap_or_else(|error| panic!("failed to read {}: {error}", file.display()));
        for token in FORBIDDEN_TOKENS {
            assert!(
                !source.contains(token),
                "{} must not contain '{token}' — language concepts stay in nyar-language / guests",
                file.display()
            );
        }
    }
}

#[test]
fn nvm_must_not_depend_on_nyar_language() {
    let manifest = fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).expect("read nvm Cargo.toml");
    let has_dep = manifest.lines().any(|line| {
        let trimmed = line.trim_start();
        !trimmed.starts_with('#') && (trimmed.starts_with("nyar-language") || trimmed.contains("nyar_language"))
    });
    assert!(!has_dep, "nvm Cargo.toml must not depend on nyar-language");
}
