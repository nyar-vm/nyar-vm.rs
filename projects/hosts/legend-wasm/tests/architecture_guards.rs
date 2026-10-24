//! Architecture guards for the PE / Futamura substrate boundary.
//!
//! `legacy-vm` core must stay language-agnostic. Concrete language product
//! logic belongs in `nyar-language` guests. Evaluator modules must be thin
//! adapters that delegate to `nyar_language` (bash exemplar).

use std::{
    fs,
    path::{Path, PathBuf},
};

/// Substrate modules that must not import concrete language ASTs.
const CORE_RELATIVE_PATHS: &[&str] = &["algebra", "compiler", "value.rs", "lib.rs", "guest.rs"];

/// Tokens that must not appear in substrate core.
const FORBIDDEN_IN_CORE: &[&str] = &[
    "std_data::text::lua",
    "std_data::text::powershell",
    "std_data::text::bash",
    "std_data::text::tcl",
    "std_data::text::c",
    "std_data::text::valkyrie",
    "valkyrie::",
    "ValkyrieCompiler",
    "HostScript",
    "host_script",
];

/// Guest evaluator adapters that must stay thin (delegate to `nyar_language`).
const THIN_GUEST_ADAPTERS: &[(&str, &str)] = &[
    ("bash.rs", "evaluate_bash_source"),
    ("lua.rs", "evaluate_lua_source"),
    ("powershell.rs", "evaluate_powershell_source"),
    ("c.rs", "evaluate_c_source"),
    ("tcl.rs", "evaluate_tcl_source"),
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

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

#[test]
fn substrate_core_stays_language_agnostic() {
    let src = crate_src();
    for relative in CORE_RELATIVE_PATHS {
        let path = src.join(relative);
        let mut files = Vec::new();
        if path.is_dir() {
            collect_rs_files(&path, &mut files);
        }
        else if path.is_file() {
            files.push(path);
        }
        else {
            panic!("missing substrate path: {relative}");
        }
        for file in files {
            let source = read(&file);
            for token in FORBIDDEN_IN_CORE {
                assert!(!source.contains(token), "{} is substrate core and must not contain '{token}'", file.display());
            }
        }
    }
}

#[test]
fn crate_src_must_not_reintroduce_host_script_abstraction() {
    let mut files = Vec::new();
    collect_rs_files(&crate_src(), &mut files);
    for file in files {
        let source = read(&file);
        assert!(
            !source.contains("HostScriptModule") && !source.contains("HostScriptBridge") && !source.contains("mod host_script"),
            "{} must not reintroduce HostScript* / host_script abstraction",
            file.display()
        );
    }
}

#[test]
fn evaluators_must_not_own_full_in_vm_interpret() {
    let evaluator_dir = crate_src().join("evaluator");
    let mut files = Vec::new();
    collect_rs_files(&evaluator_dir, &mut files);

    for file in files {
        let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == "mod.rs" || name == "shell.rs" {
            continue;
        }
        let source = read(&file);
        let parses_language_ast = source.contains("std_data::text::");
        let delegates_to_language = source.contains("nyar_language::") || source.contains("nyar_language::{");
        assert!(
            !(parses_language_ast && !delegates_to_language),
            "{} parses a concrete language AST inside legacy-vm without delegating to nyar-language. \
             Guests must implement interpret in nyar-language and keep only a thin adapter here.",
            file.display()
        );
    }
}

#[test]
fn guest_adapters_remain_thin_language_delegates() {
    for (file_name, source_fn) in THIN_GUEST_ADAPTERS {
        let source = read(&crate_src().join("evaluator").join(file_name));
        assert!(
            source.contains("nyar_language") && source.contains(source_fn),
            "{file_name} must stay a thin adapter over nyar-language::{source_fn}"
        );
        assert!(
            !source.contains("std_data::text::"),
            "{file_name} must not re-own language AST walking; parse/interpret live in language package"
        );
    }
}

#[test]
fn guest_seam_module_stays_language_neutral() {
    let source = read(&crate_src().join("guest.rs"));
    assert!(source.contains("GuestInterpretFn") && source.contains("GuestInterpret"), "guest.rs must expose GuestInterpretFn / GuestInterpret");
    for token in FORBIDDEN_IN_CORE {
        assert!(!source.contains(token), "guest.rs must not contain '{token}'");
    }
}
