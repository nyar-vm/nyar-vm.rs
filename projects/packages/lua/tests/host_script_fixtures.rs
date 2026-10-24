//! Legend host script fixtures verified through legacy-vm interpretation.

use std::{collections::HashMap, fs, path::PathBuf};

use legacy_vm::LegacyVmRunner;
use nyar::{RuntimeFixtureResult, load_legend_fixture_manifest, resolve_legend_fixture_targets, verify_legend_fixture_case};

const INTERPRETED_LANGUAGES: &[&str] = &["bash", "lua", "tcl", "powershell", "c"];

fn regenerate_sidecars() -> bool {
    matches!(std::env::var("LEGEND_TEST_REGENERATE").ok().as_deref(), Some("1") | Some("true"))
        || matches!(std::env::var("NYAR_TEST_REGENERATE").ok().as_deref(), Some("1") | Some("true"))
}

#[test]
fn legend_host_script_fixtures_use_legacy_vm_targets() {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/manifest.yaml");
    let manifest = load_legend_fixture_manifest(&manifest_path);
    let base = manifest_path.parent().expect("manifest directory");

    for entry in &manifest.fixtures {
        if INTERPRETED_LANGUAGES.contains(&entry.language.as_str()) {
            for target in &entry.targets {
                assert_ne!(target, "nyar-vm", "interpreted host script '{}' must not target nyar-vm: {}", entry.language, entry.path);
            }
        }
    }

    let runner = LegacyVmRunner::new();
    let regenerate = regenerate_sidecars();

    for entry in &manifest.fixtures {
        let targets = resolve_legend_fixture_targets(entry, &["legacy-vm"]);
        if !targets.iter().any(|target| target == "legacy-vm") {
            continue;
        }

        let fixture_path = base.join(&entry.path);
        let source = fs::read_to_string(&fixture_path).unwrap_or_else(|error| panic!("failed to read {}: {error}", fixture_path.display()));

        verify_legend_fixture_case(&fixture_path, entry, regenerate, |target| {
            assert_eq!(target, "legacy-vm");
            let mut env = HashMap::new();
            let value = runner
                .run(&entry.language, &source, &mut env)
                .unwrap_or_else(|error| panic!("failed to run {}: {error}", fixture_path.display()));
            RuntimeFixtureResult {
                success: true,
                stdout: vec![value.to_string_value()],
                stderr: Vec::new(),
                allow_stderr: false,
                errors: Vec::new(),
                result: Some(0),
            }
        });
    }
}
