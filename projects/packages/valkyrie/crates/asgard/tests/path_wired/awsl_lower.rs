//! Migrated from `src/awsl/lower.rs` (`tests`).

use super::*;

use crate::awsl::is_fragment_root;
use oak_awsl::AwslParser;

fn expect_element<'a>(node: &'a AwslTemplateNode) -> &'a AwslElement {
    match node {
        AwslTemplateNode::Element(element) => element,
        other => panic!("expected element, got {other:?}"),
    }
}

use crate::awsl::render_ir::{RenderAttrValue, RenderModule, RenderNode};

fn lower_source(source: &str) -> LoweredComponent {
    let root = AwslParser::parse_root(source).expect("parse");
    lower_component(&root, "Test", "test.awsl", &LoweringOptions::default())
}

fn has_loop(module: &RenderModule) -> bool {
    module.nodes.iter().any(|node| matches!(node, RenderNode::Loop(_)))
}

fn first_element_attrs(module: &RenderModule) -> Vec<crate::awsl::RenderAttr> {
    module
        .nodes
        .iter()
        .find_map(|node| match node {
            RenderNode::Element(element) => Some(element.attrs.clone()),
            _ => None,
        })
        .expect("element")
}

#[test]
fn rewrites_single_line_handler_for_reactive_signals() {
    let lowered = lower_source(
        r#"<widget counter>
<Text>{count}</Text>
<Button @click="on_tap">+1</Button>
</widget>
<script>
let mut count: i32 = 0
micro on_tap() { count = count + 1 }
</script>"#,
    );
    assert!(lowered.synthetic_v.contains("micro on_tap()"));
    assert!(lowered.synthetic_v.contains("sig_set_i32"));
    assert!(lowered.synthetic_v.contains("sig_get_i32"));
    assert!(!lowered.synthetic_v.contains("count = count"));
}

#[test]
fn accepts_loop_item_in_expr() {
    let lowered = lower_source(
        r#"<widget>
<loop item in items>
    <span>{item}</span>
</loop>
</widget>"#,
    );
    assert!(has_loop(&lowered.render_ir));
}

#[test]
fn style_directive_merges_with_existing_class() {
    let lowered = lower_source(
        r#"<widget>
<div class="base" @style="flex w-4"></div>
</widget>"#,
    );
    let attrs = first_element_attrs(&lowered.render_ir);
    assert_eq!(attrs.iter().filter(|a| a.name == "class").count(), 1);
    let class = attrs.iter().find(|a| a.name == "class").expect("class");
    match &class.value {
        RenderAttrValue::Static(text) => {
            assert!(text.contains("base"));
            assert!(text.contains("flex"));
            assert!(text.contains("w-4"));
        }
        other => panic!("expected merged static class, got {other:?}"),
    }
    let collector = crate::tailwind::collect_from_components(&[lowered]);
    assert!(collector.utilities().any(|u| u == "flex"));
    assert!(collector.utilities().any(|u| u == "w-4"));
    assert!(collector.utilities().any(|u| u == "base"));
}

#[test]
fn rejects_for_each_tag() {
    let lowered = lower_source(
        r#"<widget>
<for each={items}>
    <span>{item}</span>
</for>
</widget>"#,
    );
    assert!(!has_loop(&lowered.render_ir));
}

#[test]
fn rejects_at_for_directive() {
    let error = AwslParser::parse_root("<widget><ul @for={item in items}><li /></ul></widget>").expect_err("@for");
    assert!(error.message.contains("@for is not supported"));
}

#[test]
fn if_directive_uses_bare_expr() {
    let source = "<widget><card @if=\"isVisible\"><text>ok</text></card></widget>";
    let root = AwslParser::parse_root(source).expect("parse @if");
    let card = expect_element(&root.template[0]);
    assert_eq!(card.directives.len(), 1);
    assert_eq!(card.directives[0].value.as_deref(), Some("isVisible"));
}

#[test]
fn rejects_braced_if_directive() {
    let error = AwslParser::parse_root("<widget><div @if={show} /></widget>").expect_err("braced @if");
    assert!(error.message.contains("quoted"));
}

#[test]
fn multi_root_widget_wraps_fragment() {
    let lowered = lower_source(
        r#"<widget>
<div class="a" />
<div class="b" />
</widget>"#,
    );
    assert!(is_fragment_root(&lowered.render_ir));
    let root = lowered.render_ir.roots[0];
    let RenderNode::Fragment(fragment) = lowered.render_ir.node(root)
    else {
        panic!("expected fragment root");
    };
    assert_eq!(lowered.render_ir.region(fragment.children).nodes.len(), 2);
}

#[test]
fn explicit_fragment_tag_lowers_transparently() {
    let lowered = lower_source(
        r#"<widget>
<fragment>
    <span>a</span>
    <span>b</span>
</fragment>
</widget>"#,
    );
    let root = lowered.render_ir.roots[0];
    let RenderNode::Fragment(fragment) = lowered.render_ir.node(root)
    else {
        panic!("expected fragment");
    };
    assert_eq!(lowered.render_ir.region(fragment.children).nodes.len(), 2);
}

#[test]
fn rejects_loop_without_item_in_header() {
    let lowered = lower_source(
        r#"<widget>
<loop each={items}>
    <span>{item}</span>
</loop>
</widget>"#,
    );
    assert!(!has_loop(&lowered.render_ir));
}
