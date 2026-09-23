//! First-party toolchain guards: panda must not shell out to foreign CLIs.
//!
//! User scripts may still invoke anything via [`nyar_package_manager::ScriptRunner`];
//! that path lives in the package-manager crate (`cmd` / `sh`), not as a panda
//! `Command::new("uv"|…)` wrapper. Product `PYTHONPATH` wiring is an adapter, not a pip shell.

#![cfg(test)]

use std::{
    fs,
    path::{Path, PathBuf},
};

/// Foreign toolchain binaries that must never appear as `Command::new("…")` in panda.
const FORBIDDEN_COMMANDS: &[&str] = &[
    "uv", "pip", "pip3", "poetry", "conda", "mamba", "ruff", "black", "flake8", "mypy", "pytest", "hatch", "pdm", "pipenv", "npm", "pnpm",
    "biome", "prettier",
];

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

fn strip_rust_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut in_block = false;
    while i < bytes.len() {
        if in_block {
            if bytes[i] == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                in_block = false;
                i += 2;
                continue;
            }
            out.push(' ');
            i += 1;
            continue;
        }
        if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }
        if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            in_block = true;
            out.push(' ');
            out.push(' ');
            i += 2;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn assert_no_forbidden_command(path: &Path, source: &str) {
    for tool in FORBIDDEN_COMMANDS {
        for quote in ['"', '\''] {
            let needle = format!("Command::new({quote}{tool}{quote})");
            assert!(
                !source.contains(&needle),
                "{} must not spawn foreign toolchain via {needle} (use nyar-package-manager + PYTHONPATH adapters / ScriptRunner for user scripts)",
                path.display()
            );
        }
    }
}

#[test]
fn panda_src_does_not_shell_foreign_toolchains() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rs_files(&root, &mut files);
    assert!(!files.is_empty(), "expected panda src/*.rs");

    for path in files {
        if path.file_name().and_then(|n| n.to_str()) == Some("architecture_guards.rs") {
            continue;
        }
        let raw = fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        assert_no_forbidden_command(&path, &strip_rust_comments(&raw));
    }
}

#[test]
fn panda_depends_on_neutral_package_manager() {
    let cargo = fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).expect("read panda Cargo.toml");
    assert!(cargo.contains("nyar-package-manager"), "panda must depend on nyar-package-manager for install");
    assert!(cargo.contains("nyar-language"), "panda must depend on nyar-language for fmt/lint");
    assert!(cargo.contains("nyar-analyzer"), "panda must depend on nyar-analyzer contracts for fmt/lint");
    for forbidden in ["ruff", "black", "uv", "poetry", "pip"] {
        assert!(
            !cargo.lines().any(|line| {
                let t = line.trim();
                t.starts_with(forbidden) || t.starts_with(&format!("{forbidden} "))
            }),
            "panda Cargo.toml must not depend on foreign toolchain crate/binary `{forbidden}`"
        );
    }
}

#[test]
fn panda_compat_is_not_driven_by_foreign_lockfiles() {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/project.rs");
    let source = strip_rust_comments(&fs::read_to_string(&project).expect("project.rs"));
    for lock in ["poetry.lock", "uv.lock", "Pipfile.lock", "conda-lock.yml", "pdm.lock"] {
        assert!(!source.contains(lock), "project.rs must not mention {lock} when choosing compat (use pyproject / tool.panda params)");
    }
}
