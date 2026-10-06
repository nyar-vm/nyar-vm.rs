//! VON CST 构建。

use std::ops::Range;

use crate::transitional::cst::von::{
    VonParseError, VonParser,
    lexer::{Lexer, Token, TokenKind},
    value::VonValue,
};

/// VON CST 元素。
#[derive(Debug, Clone, PartialEq)]
pub enum VonCstElement {
    /// trivia。
    Trivia {
        /// 原文。
        text: String,
        /// 范围。
        span: Range<usize>,
    },
    /// 根值。
    Value {
        /// 值前 trivia。
        leading: String,
        /// 解析结果。
        value: VonValue,
        /// 值后 trivia。
        trailing: String,
        /// 语法 token 覆盖范围。
        span: Range<usize>,
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

/// VON CST 根。
#[derive(Debug, Clone, PartialEq)]
pub struct VonCstRoot {
    /// 顶层元素。
    pub elements: Vec<VonCstElement>,
    /// 全文范围。
    pub span: Range<usize>,
}

/// VON CST 解析。
pub struct VonCstParser;

impl VonCstParser {
    /// 解析 `.von` 源码为 CST。
    pub fn parse(source: &str) -> Result<VonCstRoot, VonParseError> {
        let lossless = Lexer::tokenize_lossless(source)?;
        let clean: Vec<Token> = lossless.iter().filter(|t| !t.kind.is_trivia()).cloned().collect();

        let syntax_tokens: Vec<&Token> = clean.iter().filter(|t| t.kind != TokenKind::Eof).collect();
        let value_span =
            if syntax_tokens.is_empty() { 0..0 } else { syntax_tokens.first().unwrap().span.start..syntax_tokens.last().unwrap().span.end };

        match VonParser::parse(source) {
            Ok(value) => {
                let (leading, trailing) = split_edge_trivia(source, &lossless, &value_span);
                Ok(VonCstRoot { elements: vec![VonCstElement::Value { leading, value, trailing, span: value_span }], span: 0..source.len() })
            }
            Err(error) => Ok(VonCstRoot {
                elements: vec![VonCstElement::Error { message: error.to_string(), text: source.to_string(), span: 0..source.len() }],
                span: 0..source.len(),
            }),
        }
    }
}

fn split_edge_trivia(source: &str, tokens: &[Token], value_span: &Range<usize>) -> (String, String) {
    let mut leading = String::new();
    let mut trailing = String::new();
    for token in tokens {
        if !token.kind.is_trivia() {
            continue;
        }
        if token.span.end <= value_span.start {
            leading.push_str(&source[token.span.clone()]);
        }
        else if token.span.start >= value_span.end {
            trailing.push_str(&source[token.span.clone()]);
        }
    }
    (leading, trailing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn von_cst_keeps_hash_comment() {
        let source = "# cfg\n{ x: 1 }\n";
        let cst = VonCstParser::parse(source).unwrap();
        let VonCstElement::Value { leading, .. } = &cst.elements[0]
        else {
            panic!("expected value");
        };
        assert!(leading.contains("# cfg"));
    }
}
