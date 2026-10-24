mod support;

use std::path::{Path, PathBuf};
use support::run_fixture::{RunFixtureContext, can_run_fixture_context, verify_run_fixture_for_context};

const MONTHLY_BASELINE_FIXTURES: &[&str] = &["minimal", "minimal_func", "runtime_delegation", "subscript"];

fn run_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cmds_run")
}

fn bootstrap_legion_tools_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bootstrap_legion_tools")
}

#[test]
fn runs_four_target_monthly_baseline_fixtures() {
    let context = RunFixtureContext::new(&["clr", "jvm", "node", "wasi"]);
    if !can_run_fixture_context(&context) {
        return;
    }

    for fixture_name in MONTHLY_BASELINE_FIXTURES {
        let fixture_path = run_fixture_root().join(format!("{fixture_name}.valkyrie"));
        assert!(fixture_path.exists(), "missing baseline fixture {}", fixture_path.display());
        verify_run_fixture_for_context(&fixture_path, &context);
    }
}

#[test]
fn runs_bootstrap_legion_tools_minimal_cli_fixture() {
    let context = RunFixtureContext::new(&["clr", "jvm", "node", "wasi"]).with_project_name("legion");
    if !can_run_fixture_context(&context) {
        return;
    }

    let fixture_path = bootstrap_legion_tools_fixture_root().join("minimal_cli.valkyrie");
    let manifest = r#"{
    name: "legion",
    version: "0.1.0",
    build: [
        { target: "clr" },
        { target: "jvm" },
        { target: "node" },
        { target: "wasi" }
    ]
}"#;
    verify_run_fixture_for_context(&fixture_path, &context.with_manifest(manifest));
}
