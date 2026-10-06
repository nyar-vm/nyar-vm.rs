//! T-Grammar 模板解析：`<% kw %>` 为关键词指令，否则为语句。

use std::ops::Range;

use super::{
    ast::{TgIf, TgIfArm, TgKeyword, TgLoop, TgMatch, TgMatchArm, TgNode, TgRoot, TgTextPart},
    lexer::{Lexer, Token, TokenKind},
};

/// T-Grammar 解析错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TgParseError {
    pub message: String,
    pub span: Range<usize>,
}

/// 解析完整 T-Grammar 模板正文。
pub fn parse_tgrammar_template(source: &str) -> Result<TgRoot, TgParseError> {
    let tokens = Lexer::tokenize(source);
    Parser::new(source, tokens).parse_root()
}

/// 解析从 `<%` 起的一段 meta 片段，返回节点与消费字节数（供 X-Grammar 混写）。
pub fn parse_tgrammar_fragment(source: &str) -> Result<(TgRoot, usize), TgParseError> {
    if !source.starts_with("<%") {
        return Err(TgParseError { message: "expected `<%`".into(), span: 0..1 });
    }
    let tokens = Lexer::tokenize(source);
    let mut parser = Parser::new(source, tokens);
    parser.stop_at_xml_boundary = true;
    let nodes = parser.parse_nodes_until(|_| false)?;
    Ok((nodes, parser.consumed_bytes(0)))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Boundary {
    End,
    ElseIf,
    Else,
    Case,
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    index: usize,
    /// 为 X-Grammar 混写片段解析时，在裸 `<tag` 前停止。
    stop_at_xml_boundary: bool,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str, tokens: Vec<Token>) -> Self {
        Self { source, tokens, index: 0, stop_at_xml_boundary: false }
    }

    fn parse_root(mut self) -> Result<TgRoot, TgParseError> {
        self.parse_nodes_until(|_| false)
    }

    fn consumed_bytes(&self, start: usize) -> usize {
        self.current_offset().saturating_sub(start)
    }

    fn current_offset(&self) -> usize {
        self.tokens.get(self.index).map(|token| token.span.start).unwrap_or(self.source.len())
    }

    fn parse_nodes_until(&mut self, mut stop: impl FnMut(Boundary) -> bool) -> Result<TgRoot, TgParseError> {
        let mut nodes = Vec::new();
        while !self.is_at_end() {
            let checkpoint = self.index;
            match self.peek_kind() {
                TokenKind::Directive => {
                    let directive = self.parse_directive()?;
                    let boundary = match directive.keyword {
                        Some(TgKeyword::End) => Some(Boundary::End),
                        Some(TgKeyword::ElseIf) => Some(Boundary::ElseIf),
                        Some(TgKeyword::Else) => Some(Boundary::Else),
                        Some(TgKeyword::Case) => Some(Boundary::Case),
                        _ => None,
                    };
                    if let Some(boundary) = boundary {
                        if stop(boundary) {
                            self.index = checkpoint;
                            return Ok(nodes);
                        }
                        let kw = directive.keyword.map(|k| format!("{k:?}")).unwrap_or_else(|| "stmt".into());
                        return Err(self.error_at(directive.span.start, format!("unexpected `<% {kw} %>`")));
                    }
                    match directive.keyword {
                        Some(TgKeyword::If) => nodes.push(self.parse_if_block(directive)?),
                        Some(TgKeyword::Loop) => nodes.push(self.parse_loop_block(directive)?),
                        Some(TgKeyword::Match) => nodes.push(self.parse_match_block(directive)?),
                        _ => nodes.push(TgNode::Stmt { body: directive.rest, span: directive.span }),
                    }
                }
                TokenKind::Comment => nodes.push(self.parse_comment()?),
                TokenKind::Text => {
                    if self.stop_at_xml_boundary && self.is_xml_boundary_text(self.index) {
                        break;
                    }
                    nodes.push(self.parse_text_node()?);
                }
                TokenKind::Eof => break,
            }
        }
        Ok(nodes)
    }

    fn parse_if_block(&mut self, open: Directive) -> Result<TgNode, TgParseError> {
        let start = open.span.start;
        let saved_boundary = self.stop_at_xml_boundary;
        self.stop_at_xml_boundary = false;
        let mut arms = vec![TgIfArm {
            condition: Some(open.rest),
            body: self.parse_nodes_until(|b| matches!(b, Boundary::ElseIf | Boundary::Else | Boundary::End))?,
            span: open.span.clone(),
        }];
        loop {
            if !self.check(TokenKind::Directive) {
                return Err(self.error_at(start, "unclosed `<% if %>` block"));
            }
            let directive = self.parse_directive()?;
            match directive.keyword {
                Some(TgKeyword::End) => {
                    self.stop_at_xml_boundary = saved_boundary;
                    return Ok(TgNode::If(TgIf { arms, span: start..directive.span.end }));
                }
                Some(TgKeyword::ElseIf) => {
                    arms.push(TgIfArm {
                        condition: Some(directive.rest),
                        body: self.parse_nodes_until(|b| matches!(b, Boundary::ElseIf | Boundary::Else | Boundary::End))?,
                        span: directive.span,
                    });
                }
                Some(TgKeyword::Else) => {
                    arms.push(TgIfArm {
                        condition: None,
                        body: self.parse_nodes_until(|b| matches!(b, Boundary::ElseIf | Boundary::Else | Boundary::End))?,
                        span: directive.span,
                    });
                    let close = self.parse_directive()?;
                    if close.keyword != Some(TgKeyword::End) {
                        return Err(self.error_at(close.span.start, "expected `<% end %>` after `<% else %>`"));
                    }
                    self.stop_at_xml_boundary = saved_boundary;
                    return Ok(TgNode::If(TgIf { arms, span: start..close.span.end }));
                }
                _ => return Err(self.error_at(directive.span.start, "expected `<% else %>`, `<% else if %>` or `<% end %>`")),
            }
        }
    }

    fn parse_loop_block(&mut self, open: Directive) -> Result<TgNode, TgParseError> {
        let start = open.span.start;
        let saved_boundary = self.stop_at_xml_boundary;
        self.stop_at_xml_boundary = false;
        let body = self.parse_nodes_until(|b| b == Boundary::End)?;
        self.stop_at_xml_boundary = saved_boundary;
        let close = self.parse_directive()?;
        if close.keyword != Some(TgKeyword::End) {
            return Err(self.error_at(close.span.start, "expected `<% end %>` to close `<% loop %>`"));
        }
        Ok(TgNode::Loop(TgLoop { header: open.rest, body, span: start..close.span.end }))
    }

    fn parse_match_block(&mut self, open: Directive) -> Result<TgNode, TgParseError> {
        let start = open.span.start;
        let scrutinee = open.rest;
        let saved_boundary = self.stop_at_xml_boundary;
        let mut arms = Vec::new();
        loop {
            if !self.check(TokenKind::Directive) {
                return Err(self.error_at(start, "unclosed `<% match %>` block"));
            }
            let directive = self.parse_directive()?;
            match directive.keyword {
                Some(TgKeyword::Case) => {
                    self.stop_at_xml_boundary = false;
                    arms.push(TgMatchArm {
                        pattern: Some(directive.rest),
                        body: self.parse_nodes_until(|b| matches!(b, Boundary::Case | Boundary::Else | Boundary::End))?,
                        span: directive.span,
                    });
                    self.stop_at_xml_boundary = saved_boundary;
                }
                Some(TgKeyword::Else) => {
                    self.stop_at_xml_boundary = false;
                    arms.push(TgMatchArm {
                        pattern: None,
                        body: self.parse_nodes_until(|b| matches!(b, Boundary::Case | Boundary::Else | Boundary::End))?,
                        span: directive.span,
                    });
                    self.stop_at_xml_boundary = saved_boundary;
                    let close = self.parse_directive()?;
                    if close.keyword != Some(TgKeyword::End) {
                        return Err(self.error_at(close.span.start, "expected `<% end %>` after `<% else %>`"));
                    }
                    return Ok(TgNode::Match(TgMatch { scrutinee: scrutinee.clone(), arms, span: start..close.span.end }));
                }
                Some(TgKeyword::End) => {
                    if arms.is_empty() {
                        return Err(self.error_at(directive.span.start, "expected `<% case %>` or `<% else %>` before `<% end %>`"));
                    }
                    return Ok(TgNode::Match(TgMatch { scrutinee, arms, span: start..directive.span.end }));
                }
                _ => return Err(self.error_at(directive.span.start, "expected `<% case %>`, `<% else %>` or `<% end %>`")),
            }
        }
    }

    fn is_xml_boundary_text(&self, index: usize) -> bool {
        let Some(token) = self.tokens.get(index)
        else {
            return false;
        };
        if token.kind != TokenKind::Text {
            return false;
        }
        let text = self.source[token.span.clone()].trim_start();
        text.starts_with('<') && !text.starts_with("<%") && !text.starts_with("<#")
    }

    fn parse_text_node(&mut self) -> Result<TgNode, TgParseError> {
        let token = self.advance();
        let span = token.span.clone();
        let parts = parse_text_parts(&self.source[span.clone()]);
        Ok(TgNode::Text { parts, span })
    }

    fn parse_comment(&mut self) -> Result<TgNode, TgParseError> {
        let token = self.advance();
        Ok(TgNode::Comment { span: token.span })
    }

    fn parse_directive(&mut self) -> Result<Directive, TgParseError> {
        let token = self.advance();
        if token.kind != TokenKind::Directive {
            return Err(self.error_at(token.span.start, "expected directive token"));
        }
        let span = token.span.clone();
        let raw = &self.source[span.clone()];
        let inner = raw
            .strip_prefix("<%")
            .and_then(|text| text.strip_suffix("%>"))
            .ok_or_else(|| self.error_at(span.start, "invalid directive token"))?;
        let rest = inner.trim().to_string();
        let (keyword, payload) = classify_directive(&rest);
        Ok(Directive { keyword, rest: payload, span })
    }

    fn peek_kind(&self) -> TokenKind {
        self.tokens.get(self.index).map(|token| token.kind).unwrap_or(TokenKind::Eof)
    }

    fn check(&self, kind: TokenKind) -> bool {
        self.peek_kind() == kind
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

    fn error_at(&self, pos: usize, message: impl Into<String>) -> TgParseError {
        TgParseError { message: message.into(), span: pos..pos.saturating_add(1) }
    }
}

struct Directive {
    keyword: Option<TgKeyword>,
    rest: String,
    span: Range<usize>,
}

fn parse_text_parts(text: &str) -> Vec<TgTextPart> {
    let mut parts = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        if start > 0 {
            parts.push(TgTextPart::Static(rest[..start].to_string()));
        }
        match parse_braced_expression(rest, start) {
            Some((expr, consumed)) => {
                parts.push(TgTextPart::Expression(expr));
                rest = &rest[start + consumed..];
            }
            None => {
                parts.push(TgTextPart::Static(rest.to_string()));
                return parts;
            }
        }
    }
    if !rest.is_empty() {
        parts.push(TgTextPart::Static(rest.to_string()));
    }
    if parts.is_empty() {
        parts.push(TgTextPart::Static(String::new()));
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

fn classify_directive(inner: &str) -> (Option<TgKeyword>, String) {
    let trimmed = inner.trim();
    // `end` 后可跟可选标签（如 `end match` / `end if`），与 `<% end %>` 等价。
    if trimmed == "end" || trimmed.starts_with("end ") {
        return (Some(TgKeyword::End), String::new());
    }
    if trimmed == "else" {
        return (Some(TgKeyword::Else), String::new());
    }
    if let Some(rest) = trimmed.strip_prefix("else if ") {
        return (Some(TgKeyword::ElseIf), rest.trim().to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("if ") {
        return (Some(TgKeyword::If), rest.trim().to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("loop ") {
        return (Some(TgKeyword::Loop), rest.trim().to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("match ") {
        return (Some(TgKeyword::Match), rest.trim().to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("case ") {
        return (Some(TgKeyword::Case), rest.trim().to_string());
    }
    (None, trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn end_directive_accepts_optional_label_suffix() {
        assert_eq!(classify_directive("end"), (Some(TgKeyword::End), String::new()));
        assert_eq!(classify_directive("end match"), (Some(TgKeyword::End), String::new()));
        assert_eq!(classify_directive("end if"), (Some(TgKeyword::End), String::new()));
        assert_eq!(classify_directive("end loop"), (Some(TgKeyword::End), String::new()));
    }

    #[test]
    fn parse_match_block_closed_by_end_match() {
        let source = r#"<% match arch %>
<% case "clr" %>
clr_only()
<% else %>
other()
<% end match %>"#;
        let root = parse_tgrammar_fragment(source).expect("parse").0;
        assert_eq!(root.len(), 1);
        let TgNode::Match(match_node) = &root[0]
        else {
            panic!("expected match node");
        };
        assert_eq!(match_node.scrutinee, "arch");
        assert_eq!(match_node.arms.len(), 2);
    }
}
