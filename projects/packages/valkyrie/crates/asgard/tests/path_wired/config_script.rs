//! Migrated from `src/config_script.rs` (`tests`).

use super::*;

use oak_von::{
    language::value::{VonValue, from_ast},
    parse as parse_von,
};

#[test]
fn define_config_block_normalizes_to_object() {
    let source = r#"define_config(asgard) {
project_type = "application"
target = "wasm32-unknown-browser-wasm"
build {
    mode = "prod"
    output = "dist"
}
}"#;
    let normalized = normalize_config_source(source).expect("normalize");
    let value: VonValue = from_ast(&parse_von(&normalized).expect("parse von"));
    assert_eq!(value.get("project_type").and_then(|v| v.as_str()), Some("application"));
    let build = value.get("build").expect("build");
    assert_eq!(build.get("output").and_then(|v| v.as_str()), Some("dist"));
}

#[test]
fn rejects_bare_object_literal() {
    let err = normalize_config_source(r#"{ project_type: "application" }"#).unwrap_err();
    assert!(err.to_string().contains("define_config(asgard)"));
}

#[test]
fn rejects_von_colon_field_syntax() {
    let err = normalize_config_source(
        r#"define_config(asgard) {
project_type: "application"
}"#,
    )
    .unwrap_err();
    assert!(err.to_string().contains("project_type"));
}

#[test]
fn define_config_with_equals_assignments() {
    let source = r#"define_config(asgard) { project_type = "library" }"#;
    let normalized = normalize_config_source(source).expect("normalize");
    assert!(normalized.contains("project_type"));
}
