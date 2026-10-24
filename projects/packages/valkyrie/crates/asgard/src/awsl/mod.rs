//! AWSL 降级与 RenderIR。

pub mod abi_index;
pub mod compile;
pub mod expr_deps;
pub mod expr_util;
pub mod lower;
pub mod render_ir;

pub use abi_index::{
    ComponentAbiIndex, component_abi_index_from_components, extract_abi_for_script, refine_component_attr_flags, validate_component_abi,
};
pub use compile::compile_awsl_source;
pub use expr_deps::{extract_expr_idents, reactive_deps};
pub use lower::{BindingKind, LoweredComponent, LoweringOptions, ScriptBinding, SignalValueType, lower_component, script_let_prelude};
pub use render_ir::{
    RenderAttr, RenderAttrValue, RenderExprId, RenderIfNode, RenderIr, RenderLoopNode, RenderModule, RenderNode, RenderNodeId, RenderRegionId,
    RenderTextSegment, SurfaceAttr, SurfaceAttrValue, SurfaceIf, SurfaceIr, SurfaceLoop, SurfaceNode, SurfaceTextPart, TemplateNodeKind,
    ThemeRegistryOptions, canonicalize_surface, is_fragment_root, is_intrinsic_tag, run_default_passes, theme_registry_pass,
};
