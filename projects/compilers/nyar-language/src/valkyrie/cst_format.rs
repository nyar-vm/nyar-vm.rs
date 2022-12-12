//! Valkyrie CST → [`Document`]（正规源码格式化）。

use nyar_analyzer::format::{Document, FormatOptions, FormattedOutput};
use std_data::text::valkyrie::{ValCstElement, ValCstParser, ValCstRoot};

use crate::{text::FormatSyntax, valkyrie::source_format};

impl FormatSyntax for ValCstRoot {
    fn format_document(&self, options: &FormatOptions) -> Document {
        let mut parts = Vec::new();
        for element in &self.elements {
            match element {
                ValCstElement::Trivia { text, span } => {
                    parts.push(Document::trivia_span(text.clone(), span.clone()));
                }
                ValCstElement::Statement { leading, ast, trailing } => {
                    if !leading.is_empty() {
                        parts.push(Document::trivia(leading.clone()));
                    }
                    let formatted = source_format::format_statement(ast, options);
                    parts.push(Document::text(formatted));
                    if !trailing.is_empty() {
                        parts.push(Document::trivia(trailing.clone()));
                    }
                }
                ValCstElement::Error { text, span, .. } => {
                    parts.push(Document::trivia_span(text.clone(), span.clone()));
                }
            }
        }
        Document::join_with_trivia(parts)
    }
}

/// 经 CST 格式化 Valkyrie 源码。
pub fn format_valkyrie_cst(source: &str, options: &FormatOptions, vx: bool) -> Result<FormattedOutput, nyar_analyzer::format::FormatError> {
    let cst = if vx { ValCstParser::parse_vx(source) } else { ValCstParser::parse(source) }
        .map_err(|error| nyar_analyzer::format::FormatError::Parse { path: None, message: error.to_string() })?;
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
    fn comment_preserved_in_output() {
        let source = "# keep\nmicro main() { }\n";
        let options = FormatOptions::default();
        let out = format_valkyrie_cst(source, &options, false).unwrap();
        assert!(out.text.contains("# keep"), "comment should be preserved: {}", out.text);
    }

    #[test]
    fn format_idempotent_with_comment() {
        let source = "# note\nmicro main(){let x=1}\n";
        let options = FormatOptions::default();
        let once = format_valkyrie_cst(source, &options, false).unwrap().text;
        let twice = format_valkyrie_cst(&once, &options, false).unwrap().text;
        assert_eq!(once, twice);
    }
}
