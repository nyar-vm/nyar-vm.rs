use legion_workspace::{LEGION_PROJECT_LAYOUT, bootstrap_entry_aliases};
use nyar_package_manager::{PackMeta, RegistryPackOptions, pack_registry_artifact, unpack};
use std::fs;

#[test]
fn pack_resolves_historical_legion_legion_entry_to_canonical_mjs() {
    let dir = tempfile::tempdir().expect("temp");
    let artifact = dir.path().join("dist");
    fs::create_dir_all(&artifact).expect("mkdir");
    // Canonical on-disk names after Node bootstrap rename.
    fs::write(artifact.join("legion.mjs"), "export {}").expect("write mjs");
    fs::write(artifact.join("legion.wasm"), b"\0asm").expect("write wasm");
    // Historical contract still points at the mangled physical entry.
    fs::write(
        artifact.join("run-contracts.txt"),
        r#"{
    run_contracts: [
        {
            logical_entry: "legion::legion",
            physical_entry: "legion_legion.mjs",
            invocation: "node",
            validate: "node legion_legion.mjs"
        }
    ]
}
"#,
    )
    .expect("write contract");

    let packed = pack_registry_artifact(&RegistryPackOptions {
        artifact_dir: artifact,
        package_root: dir.path().to_path_buf(),
        registry: "npm".to_string(),
        meta: PackMeta {
            name: "@valkyrie-language/legion".to_string(),
            version: "1.0.0".to_string(),
            description: "legion".to_string(),
            license: Some("MIT".to_string()),
        },
        include_files: Vec::new(),
        flat_layout: false,
        layout: LEGION_PROJECT_LAYOUT,
    })
    .expect("pack with Legion aliases");

    let unpack_dir = dir.path().join("unpacked");
    fs::create_dir_all(&unpack_dir).expect("unpack dir");
    unpack(&packed.tarball_data, &unpack_dir).expect("unpack");
    let package_json = fs::read_to_string(unpack_dir.join("package.json")).expect("package.json");
    assert!(package_json.contains("legion.mjs"), "expected canonical legion.mjs in package.json:\n{package_json}");
    assert!(!package_json.contains("legion_legion.mjs"));
    assert!(package_json.contains("\"legion\""));
    assert!(unpack_dir.join("legion.mjs").is_file());
    assert!(unpack_dir.join("legion.wasm").is_file());
}

#[test]
fn bootstrap_entry_aliases_cover_mjs_and_wasm() {
    assert_eq!(bootstrap_entry_aliases("legion_legion.mjs"), &["legion.mjs"]);
    assert_eq!(bootstrap_entry_aliases("legion_legion.wasm"), &["legion.wasm"]);
    assert_eq!(bootstrap_entry_aliases("legion__main_legion.mjs"), &["legion.mjs"]);
    assert_eq!(bootstrap_entry_aliases("legion__main_legion.wasm"), &["legion.wasm"]);
    assert!(bootstrap_entry_aliases("legion.mjs").is_empty());
    assert!(bootstrap_entry_aliases("demo_cli.mjs").is_empty());
}
