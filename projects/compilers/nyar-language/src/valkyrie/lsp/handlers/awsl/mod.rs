//! AWSL LSP 支持模块

mod completion;
mod definition;
mod diagnostics;
mod document;
mod hover;
mod inlay_hint;
mod references;
mod script;
mod semantic_tokens;
mod vx;
mod widget;

pub use completion::AwslCompletionHandler;
pub use definition::AwslDefinitionHandler;
pub use diagnostics::compile_awsl_document;
pub use document::{is_awsl_uri, script_offset_at, script_offset_range};
pub use hover::AwslHoverHandler;
pub use inlay_hint::AwslInlayHintHandler;
pub use references::AwslReferencesHandler;
pub use script::{map_script_range_to_file, resolve_script_view, AwslScriptView};
pub use semantic_tokens::AwslSemanticTokensHandler;
pub use widget::{
    component_stem_from_uri, map_synthetic_span_to_file, synthetic_widget_source, widget_name_from_root,
};
