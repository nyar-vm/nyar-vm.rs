//! Migrated from `src/manifest.rs` (`tests`).

use legion_workspace::manifest::*;
use miette::Diagnostic;
use oak_core::{OakError as VonError, OakErrorKind};
use rusty_valkyrie::CanonicalTarget;
use std::{fs, path::Path};

#[test]
fn workspace_manifest_accepts_plain_string_name() {
    let manifest = WorkspaceManifest::parse(
        r#"{
name: "runtime-smoke",
members: ["app"]
}"#,
    )
    .expect("workspace manifest must accept plain string option fields");
    assert_eq!(manifest.name.as_deref(), Some("runtime-smoke"));
}

#[test]
fn project_manifest_accepts_workspace_dependency_string() {
    let manifest = ProjectManifest::parse(
        r#"{
name: "demo",
dependencies: { core: "workspace" }
}"#,
    )
    .expect("workspace dependency string must parse");
    assert_eq!(manifest.dependencies.get("core").map(DependencySpec::source_preference), Some(DependencySourcePreference::Workspace));
}

#[test]
fn project_manifest_accepts_sdk_vendor_block() {
    ProjectManifest::parse(
        r#"{
name: "demo",
sdk_vendor: {
organization: "valkyrie",
host: "browser",
kind: "platform-sdk",
targets: ["wasm32-unknown-browser-wasm"]
}
}"#,
    )
    .unwrap_or_else(|error| panic!("sdk_vendor block must parse: {error}"));
}

#[test]
fn project_manifest_accepts_wasm_build_target() {
    ProjectManifest::parse(
        r#"{
name: "demo",
build: [ { target: "wasm32-unknown-browser-wasm", source_map: true, typescript: true, wat: true } ]
}"#,
    )
    .unwrap_or_else(|error| panic!("wasm build target must parse: {error}"));
}

#[test]
fn project_manifest_accepts_engines_block() {
    ProjectManifest::parse(
        r#"{
name: "demo",
engines: { vcc: "0.1.*" }
}"#,
    )
    .unwrap_or_else(|error| panic!("engines block must parse: {error}"));
}

#[test]
fn project_manifest_accepts_std_adaptor_wasm_shape() {
    let manifest = ProjectManifest::parse(
        r#"{
name: "std.adaptor.wasm",
description: "Valkyrie WebAssembly SDK",
dependencies: { core: "workspace" },
sdk_vendor: {
organization: "valkyrie",
host: "browser",
kind: "platform-sdk",
targets: ["wasm32-unknown-browser-wasm"]
},
build: [
{
target: "wasm32-unknown-browser-wasm",
source_map: true,
typescript: true,
wat: true
}
],
engines: { vcc: "0.1.*" }
}"#,
    )
    .unwrap_or_else(|error| panic!("std.adaptor.wasm legion.von shape must parse: {error}"));
    assert_eq!(manifest.name, "std.adaptor.wasm");
    assert!(manifest.sdk_vendor.is_some());
}

#[test]
fn project_manifest_accepts_library_artifact_and_node_target() {
    let manifest = ProjectManifest::parse(
        r#"{
name: "leetcode-two-sum",
version: "0.1.0",
artifact: "library",
dependencies: { core: true, std: true },
build: [ { target: "node" } ]
}"#,
    )
    .expect("leetcode legion.von shape must parse");
    assert_eq!(manifest.artifact, ProjectArtifactKind::Library);
    assert_eq!(manifest.build[0].target, CanonicalTarget::parse("node").expect("node"));
}

#[test]
fn manifest_error_exposes_oak_source_offset_as_label() {
    let error =
        ManifestError::from(VonError::new(OakErrorKind::SyntaxError { message: "expected value".to_string(), offset: 12, source_id: None }));
    let labels: Vec<_> = Diagnostic::labels(&error).unwrap().collect();
    assert_eq!(labels.len(), 1);
    let ManifestError::Parse { span: Some(span), .. } = error
    else {
        panic!("expected parse error with span");
    };
    assert_eq!(span, 12..13);
}
