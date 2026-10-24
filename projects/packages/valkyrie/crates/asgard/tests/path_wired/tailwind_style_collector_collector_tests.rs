//! Migrated from `src/tailwind/style_collector.rs` (`collector_tests`).

use super::*;

use crate::awsl::{LoweringOptions, RenderAttrValue, RenderNode, lower_component};
use oak_awsl::AwslParser;

fn first_element_attrs(lowered: &crate::awsl::LoweredComponent) -> Option<&[crate::awsl::RenderAttr]> {
    let module = &lowered.render_ir;
    let root = module.roots.first().copied()?;
    match module.node(root) {
        RenderNode::Element(element) => Some(&element.attrs),
        RenderNode::Component(component) => Some(&component.attrs),
        _ => None,
    }
}

#[test]
fn collects_static_style_utilities() {
    let source = r#"<widget><div @style="flex w-4"></div></widget>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    let lowered = lower_component(&root, "demo", "demo.awsl", &LoweringOptions::default());
    let collector = collect_from_components(&[lowered]);
    assert!(collector.utilities().any(|u| u == "flex"));
    assert!(collector.utilities().any(|u| u == "w-4"));
}

#[test]
fn style_lowers_to_class_not_style_attr() {
    let source = r#"<widget><div @style="flex"></div></widget>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    let lowered = lower_component(&root, "demo", "demo.awsl", &LoweringOptions::default());
    let tag = first_element_attrs(&lowered).expect("tag");
    assert!(tag.iter().any(|attr| attr.name == "class"));
    assert!(!tag.iter().any(|attr| attr.name == "style"));
}

#[test]
fn collects_script_push_literals() {
    let source = r#"<widget></widget>
<script>
StyleCollector.push("foo bar")
</script>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    let lowered = lower_component(&root, "demo", "demo.awsl", &LoweringOptions::default());
    let collector = collect_from_components(&[lowered]);
    assert!(collector.utilities().any(|u| u == "foo"));
    assert!(collector.utilities().any(|u| u == "bar"));
}

#[test]
fn collects_if_branches() {
    let source = r#"<widget>
<if :condition="show">
<div @style="flex p-2"></div>
<else/>
<div @style="hidden"></div>
</if>
</widget>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    let lowered = lower_component(&root, "demo", "demo.awsl", &LoweringOptions::default());
    let collector = collect_from_components(&[lowered]);
    assert!(collector.utilities().any(|u| u == "flex"));
    assert!(collector.utilities().any(|u| u == "p-2"));
    assert!(collector.utilities().any(|u| u == "hidden"));
}

#[test]
fn mixed_template_collects_static_segments() {
    let source = r#"<widget><div @style="ui-{size} active"></div></widget>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    let lowered = lower_component(&root, "demo", "demo.awsl", &LoweringOptions::default());
    let tag = first_element_attrs(&lowered).expect("tag");
    let class_attr = tag.iter().find(|attr| attr.name == "class").expect("class");
    assert!(matches!(class_attr.value, RenderAttrValue::Template(_)));
    let collector = collect_from_components(&[lowered]);
    assert!(collector.utilities().any(|u| u == "ui-"));
    assert!(collector.utilities().any(|u| u == "active"));
}
