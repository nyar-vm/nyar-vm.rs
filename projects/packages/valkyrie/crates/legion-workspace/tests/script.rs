//! Migrated from `src/script.rs` (`tests`).

use legion_workspace::{manifest::ProjectManifest, script::*};
use std::{fs, path::Path};

#[test]
fn extracts_embedded_manifest_from_comment_fence() {
    let source = r#"#!/usr/bin/env legion run
# ```legion
# {
#     name: "demo",
#     build: [{ target: "node" }]
# }
# ```

namespace demo;
"#;
    let manifest = extract_embedded_manifest(source, Path::new("demo.v")).unwrap().unwrap();
    assert!(manifest.contains("name: \"demo\""));
    let parsed = ProjectManifest::parse(&manifest).unwrap();
    assert_eq!(parsed.name, "demo");
}
