use core::range::Range;

use crate::{source_surface::SourceKind, state::ServerState};
use nyar_analyzer::format::{ByteRange, FormatConfigLoader, FormatError, FormatOptions, FormattedOutput, SourceFormatter};
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

/// 单文件格式化处理器（正规 SourceFormatter + `.editorconfig`）。
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

    pub async fn handle_range(state: &ServerState, uri: &str, range: Range<usize>, lsp_options: FormattingOptions) -> Vec<TextEdit> {
        let doc = match state.get_document(uri) {
            Some(d) => d,
            None => return vec![],
        };

        let kind = match source_kind_from_uri(uri) {
            Some(k) => k,
            None => return vec![],
        };
        let options = resolve_options(uri, &lsp_options);

        let formatter = formatter_for(kind);
        let output = match formatter.format_range(&doc.text, ByteRange::new(range.clone()), &options) {
            Ok(out) => out,
            Err(_) => return vec![],
        };

        if output.text == doc.text[range.clone()] {
            return vec![];
        }

        vec![TextEdit { range, new_text: output.text }]
    }
}

/// 由路径后缀决定源码种类（`.valkyrie` 先于 `.v`）。
fn source_kind_from_uri(uri: &str) -> Option<SourceKind> {
    let path = uri.to_ascii_lowercase();
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
        path.rsplit('.').next().and_then(crate::source_surface::source_kind_from_extension)
    }
}

/// 语言文件格式化器：只注册 Valkyrie family 与同仓格式协议。
fn formatter_for(kind: SourceKind) -> LanguageFormatter {
    let id = match kind {
        SourceKind::V => "v",
        SourceKind::Vx => "vx",
        SourceKind::Von => "von",
        SourceKind::Awsl => "awsl",
        _ => "unsupported",
    };
    LanguageFormatter { language_id: id }
}

/// 本服务支持的语言格式化器。
struct LanguageFormatter {
    language_id: &'static str,
}

impl SourceFormatter for LanguageFormatter {
    fn language_id(&self) -> &str {
        self.language_id
    }

    fn format_document(&self, source: &str, options: &FormatOptions) -> Result<FormattedOutput, FormatError> {
        match self.language_id {
            "v" => crate::source_format::format_valkyrie(source, options, false),
            "vx" => crate::source_format::format_valkyrie(source, options, true),
            "von" => format_with_oak(
                oak_von::formatter::format_source(
                    source,
                    &oak_von::formatter::FormatOptions { indent_width: options.indent_width.min(255) as u8, line_width: options.max_width },
                ),
                options,
            ),
            "awsl" => format_with_oak(
                oak_awsl::formatter::format_source(
                    source,
                    &oak_awsl::formatter::FormatOptions { indent_width: options.indent_width.min(255) as u8, line_width: options.max_width },
                ),
                options,
            ),
            other => Err(FormatError::unsupported_language(other)),
        }
    }
}

fn format_source(kind: SourceKind, source: &str, options: &FormatOptions) -> Result<String, FormatError> {
    formatter_for(kind).format(source, options)
}

fn format_with_oak<T>(result: Result<T, impl ToString>, options: &FormatOptions) -> Result<FormattedOutput, FormatError> {
    let text = result.map_err(|error| FormatError::Parse { path: None, message: error.to_string() })?;
    let mut out = text.to_string();
    if options.ensure_trailing_newline && !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(FormattedOutput::text_only(out))
}

fn resolve_options(uri: &str, lsp: &FormattingOptions) -> FormatOptions {
    let resolution = FormatConfigLoader::for_uri(uri);
    FormatConfigLoader::merge_lsp(resolution, lsp.tab_size, lsp.insert_spaces, lsp.insert_final_newline)
}
