//! M4：Legion 生产 build/run 链只装配，不直接生产语义 MIR。

use std::{fs, path::PathBuf};

fn read_production_rs(path: &PathBuf) -> String {
    let source = fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    if let Some(index) = source.find("#[cfg(test)]") { source[..index].to_string() } else { source }
}

#[test]
fn build_must_use_single_compiler_artifact_entry() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/cmds/build/mod.rs");
    let production = read_production_rs(&path);
    assert!(production.contains("compile_source_groups_to_artifacts"), "{} must call compile_source_groups_to_artifacts", path.display());
    for forbidden in [
        "compile_source_to_program",
        "compile_source_groups_to_program",
        "canonical_program_from_semantic_mir",
        "MirModule",
        "HirModule",
        "body_source",
        "ExecutableModule",
    ] {
        assert!(!production.contains(forbidden), "{} still references `{forbidden}`", path.display());
    }
}

#[test]
fn run_must_not_recompile_semantic_mir() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/cmds/run/mod.rs");
    let production = read_production_rs(&path);
    for forbidden in [
        "compile_source_groups_to_artifacts",
        "compile_source_to_program",
        "compile_source_groups_to_program",
        "canonical_program_from_semantic_mir",
        "ValkyrieCompiler",
        "MirModule",
        "HirModule",
        "body_source",
        "ExecutableModule",
    ] {
        assert!(!production.contains(forbidden), "{} still references `{forbidden}`", path.display());
    }
}

#[test]
fn lint_must_use_hir_diagnostic_entry_not_artifact_pipeline() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/cmds/lint.rs");
    let production = read_production_rs(&path);
    assert!(production.contains("lint_valkyrie_source"));
    assert!(production.contains("lint_valkyrie_vx_source"));
    for forbidden in [
        "compile_source_groups_to_artifacts",
        "compile_source_to_program",
        "compile_source_groups_to_program",
        "canonical_program_from_semantic_mir",
        "MirModule",
        "body_source",
        "ExecutableModule",
    ] {
        assert!(!production.contains(forbidden), "{} still references `{forbidden}`", path.display());
    }
}

#[test]
fn build_must_apply_resolver_host_provider_selection() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/cmds/build/mod.rs");
    let production = read_production_rs(&path);
    assert!(production.contains("selected_host_providers"));
    assert!(production.contains("CompilerHostProviderBinding"));
    assert!(production.contains("CompilerBuildContext"));
}
