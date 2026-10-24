//! `legion doc` 集成冒烟测试。

use std::path::PathBuf;

use legion::cmds::doc::generate_for_project;

#[test]
fn doc_smoke_legion_tools_fixture() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = manifest_dir.join("../../../../valkyrie.v/projects/legion._/projects/legion.tools");
    if !fixture.join("documentation/pages/zh-hans/workflow.md").is_file() {
        return;
    }

    let output = tempfile::tempdir().expect("tempdir");
    generate_for_project(&fixture, output.path()).expect("legion doc generate");

    let workflow_html = output.path().join("doc").join("用户文档").join("workflow.html");
    assert!(workflow_html.is_file(), "missing {}", workflow_html.display());

    let content = std::fs::read_to_string(&workflow_html).expect("read workflow.html");
    assert!(content.contains("工作流"), "workflow page should contain title");
    assert!(content.contains("legion doc"), "page should mention generator");

    let hub = output.path().join("index.html");
    assert!(hub.is_file());
    let hub_content = std::fs::read_to_string(hub).expect("read hub");
    assert!(hub_content.contains("doc/index.html"));
    assert!(!hub_content.contains("asgard-runtime.js"));
}
