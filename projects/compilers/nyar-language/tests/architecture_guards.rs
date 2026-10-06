//! Architecture guards for language crate boundaries.
//!
//! Expected crate layering: `nyar-language` → `nyar-emitter` → `acorn-*` binary formats. Text CST 过渡层在 `transitional/` 内联。
//!
//! Concrete language/framework frontends (guests) may live here; shared
//! `host_script` / `HostScript*` trait layers do **not** belong in this crate.

use std::{
    fs,
    path::{Path, PathBuf},
};

/// Concrete frontend dirs that must stay free of Valkyrie MIR / driver imports.
const CONCRETE_FRONTEND_DIRS: &[&str] = &["bash", "lua", "tcl", "powershell", "c", "javascript", "python"];
const FORBIDDEN_TOKENS: &[&str] = &["valkyrie::mir", "MirFunction", "MirInstruction", "nyar_emitter", "use nyar_emitter", "MirModule", "MirLowerer"];

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
fn concrete_guest_frontends_must_not_import_vcc_data_directly() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    for dir in CONCRETE_FRONTEND_DIRS {
        let mut files = Vec::new();
        collect_rs_files(&root.join(dir), &mut files);
        for path in files {
            let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            assert!(!source.contains("vcc_data::"), "{} must not import `vcc-data`", path.display());
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
fn nyar_emitter_is_a_direct_dependency() {
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
        if in_dependencies && trimmed.starts_with("nyar-emitter") {
            return;
        }
    }
    panic!("nyar-emitter must be listed under [dependencies] (language → nyar-emitter → acorn-*)");
}

#[test]
fn nyar_emitter_crate_does_not_depend_on_language() {
    let cargo_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../nyar-emitter/Cargo.toml");
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
            panic!("nyar-emitter [dependencies] must not include nyar-language (layering: language → nyar-emitter → acorn-*)");
        }
    }
}

#[test]
fn nyar_language_must_not_import_vcc_data() {
    let src_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rs_files(&src_root, &mut files);
    for path in files {
        let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        assert!(!source.contains("vcc_data::"), "{} must not import `vcc-data`", path.display());
    }
}

#[test]
fn nyar_language_cargo_must_not_list_vcc_data() {
    let cargo_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let cargo = fs::read_to_string(&cargo_path).unwrap_or_else(|error| panic!("failed to read Cargo.toml: {error}"));
    for line in cargo.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("vcc-data") {
            panic!("nyar-language must not depend on `vcc-data` (transitional layers live under `src/transitional/`)");
        }
    }
}

const LEGACY_TEXT_PARSER_TOKENS: &[&str] = &[
    "transitional::cst",
    "AstParser::",
    "ValCstParser::",
    "VonCstParser::",
    "AwslCstParser::",
];

#[test]
fn production_code_must_not_use_legacy_text_parsers() {
    let src_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rs_files(&src_root, &mut files);
    for path in files {
        let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        for token in LEGACY_TEXT_PARSER_TOKENS {
            assert!(
                !source.contains(token),
                "{} must not use legacy text parser `{}` (use `oak-valkyrie` / `oak-von` / `oak-awsl`)",
                path.display(),
                token
            );
        }
    }
}

#[test]
fn cst_formatter_rules_must_not_spread_beyond_transitional() {
    let src_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rs_files(&src_root, &mut files);
    for path in files {
        let rel = path.strip_prefix(&src_root).unwrap().to_string_lossy().replace('\\', "/");
        if rel.starts_with("transitional/") || rel == "valkyrie/formatter/mod.rs" {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        assert!(
            !source.contains("FormatBuffer::new"),
            "{} must not add CST formatter rules. Source formatting belongs in `oak-<language>/src/formatter/` (see `oak-typescript`).",
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
    let src_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rs_files(&src_root, &mut files);
    for path in files {
        let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        assert!(!source.contains("std_data::"), "{} must not import `std_data`", path.display());
        assert!(!source.contains("use std_data"), "{} must not import `std_data`", path.display());
    }
}

#[test]
fn production_compile_pipeline_must_not_preprocess_tgrammar_text() {
    let pipeline_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/valkyrie");
    let mut files = Vec::new();
    collect_rs_files(&pipeline_root.join("compile_pipeline"), &mut files);
    collect_rs_files(&pipeline_root.join("hir/lowering"), &mut files);
    collect_rs_files(&pipeline_root.join("frontend"), &mut files);
    for path in files {
        let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        assert!(
            !source.contains("transitional::tgrammar"),
            "{} must not call transitional tgrammar text preprocessing",
            path.display()
        );
        assert!(
            !source.contains("preprocess_target_templates"),
            "{} must not expand `<% match arch %>` inside Compiler",
            path.display()
        );
        assert!(
            !source.contains("expand_tgrammar_in_root"),
            "{} must not expand tgrammar inside Oak AST lowering",
            path.display()
        );
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
