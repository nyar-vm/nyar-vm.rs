//! VON CST → Document（源码正规格式化）。

use nyar_analyzer::format::{Document, FormatOptions, FormattedOutput};
use std_data::text::von::{VonCstElement, VonCstParser, VonCstRoot, VonValue};

use crate::text::{FormatSyntax, ToDocument};

impl FormatSyntax for VonCstRoot {
    fn format_document(&self, options: &FormatOptions) -> Document {
        let mut parts = Vec::new();
        for element in &self.elements {
            match element {
                VonCstElement::Trivia { text, span } => {
                    parts.push(Document::trivia_span(text.clone(), span.clone()));
                }
                VonCstElement::Value { leading, value, trailing, .. } => {
                    if !leading.is_empty() {
                        parts.push(Document::trivia(leading.clone()));
                    }
                    let mut layout = options.clone();
                    layout.max_width = 0;
                    parts.push(value.to_document(&layout));
                    if !trailing.is_empty() {
                        parts.push(Document::trivia(trailing.clone()));
                    }
                }
                VonCstElement::Error { text, span, .. } => {
                    parts.push(Document::trivia_span(text.clone(), span.clone()));
                }
            }
        }
        Document::join_with_trivia(parts)
    }
}

/// 经 CST 格式化 VON 源码。
pub fn format_von_cst(source: &str, options: &FormatOptions) -> Result<FormattedOutput, nyar_analyzer::format::FormatError> {
    let cst =
        VonCstParser::parse(source).map_err(|error| nyar_analyzer::format::FormatError::Parse { path: None, message: error.to_string() })?;
    let force_flat = options.max_width == 0;
    let (text, map) = cst.format_document(options).render_with_map(options, force_flat);
    let mut out = text;
    if options.ensure_trailing_newline && !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(FormattedOutput { text: out, map })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn von_comment_preserved() {
        let source = "# note\n{ x: 1 }\n";
        let out = format_von_cst(source, &FormatOptions::default()).unwrap();
        assert!(out.text.contains("# note"));
    }
}
