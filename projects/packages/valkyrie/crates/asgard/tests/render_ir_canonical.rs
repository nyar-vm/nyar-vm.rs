//! Golden checks for canonical RenderIR (no stringly dynamic fields in module graph).

use asgard::awsl::{LoweringOptions, RenderModule, RenderNode, RenderTextSegment, compile_awsl_source};

#[test]
fn canonical_module_has_no_stringly_dynamic_fields_in_reachable_graph() {
    let source = r#"<widget demo>
    <Column>
        <Text>{label}</Text>
        <if condition="show">
            <Box @style="flex p-2" />
        <else/>
            <Box @style="hidden" />
        </if>
    </Column>
</widget>
<script>
let label: string = "hi"
let show: bool = true
</script>"#;
    let component = compile_awsl_source(source, "demo", "demo.awsl", &LoweringOptions::default()).expect("compile");
    assert_canonical_shape(&component.render_ir);
}

fn assert_canonical_shape(module: &RenderModule) {
    for node in &module.nodes {
        match node {
            RenderNode::Element(element) => {
                for attr in &element.attrs {
                    let _ = &attr.value;
                    assert!(!format!("{:?}", attr.value).contains("Dynamic("));
                }
            }
            RenderNode::Text(text) => {
                for segment in &text.segments {
                    assert!(matches!(segment, RenderTextSegment::Static(_) | RenderTextSegment::Expr(_)));
                }
            }
            RenderNode::If(render_if) => {
                let _ = module.expr(render_if.condition);
                let _ = module.region(render_if.then_region);
                let _ = module.region(render_if.else_region);
            }
            RenderNode::Loop(render_loop) => {
                let _ = module.expr(render_loop.items);
                if let Some(key) = render_loop.key {
                    let _ = module.expr(key);
                }
                let _ = module.region(render_loop.body_region);
            }
            RenderNode::Component(component) => {
                for attr in &component.attrs {
                    let _ = &attr.value;
                }
            }
            RenderNode::Fragment(fragment) => {
                let _ = module.region(fragment.children);
            }
        }
    }
    assert!(!module.roots.is_empty(), "canonical module must have roots");
}
