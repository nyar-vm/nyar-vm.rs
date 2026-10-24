//! RenderIR: surface → canonical → passes → backend projection.

pub mod backend;
pub mod canonical;
pub mod canonicalize;
pub mod expr;
pub mod ids;
pub mod passes;
pub mod surface;

pub use backend::{attr_value_source, node_children_region, node_kind, region_nodes, text_segments_source, walk_region};
pub use canonical::{
    RenderAttr, RenderAttrValue, RenderComponentNode, RenderElement, RenderFragment, RenderIfNode, RenderIr, RenderLoopNode, RenderModule,
    RenderNode, RenderRegion, RenderText, RenderTextSegment, is_fragment_root, is_intrinsic_tag,
};
pub use canonicalize::canonicalize_surface;
pub use expr::{RenderBinding, RenderDiagnostic, RenderEvent, RenderExpr, RenderExprKind, RenderPurity, RenderValueKind};
pub use ids::{RenderBindingId, RenderEventId, RenderExprId, RenderNodeId, RenderRegionId};
pub use passes::{RenderPass, ThemeRegistryOptions, run_default_passes, theme_registry_pass};
pub use surface::{SurfaceAttr, SurfaceAttrValue, SurfaceIf, SurfaceIr, SurfaceLoop, SurfaceNode, SurfaceTextPart, TemplateNodeKind};
