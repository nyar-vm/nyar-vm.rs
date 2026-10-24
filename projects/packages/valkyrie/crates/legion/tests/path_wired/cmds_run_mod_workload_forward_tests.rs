//! Migrated from `src/cmds/run/mod.rs` (`workload_forward_tests`).

use super::*;

fn bare_run_args() -> RunArgs {
    RunArgs {
        project_dir: PathBuf::from("."),
        target: CanonicalTarget::parse("ovm").expect("ovm"),
        output_dir: None,
        workspace: false,
        runner: Vec::new(),
        artifact: None,
        dry_run: true,
        debug_artifacts: false,
        workload_json: None,
        workload_file: None,
    }
}

#[test]
fn appends_workload_json_for_nyar_vm() {
    let mut command =
        RunCommand { target: RunnerFamily::Ovm, artifact: PathBuf::from("out.nyar"), command: "odin-vm".into(), args: vec!["out.nyar".into()] };
    let mut args = bare_run_args();
    args.workload_json = Some(r#"{"pause_budget_ms":5}"#.into());
    append_ovm_workload_args(&mut command, &args).expect("append");
    assert!(command.args.windows(2).any(|w| w[0] == "--workload-json" && w[1].contains("pause_budget_ms")));
}

#[test]
fn resolver_compile_evidence_matches_compiler_helpers() {
    let groups = vec![nyar_types::compiler_input::CompilerSourceGroup::single_source("app", "app", "[main] micro main() -> i32 { return 1 }")];
    let digest = expected_resolver_compile_evidence_from_groups(&groups, &[], "ovm");
    assert_eq!(digest.semantic_closure_hash, rusty_valkyrie::semantic_closure_hash_for_groups(&groups, "ovm"));
    assert_eq!(digest.host_provider_binding_hash, rusty_valkyrie::host_provider_binding_hash_for_bindings(&[]));
}

#[test]
fn is_fresh_rejects_stale_host_provider_compile_evidence() {
    use legion_workspace::{
        manifest::ProjectArtifactKind,
        planner::{BuildPlan, PlannedHostProvider, PlannedProject, PlannedSemanticSourceGroup},
    };
    use std::fs;
    use tempfile::tempdir;

    let directory = tempdir().expect("tempdir");
    let root = directory.path();
    let source_path = root.join("main.v");
    fs::write(&source_path, "[main] micro main() -> i32 { return 1 }").expect("write source");
    let manifest_path = root.join("legion.von");
    fs::write(&manifest_path, "name: demo\n").expect("write manifest");
    let group = PlannedSemanticSourceGroup {
        dependency_key: "demo".into(),
        name: "demo".into(),
        manifest_dir: root.to_path_buf(),
        source_files: vec![source_path.clone()],
        direct_dependencies: Vec::new(),
        is_entry: true,
    };
    let target = CanonicalTarget::parse("ovm").expect("ovm");
    let plan = BuildPlan {
        workspace_root: root.to_path_buf(),
        output_dir: root.join("dist"),
        project: PlannedProject {
            name: "demo".into(),
            manifest_dir: root.to_path_buf(),
            manifest_path: manifest_path.clone(),
            source_files: vec![source_path.clone()],
            semantic_source_groups: vec![group],
            host_contracts: Vec::new(),
            host_provider_candidates: Vec::new(),
            selected_host_providers: vec![PlannedHostProvider {
                contract: "demo::write".into(),
                contract_key: Some(micro_declaration("demo", "demo.v", "write")),
                symbol: "std::write".into(),
                key: micro_declaration("std", "std.v", "write"),
                source_file: manifest_path.clone(),
                line: 1,
            }],
            build_target: legion_workspace::manifest::BuildTargetSpec { target, ..Default::default() },
            artifact_kind: ProjectArtifactKind::Binary,
            dependencies: Vec::new(),
        },
    };
    fs::create_dir_all(&plan.output_dir).expect("output dir");
    let artifact_path = plan.output_dir.join("app.nyar");
    fs::write(&artifact_path, b"nyar-artifact-stub").expect("artifact");
    let expected = expected_resolver_compile_evidence(&plan).expect("expected evidence");
    let inputs = collect_execution_input_digests(&plan).expect("input digests");
    let manifest = ExecutionManifest {
        schema_version: EXECUTION_MANIFEST_SCHEMA_VERSION,
        identity_schema_version: nyar_types::IDENTITY_SCHEMA_VERSION,
        mir_contract_version: nyar_types::MIR_CONTRACT_VERSION,
        layout_plan_version: nyar_types::LAYOUT_PLAN_VERSION,
        bytecode_format_version: odin_bytecode::BYTECODE_FORMAT_VERSION,
        project_name: plan.project.name.clone(),
        target: plan.project.build_target.target.to_string(),
        inputs,
        artifacts: vec![ExecutionArtifactDigest { path: "app.nyar".into(), hash: hash_file_sha256(&artifact_path).expect("artifact hash") }],
        run_contracts: vec![RunContract {
            logical_entry: "main".into(),
            physical_entry: "app.nyar".into(),
            invocation: "odin-vm app.nyar".into(),
            validate: "exists".into(),
        }],
        compile_evidence: Some(CompileEvidenceDigest {
            semantic_closure_hash: expected.semantic_closure_hash.clone(),
            canonical_surface_hash: String::new(),
            host_provider_binding_hash: "stale-host-provider-hash".into(),
            generated_source_hash: String::new(),
        }),
    };
    assert!(!manifest.is_fresh_for_plan(&plan).expect("freshness check"), "stale host provider evidence must invalidate execution manifest");
}

fn write_test_compile_evidence(output_dir: &Path, digest: &CompileEvidenceDigest) {
    use std::fs;

    let evidence = rusty_valkyrie::CompilerCompileEvidence {
        semantic_closure_hash: digest.semantic_closure_hash.clone(),
        canonical_surface_hash: digest.canonical_surface_hash.clone(),
        host_provider_binding_hash: digest.host_provider_binding_hash.clone(),
        generated_source_hash: digest.generated_source_hash.clone(),
    };
    let content = crate::write_von_indented(&evidence).expect("serialize compile evidence");
    fs::write(output_dir.join(COMPILE_EVIDENCE_FILE_NAME), content).expect("write compile evidence");
}

fn sample_freshness_plan() -> (tempfile::TempDir, BuildPlan, PathBuf) {
    use legion_workspace::{
        manifest::ProjectArtifactKind,
        planner::{BuildPlan, PlannedProject, PlannedSemanticSourceGroup},
    };
    use std::fs;
    use tempfile::tempdir;

    let directory = tempdir().expect("tempdir");
    let root = directory.path();
    let source_path = root.join("main.v");
    fs::write(&source_path, "[main] micro main() -> i32 { return 1 }").expect("write source");
    let manifest_path = root.join("legion.von");
    fs::write(&manifest_path, "name: demo\n").expect("write manifest");
    let group = PlannedSemanticSourceGroup {
        dependency_key: "demo".into(),
        name: "demo".into(),
        manifest_dir: root.to_path_buf(),
        source_files: vec![source_path.clone()],
        direct_dependencies: Vec::new(),
        is_entry: true,
    };
    let target = CanonicalTarget::parse("ovm").expect("ovm");
    let plan = BuildPlan {
        workspace_root: root.to_path_buf(),
        output_dir: root.join("dist"),
        project: PlannedProject {
            name: "demo".into(),
            manifest_dir: root.to_path_buf(),
            manifest_path: manifest_path.clone(),
            source_files: vec![source_path],
            semantic_source_groups: vec![group],
            host_contracts: Vec::new(),
            host_provider_candidates: Vec::new(),
            selected_host_providers: Vec::new(),
            build_target: legion_workspace::manifest::BuildTargetSpec { target, ..Default::default() },
            artifact_kind: ProjectArtifactKind::Binary,
            dependencies: Vec::new(),
        },
    };
    fs::create_dir_all(&plan.output_dir).expect("output dir");
    let artifact_path = plan.output_dir.join("app.nyar");
    fs::write(&artifact_path, b"nyar-artifact-stub").expect("artifact");
    (directory, plan, artifact_path)
}

fn sample_execution_manifest(plan: &BuildPlan, artifact_path: &Path, evidence: CompileEvidenceDigest) -> ExecutionManifest {
    ExecutionManifest {
        schema_version: EXECUTION_MANIFEST_SCHEMA_VERSION,
        identity_schema_version: nyar_types::IDENTITY_SCHEMA_VERSION,
        mir_contract_version: nyar_types::MIR_CONTRACT_VERSION,
        layout_plan_version: nyar_types::LAYOUT_PLAN_VERSION,
        bytecode_format_version: odin_bytecode::BYTECODE_FORMAT_VERSION,
        project_name: plan.project.name.clone(),
        target: plan.project.build_target.target.to_string(),
        inputs: collect_execution_input_digests(plan).expect("input digests"),
        artifacts: vec![ExecutionArtifactDigest { path: "app.nyar".into(), hash: hash_file_sha256(artifact_path).expect("artifact hash") }],
        run_contracts: vec![RunContract {
            logical_entry: "main".into(),
            physical_entry: "app.nyar".into(),
            invocation: "odin-vm app.nyar".into(),
            validate: "exists".into(),
        }],
        compile_evidence: Some(evidence),
    }
}

#[test]
fn is_fresh_rejects_stale_semantic_closure_compile_evidence() {
    let (_directory, plan, artifact_path) = sample_freshness_plan();
    let expected = expected_resolver_compile_evidence(&plan).expect("expected evidence");
    let evidence = CompileEvidenceDigest {
        semantic_closure_hash: "stale-semantic-closure-hash".into(),
        canonical_surface_hash: "canonical-surface-digest".into(),
        host_provider_binding_hash: expected.host_provider_binding_hash,
        generated_source_hash: String::new(),
    };
    write_test_compile_evidence(&plan.output_dir, &evidence);
    let manifest = sample_execution_manifest(&plan, &artifact_path, evidence);
    assert!(!manifest.is_fresh_for_plan(&plan).expect("freshness check"), "stale semantic closure evidence must invalidate execution manifest");
}

#[test]
fn is_fresh_accepts_matching_compile_evidence_sidecar() {
    let (_directory, plan, artifact_path) = sample_freshness_plan();
    let expected = expected_resolver_compile_evidence(&plan).expect("expected evidence");
    let evidence = CompileEvidenceDigest {
        semantic_closure_hash: expected.semantic_closure_hash,
        canonical_surface_hash: "canonical-surface-digest".into(),
        host_provider_binding_hash: expected.host_provider_binding_hash,
        generated_source_hash: String::new(),
    };
    write_test_compile_evidence(&plan.output_dir, &evidence);
    let manifest = sample_execution_manifest(&plan, &artifact_path, evidence);
    assert!(
        manifest.is_fresh_for_plan(&plan).expect("freshness check"),
        "matching compile-evidence sidecar must keep execution manifest fresh"
    );
}

#[test]
fn is_fresh_rejects_mismatched_compile_evidence_sidecar() {
    let (_directory, plan, artifact_path) = sample_freshness_plan();
    let expected = expected_resolver_compile_evidence(&plan).expect("expected evidence");
    let evidence = CompileEvidenceDigest {
        semantic_closure_hash: expected.semantic_closure_hash,
        canonical_surface_hash: "canonical-surface-digest".into(),
        host_provider_binding_hash: expected.host_provider_binding_hash,
        generated_source_hash: String::new(),
    };
    write_test_compile_evidence(
        &plan.output_dir,
        &CompileEvidenceDigest { canonical_surface_hash: "different-canonical-surface".into(), ..evidence.clone() },
    );
    let manifest = sample_execution_manifest(&plan, &artifact_path, evidence);
    assert!(
        !manifest.is_fresh_for_plan(&plan).expect("freshness check"),
        "mismatched compile-evidence sidecar must invalidate execution manifest"
    );
}

#[test]
fn resolver_compile_evidence_host_provider_hash_is_binding_sensitive() {
    let groups = vec![nyar_types::compiler_input::CompilerSourceGroup::single_source("app", "app", "[main] micro main() -> i32 { return 1 }")];
    let first = rusty_valkyrie::CompilerHostProviderBinding {
        contract: micro_declaration("demo", "demo.v", "write"),
        provider: micro_declaration("std", "std.v", "write"),
    };
    let second = rusty_valkyrie::CompilerHostProviderBinding {
        contract: micro_declaration("demo", "demo.v", "write"),
        provider: micro_declaration("std", "std.v", "print"),
    };
    let without = expected_resolver_compile_evidence_from_groups(&groups, &[], "ovm");
    let with_first = expected_resolver_compile_evidence_from_groups(&groups, &[first.clone()], "ovm");
    let with_second = expected_resolver_compile_evidence_from_groups(&groups, &[second], "ovm");
    assert_eq!(without.semantic_closure_hash, with_first.semantic_closure_hash);
    assert_ne!(with_first.host_provider_binding_hash, without.host_provider_binding_hash);
    assert_ne!(with_first.host_provider_binding_hash, with_second.host_provider_binding_hash);
}

#[test]
fn rejects_workload_json_for_non_ovm() {
    let mut command =
        RunCommand { target: RunnerFamily::Node, artifact: PathBuf::from("out.mjs"), command: "node".into(), args: vec!["out.mjs".into()] };
    let mut args = bare_run_args();
    args.workload_json = Some("{}".into());
    let err = append_ovm_workload_args(&mut command, &args).expect_err("must reject");
    assert!(err.to_string().contains("ovm"));
}

fn micro_declaration(dependency_key: &str, source_path: &str, name: &str) -> rusty_valkyrie::DeclarationKey {
    rusty_valkyrie::DeclarationKey {
        dependency_key: dependency_key.to_owned(),
        source_path: source_path.to_owned(),
        namespace: Vec::new(),
        kind: rusty_valkyrie::DeclarationKind::Micro,
        owner: None,
        name: name.to_owned(),
    }
}
