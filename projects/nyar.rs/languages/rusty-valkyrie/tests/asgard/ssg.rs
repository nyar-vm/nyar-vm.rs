//! Migrated from `src/ssg/mod.rs` (`tests`).

use rusty_valkyrie::asgard::{codegen::StaticRenderResult, ssg::*};

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

/// 属性值由表达式求值后进入 HTML，必须同时转义标签字符与双引号，
/// 否则插值内容可以提前闭合属性或注入新属性。
#[test]
fn static_page_escapes_expression_attribute_quotes() {
    let source = r#"<widget>
<div :title="value">text</div>
</widget>"#;
    let result = render_awsl_static(source, "AttrSafe", "attr.awsl", &serde_json::json!({ "value": "<x>&\"" }));
    assert!(result.html.contains("title=\"&lt;x&gt;&amp;&quot;\""), "属性插值必须完整转义:\n{}", result.html);
    assert!(!result.html.contains("<x>"), "属性插值不得注入标签:\n{}", result.html);
}

/// Unicode 标量插值必须原样保留，不按 UTF-16 码元截断。
#[test]
fn static_page_keeps_unicode_interpolation() {
    let source = r#"<widget>
<p>{msg}</p>
</widget>"#;
    let result = render_awsl_static(source, "Unicode", "unicode.awsl", &serde_json::json!({ "msg": "你好 🌍" }));
    assert!(result.html.contains("你好 🌍"), "Unicode 插值不得被截断或转义成实体:\n{}", result.html);
}

/// 多根节点按声明顺序输出，不发生重排或丢失。
#[test]
fn static_page_emits_multiple_root_nodes_in_order() {
    let source = r#"<widget>
<div>first</div>
<div>second</div>
</widget>"#;
    let result = render_awsl_static(source, "Roots", "roots.awsl", &serde_json::Value::Null);
    let first = result.html.find("first").expect("first root");
    let second = result.html.find("second").expect("second root");
    assert!(first < second, "多根节点必须按声明顺序输出:\n{}", result.html);
}
