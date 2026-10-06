//! X-Grammar 内联标记解析（TSX 子集）。

use std::ops::Range;

use crate::transitional::cst::valkyrie::tgrammar::parse_tgrammar_fragment;

use super::{
    ast::{XgAttrValue, XgElement, XgNode, XgRoot, XgTextPart},
    lexer::{Lexer, Token, TokenKind},
};

/// X-Grammar 解析错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XgParseError {
    pub message: String,
    pub span: Range<usize>,
}

/// 解析完整模板正文（至输入结束，含元素间静态文本与 T-Grammar meta）。
pub fn parse_xgrammar_template(source: &str) -> Result<XgRoot, XgParseError> {
    let tokens = Lexer::tokenize(source);
    Parser::new(source, tokens).parse_all_nodes()
}

/// 解析 X-Grammar 与 T-Grammar meta 混写片段，返回节点与消费字节数。
pub fn parse_xgrammar_with_meta(source: &str) -> Result<(XgRoot, usize), XgParseError> {
    let tokens = Lexer::tokenize(source);
    let mut parser = Parser::new(source, tokens);
    let start = parser.current_offset();
    let nodes = parser.parse_markup_nodes_until_boundary()?;
    if nodes.is_empty() {
        return Err(parser.error_at(start, "expected X-Grammar markup or meta directive"));
    }
    Ok((nodes, parser.current_offset().saturating_sub(start)))
}

/// 解析一段内联 X-Grammar 标记（一个或多个相邻元素），返回节点与消费的字节数。
pub fn parse_xgrammar_markup(source: &str) -> Result<(XgRoot, usize), XgParseError> {
    let tokens = Lexer::tokenize(source);
    let mut parser = Parser::new(source, tokens);
    let start = parser.current_offset();
    let mut nodes = Vec::new();
    loop {
        parser.skip_leading_trivia();
        if parser.check(TokenKind::Directive) {
            break;
        }
        if parser.check(TokenKind::Lt) {
            nodes.push(XgNode::Element(parser.parse_element()?));
            continue;
        }
        break;
    }
    if nodes.is_empty() {
        return Err(parser.error_at(start, "expected X-Grammar markup"));
    }
    Ok((nodes, parser.current_offset().saturating_sub(start)))
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    index: usize,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str, tokens: Vec<Token>) -> Self {
        Self { source, tokens, index: 0 }
    }

    fn current_offset(&self) -> usize {
        self.tokens.get(self.index).map(|token| token.span.start).unwrap_or(self.source.len())
    }

    fn parse_all_nodes(&mut self) -> Result<XgRoot, XgParseError> {
        let mut nodes = Vec::new();
        loop {
            self.skip_leading_trivia();
            if self.is_at_end() {
                break;
            }
            if let Some(node) = self.parse_next_node()? {
                nodes.push(node);
            }
        }
        Ok(nodes)
    }

    fn parse_markup_nodes_until_boundary(&mut self) -> Result<XgRoot, XgParseError> {
        let mut nodes = Vec::new();
        loop {
            self.skip_leading_trivia();
            if self.is_at_end() {
                break;
            }
            if self.check(TokenKind::Directive) || self.check(TokenKind::Lt) {
                if let Some(node) = self.parse_next_node()? {
                    nodes.push(node);
                }
                continue;
            }
            break;
        }
        Ok(nodes)
    }

    fn parse_next_node(&mut self) -> Result<Option<XgNode>, XgParseError> {
        self.skip_leading_trivia();
        if self.is_at_end() {
            return Ok(None);
        }
        match self.peek_kind() {
            TokenKind::Directive => Ok(Some(self.parse_meta_node()?)),
            TokenKind::Lt => Ok(Some(XgNode::Element(self.parse_element()?))),
            TokenKind::Text => Ok(Some(self.parse_text_node()?)),
            TokenKind::Comment => {
                self.advance();
                Ok(None)
            }
            TokenKind::Eof => Ok(None),
            _ => Err(self.error(format!("unexpected token {:?}", self.peek_kind()))),
        }
    }

    fn parse_meta_node(&mut self) -> Result<XgNode, XgParseError> {
        let start = self.current_offset();
        let (nodes, consumed) = parse_tgrammar_fragment(&self.source[start..]).map_err(|error| self.error_at(start, error.message))?;
        self.sync_past_offset(start + consumed);
        Ok(XgNode::Meta { nodes, span: start..start + consumed })
    }

    fn parse_element(&mut self) -> Result<XgElement, XgParseError> {
        let start = self.current_offset();
        self.expect(TokenKind::Lt)?;
        if self.check(TokenKind::Slash) {
            return Err(self.error("unexpected closing tag"));
        }
        let tag = self.expect_identifier()?;
        let mut attrs = Vec::new();
        while self.check(TokenKind::Identifier) {
            let raw_name = self.expect_identifier()?;
            let (name, is_binding) = if let Some(stripped) = raw_name.strip_prefix(':') {
                if stripped.is_empty() {
                    return Err(self.error("binding attribute name cannot be empty after `:`"));
                }
                (stripped.to_string(), true)
            }
            else {
                (raw_name, false)
            };
            if self.consume_if(TokenKind::Eq) {
                let value = self.parse_attr_value(&name, is_binding)?;
                attrs.push((name, value));
            }
            else {
                attrs.push((name, XgAttrValue::Literal(String::new())));
            }
        }
        let self_closing = self.consume_if(TokenKind::Slash);
        self.expect(TokenKind::Gt)?;
        let mut children = Vec::new();
        if !self_closing {
            loop {
                self.skip_leading_trivia();
                if self.is_close_tag(&tag) {
                    self.expect(TokenKind::Lt)?;
                    self.expect(TokenKind::Slash)?;
                    let close_name = self.expect_identifier()?;
                    if close_name != tag {
                        return Err(self.error(format!("expected closing tag `</{tag}>`")));
                    }
                    self.expect(TokenKind::Gt)?;
                    break;
                }
                if self.is_at_end() {
                    return Err(self.error_at(start, format!("unclosed `<{tag}>`")));
                }
                match self.parse_child_node()? {
                    Some(child) => children.push(child),
                    None => {}
                }
            }
        }
        Ok(XgElement { tag, attrs, children, self_closing, span: start..self.current_offset() })
    }

    fn parse_child_node(&mut self) -> Result<Option<XgNode>, XgParseError> {
        self.skip_leading_trivia();
        if self.is_at_end() {
            return Ok(None);
        }
        self.parse_next_node()
    }

    fn parse_attr_value(&mut self, attr_name: &str, is_binding: bool) -> Result<XgAttrValue, XgParseError> {
        let token = self.advance();
        match token.kind {
            TokenKind::StringLiteral => {
                let inner = self.parse_quoted_literal(&token)?;
                if is_binding { Ok(XgAttrValue::Expression(inner)) } else { Ok(XgAttrValue::Literal(inner)) }
            }
            TokenKind::BracedExpr => Err(self.error_at(
                token.span.start,
                format!("attribute `{attr_name}` does not use `={{...}}`; use `:attr=\"expr\"` for dynamic bindings"),
            )),
            TokenKind::Identifier if is_binding => {
                Err(self.error_at(token.span.start, format!("binding attribute `:{attr_name}` requires a quoted expression `\"...\"`")))
            }
            TokenKind::Identifier => {
                Err(self.error_at(token.span.start, format!("attribute `{attr_name}` requires a quoted string literal `\"...\"`")))
            }
            _ => Err(self.error_at(token.span.start, "expected attribute value")),
        }
    }

    fn parse_quoted_literal(&self, token: &Token) -> Result<String, XgParseError> {
        let raw = self.text(token);
        let quote = raw.chars().next().ok_or_else(|| self.error_at(token.span.start, "invalid string literal"))?;
        raw.strip_prefix(quote)
            .and_then(|text| text.strip_suffix(quote))
            .map(|inner| inner.to_string())
            .ok_or_else(|| self.error_at(token.span.start, "invalid string literal"))
    }

    fn parse_text_node(&mut self) -> Result<XgNode, XgParseError> {
        let token = self.advance();
        let parts = parse_text_parts(self.text(&token));
        Ok(XgNode::Text { parts, span: token.span })
    }

    fn is_close_tag(&self, name: &str) -> bool {
        if !self.check(TokenKind::Lt) {
            return false;
        }
        if self.tokens.get(self.index + 1).map(|token| token.kind) != Some(TokenKind::Slash) {
            return false;
        }
        self.tokens.get(self.index + 2).filter(|token| token.kind == TokenKind::Identifier).is_some_and(|token| self.text(token) == name)
    }

    fn sync_past_offset(&mut self, offset: usize) {
        while self.index < self.tokens.len() && self.tokens[self.index].span.start < offset {
            self.index += 1;
        }
    }

    fn skip_leading_trivia(&mut self) {
        while self.check(TokenKind::Comment) {
            self.advance();
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Result<(), XgParseError> {
        if self.check(kind) {
            self.advance();
            Ok(())
        }
        else {
            Err(self.error(format!("expected {:?}", kind)))
        }
    }

    fn expect_identifier(&mut self) -> Result<String, XgParseError> {
        let token = self.advance();
        if token.kind != TokenKind::Identifier {
            return Err(self.error_at(token.span.start, "expected identifier"));
        }
        Ok(self.text(&token).to_string())
    }

    fn consume_if(&mut self, kind: TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        }
        else {
            false
        }
    }

    fn text(&self, token: &Token) -> &str {
        &self.source[token.span.clone()]
    }

    fn check(&self, kind: TokenKind) -> bool {
        self.peek_kind() == kind
    }

    fn peek_kind(&self) -> TokenKind {
        self.tokens.get(self.index).map(|token| token.kind).unwrap_or(TokenKind::Eof)
    }

    fn advance(&mut self) -> Token {
        let token = self.tokens[self.index].clone();
        if !self.is_at_end() {
            self.index += 1;
        }
        token
    }

    fn is_at_end(&self) -> bool {
        self.peek_kind() == TokenKind::Eof
    }

    fn error(&self, message: impl Into<String>) -> XgParseError {
        self.error_at(self.current_offset(), message)
    }

    fn error_at(&self, pos: usize, message: impl Into<String>) -> XgParseError {
        XgParseError { message: message.into(), span: pos..pos.saturating_add(1) }
    }
}

fn parse_text_parts(text: &str) -> Vec<XgTextPart> {
    let mut parts = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        if start > 0 {
            parts.push(XgTextPart::Static(rest[..start].to_string()));
        }
        match parse_braced_expression(rest, start) {
            Some((expr, consumed)) => {
                parts.push(XgTextPart::Expression(expr));
                rest = &rest[start + consumed..];
            }
            None => {
                parts.push(XgTextPart::Static(rest.to_string()));
                return parts;
            }
        }
    }
    if !rest.is_empty() {
        parts.push(XgTextPart::Static(rest.to_string()));
    }
    if parts.is_empty() {
        parts.push(XgTextPart::Static(String::new()));
    }
    parts
}

fn parse_braced_expression(source: &str, brace_start: usize) -> Option<(String, usize)> {
    if source.as_bytes().get(brace_start) != Some(&b'{') {
        return None;
    }
    let mut depth = 0usize;
    for (offset, ch) in source[brace_start..].char_indices() {
        if ch == '{' {
            depth += 1;
        }
        else if ch == '}' {
            depth -= 1;
            if depth == 0 {
                let inner = source[brace_start + 1..brace_start + offset].trim();
                return Some((inner.to_string(), offset + 1));
            }
        }
    }
    None
}
