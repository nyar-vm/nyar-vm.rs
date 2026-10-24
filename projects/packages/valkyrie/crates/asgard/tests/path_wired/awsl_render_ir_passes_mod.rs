//! Migrated from `src/awsl/render_ir/passes/mod.rs` (`tests`).

use super::*;

use crate::awsl::render_ir::{
    canonical::{RenderAttr, RenderAttrValue, RenderElement, RenderNode, RenderRegion, RenderText, RenderTextSegment},
    canonicalize::canonicalize_surface,
    ids::{RenderNodeId, RenderRegionId},
    surface::{SurfaceAttr, SurfaceAttrValue, SurfaceIr, SurfaceNode, TemplateNodeKind},
};

fn sample_module() -> RenderModule {
    let surface = SurfaceIr::from([SurfaceNode::Tag {
        tag: "Box".into(),
        kind: TemplateNodeKind::Intrinsic,
        attrs: vec![
            SurfaceAttr { name: "class".into(), value: SurfaceAttrValue::Static("a".into()), is_event: false, is_prop: false },
            SurfaceAttr { name: "class".into(), value: SurfaceAttrValue::Static("b".into()), is_event: false, is_prop: false },
        ],
        children: SurfaceIr::from([
            SurfaceNode::Text {
                parts: vec![
                    super::super::surface::SurfaceTextPart::Static("hello ".into()),
                    super::super::surface::SurfaceTextPart::Static("world".into()),
                ],
                span: 0..1,
            },
            SurfaceNode::Text { parts: vec![super::super::surface::SurfaceTextPart::Static("".into())], span: 1..2 },
        ]),
        span: 0..3,
    }]);
    canonicalize_surface(surface, &[])
}

#[test]
fn merge_and_drop_passes_run() {
    let mut module = sample_module();
    run_default_passes(&mut module, ThemeRegistryOptions { single_theme: None, single_mode: None });
    let root = module.node(module.roots[0]);
    let RenderNode::Element(element) = root
    else {
        panic!("expected element")
    };
    assert_eq!(element.attrs.len(), 1);
    let region = module.region(element.children);
    assert_eq!(region.nodes.len(), 1);
}
