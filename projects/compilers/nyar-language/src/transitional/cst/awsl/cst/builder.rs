//! AWSL CST 构建。

use std::ops::Range;

use crate::transitional::cst::awsl::{AwslParseError, AwslParser, AwslRoot};

/// AWSL CST 元素。
#[derive(Debug, Clone, PartialEq)]
pub enum AwslCstElement {
    /// HTML 注释 trivia（段内独立注释）。
    Trivia {
        /// 原文。
        text: String,
        /// 范围。
        span: Range<usize>,
    },
    /// 已解析根。
    Root {
        /// 根前 trivia。
        leading: String,
        /// AST。
        root: AwslRoot,
        /// 根后 trivia。
        trailing: String,
    },
    /// 错误恢复。
    Error {
        /// 消息。
        message: String,
        /// 原文。
        text: String,
        /// 范围。
        span: Range<usize>,
    },
}

/// AWSL CST 根。
#[derive(Debug, Clone, PartialEq)]
pub struct AwslCstRoot {
    /// 有序元素。
    pub elements: Vec<AwslCstElement>,
    /// 全文范围。
    pub span: Range<usize>,
}

/// AWSL CST 解析。
pub struct AwslCstParser;

impl AwslCstParser {
    /// 解析 `.awsl` 为 CST。
    pub fn parse(source: &str) -> Result<AwslCstRoot, AwslParseError> {
        let trivia = scan_html_comments(source);
        match AwslParser::parse_root(source) {
            Ok(root) => {
                let root_span = root.span.clone();
                let (leading, trailing) = partition_edge_trivia(&trivia, &root_span, source);
                let mut elements = Vec::new();
                for piece in &trivia {
                    if piece.span.start >= root_span.start && piece.span.end <= root_span.end {
                        elements.push(AwslCstElement::Trivia { text: source[piece.span.clone()].to_string(), span: piece.span.clone() });
                    }
                }
                elements.push(AwslCstElement::Root { leading, root, trailing });
                Ok(AwslCstRoot { elements, span: 0..source.len() })
            }
            Err(error) => Ok(AwslCstRoot {
                elements: vec![
                    AwslCstElement::Trivia { text: leading_trivia_from_scan(&trivia, source), span: 0..source.len() },
                    AwslCstElement::Error { message: error.message.clone(), text: source.to_string(), span: 0..source.len() },
                ],
                span: 0..source.len(),
            }),
        }
    }
}

#[derive(Clone)]
struct TriviaPiece {
    span: Range<usize>,
}

fn scan_html_comments(source: &str) -> Vec<TriviaPiece> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < source.len() {
        if source[i..].starts_with("<!--") {
            let start = i;
            i += 4;
            while i < source.len() && !source[i..].starts_with("-->") {
                i += source[i..].chars().next().unwrap().len_utf8();
            }
            if source[i..].starts_with("-->") {
                i += 3;
            }
            out.push(TriviaPiece { span: start..i });
            continue;
        }
        i += source[i..].chars().next().unwrap().len_utf8();
    }
    out
}

fn partition_edge_trivia(trivia: &[TriviaPiece], root_span: &Range<usize>, source: &str) -> (String, String) {
    let mut leading = String::new();
    let mut trailing = String::new();
    for piece in trivia {
        if piece.span.end <= root_span.start {
            leading.push_str(&source[piece.span.clone()]);
        }
        else if piece.span.start >= root_span.end {
            trailing.push_str(&source[piece.span.clone()]);
        }
    }
    (leading, trailing)
}

fn leading_trivia_from_scan(trivia: &[TriviaPiece], source: &str) -> String {
    trivia.iter().map(|p| source[p.span.clone()].to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn awsl_cst_keeps_html_comment() {
        let source = "<!-- keep -->\n<template>\n<div/>\n</template>\n";
        let cst = AwslCstParser::parse(source).unwrap();
        let has_comment = cst.elements.iter().any(|e| match e {
            AwslCstElement::Trivia { text, .. } => text.contains("keep"),
            AwslCstElement::Root { leading, .. } => leading.contains("keep"),
            _ => false,
        });
        assert!(has_comment);
    }
}
