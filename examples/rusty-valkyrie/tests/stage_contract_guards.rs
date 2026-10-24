//! M2/M3：阶段合同门禁（数据结构 + 生产源码扫描）。

use std::{fs, path::PathBuf};

fn collect_rs_files(dir: &PathBuf, out: &mut Vec<PathBuf>) {
    if !dir.is_dir() {
        return;
    }
    for entry in fs::read_dir(dir).unwrap_or_else(|error| panic!("read {}: {error}", dir.display())) {
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

fn valkyrie_production_sources() -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/valkyrie");
    let mut files = Vec::new();
    collect_rs_files(&root, &mut files);
    files
}

fn strip_test_modules(source: &str) -> String {
    if let Some(index) = source.find("#[cfg(test)]") { source[..index].to_string() } else { source.to_string() }
}

fn extract_function_body(source: &str, function_name: &str) -> String {
    let signatures = [format!("fn {function_name}"), format!("pub fn {function_name}"), format!("pub(crate) fn {function_name}")];
    let start = signatures
        .iter()
        .filter_map(|signature| source.find(signature))
        .min()
        .unwrap_or_else(|| panic!("function `{function_name}` not found in source"));
    let rest = &source[start..];
    let open = rest.find('{').unwrap_or_else(|| panic!("function `{function_name}` has no body"));
    let mut depth = 0usize;
    let mut end = open + 1;
    for (offset, ch) in rest[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = open + offset + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    rest[..end].to_string()
}

#[test]
fn hir_must_not_keep_collection_facade_whitelist() {
    let pattern = "collection_facade_compatible";
    for path in valkyrie_production_sources() {
        if !path.components().any(|c| c.as_os_str() == "hir") {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let production = strip_test_modules(&source);
        assert!(!production.contains(pattern), "{} still contains `{pattern}`", path.display());
    }
}

#[test]
fn mir_must_not_guess_result_owner_by_suffix() {
    let pattern = "ends_with(\"Result\")";
    for path in valkyrie_production_sources() {
        if !path.components().any(|c| c.as_os_str() == "mir") {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let production = strip_test_modules(&source);
        assert!(!production.contains(pattern), "{} still contains `{pattern}`", path.display());
    }
}

#[test]
fn compile_pipeline_exports_single_artifact_entry() {
    let pipeline = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/valkyrie/compile_pipeline/mod.rs");
    let source = fs::read_to_string(&pipeline).unwrap_or_else(|error| panic!("read pipeline: {error}"));
    assert!(source.contains("compile_source_groups_to_artifacts"));
    assert!(!source.contains("body_source"));
    assert!(!source.contains("ExecutableModule"));
}

#[test]
fn representation_planner_must_only_consume_canonical_program() {
    let representation = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/valkyrie/compile_pipeline/representation.rs");
    let source = fs::read_to_string(&representation).unwrap_or_else(|error| panic!("read representation: {error}"));
    let production = strip_test_modules(&source);
    assert!(production.contains("CanonicalProgram"));
    assert!(!production.contains("MirModule"));
    assert!(!production.contains("HirModule"));
    assert!(!production.contains("body_source"));
}

#[test]
fn driver_must_validate_semantic_mir_before_canonical_program() {
    let driver = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/valkyrie/compile_pipeline/driver.rs");
    let source = fs::read_to_string(&driver).unwrap_or_else(|error| panic!("read driver: {error}"));
    let production = strip_test_modules(&source);
    let body = extract_function_body(&production, "compile_linked_semantic_mir");
    let validate = body.find("validate_semantic_module").expect("driver must validate Semantic MIR");
    let canonical = body.find("canonical_program_from_semantic_mir").expect("driver must produce CanonicalProgram");
    assert!(validate < canonical, "validate_semantic_module must run before canonical_program_from_semantic_mir");
    let representation = body.find("CanonicalRepresentationPlanner").expect("driver must plan RepresentationPlan");
    assert!(canonical < representation, "CanonicalProgram must exist before RepresentationPlan planning");
}

#[test]
fn mir_must_not_lower_nullary_variants_by_variant_name() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/valkyrie/mir/ssa/expr_lowering.rs");
    let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let production = strip_test_modules(&source);
    assert!(!production.contains("try_lower_nullary_sum_variant"), "{} still lowers nullary unite arms by variant spelling", path.display());
    assert!(!production.contains("variant.name == variant_name"), "{} still scans sum tables by variant spelling", path.display());
}

#[test]
fn mir_ssa_must_not_keep_spelling_based_sum_identity_helper() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/valkyrie/mir/ssa/mod.rs");
    let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let production = strip_test_modules(&source);
    assert!(!production.contains("fn sum_identity("), "{} still exposes spelling-based sum identity lookup", path.display());
}

#[test]
fn mir_builtin_constructor_lowering_must_not_use_spelling_based_sum_identity() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/valkyrie/mir/ssa/expr_lowering.rs");
    let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let production = strip_test_modules(&source);
    for pattern in ["sum_identity(\"Option\",", "sum_identity(\"Result\",", "sum_identity(&sum_name, \"Some\")"] {
        assert!(!production.contains(pattern), "{} still resolves builtin unite constructors by spelling", path.display());
    }
}

#[test]
fn mir_pattern_lowering_must_not_rediscover_sum_variant_identity_by_name() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/valkyrie/mir/ssa/pattern_lowering.rs");
    let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let production = strip_test_modules(&source);
    for pattern in [
        "sum_identity(owner_name, variant_name)",
        "item.name == variant_name",
        "try_lower_sum_variant_tag_match(&actual_type, name, value.clone(), None)",
    ] {
        assert!(!production.contains(pattern), "{} still resolves sum pattern identity by spelling fallback", path.display());
    }
}

#[test]
fn mir_try_propagate_lowering_must_not_use_spelling_based_sum_identity() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/valkyrie/mir/ssa/try_propagate_lowering.rs");
    let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let production = strip_test_modules(&source);
    for pattern in ["self.sum_identity(", "sum_variant_declaration_ids(", "HirPattern::Name", "lower_pattern_match_operand"] {
        assert!(!production.contains(pattern), "{} still resolves try-propagate sum identity by spelling fallback", path.display());
    }
}

#[test]
fn mir_construct_lowering_must_not_rediscover_variant_identity_by_name() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/valkyrie/mir/ssa/expr_lowering.rs");
    let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let production = strip_test_modules(&source);
    assert!(
        !production.contains("variant.name == name.as_str()"),
        "{} still rediscovers unite variant identity from spellings",
        path.display()
    );
}

#[test]
fn backend_bundle_must_plan_artifacts_from_context_target_after_compiled_program() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/valkyrie/compile_pipeline/backend_bundle.rs");
    let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let production = strip_test_modules(&source);
    let body = extract_function_body(&production, "compile_source_groups_to_backend_bundle");
    let program = body.find("compile_source_groups_to_program_with_host_bindings").expect("backend bundle must compile semantic program first");
    let plan = body.find("plan_artifacts_from_compiled_program").expect("backend bundle must plan artifacts from CompiledProgram");
    assert!(program < plan, "artifact planning must run after semantic program production");
    assert!(body.contains("context.target"), "artifact planning must consume CompilerBuildContext target");
    assert!(body.contains("context.selected_host_providers"), "semantic program production must apply resolver host provider bindings");
}

#[test]
fn lint_entry_must_not_reference_artifact_pipeline() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/valkyrie/lint.rs");
    let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let production = strip_test_modules(&source);
    assert!(production.contains("lint_valkyrie_source"));
    assert!(!production.contains("compile_source_groups_to_artifacts"));
    assert!(!production.contains("canonical_program_from_semantic_mir"));
    assert!(!production.contains("MirModule"));
    assert!(!production.contains("ExecutableModule"));
}

#[test]
fn compiler_build_context_must_carry_target_and_host_bindings() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/valkyrie/compile_pipeline/context.rs");
    let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    assert!(source.contains("CanonicalTarget"));
    assert!(source.contains("selected_host_providers"));
    assert!(!source.contains("body_source"));
    assert!(!source.contains("ExecutableModule"));
}

#[test]
fn processing_half_must_not_reference_parallel_emitter_bypasses() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/valkyrie/compile_pipeline");
    for name in ["representation.rs", "backend_bundle.rs", "driver.rs"] {
        let path = root.join(name);
        let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let production = strip_test_modules(&source);
        assert!(!production.contains("ExecutableModule"), "{} still references ExecutableModule", path.display());
        assert!(!production.contains("body_source"), "{} still references body_source bypass", path.display());
    }
}
