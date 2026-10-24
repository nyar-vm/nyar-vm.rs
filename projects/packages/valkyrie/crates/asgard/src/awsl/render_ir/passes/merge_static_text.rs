use super::{
    super::{
        canonical::{RenderModule, RenderNode, RenderText, RenderTextSegment},
        ids::RenderRegionId,
    },
    RenderPass,
};

pub struct MergeAdjacentStaticText;

impl RenderPass for MergeAdjacentStaticText {
    fn name(&self) -> &'static str {
        "merge_adjacent_static_text"
    }

    fn run(&self, module: &mut RenderModule) {
        for region_index in 0..module.regions.len() {
            let region_id = RenderRegionId(region_index as u32);
            merge_region(module, region_id);
        }
    }
}

fn merge_region(module: &mut RenderModule, region_id: RenderRegionId) {
    if !region_id.is_valid() {
        return;
    }
    let node_ids = module.region(region_id).nodes.clone();
    let mut merged = Vec::new();
    let mut pending_static: Option<(String, std::ops::Range<usize>)> = None;
    for node_id in node_ids {
        let node = module.node(node_id).clone();
        match node {
            RenderNode::Text(text) => {
                if let Some((acc, span)) = take_static_only(&text) {
                    if let Some((prev, prev_span)) = pending_static.take() {
                        pending_static = Some((format!("{prev}{acc}"), prev_span.start..span.end));
                    }
                    else {
                        pending_static = Some((acc, span));
                    }
                    continue;
                }
                flush_static(module, &mut merged, &mut pending_static);
                merged.push(node_id);
            }
            other => {
                flush_static(module, &mut merged, &mut pending_static);
                recurse_other(module, &other);
                merged.push(node_id);
            }
        }
    }
    flush_static(module, &mut merged, &mut pending_static);
    module.regions[region_id.0 as usize].nodes = merged;
}

fn take_static_only(text: &RenderText) -> Option<(String, std::ops::Range<usize>)> {
    if text.segments.iter().any(|segment| matches!(segment, RenderTextSegment::Expr(_))) {
        return None;
    }
    let merged = text
        .segments
        .iter()
        .map(|segment| match segment {
            RenderTextSegment::Static(value) => value.as_str(),
            RenderTextSegment::Expr(_) => "",
        })
        .collect::<String>();
    Some((merged, text.span.clone()))
}

fn flush_static(
    module: &mut RenderModule,
    merged: &mut Vec<super::super::ids::RenderNodeId>,
    pending: &mut Option<(String, std::ops::Range<usize>)>,
) {
    let Some((text, span)) = pending.take()
    else {
        return;
    };
    let id = super::super::ids::RenderNodeId(module.nodes.len() as u32);
    module.nodes.push(RenderNode::Text(RenderText { segments: vec![RenderTextSegment::Static(text)], span }));
    merged.push(id);
}

fn recurse_other(module: &mut RenderModule, node: &RenderNode) {
    match node {
        RenderNode::Element(element) => merge_region(module, element.children),
        RenderNode::Component(component) => merge_region(module, component.children),
        RenderNode::Fragment(fragment) => merge_region(module, fragment.children),
        RenderNode::If(render_if) => {
            merge_region(module, render_if.then_region);
            merge_region(module, render_if.else_region);
        }
        RenderNode::Loop(render_loop) => merge_region(module, render_loop.body_region),
        RenderNode::Text(_) => {}
    }
}
