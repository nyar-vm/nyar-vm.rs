//! TGrammar 编译期常量求值（`arch` 谓词、字面量、整数范围）。

use crate::valkyrie::frontend::ParseError;
use oak_core::{Token, TokenType};
use oak_valkyrie::lexer::{ValkyrieKeywords, token_type::ValkyrieTokenType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ConstValue {
    String(String),
    Int(i64),
    Bool(bool),
}

pub(super) fn eval_const_condition(source: &str, tokens: &[Token<ValkyrieTokenType>], arch: &str) -> Result<bool, ParseError> {
    if tokens.is_empty() {
        return Ok(true);
    }
    match eval_const_expr(source, tokens, arch)? {
        ConstValue::Bool(value) => Ok(value),
        ConstValue::Int(value) => Ok(value != 0),
        ConstValue::String(value) => Ok(!value.is_empty()),
    }
}

pub(super) fn eval_const_expr(source: &str, tokens: &[Token<ValkyrieTokenType>], arch: &str) -> Result<ConstValue, ParseError> {
    let meaningful = meaningful_tokens(tokens);
    if meaningful.is_empty() {
        return Err(ParseError::invalid("TGrammar 元表达式为空"));
    }
    let mut parser = ConstExprParser { source, tokens: &meaningful, index: 0, arch };
    let value = parser.parse_or()?;
    if parser.index < parser.tokens.len() {
        return Err(ParseError::invalid("TGrammar 元表达式含有无法求值的语法"));
    }
    Ok(value)
}

pub(super) fn const_value_to_source(value: &ConstValue) -> String {
    match value {
        ConstValue::String(text) => format!("\"{}\"", escape_string(text)),
        ConstValue::Int(value) => value.to_string(),
        ConstValue::Bool(value) => if *value { "true".to_string() } else { "false".to_string() },
    }
}

/// `loop _ in start..end` 的半开整数区间 `[start, end)`。
pub(super) fn parse_const_loop_range(source: &str, header: &[Token<ValkyrieTokenType>], arch: &str) -> Result<std::ops::Range<i64>, ParseError> {
    let tokens = meaningful_tokens(header);
    if tokens.len() < 3 {
        return Err(ParseError::invalid("TGrammar `loop` 仅支持 `loop _ in start..end` 常量整数范围"));
    }
    if !is_discard_binding(source, &tokens[0]) {
        return Err(ParseError::invalid("TGrammar `loop` 当前仅支持 `_` 绑定名的常量范围展开"));
    }
    if tokens[1].kind != ValkyrieTokenType::Keyword(ValkyrieKeywords::In) {
        return Err(ParseError::invalid("TGrammar `loop` 头部必须是 `_ in start..end`"));
    }
    let range = parse_const_range_tokens(source, &tokens[2..])?;
    if range.start > range.end {
        return Err(ParseError::invalid(format!("TGrammar `loop` 范围无效: {}..{}", range.start, range.end)));
    }
    let _ = arch;
    Ok(range)
}

fn meaningful_tokens(tokens: &[Token<ValkyrieTokenType>]) -> Vec<Token<ValkyrieTokenType>> {
    tokens.iter().filter(|token| !token.kind.is_ignored()).copied().collect()
}

fn token_text(source: &str, token: &Token<ValkyrieTokenType>) -> String {
    source[token.span.clone()].trim().to_string()
}

fn decode_quoted(text: &str) -> String {
    let text = text.trim();
    if (text.starts_with('"') && text.ends_with('"')) || (text.starts_with('\'') && text.ends_with('\'')) {
        text[1..text.len().saturating_sub(1)].to_string()
    }
    else {
        text.to_string()
    }
}

fn escape_string(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn is_discard_binding(source: &str, token: &Token<ValkyrieTokenType>) -> bool {
    token.kind == ValkyrieTokenType::Underscore || (token.kind == ValkyrieTokenType::Identifier && token_text(source, token) == "_")
}

fn parse_const_range_tokens(source: &str, tokens: &[Token<ValkyrieTokenType>]) -> Result<std::ops::Range<i64>, ParseError> {
    if tokens.is_empty() {
        return Err(ParseError::invalid("TGrammar `loop` 缺少范围表达式"));
    }
    if tokens.len() >= 3 && tokens[1].kind == ValkyrieTokenType::DotDot {
        let start = parse_const_int_token(source, &tokens[0])?;
        let end = parse_const_int_token(source, &tokens[2])?;
        if tokens.len() > 3 {
            return Err(ParseError::invalid("TGrammar `loop` 头部含有无法求值的语法"));
        }
        return Ok(start..end);
    }
    if tokens.len() == 1 {
        if let Some(range) = parse_const_range_text(&token_text(source, &tokens[0])) {
            return Ok(range);
        }
    }
    Err(ParseError::invalid("TGrammar `loop` 范围必须是 `start..end` 常量整数"))
}

fn parse_const_range_text(text: &str) -> Option<std::ops::Range<i64>> {
    let text = text.trim();
    let split = text.find("..")?;
    let start = text[..split].trim().parse::<i64>().ok()?;
    let end = text[split + 2..].trim().parse::<i64>().ok()?;
    Some(start..end)
}

fn parse_const_int_token(source: &str, token: &Token<ValkyrieTokenType>) -> Result<i64, ParseError> {
    match token.kind {
        ValkyrieTokenType::IntegerLiteral => {}
        ValkyrieTokenType::FloatLiteral => {
            let text = token_text(source, token);
            if let Some(value) = text.split('.').next() {
                return value
                    .parse::<i64>()
                    .map_err(|_| ParseError::invalid("TGrammar 范围端点必须是整数字面量"));
            }
            return Err(ParseError::invalid("TGrammar 范围端点必须是整数字面量"));
        }
        _ => return Err(ParseError::invalid("TGrammar 范围端点必须是整数字面量")),
    }
    token_text(source, token)
        .parse::<i64>()
        .map_err(|_| ParseError::invalid("TGrammar 范围端点必须是整数字面量"))
}

struct ConstExprParser<'a> {
    source: &'a str,
    tokens: &'a [Token<ValkyrieTokenType>],
    index: usize,
    arch: &'a str,
}

impl ConstExprParser<'_> {
    fn parse_or(&mut self) -> Result<ConstValue, ParseError> {
        let mut value = self.parse_and()?;
        while self.match_kind(ValkyrieTokenType::OrOr) {
            let rhs = self.parse_and()?;
            value = ConstValue::Bool(value.as_bool()? || rhs.as_bool()?);
        }
        Ok(value)
    }

    fn parse_and(&mut self) -> Result<ConstValue, ParseError> {
        let mut value = self.parse_equality()?;
        while self.match_kind(ValkyrieTokenType::AndAnd) {
            let rhs = self.parse_equality()?;
            value = ConstValue::Bool(value.as_bool()? && rhs.as_bool()?);
        }
        Ok(value)
    }

    fn parse_equality(&mut self) -> Result<ConstValue, ParseError> {
        let left = self.parse_unary()?;
        if self.match_kind(ValkyrieTokenType::EqEq) {
            let right = self.parse_unary()?;
            return Ok(ConstValue::Bool(values_equal(&left, &right)?));
        }
        if self.match_kind(ValkyrieTokenType::NotEq) {
            let right = self.parse_unary()?;
            return Ok(ConstValue::Bool(!values_equal(&left, &right)?));
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<ConstValue, ParseError> {
        if self.match_kind(ValkyrieTokenType::Bang) {
            return Ok(ConstValue::Bool(!self.parse_unary()?.as_bool()?));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<ConstValue, ParseError> {
        if self.match_kind(ValkyrieTokenType::ParenthesisL) {
            let value = self.parse_or()?;
            if !self.match_kind(ValkyrieTokenType::ParenthesisR) {
                return Err(ParseError::invalid("TGrammar 元表达式括号未闭合"));
            }
            return Ok(value);
        }

        let token = self.current().ok_or_else(|| ParseError::invalid("TGrammar 元表达式不完整"))?;
        self.index += 1;
        match token.kind {
            ValkyrieTokenType::BoolLiteral => Ok(ConstValue::Bool(token_text(self.source, &token) == "true")),
            ValkyrieTokenType::StringLiteral => Ok(ConstValue::String(decode_quoted(&token_text(self.source, &token)))),
            ValkyrieTokenType::IntegerLiteral => token_text(self.source, &token)
                .parse::<i64>()
                .map(ConstValue::Int)
                .map_err(|_| ParseError::invalid("TGrammar 整数字面量无法解析")),
            ValkyrieTokenType::Identifier if token_text(self.source, &token) == "arch" => Ok(ConstValue::String(self.arch.to_string())),
            _ => Err(ParseError::invalid("TGrammar 元表达式含有无法求值的 token")),
        }
    }

    fn current(&self) -> Option<Token<ValkyrieTokenType>> {
        self.tokens.get(self.index).copied()
    }

    fn match_kind(&mut self, kind: ValkyrieTokenType) -> bool {
        if self.current().is_some_and(|token| token.kind == kind) {
            self.index += 1;
            true
        }
        else {
            false
        }
    }
}

impl ConstValue {
    fn as_bool(&self) -> Result<bool, ParseError> {
        match self {
            Self::Bool(value) => Ok(*value),
            Self::Int(value) => Ok(*value != 0),
            Self::String(value) => Ok(!value.is_empty()),
        }
    }
}

fn values_equal(left: &ConstValue, right: &ConstValue) -> Result<bool, ParseError> {
    match (left, right) {
        (ConstValue::Bool(lhs), ConstValue::Bool(rhs)) => Ok(lhs == rhs),
        (ConstValue::Int(lhs), ConstValue::Int(rhs)) => Ok(lhs == rhs),
        (ConstValue::String(lhs), ConstValue::String(rhs)) => Ok(lhs == rhs),
        (ConstValue::Int(lhs), ConstValue::String(rhs)) | (ConstValue::String(rhs), ConstValue::Int(lhs)) => {
            Ok(lhs.to_string() == *rhs)
        }
        (ConstValue::Bool(lhs), ConstValue::Int(rhs)) | (ConstValue::Int(rhs), ConstValue::Bool(lhs)) => Ok((*lhs && *rhs != 0) || (!*lhs && *rhs == 0)),
        _ => Err(ParseError::invalid("TGrammar 比较两侧类型不兼容")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oak_core::Token;

    fn tokens_from(source: &str, kinds: &[(ValkyrieTokenType, std::ops::Range<usize>)]) -> Vec<Token<ValkyrieTokenType>> {
        kinds
            .iter()
            .map(|(kind, span)| Token { kind: *kind, span: oak_core::Range { start: span.start, end: span.end } })
            .collect()
    }

    #[test]
    fn range_i64_iterates_twice_for_zero_to_two() {
        let range = parse_const_range_text("0..2").expect("range");
        assert_eq!(range, 0..2);
        assert_eq!(range.count(), 2);
    }

    #[test]
    fn parse_loop_range_from_parsed_header() {
        use crate::valkyrie::frontend::{ast::Statement, parse_source};
        use oak_valkyrie::ast::{StatementNode, TemplateNode};
        let source = r#"micro main() -> i32 {
<% loop _ in 0..2 %>
return 1
<% end %>
}"#;
        let root = parse_source(source).expect("parse");
        let StatementNode::Micro(micro) = &root.items[0] else { panic!("expected micro") };
        let Statement::Template(template) = &micro.body.statements[0] else { panic!("expected template") };
        let TemplateNode::Loop(loop_node) = template.as_ref() else { panic!("expected loop") };
        assert_eq!(parse_const_loop_range(source, &loop_node.header, "native").expect("range"), 0..2);
    }

    #[test]
    fn parse_loop_range_from_compact_literal() {
        let source = "_ in 0..2";
        let tokens = tokens_from(
            source,
            &[
                (ValkyrieTokenType::Underscore, 0..1),
                (ValkyrieTokenType::Keyword(ValkyrieKeywords::In), 2..4),
                (ValkyrieTokenType::FloatLiteral, 5..9),
            ],
        );
        assert_eq!(parse_const_loop_range(source, &tokens, "native").expect("range"), 0..2);
    }

    #[test]
    fn eval_arch_equality_predicate() {
        let source = r#"arch == "wasm32""#;
        let tokens = tokens_from(
            source,
            &[
                (ValkyrieTokenType::Identifier, 0..4),
                (ValkyrieTokenType::EqEq, 5..7),
                (ValkyrieTokenType::StringLiteral, 8..16),
            ],
        );
        assert!(eval_const_condition(source, &tokens, "wasm32").expect("eval"));
        assert!(!eval_const_condition(source, &tokens, "native").expect("eval"));
    }
}
