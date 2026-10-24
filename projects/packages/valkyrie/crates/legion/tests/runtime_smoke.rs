mod support;

use legion::{
    CanonicalTarget,
    cmds::{
        build::{BuildArgs, run as run_build},
        run::{RunArgs, run as run_project},
    },
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};
use support::runtime_fixture::{can_run_all_required_commands, collect_runtime_fixture_cases, regenerate_enabled, verify_runtime_fixture};

#[test]
fn runs_runtime_fixtures_across_declared_targets() {
    let fixtures_root = runtime_fixture_root();
    let fixtures = collect_runtime_fixture_cases(&fixtures_root);
    assert!(!fixtures.is_empty(), "no runtime fixtures found under '{}'", fixtures_root.display());

    if !can_run_all_required_commands(&fixtures) {
        return;
    }

    for fixture in &fixtures {
        verify_runtime_fixture(fixture);
    }

    if regenerate_enabled() {
        eprintln!("runtime fixtures regenerated under {}", fixtures_root.display());
    }
}

#[test]
#[ignore = "known clr std fs runtime regression; kept at legion integration layer"]
fn runs_std_fs_smoke_on_clr() {
    if !command_exists("dotnet") {
        return;
    }

    let project_dir = workspace_examples_root().join("test.fs");
    if !project_dir.exists() {
        eprintln!("skip runtime smoke: missing {}", project_dir.display());
        return;
    }

    let output_dir = project_dir.join("dist").join("clr");
    let smoke_file = project_dir.join("test.fs.smoke.txt");
    let _ = fs::remove_file(&smoke_file);

    assert_build_and_run(&project_dir, &output_dir, CanonicalTarget::clr(), "clr");
    assert_eq!(fs::read_to_string(&smoke_file).unwrap(), "fs smoke ok");
    let _ = fs::remove_file(smoke_file);
}

fn assert_build_and_run(project_dir: &Path, output_dir: &Path, target: CanonicalTarget, target_name: &str) {
    let build_status = run_build(&BuildArgs {
        project_dir: project_dir.to_path_buf(),
        target: target.clone(),
        output_dir: Some(output_dir.to_path_buf()),
        workspace: false,
        debug_artifacts: false,
    })
    .unwrap();
    assert_eq!(build_status, ExitCode::SUCCESS, "build failed for {target_name}");

    let run_status = run_project(&RunArgs {
        project_dir: project_dir.to_path_buf(),
        target,
        output_dir: Some(output_dir.to_path_buf()),
        workspace: false,
        runner: Vec::new(),
        artifact: None,
        dry_run: false,
        debug_artifacts: false,
        workload_json: None,
        workload_file: None,
    })
    .unwrap();
    assert_eq!(run_status, ExitCode::SUCCESS, "run failed for {target_name}");
}

fn runtime_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("runtime_smoke")
}

fn command_exists(command: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|dir| {
            let base = dir.join(command);
            base.is_file()
                || [".exe", ".cmd", ".bat", ".com"].iter().map(|ext| dir.join(format!("{command}{ext}"))).any(|candidate| candidate.is_file())
        })
    })
}

fn workspace_examples_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("..").join("..").join("valkyrie.v").join("examples")
}
