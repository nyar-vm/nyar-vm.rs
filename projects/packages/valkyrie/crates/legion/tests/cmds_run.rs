mod support;

use legion::{
    CanonicalTarget,
    cmds::{
        build::{BuildArgs, run as run_build},
        run::{RunArgs, run as run_project},
    },
};
use std::{
    env,
    path::{Path, PathBuf},
    process::ExitCode,
};
use support::run_fixture::{
    RunFixtureContext, can_run_all_runtime_targets, can_run_fixture_context, collect_run_fixture_cases, verify_run_fixture,
    verify_run_fixture_for_context,
};

#[test]
fn runs_declared_target_fixtures() {
    let fixtures_root = run_fixture_root();
    let fixtures = collect_run_fixture_cases(&fixtures_root)
        .into_iter()
        .filter(|fixture| !explicit_interop_fixture_paths().contains(fixture))
        .collect::<Vec<_>>();
    assert!(!fixtures.is_empty(), "no run fixtures found under '{}'", fixtures_root.display());

    if !can_run_all_runtime_targets(&fixtures) {
        return;
    }

    for fixture in &fixtures {
        verify_run_fixture(fixture);
    }
}

#[test]
fn runs_jvm_interop_fixture() {
    let context = RunFixtureContext::new(&["jvm"]);
    if !can_run_fixture_context(&context) {
        return;
    }

    verify_run_fixture_for_context(&run_fixture_root().join("jvm_interop_stdout.valkyrie"), &context);
}

#[test]
fn runs_wasm_interop_fixture() {
    let context = RunFixtureContext::new(&["node"]);
    if !can_run_fixture_context(&context) {
        return;
    }

    verify_run_fixture_for_context(&run_fixture_root().join("wasm_interop_stdout.valkyrie"), &context);
}

#[test]
fn runs_wasi_component_clock_fixture() {
    let context = RunFixtureContext::new(&["wasi"]);
    if !can_run_fixture_context(&context) {
        return;
    }

    verify_run_fixture_for_context(&run_fixture_root().join("wasi_component_clock.valkyrie"), &context);
}

#[test]
fn runs_multiple_main_stdout_fixture() {
    let context = RunFixtureContext::new(&["node"]);
    if !can_run_fixture_context(&context) {
        return;
    }

    verify_run_fixture_for_context(&run_fixture_root().join("multiple_main_stdout.valkyrie"), &context);
}

#[test]
fn runs_clr_local_package_fixture() {
    let context = RunFixtureContext::new(&["clr"]).local_package().with_manifest(
        r#"{
    name: "local-package-minimal",
    version: "0.1.0",
    dependencies: {
        "std": false,
        "core": false
    },
    build: [
        {
            target: "clr",
            msil: true
        }
    ]
}
"#,
    );
    if !can_run_fixture_context(&context) {
        return;
    }

    verify_run_fixture_for_context(&run_fixture_root().join("local_package_minimal.valkyrie"), &context);
}

#[test]
fn runs_clr_script_fixture() {
    let context = RunFixtureContext::new(&["clr"]).script().with_manifest(
        r#"{
    name: "script-minimal",
    version: "0.1.0",
    dependencies: {
        "std": false,
        "core": false
    },
    build: [
        {
            target: "clr",
            msil: true
        }
    ]
}
"#,
    );
    if !can_run_fixture_context(&context) {
        return;
    }

    verify_run_fixture_for_context(&run_fixture_root().join("script_minimal.valkyrie"), &context);
}

#[test]
fn runs_clr_nested_workspace_fixture() {
    let context = RunFixtureContext::new(&["clr"]).nested_workspace_member().with_manifest(
        r#"{
    name: "nested-workspace-minimal",
    version: "0.1.0",
    dependencies: {
        "std": false,
        "core": false
    },
    build: [
        {
            target: "clr",
            msil: true
        }
    ]
}
"#,
    );
    if !can_run_fixture_context(&context) {
        return;
    }

    verify_run_fixture_for_context(&run_fixture_root().join("nested_workspace_minimal.valkyrie"), &context);
}

#[test]
fn runs_option_nyar_vm_project_with_std() {
    if !support::valkyrie_v_std_workspace_available() {
        eprintln!("skip runs_option_nyar_vm_project_with_std: sibling valkyrie.v core/std missing");
        return;
    }
    let Some(nyar_vm) = resolve_nyar_vm_runner()
    else {
        eprintln!("skip runs_option_nyar_vm_project_with_std: nyar-vm binary not found");
        return;
    };

    let fixture = support::create_nyar_vm_std_project(
        "legion-run-nyar-vm-option",
        r#"namespace opt.smoke;

micro value_or_zero(o: Option<i64>): i64 {
    if o.is_none() {
        return 0
    }
    return o.unwrap()
}

[main]
micro main(): i64 {
    return value_or_zero(Some(7))
}
"#,
    );
    let output_dir = fixture.project_dir.join("dist").join("run-option-nyar-vm");
    let target = CanonicalTarget::parse("ovm").expect("nyar-vm target");
    assert_eq!(
        run_build(&BuildArgs {
            project_dir: fixture.project_dir.clone(),
            target,
            output_dir: Some(output_dir.clone()),
            workspace: false,
            debug_artifacts: false,
        })
        .unwrap(),
        ExitCode::SUCCESS
    );
    assert_eq!(
        run_project(&RunArgs {
            project_dir: fixture.project_dir.clone(),
            target: CanonicalTarget::parse("ovm").expect("nyar-vm target"),
            output_dir: Some(output_dir),
            workspace: false,
            runner: vec![format!("ovm={}", nyar_vm.display())],
            artifact: None,
            dry_run: false,
            debug_artifacts: false,
            workload_json: None,
            workload_file: None,
        })
        .unwrap(),
        ExitCode::SUCCESS
    );
}

#[test]
fn runs_cross_file_nyar_vm_project() {
    let Some(nyar_vm) = resolve_nyar_vm_runner()
    else {
        eprintln!("skip runs_cross_file_nyar_vm_project: nyar-vm binary not found (set NYAR_VM or build sibling nyar-vm.rs)");
        return;
    };

    let fixture = support::create_smoke_project_with_build(
        "legion-run-nyar-vm-cross-file",
        r#"{
            target: "ovm"
        }"#,
        r#"namespace app.smoke;

[main]
micro main(): i64 {
    return add_one(41)
}
"#,
    );
    std::fs::write(
        fixture.project_dir.join("source").join("helper.v"),
        r#"namespace app.smoke;

micro add_one(x: i64): i64 {
    return x + 1
}
"#,
    )
    .unwrap();

    let output_dir = fixture.project_dir.join("dist").join("run-cross-file-nyar-vm");
    let target = CanonicalTarget::parse("ovm").expect("nyar-vm target");
    assert_eq!(
        run_build(&BuildArgs {
            project_dir: fixture.project_dir.clone(),
            target,
            output_dir: Some(output_dir.clone()),
            workspace: false,
            debug_artifacts: false,
        })
        .unwrap(),
        ExitCode::SUCCESS
    );
    assert_eq!(
        run_project(&RunArgs {
            project_dir: fixture.project_dir.clone(),
            target: CanonicalTarget::parse("ovm").expect("nyar-vm target"),
            output_dir: Some(output_dir),
            workspace: false,
            runner: vec![format!("ovm={}", nyar_vm.display())],
            artifact: None,
            dry_run: false,
            debug_artifacts: false,
            workload_json: None,
            workload_file: None,
        })
        .unwrap(),
        ExitCode::SUCCESS
    );
}

#[test]
fn runs_minimal_nyar_vm_project() {
    let Some(nyar_vm) = resolve_nyar_vm_runner()
    else {
        eprintln!("skip runs_minimal_nyar_vm_project: nyar-vm binary not found (set NYAR_VM or build sibling nyar-vm.rs)");
        return;
    };

    let fixture = support::create_smoke_project_with_build(
        "legion-run-nyar-vm",
        r#"{
            target: "ovm"
        }"#,
        r#"[main]
micro main(): i64 {
    return 0
}
"#,
    );
    let output_dir = fixture.project_dir.join("dist").join("run-nyar-vm");
    let target = CanonicalTarget::parse("ovm").expect("nyar-vm target");

    let build_status = run_build(&BuildArgs {
        project_dir: fixture.project_dir.clone(),
        target,
        output_dir: Some(output_dir.clone()),
        workspace: false,
        debug_artifacts: false,
    })
    .unwrap();
    assert_eq!(build_status, ExitCode::SUCCESS);

    let run_status = run_project(&RunArgs {
        project_dir: fixture.project_dir.clone(),
        target: CanonicalTarget::parse("ovm").expect("nyar-vm target"),
        output_dir: Some(output_dir),
        workspace: false,
        runner: vec![format!("ovm={}", nyar_vm.display())],
        artifact: None,
        dry_run: false,
        debug_artifacts: false,
        workload_json: None,
        workload_file: None,
    })
    .unwrap();
    assert_eq!(run_status, ExitCode::SUCCESS);
}

#[test]
fn dry_run_nyar_vm_accepts_workload_json() {
    let fixture = support::create_smoke_project_with_build(
        "legion-run-nyar-vm-workload",
        r#"{
            target: "ovm"
        }"#,
        r#"[main]
micro main(): i64 {
    return 0
}
"#,
    );
    let output_dir = fixture.project_dir.join("dist").join("run-nyar-vm-workload");
    let target = CanonicalTarget::parse("ovm").expect("nyar-vm target");
    let build_status = run_build(&BuildArgs {
        project_dir: fixture.project_dir.clone(),
        target,
        output_dir: Some(output_dir.clone()),
        workspace: false,
        debug_artifacts: false,
    })
    .unwrap();
    assert_eq!(build_status, ExitCode::SUCCESS);

    let status = run_project(&RunArgs {
        project_dir: fixture.project_dir.clone(),
        target: CanonicalTarget::parse("ovm").expect("nyar-vm target"),
        output_dir: Some(output_dir),
        workspace: false,
        runner: vec!["ovm=ovm".to_string()],
        artifact: None,
        dry_run: true,
        debug_artifacts: false,
        workload_json: Some(r#"{"phase":"request","pause_budget_ms":5,"preferred_mode":"generational_low_latency"}"#.into()),
        workload_file: None,
    })
    .unwrap();
    assert_eq!(status, ExitCode::SUCCESS);
}

fn resolve_nyar_vm_runner() -> Option<PathBuf> {
    if let Ok(path) = env::var("NYAR_VM") {
        let candidate = PathBuf::from(path);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    let legion_manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // projects/compilers/legion → workspace root → sibling nyar-vm.rs
    let sibling_root = legion_manifest.join("../../../..").join("nyar-vm.rs");
    for relative in ["target/debug/nyar-vm.exe", "target/debug/nyar-vm", "target/release/nyar-vm.exe", "target/release/nyar-vm"] {
        let candidate = sibling_root.join(relative);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn run_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("cmds_run")
}

fn explicit_interop_fixture_paths() -> Vec<PathBuf> {
    let root = run_fixture_root();
    vec![
        root.join("jvm_interop_stdout.valkyrie"),
        root.join("wasm_interop_stdout.valkyrie"),
        root.join("wasi_component_clock.valkyrie"),
        root.join("local_package_minimal.valkyrie"),
        root.join("script_minimal.valkyrie"),
        root.join("nested_workspace_minimal.valkyrie"),
    ]
}
