//! 源码 inventory 合同：target 只能选择表示与 adaptor，不能决定语言声明是否存在。

use std::fs;

use legion_workspace::planner::*;
use rusty_valkyrie::CanonicalTarget;
use tempfile::tempdir;

#[test]
fn collect_source_files_includes_root_v_without_manifest_entry() {
    let directory = tempdir().expect("project directory");
    fs::write(directory.path().join("solution.v"), "[export] micro solve() -> unit {}").expect("solution");
    let files = collect_source_files(directory.path()).expect("collect root sources");
    assert_eq!(files, vec![directory.path().join("solution.v")]);
}

#[test]
fn semantic_source_groups_do_not_depend_on_target() {
    let directory = tempdir().expect("project directory");
    fs::create_dir_all(directory.path().join("source/nested")).expect("sources");
    fs::write(directory.path().join("source/main.v"), "[export] micro main() -> i32 { return 23 }").expect("source");
    fs::write(directory.path().join("source/nested/extra.v"), "micro extra() -> i32 { return 1 }").expect("source");
    fs::write(
        directory.path().join("legion.von"),
        r#"{
    name: "inventory-app",
    artifact: "library",
    build: [
        { target: "ovm" },
        { target: "node" }
    ]
}"#,
    )
    .expect("manifest");

    let workspace = WorkspaceResolver::discover_for_project(directory.path()).expect("workspace");
    let groups_for = |target: &str| {
        let request = BuildRequest {
            project_dir: directory.path().to_path_buf(),
            target: CanonicalTarget::parse(target).expect("target"),
            output_dir: None,
        };
        workspace
            .build_plan(&request)
            .expect("build plan")
            .project
            .semantic_source_groups
            .into_iter()
            .map(|group| (group.dependency_key, group.source_files))
            .collect::<Vec<_>>()
    };

    // 同一个工程的声明集合必须与 target 无关；只有表示、adaptor 与 backend private plan 可以不同。
    assert_eq!(groups_for("ovm"), groups_for("node"));
}
