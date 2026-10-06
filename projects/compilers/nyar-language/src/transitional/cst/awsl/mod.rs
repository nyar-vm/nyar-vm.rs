#![doc = include_str!("readme.md")]
#![warn(missing_docs)]

mod abi;
mod ast;
mod component;
mod cst;
mod error;
mod html;
mod lexer;
mod parser;

pub use abi::{
    AbiDerived, AbiEffect, AbiEvent, AbiExtractResult, AbiIssue, AbiIssueKind, AbiMemo, AbiParam, AbiProperty, AbiReference, AbiSeverity,
    AbiState, AbiSymbolKind, ComponentAbi, ComponentAbiIndex, TemplateBinding, TemplateBindingKind, abi_declaration_span, classify_abi_cursor,
    collect_abi_references, collect_template_bindings, extract_component_abi_from_script, extract_component_abi_from_vx,
    find_template_binding_at, is_snake_case, normalize_event_name, normalize_prop_name, refine_binding_kind,
};
pub use ast::{
    AwslAttribute, AwslAttributeValue, AwslDirective, AwslDirectiveKind, AwslElement, AwslImport, AwslRoot, AwslTemplateNode, AwslTextPart,
};
pub use component::{awsl_stem_from_component_tag, resolve_widget_name, validate_component_contract, widget_name_from_stem};
pub use cst::{AwslCstElement, AwslCstParser, AwslCstRoot};
pub use error::AwslParseError;
pub use html::{HTML_VOID_ELEMENTS, is_html_void_element};
pub use lexer::Lexer;
pub use parser::AwslParser;
