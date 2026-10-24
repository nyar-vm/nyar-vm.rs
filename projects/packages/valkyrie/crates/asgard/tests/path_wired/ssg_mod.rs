//! Migrated from `src/ssg/mod.rs` (`tests`).

use super::*;

#[test]
fn wrap_with_layout_replaces_slot() {
    let layout = "<widget><main><slot /></main></widget>";
    let body = "<article>content</article>";
    let merged = wrap_with_layout(layout, body);
    assert!(merged.contains("<article>content</article>"));
    assert!(!merged.contains("<slot"));
}

#[test]
fn generate_static_page_has_no_runtime() {
    let result = StaticRenderResult { html: "<div>ok</div>".into(), css: ".x{color:red}".into(), scope: "asgard-static-test".into() };
    let page = generate_static_page("Test", &[result], "");
    assert!(page.contains("<!DOCTYPE html>"));
    assert!(page.contains("ok"));
    assert!(page.contains("color:red"));
    assert!(!page.contains("asgard-runtime.js"));
}

#[test]
fn render_awsl_static_hub() {
    let source = r#"<widget>
<h1>{title}</h1>
<a :href="doc_href">用户文档</a>
</widget>"#;
    let data = serde_json::json!({ "title": "文档", "doc_href": "doc/index.html" });
    let result = render_awsl_static(source, "Hub", "hub.awsl", &data);
    assert!(result.html.contains("文档"));
    assert!(result.html.contains("doc/index.html"));
}
