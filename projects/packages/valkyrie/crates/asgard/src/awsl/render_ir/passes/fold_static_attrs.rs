use super::{
    super::canonical::{RenderAttrValue, RenderModule, RenderNode},
    RenderPass,
};

pub struct FoldStaticAttrs;

impl RenderPass for FoldStaticAttrs {
    fn name(&self) -> &'static str {
        "fold_static_attrs"
    }

    fn run(&self, module: &mut RenderModule) {
        for index in 0..module.nodes.len() {
            let node = module.nodes[index].clone();
            match node {
                RenderNode::Element(mut element) => {
                    fold_attrs(&mut element.attrs);
                    module.nodes[index] = RenderNode::Element(element);
                }
                RenderNode::Component(mut component) => {
                    fold_attrs(&mut component.attrs);
                    module.nodes[index] = RenderNode::Component(component);
                }
                _ => {}
            }
        }
    }
}

fn fold_attrs(attrs: &mut Vec<super::super::canonical::RenderAttr>) {
    let mut folded: Vec<super::super::canonical::RenderAttr> = Vec::new();
    for attr in attrs.drain(..) {
        if let Some(existing) = folded.iter_mut().find(|existing| existing.name == attr.name && !existing.is_event) {
            if let (RenderAttrValue::Static(left), RenderAttrValue::Static(right)) = (&existing.value, &attr.value) {
                existing.value = RenderAttrValue::Static(format!("{left} {right}").split_whitespace().collect::<Vec<_>>().join(" "));
                continue;
            }
        }
        folded.push(attr);
    }
    *attrs = folded;
}
