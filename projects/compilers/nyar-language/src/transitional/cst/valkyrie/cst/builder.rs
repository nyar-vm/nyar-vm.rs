//! CST 构建：lossless token 流 + AST 合并。

use std::ops::Range;

use crate::transitional::cst::valkyrie::{
    ast::{RootStatement, ValkyrieRoot},
    lexer::{Lexer, Token, TokenKind},
    parser::{AstParser, ParseError, fixup_vx_widget_view_markup},
};

use super::kinds::{ValCstElement, ValSyntaxKind};

/// Valkyrie CST 根。
#[derive(Debug, Clone, PartialEq)]
pub struct ValCstRoot {
    /// 有序顶层元素（trivia + 语句 + 错误）。
    pub elements: Vec<ValCstElement>,
    /// 全文范围。
    pub span: Range<usize>,
}

/// CST 解析入口。
pub struct ValCstParser;

impl ValCstParser {
    /// 解析 `.v` 源码为 CST。
    pub fn parse(source: &str) -> Result<ValCstRoot, ParseError> {
        Self::parse_inner(source, false)
    }

    /// 解析 `.vx` 源码为 CST。
    pub fn parse_vx(source: &str) -> Result<ValCstRoot, ParseError> {
        Self::parse_inner(source, true)
    }

    fn parse_inner(source: &str, vx: bool) -> Result<ValCstRoot, ParseError> {
        let lossless = Lexer::tokenize_lossless(source)?;
        let clean: Vec<Token> = lossless.iter().filter(|t| !t.kind.is_trivia()).cloned().collect();

        match AstParser::parse_tokens(source, clean) {
            Ok(mut root) => {
                if vx {
                    fixup_vx_widget_view_markup(&mut root, source);
                }
                Ok(build_from_ast(source, &lossless, &root))
            }
            Err(error) => Ok(build_error_recovery(source, &lossless, error)),
        }
    }
}

fn build_from_ast(source: &str, tokens: &[Token], root: &ValkyrieRoot) -> ValCstRoot {
    let mut elements = Vec::new();
    let mut cursor = 0usize;
    let statements: Vec<&RootStatement> = root.statements.iter().collect();

    for (index, stmt) in statements.iter().enumerate() {
        let stmt_span = stmt.span();
        let leading_end = stmt_span.start;
        push_trivia_between(source, tokens, &mut cursor, leading_end, &mut elements);

        let trailing_end = if index + 1 < statements.len() { statements[index + 1].span().start } else { source.len() };
        let (leading, trailing) = split_trivia_around(source, tokens, cursor, stmt_span, trailing_end);
        cursor = stmt_span.end.max(cursor);
        let trailing_text = trailing;
        cursor = trailing_end;

        elements.push(ValCstElement::Statement { leading, ast: (*stmt).clone(), trailing: trailing_text });
    }

    if cursor < source.len() {
        push_trivia_between(source, tokens, &mut cursor, source.len(), &mut elements);
    }

    ValCstRoot { elements, span: 0..source.len() }
}

fn build_error_recovery(source: &str, tokens: &[Token], error: ParseError) -> ValCstRoot {
    let message = error.to_string();
    let mut elements = Vec::new();
    let mut cursor = 0usize;

    for token in tokens {
        if token.kind == TokenKind::Eof {
            break;
        }
        if token.kind.is_trivia() {
            if token.span.start >= cursor {
                let text = source[token.span.clone()].to_string();
                if !text.is_empty() {
                    elements.push(ValCstElement::Trivia { text, span: token.span.clone() });
                }
                cursor = token.span.end;
            }
            continue;
        }
        if token.span.start > cursor {
            let text = source[cursor..token.span.start].to_string();
            if !text.is_empty() {
                elements.push(ValCstElement::Trivia { text, span: cursor..token.span.start });
            }
        }
        let text = source[token.span.clone()].to_string();
        elements.push(ValCstElement::Error { message: message.clone(), text, span: token.span.clone() });
        cursor = token.span.end;
    }

    if cursor < source.len() {
        elements.push(ValCstElement::Trivia { text: source[cursor..].to_string(), span: cursor..source.len() });
    }

    ValCstRoot { elements, span: 0..source.len() }
}

fn push_trivia_between(source: &str, tokens: &[Token], cursor: &mut usize, end: usize, out: &mut Vec<ValCstElement>) {
    while *cursor < end {
        let Some(token) = tokens.iter().find(|t| t.kind.is_trivia() && t.span.start == *cursor)
        else {
            break;
        };
        let text = source[token.span.clone()].to_string();
        if !text.is_empty() {
            out.push(ValCstElement::Trivia { text, span: token.span.clone() });
        }
        *cursor = token.span.end;
    }
}

fn split_trivia_around(source: &str, tokens: &[Token], cursor: usize, stmt_span: &Range<usize>, region_end: usize) -> (String, String) {
    let mut leading = String::new();
    let mut trailing = String::new();
    let mut pos = cursor;

    while pos < stmt_span.start {
        if let Some(token) = tokens.iter().find(|t| t.kind.is_trivia() && t.span.start == pos) {
            leading.push_str(&source[token.span.clone()]);
            pos = token.span.end;
        }
        else {
            break;
        }
    }

    pos = stmt_span.end;
    while pos < region_end {
        if let Some(token) = tokens.iter().find(|t| t.kind.is_trivia() && t.span.start == pos) {
            trailing.push_str(&source[token.span.clone()]);
            pos = token.span.end;
        }
        else {
            break;
        }
    }

    (leading, trailing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cst_preserves_line_comment() {
        let source = "# header\nmicro main() { }\n";
        let cst = ValCstParser::parse(source).unwrap();
        assert!(cst.elements.iter().any(|e| matches!(e, ValCstElement::Trivia { text, .. } if text.contains("# header"))));
    }

    #[test]
    fn cst_error_recovery_keeps_trivia() {
        let source = "# note\n!!! bad\n";
        let cst = ValCstParser::parse(source).unwrap();
        assert!(cst.elements.iter().any(|e| matches!(e, ValCstElement::Trivia { .. })));
    }
}
