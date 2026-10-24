//! Migrated from `src/cmds/doc/markdown.rs` (`tests`).

use super::*;

#[test]
fn renders_heading_and_paragraph() {
    let html = render_markdown_to_html_simple("# Hello\n\nWorld");
    assert!(html.contains("<h1"));
    assert!(html.contains("Hello"));
    assert!(html.contains("<p>"));
}

#[test]
fn extracts_title() {
    assert_eq!(extract_title("# 工作流\n\nbody"), Some("工作流".into()));
}

#[test]
fn rewrites_md_link_with_context() {
    let ctx = DocRenderContext::from_relative_md("guide/foo.md");
    let result = render_markdown_to_html("[x](../other.md)", &ctx);
    assert!(result.html.contains(".html"));
}
