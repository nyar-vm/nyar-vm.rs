pub mod run_fixture;
pub mod runtime_fixture;

use std::{
    fs,
    path::{Path, PathBuf},
};

use tempfile::TempDir;

pub struct SmokeProject {
    _temp_dir: TempDir,
    pub project_dir: PathBuf,
}

pub fn create_smoke_project(prefix: &str) -> SmokeProject {
    create_fixture(
        prefix,
        default_build_manifest(),
        r#"[main]
micro main(): i64 {
    return 0
}
"#,
    )
}

pub fn create_smoke_project_with_source(prefix: &str, source: &str) -> SmokeProject {
    create_fixture(prefix, default_build_manifest(), source)
}

pub fn create_smoke_project_with_build(prefix: &str, build_block: &str, source: &str) -> SmokeProject {
    create_fixture(prefix, build_block, source)
}

pub fn create_smoke_project_with_manifest(prefix: &str, manifest: &str, source: &str) -> SmokeProject {
    create_manifest_fixture(prefix, manifest, source, true)
}

pub fn create_local_package_project_with_manifest(prefix: &str, manifest: &str, source: &str) -> SmokeProject {
    create_manifest_fixture(prefix, manifest, source, false)
}

pub fn create_script_project_with_manifest(prefix: &str, manifest: &str, source: &str) -> SmokeProject {
    create_standalone_manifest_fixture(prefix, manifest, source)
}

pub fn create_nested_workspace_member_project_with_manifest(prefix: &str, manifest: &str, source: &str) -> SmokeProject {
    create_nested_workspace_fixture(prefix, Path::new("subspace").join("projects").join("app"), manifest, source)
}

pub fn create_local_package_project(prefix: &str, build_block: &str, source: &str) -> SmokeProject {
    let temp_dir = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
    let root = temp_dir.path();
    let project_dir = root.join("app");
    let source_dir = project_dir.join("source");
    fs::create_dir_all(&source_dir).unwrap();
    fs::write(
        root.join("legions.von"),
        r#"{
    name: "runtime-smoke",
    members: []
}
"#,
    )
    .unwrap();
    fs::write(
        project_dir.join("legion.von"),
        format!(
            r#"{{
    name: "app",
    build: [
        {}
    ]
}}
"#,
            build_block
        ),
    )
    .unwrap();
    fs::write(source_dir.join("main.v"), source).unwrap();

    SmokeProject { _temp_dir: temp_dir, project_dir: canonicalize_lossy(&project_dir) }
}

fn create_fixture(prefix: &str, build_block: &str, source: &str) -> SmokeProject {
    let manifest = format!(
        r#"{{
    name: "app",
    build: [
        {}
    ]
}}
"#,
        build_block
    );
    create_manifest_fixture(prefix, &manifest, source, true)
}

fn create_manifest_fixture(prefix: &str, manifest: &str, source: &str, include_workspace_member: bool) -> SmokeProject {
    create_nested_manifest_fixture(prefix, PathBuf::from("app"), manifest, source, include_workspace_member)
}

fn create_nested_manifest_fixture(
    prefix: &str,
    project_relative_dir: PathBuf,
    manifest: &str,
    source: &str,
    include_workspace_member: bool,
) -> SmokeProject {
    let temp_dir = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
    let root = temp_dir.path();
    let project_dir = root.join(&project_relative_dir);
    let source_dir = project_dir.join("source");
    fs::create_dir_all(&source_dir).unwrap();
    let workspace_member = project_relative_dir.to_string_lossy().replace('\\', "/");
    let workspace_manifest = if include_workspace_member {
        format!(
            r#"{{
    name: "runtime-smoke",
    members: [
        "{}"
    ]
}}
"#,
            workspace_member
        )
    }
    else {
        r#"{
    name: "runtime-smoke",
    members: []
}
"#
        .to_string()
    };
    fs::write(root.join("legions.von"), workspace_manifest).unwrap();
    fs::write(project_dir.join("legion.von"), manifest).unwrap();
    fs::write(source_dir.join("main.v"), source).unwrap();

    SmokeProject { _temp_dir: temp_dir, project_dir: canonicalize_lossy(&project_dir) }
}

fn create_nested_workspace_fixture(prefix: &str, project_relative_dir: PathBuf, manifest: &str, source: &str) -> SmokeProject {
    let temp_dir = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
    let root = temp_dir.path();
    let subspace_dir = root.join("subspace");
    let project_dir = root.join(&project_relative_dir);
    let source_dir = project_dir.join("source");
    let nested_member = project_relative_dir
        .strip_prefix("subspace")
        .unwrap_or(project_relative_dir.as_path())
        .to_string_lossy()
        .trim_start_matches(['\\', '/'])
        .replace('\\', "/");
    fs::create_dir_all(&source_dir).unwrap();
    fs::write(
        root.join("legions.von"),
        r#"{
    name: "runtime-smoke",
    members: [
        "subspace"
    ]
}
"#,
    )
    .unwrap();
    fs::write(
        subspace_dir.join("legions.von"),
        format!(
            r#"{{
    name: "nested-runtime-smoke",
    members: [
        "{nested_member}"
    ]
}}
"#
        ),
    )
    .unwrap();
    fs::write(project_dir.join("legion.von"), manifest).unwrap();
    fs::write(source_dir.join("main.v"), source).unwrap();

    SmokeProject { _temp_dir: temp_dir, project_dir: canonicalize_lossy(&project_dir) }
}

fn create_root_manifest_fixture(prefix: &str, manifest: &str, source: &str, include_workspace_member: bool) -> SmokeProject {
    let temp_dir = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
    let root = temp_dir.path().to_path_buf();
    let source_dir = root.join("source");
    fs::create_dir_all(&source_dir).unwrap();
    let workspace_manifest = if include_workspace_member {
        r#"{
    name: "runtime-smoke",
    members: [
        "."
    ]
}
"#
    }
    else {
        r#"{
    name: "runtime-smoke",
    members: []
}
"#
    };
    fs::write(root.join("legions.von"), workspace_manifest).unwrap();
    fs::write(root.join("legion.von"), manifest).unwrap();
    fs::write(source_dir.join("main.v"), source).unwrap();

    SmokeProject { _temp_dir: temp_dir, project_dir: canonicalize_lossy(&root) }
}

fn create_standalone_manifest_fixture(prefix: &str, manifest: &str, source: &str) -> SmokeProject {
    let temp_dir = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
    let root = temp_dir.path().to_path_buf();
    let source_dir = root.join("source");
    fs::create_dir_all(&source_dir).unwrap();
    fs::write(root.join("legion.von"), manifest).unwrap();
    fs::write(source_dir.join("main.v"), source).unwrap();

    SmokeProject { _temp_dir: temp_dir, project_dir: canonicalize_lossy(&root) }
}

fn default_build_manifest() -> &'static str {
    r#"{
            target: "clr",
            msil: true
        }"#
}

/// 规范化路径（失败时保留原路径）。
fn canonicalize_lossy(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
