//! Migrated from `src/unity_export.rs` (`tests`).

use super::*;

use std::fs;

#[test]
fn artifact_from_dll_name_maps_export_partition_suffix() {
    let artifact = artifact_from_dll_name("valkyrie.unity", "valkyrie.unity__unity.runtime.dll").expect("artifact");
    assert_eq!(artifact.partition.as_deref(), Some("unity.runtime"));
    assert_eq!(artifact.role, "plugin-runtime");
    assert_eq!(artifact_destination_name(&artifact), "Valkyrie.Unity.Runtime.dll");
}

#[test]
fn sync_msil_with_routes_places_partition_dlls() {
    let temp = std::env::temp_dir().join(format!("valkyrie-export-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp);
    let msil_dir = temp.join("msil");
    let unity_root = temp.join("unity");
    fs::create_dir_all(&msil_dir).unwrap();
    fs::write(msil_dir.join("valkyrie.unity__unity.runtime.dll"), b"runtime").unwrap();
    fs::write(
        msil_dir.join("valkyrie-export.json"),
        br#"{
  "version": 2,
  "artifacts": [
{ "assembly": "valkyrie.unity__unity.runtime.dll", "partition": "unity.runtime", "role": "plugin-runtime" }
  ]
}"#,
    )
    .unwrap();

    let mut routes = BTreeMap::new();
    routes.insert("unity.runtime".to_string(), "Packages/com.valkyrie.unity/Runtime".to_string());
    sync_msil_with_routes(&msil_dir, &unity_root, &routes).unwrap();
    assert!(unity_root.join("Packages/com.valkyrie.unity/Runtime/Valkyrie.Unity.Runtime.dll").is_file());
    let _ = fs::remove_dir_all(&temp);
}
