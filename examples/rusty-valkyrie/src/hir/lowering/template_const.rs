//! TGrammar 编译期常量求值（`arch` 谓词、字面量、整数范围）。

use super::errors::{TemplateConstruct, TemplateFailure};
use crate::HirError;
use nyar_types::DiagnosticSpan;
use oak_core::{Token, TokenType};
use oak_valkyrie::lexer::{ValkyrieKeywords, token_type::ValkyrieTokenType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ConstantValue {
    Utf8(String),
    Integer64(i64),
    Boolean(bool),
}

#[doc(hidden)]
pub fn eval_const_condition(source: &str, tokens: &[Token<ValkyrieTokenType>], arch: &str) -> Result<bool, HirError> {
    if tokens.is_empty() {
        return Ok(true);
    }
    match eval_const_expr(source, tokens, arch)? {
        ConstantValue::Boolean(value) => Ok(value),
        ConstantValue::Integer64(value) => Ok(value != 0),
        ConstantValue::Utf8(value) => Ok(!value.is_empty()),
    }
}

pub(super) fn eval_const_expr(source: &str, tokens: &[Token<ValkyrieTokenType>], arch: &str) -> Result<ConstantValue, HirError> {
    let meaningful = meaningful_tokens(tokens);
    if meaningful.is_empty() {
        return Err(template_failure(TemplateConstruct::MetaExpression, TemplateFailure::EmptyExpression, tokens_span(tokens)));
    }
    let mut parser = ConstExprParser { source, tokens: &meaningful, index: 0, arch, span: tokens_span(&meaningful) };
    let value = parser.parse_or()?;
    if parser.index < parser.tokens.len() {
        return Err(template_failure(
            TemplateConstruct::MetaExpression,
            TemplateFailure::TrailingTokens,
            tokens_span(&parser.tokens[parser.index..]),
        ));
    }
    Ok(value)
}

/// 构造模板展开失败事实；span 指向触发失败的 token 位置。
fn template_failure(construct: TemplateConstruct, failure: TemplateFailure, span: DiagnosticSpan) -> HirError {
    HirError::template_expansion_failed(construct, failure, span)
}

/// token 流的源码跨度；空流退化为 `0..0`。
fn tokens_span(tokens: &[Token<ValkyrieTokenType>]) -> DiagnosticSpan {
    match (tokens.first(), tokens.last()) {
        (Some(first), Some(last)) => DiagnosticSpan { start: first.span.start, end: last.span.end },
        _ => DiagnosticSpan { start: 0, end: 0 },
    }
}

/// 单个 token 的源码跨度。
fn token_span(token: &Token<ValkyrieTokenType>) -> DiagnosticSpan {
    DiagnosticSpan { start: token.span.start, end: token.span.end }
}

pub(super) fn const_value_to_source(value: &ConstantValue) -> String {
    match value {
        ConstantValue::Utf8(text) => format!("\"{}\"", escape_string(text)),
        ConstantValue::Integer64(value) => value.to_string(),
        ConstantValue::Boolean(value) => {
            if *value {
                "true".to_string()
            }
            else {
                "false".to_string()
            }
        }
    }
}

/// `loop _ in start..end` 的半开整数区间 `[start, end)`。
#[doc(hidden)]
pub fn parse_const_loop_range(source: &str, header: &[Token<ValkyrieTokenType>], arch: &str) -> Result<std::ops::Range<i64>, HirError> {
    let tokens = meaningful_tokens(header);
    let header_span = tokens_span(&tokens);
    if tokens.len() < 3 {
        return Err(template_failure(TemplateConstruct::Loop, TemplateFailure::LoopHeaderTooShort, header_span));
    }
    if !is_discard_binding(source, &tokens[0]) {
        return Err(template_failure(TemplateConstruct::Loop, TemplateFailure::LoopBindingNotDiscard, token_span(&tokens[0])));
    }
    if tokens[1].kind != ValkyrieTokenType::Keyword(ValkyrieKeywords::In) {
        return Err(template_failure(TemplateConstruct::Loop, TemplateFailure::LoopMissingInKeyword, header_span));
    }
    let range = parse_const_range_tokens(source, &tokens[2..], header_span.clone())?;
    if range.start > range.end {
        return Err(template_failure(
            TemplateConstruct::Loop,
            TemplateFailure::LoopRangeInvalid { start: range.start, end: range.end },
            header_span,
        ));
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

fn parse_const_range_tokens(
    source: &str,
    tokens: &[Token<ValkyrieTokenType>],
    header_span: DiagnosticSpan,
) -> Result<std::ops::Range<i64>, HirError> {
    if tokens.is_empty() {
        return Err(template_failure(TemplateConstruct::Loop, TemplateFailure::LoopRangeMissing, header_span));
    }
    if tokens.len() >= 3 && tokens[1].kind == ValkyrieTokenType::DotDot {
        let start = parse_const_int_token(source, &tokens[0])?;
        let end = parse_const_int_token(source, &tokens[2])?;
        if tokens.len() > 3 {
            return Err(template_failure(TemplateConstruct::Loop, TemplateFailure::LoopHeaderTrailingTokens, tokens_span(&tokens[3..])));
        }
        return Ok(start..end);
    }
    if tokens.len() == 1 {
        if let Some(range) = parse_const_range_text(&token_text(source, &tokens[0])) {
            return Ok(range);
        }
    }
    Err(template_failure(TemplateConstruct::Loop, TemplateFailure::RangeShapeUnsupported, header_span))
}

#[doc(hidden)]
pub fn parse_const_range_text(text: &str) -> Option<std::ops::Range<i64>> {
    let text = text.trim();
    let split = text.find("..")?;
    let start = text[..split].trim().parse::<i64>().ok()?;
    let end = text[split + 2..].trim().parse::<i64>().ok()?;
    Some(start..end)
}

fn parse_const_int_token(source: &str, token: &Token<ValkyrieTokenType>) -> Result<i64, HirError> {
    let endpoint_failure = || template_failure(TemplateConstruct::Loop, TemplateFailure::RangeEndpointNotInteger, token_span(token));
    match token.kind {
        ValkyrieTokenType::IntegerLiteral => {}
        ValkyrieTokenType::FloatLiteral => {
            let text = token_text(source, token);
            if let Some(value) = text.split('.').next() {
                return value.parse::<i64>().map_err(|_| endpoint_failure());
            }
            return Err(endpoint_failure());
        }
        _ => return Err(endpoint_failure()),
    }
    token_text(source, token).parse::<i64>().map_err(|_| endpoint_failure())
}

struct ConstExprParser<'a> {
    source: &'a str,
    tokens: &'a [Token<ValkyrieTokenType>],
    index: usize,
    arch: &'a str,
    /// 整个元表达式的源码跨度。
    span: DiagnosticSpan,
}

impl ConstExprParser<'_> {
    /// 以整个元表达式 span 记录一次求值失败。
    fn fail(&self, failure: TemplateFailure) -> HirError {
        template_failure(TemplateConstruct::MetaExpression, failure, self.span.clone())
    }

    /// 以指定 token span 记录一次求值失败。
    fn fail_at(&self, failure: TemplateFailure, token: &Token<ValkyrieTokenType>) -> HirError {
        template_failure(TemplateConstruct::MetaExpression, failure, token_span(token))
    }

    fn parse_or(&mut self) -> Result<ConstantValue, HirError> {
        let mut value = self.parse_and()?;
        while self.match_kind(ValkyrieTokenType::OrOr) {
            let rhs = self.parse_and()?;
            value = ConstantValue::Boolean(value.as_bool()? || rhs.as_bool()?);
        }
        Ok(value)
    }

    fn parse_and(&mut self) -> Result<ConstantValue, HirError> {
        let mut value = self.parse_equality()?;
        while self.match_kind(ValkyrieTokenType::AndAnd) {
            let rhs = self.parse_equality()?;
            value = ConstantValue::Boolean(value.as_bool()? && rhs.as_bool()?);
        }
        Ok(value)
    }

    fn parse_equality(&mut self) -> Result<ConstantValue, HirError> {
        let left = self.parse_unary()?;
        if self.match_kind(ValkyrieTokenType::EqEq) {
            let right = self.parse_unary()?;
            return Ok(ConstantValue::Boolean(self.values_equal(&left, &right)?));
        }
        if self.match_kind(ValkyrieTokenType::NotEq) {
            let right = self.parse_unary()?;
            return Ok(ConstantValue::Boolean(!self.values_equal(&left, &right)?));
        }
        Ok(left)
    }

    fn values_equal(&self, left: &ConstantValue, right: &ConstantValue) -> Result<bool, HirError> {
        values_equal(left, right).ok_or_else(|| self.fail(TemplateFailure::IncompatibleComparisonOperands))
    }

    fn parse_unary(&mut self) -> Result<ConstantValue, HirError> {
        if self.match_kind(ValkyrieTokenType::Bang) {
            return Ok(ConstantValue::Boolean(!self.parse_unary()?.as_bool()?));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<ConstantValue, HirError> {
        if self.match_kind(ValkyrieTokenType::ParenthesisL) {
            let value = self.parse_or()?;
            if !self.match_kind(ValkyrieTokenType::ParenthesisR) {
                return Err(HirError::template_expansion_failed(
                    TemplateConstruct::MetaExpression,
                    TemplateFailure::UnclosedParenthesis,
                    self.span.clone(),
                ));
            }
            return Ok(value);
        }

        let token = self.current().ok_or_else(|| self.fail(TemplateFailure::IncompleteExpression))?;
        self.index += 1;
        match token.kind {
            ValkyrieTokenType::BoolLiteral => Ok(ConstantValue::Boolean(token_text(self.source, &token) == "true")),
            ValkyrieTokenType::StringLiteral => Ok(ConstantValue::Utf8(decode_quoted(&token_text(self.source, &token)))),
            ValkyrieTokenType::IntegerLiteral => token_text(self.source, &token)
                .parse::<i64>()
                .map(ConstantValue::Integer64)
                .map_err(|_| self.fail_at(TemplateFailure::IntegerLiteralUnparsable, &token)),
            ValkyrieTokenType::Identifier if token_text(self.source, &token) == "arch" => Ok(ConstantValue::Utf8(self.arch.to_string())),
            _ => Err(self.fail_at(TemplateFailure::UnevaluableToken, &token)),
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

impl ConstantValue {
    fn as_bool(&self) -> Result<bool, HirError> {
        match self {
            Self::Boolean(value) => Ok(*value),
            Self::Integer64(value) => Ok(*value != 0),
            Self::Utf8(value) => Ok(!value.is_empty()),
        }
    }
}

fn values_equal(left: &ConstantValue, right: &ConstantValue) -> Option<bool> {
    match (left, right) {
        (ConstantValue::Boolean(lhs), ConstantValue::Boolean(rhs)) => Some(lhs == rhs),
        (ConstantValue::Integer64(lhs), ConstantValue::Integer64(rhs)) => Some(lhs == rhs),
        (ConstantValue::Utf8(lhs), ConstantValue::Utf8(rhs)) => Some(lhs == rhs),
        (ConstantValue::Integer64(lhs), ConstantValue::Utf8(rhs)) | (ConstantValue::Utf8(rhs), ConstantValue::Integer64(lhs)) => {
            Some(lhs.to_string() == *rhs)
        }
        (ConstantValue::Boolean(lhs), ConstantValue::Integer64(rhs)) | (ConstantValue::Integer64(rhs), ConstantValue::Boolean(lhs)) => {
            Some((*lhs && *rhs != 0) || (!*lhs && *rhs == 0))
        }
        _ => None,
    }
}
