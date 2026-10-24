//! StyleCollector / @style 降级与 codegen 测试。

use asgard::{
    awsl::{LoweringOptions, RenderNode, lower_component},
    codegen::build_awsl_wasm_source,
};
use oak_awsl::AwslParser;

fn first_element_class_attr(lowered: &asgard::awsl::LoweredComponent) -> Option<&asgard::awsl::RenderAttr> {
    let module = &lowered.render_ir;
    module.roots.iter().find_map(|&id| match module.node(id) {
        RenderNode::Element(element) => element.attrs.iter().find(|attr| attr.name == "class"),
        _ => None,
    })
}

#[test]
fn style_codegen_uses_class_binding() {
    let source = r#"<widget><div @style="flex w-4"></div></widget>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    let lowered = lower_component(&root, "demo", "demo.awsl", &LoweringOptions::default());
    let v_source = build_awsl_wasm_source(&[lowered]);
    assert!(v_source.contains("rx_bind_class_utf8"));
    assert!(!v_source.contains("rx_bind_attr_utf8") || !v_source.contains("\"style\""));
}

#[test]
fn ternary_style_collects_both_arms() {
    let source = r#"<widget><div @style="ready ? 'flex p-2' : 'hidden'"></div></widget>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    let lowered = lower_component(&root, "demo", "demo.awsl", &LoweringOptions::default());
    let collector = asgard::tailwind::collect_from_components(&[lowered]);
    assert!(collector.utilities().any(|u| u == "flex"));
    assert!(collector.utilities().any(|u| u == "p-2"));
    assert!(collector.utilities().any(|u| u == "hidden"));
}

#[test]
fn inline_style_binding_not_collected_as_utility() {
    let source = r#"<widget><div :style="width: 100px"></div></widget>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    let lowered = lower_component(&root, "demo", "demo.awsl", &LoweringOptions::default());
    let module = &lowered.render_ir;
    let style_attr = module.roots.iter().find_map(|&id| match module.node(id) {
        RenderNode::Element(element) => element.attrs.iter().find(|attr| attr.name == "style"),
        _ => None,
    });
    assert!(style_attr.is_some());
    assert!(first_element_class_attr(&lowered).is_none());
    let collector = asgard::tailwind::collect_from_components(&[lowered]);
    assert_eq!(collector.utilities().count(), 0);
}
