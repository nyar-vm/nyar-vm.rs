use nyar_language::formatter::{FormatConfigLoader, FormatOptions, SourceKind, source_kind_from_extension, format_source, format_source_range};
use crate::state::ServerState;
use core::range::Range;
use nyar_analyzer::format::ByteRange;
use oak_lsp::types::*;
use serde::{Deserialize, Serialize};

/// 格式化选项（LSP 入参）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormattingOptions {
    pub tab_size: u32,
    pub insert_spaces: bool,
    pub trim_trailing_whitespace: Option<bool>,
    pub insert_final_newline: Option<bool>,
    pub trim_final_newlines: Option<bool>,
}

/// 格式化处理器（正规 SourceFormatter + `.editorconfig`）。
pub struct FormattingHandler;

impl FormattingHandler {
    pub async fn handle(state: &ServerState, uri: &str, lsp_options: FormattingOptions) -> Vec<TextEdit> {
        let doc = match state.get_document(uri) {
            Some(d) => d,
            None => return vec![],
        };

        let kind = source_kind_from_uri(uri);
        if kind.is_none() {
            return vec![];
        }
        let kind = kind.unwrap();
        let options = resolve_options(uri, &lsp_options);

        let output = match format_source(kind, &doc.text, &options) {
            Ok(text) => text,
            Err(_) => return vec![],
        };

        if output == doc.text {
            return vec![];
        }

        vec![TextEdit { range: Range { start: 0, end: doc.text.len() }, new_text: output }]
    }

    pub async fn handle_range(
        state: &ServerState,
        uri: &str,
        range: Range<usize>,
        lsp_options: FormattingOptions,
    ) -> Vec<TextEdit> {
        let doc = match state.get_document(uri) {
            Some(d) => d,
            None => return vec![],
        };

        let kind = match source_kind_from_uri(uri) {
            Some(k) => k,
            None => return vec![],
        };
        let options = resolve_options(uri, &lsp_options);

        let output = match format_source_range(kind, &doc.text, ByteRange::new(range.clone()), &options) {
            Ok(out) => out,
            Err(_) => return vec![],
        };

        if output.text == doc.text[range.clone()] {
            return vec![];
        }

        vec![TextEdit { range, new_text: output.text }]
    }
}

fn resolve_options(uri: &str, lsp: &FormattingOptions) -> FormatOptions {
    let resolution = FormatConfigLoader::for_uri(uri);
    FormatConfigLoader::merge_lsp(resolution, lsp.tab_size, lsp.insert_spaces, lsp.insert_final_newline)
}

fn source_kind_from_uri(uri: &str) -> Option<SourceKind> {
    let path = uri.to_ascii_lowercase();
    // Order: longest suffix first (`.valkyrie` before `.v`).
    if path.ends_with(".awsl") {
        Some(SourceKind::Awsl)
    }
    else if path.ends_with(".valkyrie") {
        Some(SourceKind::V)
    }
    else if path.ends_with(".vx") {
        Some(SourceKind::Vx)
    }
    else if path.ends_with(".von") {
        Some(SourceKind::Von)
    }
    else if path.ends_with(".v") {
        Some(SourceKind::V)
    }
    else {
        path.rsplit('.').next().and_then(source_kind_from_extension)
    }
}
