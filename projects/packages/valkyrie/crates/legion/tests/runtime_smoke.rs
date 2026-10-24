mod support;

use std::path::{Path, PathBuf};
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

fn runtime_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("runtime_smoke")
}
