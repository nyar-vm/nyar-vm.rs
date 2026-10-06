//! Architecture guards: JIT crate must stay language-agnostic.

use std::{
    fs,
    path::{Path, PathBuf},
};

const FORBIDDEN_TOKENS: &[&str] = &["nyar_language::", "use nyar_language", "vcc_data::text::", "ValkyrieCompiler", "LoadedModule"];

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
fn nyar_jit_src_must_not_import_concrete_languages() {
    let mut files = Vec::new();
    collect_rs_files(&crate_src(), &mut files);
    assert!(!files.is_empty(), "nyar-jit src must exist");
    for file in files {
        let source = fs::read_to_string(&file).unwrap_or_else(|error| panic!("failed to read {}: {error}", file.display()));
        for token in FORBIDDEN_TOKENS {
            assert!(!source.contains(token), "{} must not contain '{token}'", file.display());
        }
    }
}

#[test]
fn nyar_jit_must_not_depend_on_nyar_language_or_nyar_vm() {
    let manifest = fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).expect("read nyar-jit Cargo.toml");
    let has_dep = manifest.lines().any(|line| {
        let trimmed = line.trim_start();
        !trimmed.starts_with('#')
            && (trimmed.starts_with("nyar-language")
                || trimmed.starts_with("nyar-vm")
                || trimmed.contains("nyar_language")
                || trimmed.contains("nyar_vm"))
    });
    assert!(!has_dep, "nyar-jit Cargo.toml must not depend on nyar-language or nyar-vm");
}
