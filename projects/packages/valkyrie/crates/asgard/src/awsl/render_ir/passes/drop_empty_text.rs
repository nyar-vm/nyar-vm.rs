use super::{
    super::{
        canonical::{RenderModule, RenderNode, RenderText, RenderTextSegment},
        ids::RenderRegionId,
    },
    RenderPass,
};

pub struct DropEmptyText;

impl RenderPass for DropEmptyText {
    fn name(&self) -> &'static str {
        "drop_empty_text"
    }

    fn run(&self, module: &mut RenderModule) {
        for region_index in 0..module.regions.len() {
            let region_id = RenderRegionId(region_index as u32);
            drop_empty_in_region(module, region_id);
        }
    }
}

fn drop_empty_in_region(module: &mut RenderModule, region_id: RenderRegionId) {
    if !region_id.is_valid() {
        return;
    }
    let node_ids = module.region(region_id).nodes.clone();
    let mut kept = Vec::new();
    for node_id in node_ids {
        let node = module.node(node_id).clone();
        match &node {
            RenderNode::Text(text) if is_empty_text(text) => continue,
            other => {
                recurse_other(module, other);
                kept.push(node_id);
            }
        }
    }
    module.regions[region_id.0 as usize].nodes = kept;
}

fn is_empty_text(text: &RenderText) -> bool {
    text.segments.iter().all(|segment| match segment {
        RenderTextSegment::Static(value) => value.is_empty(),
        RenderTextSegment::Expr(_) => false,
    })
}

fn recurse_other(module: &mut RenderModule, node: &RenderNode) {
    match node {
        RenderNode::Element(element) => drop_empty_in_region(module, element.children),
        RenderNode::Component(component) => drop_empty_in_region(module, component.children),
        RenderNode::Fragment(fragment) => drop_empty_in_region(module, fragment.children),
        RenderNode::If(render_if) => {
            drop_empty_in_region(module, render_if.then_region);
            drop_empty_in_region(module, render_if.else_region);
        }
        RenderNode::Loop(render_loop) => drop_empty_in_region(module, render_loop.body_region),
        RenderNode::Text(_) => {}
    }
}
