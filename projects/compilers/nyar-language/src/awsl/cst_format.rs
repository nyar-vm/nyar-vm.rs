//! AWSL CST → Document。

use nyar_analyzer::format::{Document, FormatOptions, FormattedOutput};
use std_data::text::awsl::{AwslCstElement, AwslCstParser, AwslCstRoot};

use crate::{awsl::source_format, text::FormatSyntax};

impl FormatSyntax for AwslCstRoot {
    fn format_document(&self, options: &FormatOptions) -> Document {
        let mut parts = Vec::new();
        for element in &self.elements {
            match element {
                AwslCstElement::Trivia { text, span } => {
                    parts.push(Document::trivia_span(text.clone(), span.clone()));
                }
                AwslCstElement::Root { leading, root, trailing } => {
                    if !leading.is_empty() {
                        parts.push(Document::trivia(leading.clone()));
                    }
                    parts.push(Document::text(source_format::format_root(root, options)));
                    if !trailing.is_empty() {
                        parts.push(Document::trivia(trailing.clone()));
                    }
                }
                AwslCstElement::Error { text, span, .. } => {
                    parts.push(Document::trivia_span(text.clone(), span.clone()));
                }
            }
        }
        Document::join_with_trivia(parts)
    }
}

/// 经 CST 格式化 AWSL 源码。
pub fn format_awsl_cst(source: &str, options: &FormatOptions) -> Result<FormattedOutput, nyar_analyzer::format::FormatError> {
    let cst = AwslCstParser::parse(source).map_err(|error| nyar_analyzer::format::FormatError::Parse { path: None, message: error.message })?;
    let force_flat = options.max_width == 0;
    let (text, map) = cst.format_document(options).render_with_map(options, force_flat);
    Ok(FormattedOutput { text, map })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn awsl_comment_preserved() {
        let source = "<!-- note -->\n<template>\n<div/>\n</template>\n";
        let out = format_awsl_cst(source, &FormatOptions::default()).unwrap();
        assert!(out.text.contains("note"));
    }
}
