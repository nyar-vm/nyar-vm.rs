//! Migrated from `src/codegen/html_render.rs` (`tests`).

use super::*;

use crate::awsl::{LoweringOptions, lower_component};
use oak_awsl::AwslParser;

#[test]
fn static_render_simple_component() {
    let source = r#"<widget>
<div>Hello World</div>
</widget>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    let component = lower_component(&root, "HelloWorld", "hello.awsl", &LoweringOptions::default());
    let result = render_static_ir(&component, &Value::Null);
    assert!(result.html.contains("Hello World"));
    assert!(result.html.contains("data-asgard-static=\"HelloWorld\""));
    assert_eq!(result.scope, "asgard-static-hello-world");
}

#[test]
fn static_render_interpolation() {
    let source = r#"<widget>
<h1>{title}</h1>
</widget>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    let component = lower_component(&root, "Page", "page.awsl", &LoweringOptions::default());
    let data = serde_json::json!({ "title": "文档首页" });
    let result = render_static_ir(&component, &data);
    assert!(result.html.contains("文档首页"));
}

#[test]
fn static_render_loop_items() {
    let source = r#"<widget>
<loop item in items>
<a :href="item.href">{item.title}</a>
</loop>
</widget>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    let component = lower_component(&root, "List", "list.awsl", &LoweringOptions::default());
    let data = serde_json::json!({
        "items": [
            { "href": "/a", "title": "A" },
            { "href": "/b", "title": "B" }
        ]
    });
    let result = render_static_ir(&component, &data);
    assert!(result.html.contains("href=\"/a\""));
    assert!(result.html.contains(">A<"));
    assert!(result.html.contains("href=\"/b\""));
}

#[test]
fn static_render_link_component() {
    let source = r#"<widget>
<Link href="/docs">文档</Link>
</widget>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    let component = lower_component(&root, "Nav", "nav.awsl", &LoweringOptions::default());
    let result = render_static_ir(&component, &Value::Null);
    assert!(result.html.contains("<a href=\"/docs\">"));
}

#[test]
fn static_render_if_else_from_script_binding() {
    let source = r#"<widget>
<if visible>
    <div>Shown</div>
<else/>
    <div>Hidden</div>
</if>
</widget>
<script>
let visible: bool = false
</script>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    let component = lower_component(&root, "Panel", "panel.awsl", &LoweringOptions::default());
    let result = render_static_ir(&component, &Value::Null);
    assert!(result.html.contains("Hidden"));
    assert!(!result.html.contains(">Shown<"));
}

#[test]
fn static_render_loop_string_items() {
    let source = r#"<widget>
<loop item in items>
<span>{item}</span>
</loop>
</widget>
<script>
let items: list = ["a", "b", "c"]
</script>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    let component = lower_component(&root, "Tags", "tags.awsl", &LoweringOptions::default());
    let result = render_static_ir(&component, &serde_json::json!({ "items": ["a", "b", "c"] }));
    assert!(result.html.contains(">a<"));
    assert!(result.html.contains(">b<"));
    assert!(result.html.contains(">c<"));
}

#[test]
fn static_render_class_ternary() {
    let source = r#"<widget>
<a href="/x" @class="active ? 'active' : ''">Link</a>
</widget>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    let component = lower_component(&root, "Nav", "nav.awsl", &LoweringOptions::default());
    let result = render_static_ir(&component, &serde_json::json!({ "active": true }));
    assert!(result.html.contains("class=\"active\""));
}

#[test]
fn static_render_nav_sections_nested() {
    let source = r#"<widget doc_page>
<loop section in sections>
<div class="vp-nav-section">
    <span class="vp-nav-section-title">{section.title}</span>
    <loop page in section.pages>
        <a :href="page.href" @class="page.link_class">{page.title}</a>
    </loop>
</div>
</loop>
</widget>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    let component = lower_component(&root, "doc-page", "doc-page.awsl", &LoweringOptions::default());
    let data = serde_json::json!({
        "sections": [
            {
                "title": "指南",
                "pages": [
                    { "title": "工作流", "href": "workflow.html", "link_class": "vp-nav-link active" },
                    { "title": "架构", "href": "architecture.html", "link_class": "vp-nav-link" }
                ]
            }
        ]
    });
    let result = render_static_ir(&component, &data);
    assert!(result.html.contains("vp-nav-section-title"));
    assert!(result.html.contains("指南"));
    assert!(result.html.contains("vp-nav-link active"));
    assert!(result.html.contains("工作流"));
}

#[test]
fn static_render_is_active_helper() {
    let source = r#"<widget>
<loop page in pages>
<a :href="page.href" @class="isActive(page.href) ? 'active' : ''">{page.title}</a>
</loop>
</widget>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    let component = lower_component(&root, "Nav", "nav.awsl", &LoweringOptions::default());
    let data = serde_json::json!({
        "__current_path": "workflow.html",
        "pages": [
            { "title": "工作流", "href": "workflow.html" },
            { "title": "架构", "href": "architecture.html" }
        ]
    });
    let result = render_static_ir(&component, &data);
    assert!(result.html.contains("class=\"active\""));
    assert!(result.html.contains("工作流"));
    assert!(result.html.contains("href=\"architecture.html\""));
    assert!(!result.html.contains("architecture.html\" class=\"active\""));
}

#[test]
fn static_render_string_concat_in_attr() {
    let source = r###"<widget>
<loop section in sections>
<a :href="'#' + section.id">{section.title}</a>
</loop>
</widget>"###;
    let root = AwslParser::parse_root(source).expect("parse");
    let component = lower_component(&root, "Nav", "nav.awsl", &LoweringOptions::default());
    let data = serde_json::json!({
        "sections": [
            { "id": "button", "title": "Button" },
            { "id": "card", "title": "Card" }
        ]
    });
    let result = render_static_ir(&component, &data);
    assert!(result.html.contains("href=\"#button\""));
    assert!(result.html.contains(">Button<"));
}

#[test]
fn static_render_if_hides_empty_section_title() {
    let source = r#"<widget>
<loop section in sections>
    <if section.title != ''>
        <span>{section.title}</span>
    </if>
</loop>
</widget>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    let component = lower_component(&root, "Side", "side.awsl", &LoweringOptions::default());
    let data = serde_json::json!({
        "sections": [
            { "title": "指南", "pages": [] },
            { "title": "", "pages": [] }
        ]
    });
    let result = render_static_ir(&component, &data);
    assert!(result.html.contains("指南"));
    assert_eq!(result.html.matches("指南").count(), 1);
}

#[test]
fn static_render_nav_items_with_active_class() {
    let source = r#"<widget doc_page>
<loop item in nav_items>
<a :href="item.href" @class="item.link_class">{item.title}</a>
</loop>
</widget>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    let component = lower_component(&root, "doc-page", "doc-page.awsl", &LoweringOptions::default());
    let data = serde_json::json!({
        "nav_items": [
            { "title": "工作流", "href": "workflow.html", "link_class": "vp-nav-link active" },
            { "title": "架构", "href": "architecture.html", "link_class": "vp-nav-link" }
        ]
    });
    let result = render_static_ir(&component, &data);
    assert!(result.html.contains("vp-nav-link active"));
    assert!(result.html.contains("工作流"));
}

#[test]
fn static_render_escapes_interpolation() {
    let source = r#"<widget>
<div>{msg}</div>
</widget>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    let component = lower_component(&root, "Msg", "msg.awsl", &LoweringOptions::default());
    let result = render_static_ir(&component, &serde_json::json!({ "msg": "<script>" }));
    assert!(!result.html.contains("<script>"));
    assert!(result.html.contains("&lt;"));
}
