mod support;

use legion::cmds::{
    build::{BuildArgs, run},
    run::ExecutionManifest,
};
use rusty_valkyrie::CanonicalTarget;
use std::{env, fs, path::Path, process::Command};
use support::create_smoke_project_with_manifest;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bootstrap_node/entry_contract_canonical")
}

#[test]
fn bootstrap_node_entry_fixture_wired_from_repository() {
    let fixture_root = fixture_root();
    let manifest = fs::read_to_string(fixture_root.join("legion.von")).expect("fixture legion.von");
    let source = fs::read_to_string(fixture_root.join("source/main.v")).expect("fixture main.v");
    let fixture = create_smoke_project_with_manifest("bootstrap-node-repo-fixture", &manifest, &source);
    let output_dir = fixture.project_dir.join("dist").join("node-bootstrap");
    let status = run(&BuildArgs {
        project_dir: fixture.project_dir.clone(),
        target: CanonicalTarget::parse("node").unwrap(),
        output_dir: Some(output_dir.clone()),
        workspace: false,
        debug_artifacts: false,
    })
    .unwrap();
    assert_eq!(status, std::process::ExitCode::SUCCESS);
    assert!(output_dir.join("legion.mjs").exists());
    assert!(output_dir.join("legion.wasm").exists());
    assert!(!output_dir.join("legion_tools.mjs").exists());
}

#[test]
fn bootstrap_node_entry_contract_uses_canonical_legion_artifacts() {
    let fixture = create_smoke_project_with_manifest(
        "bootstrap-node-entry",
        r#"{
    name: "legion",
    version: "0.1.0",
    build: [
        { target: "node" }
    ]
}"#,
        r#"[main]
micro legion(args: [utf8]) -> i32 {
    return 0;
}
"#,
    );
    let output_dir = fixture.project_dir.join("dist").join("node-bootstrap");
    let status = run(&BuildArgs {
        project_dir: fixture.project_dir.clone(),
        target: CanonicalTarget::parse("node").unwrap(),
        output_dir: Some(output_dir.clone()),
        workspace: false,
        debug_artifacts: false,
    })
    .unwrap();
    assert_eq!(status, std::process::ExitCode::SUCCESS);
    assert!(output_dir.join("legion.mjs").exists(), "expected legion.mjs");
    assert!(output_dir.join("legion.wasm").exists(), "expected legion.wasm");
    let manifest = ExecutionManifest::read_from_output_dir(&output_dir).expect("read execution manifest").expect("execution manifest present");
    assert!(
        manifest.run_contracts.iter().any(|contract| contract.physical_entry == "legion.mjs"),
        "expected legion.mjs physical entry, got {:?}",
        manifest.run_contracts
    );
}

#[test]
fn bootstrap_node_cli_runtime_smoke() {
    let fixture = create_smoke_project_with_manifest(
        "bootstrap-node-cli",
        r#"{
    name: "legion",
    version: "0.1.0",
    build: [
        { target: "node" }
    ]
}"#,
        r#"[main]
micro legion(args: [utf8]) -> i32 {
    return 0;
}
"#,
    );
    let output_dir = fixture.project_dir.join("dist").join("node-cli");
    run(&BuildArgs {
        project_dir: fixture.project_dir.clone(),
        target: CanonicalTarget::parse("node").unwrap(),
        output_dir: Some(output_dir.clone()),
        workspace: false,
        debug_artifacts: false,
    })
    .unwrap();
    let launcher = output_dir.join("legion.mjs");
    assert!(launcher.exists(), "missing launcher at {}", launcher.display());
    if cfg!(windows) {
        // Windows 临时目录 + Command 传参在部分宿主上会导致 node 落入 smoke 路径；
        // CLI 壳行为在 dist 手测与 bootstrap-node.mjs 门禁覆盖。
        return;
    }
    let version = Command::new("node").arg(&launcher).arg("--version").output().expect("node --version");
    assert!(version.status.success(), "stderr={}", String::from_utf8_lossy(&version.stderr));
    let help = Command::new("node").arg(&launcher).arg("--help").output().expect("node --help");
    assert!(help.status.success(), "stderr={}", String::from_utf8_lossy(&help.stderr));
}
