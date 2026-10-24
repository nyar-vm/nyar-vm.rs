//! Migrated from `src/cmds/doc/mod.rs` (`tests`).

use super::*;

#[test]
fn hub_template_renders() {
    let data = json!({
        "title": "文档",
        "doc_href": "doc/index.html",
        "doc_desc": "测试",
    });
    let result = asgard::ssg::render_awsl_static(assets::embedded_hub_awsl(), "hub", "hub.awsl", &data);
    assert!(result.html.contains("doc/index.html"));
}
