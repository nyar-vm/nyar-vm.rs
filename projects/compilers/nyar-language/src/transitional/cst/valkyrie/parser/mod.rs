use std::{
    fmt::{Display, Formatter},
    path::PathBuf,
};

use miette::{Diagnostic, LabeledSpan as MietteLabeledSpan, Severity};

mod control_flow;
mod declarations;
mod expression_control;
mod match_patterns;
mod support;
mod template_literal;
mod types;
mod vx_fixup;
mod xml_markup;

#[cfg(test)]
mod combined_probe;

pub use vx_fixup::fixup_vx_widget_view_markup;

use self::support::{
    ensure_value_expression, expression_can_participate_in_value_composition, expression_requires_statement, parse_string_literal, span,
    with_term_span, with_type_span,
};

use crate::transitional::cst::valkyrie::{
    ast::{
        BinaryOperator, LiteralExpression, NamePath, TermAsExpression, TermBinaryExpression, TermExpression, TermUnaryExpression,
        UnaryOperator, ValkyrieRoot,
    },
    lexer::{Keyword, Lexer, Token, TokenKind},
};
use std::ops::Range;

const DECLARATION_MODIFIERS: &[&str] = &[
    "public",
    "private",
    "protected",
    "internal",
    "open",
    "sealed",
    "abstract",
    "final",
    "readonly",
    "virtual",
    "override",
    "static",
    "mut",
    "get",
    "set",
    "wire",
    "lazy",
];

/// Parser-side error type.
#[derive(Debug)]
pub enum ParseError {
    /// File read failure.
    Io(std::io::Error),
    /// Syntactically invalid source text.
    Invalid {
        /// 错误消息。
        message: String,
        /// 可选源码范围。
        span: Option<Range<usize>>,
    },
}

impl ParseError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid { message: message.into(), span: None }
    }

    pub fn invalid_at(message: impl Into<String>, span: Range<usize>) -> Self {
        Self::Invalid { message: message.into(), span: Some(span) }
    }
}

impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => Display::fmt(error, f),
            Self::Invalid { message, .. } => write!(f, "{}", message),
        }
    }
}

impl std::error::Error for ParseError {}

impl Diagnostic for ParseError {
    fn code<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        Some(Box::new(match self {
            ParseError::Io(_) => "valkyrie::parser::io",
            ParseError::Invalid { .. } => "valkyrie::parser::invalid",
        }))
    }

    fn severity(&self) -> Option<Severity> {
        Some(Severity::Error)
    }

    fn help<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        Some(Box::new(match self {
            ParseError::Io(_) => "请确认源文件存在且当前进程具备读取权限",
            ParseError::Invalid { .. } => "请检查语法是否完整，尤其是声明头、括号、属性与对象体",
        }))
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = MietteLabeledSpan> + '_>> {
        match self {
            ParseError::Invalid { span: Some(span), .. } => {
                let labeled =
                    MietteLabeledSpan::new_with_span(Some("解析失败位置".to_string()), (span.start, span.end.saturating_sub(span.start)));
                Some(Box::new(std::iter::once(labeled)))
            }
            _ => None,
        }
    }
}

impl From<std::io::Error> for ParseError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    index: usize,
    /// 当为 true 时，`{` 不作为结构体构造 postfix 处理（用于 if/while/loop/match 条件解析）。
    suppress_struct_constructor: bool,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str, tokens: Vec<Token>) -> Self {
        Self { source, tokens, index: 0, suppress_struct_constructor: false }
    }

    fn parse_root(&mut self) -> Result<ValkyrieRoot, ParseError> {
        let mut statements = Vec::new();
        while !self.is_eof() {
            if self.match_symbol(TokenKind::Semicolon) {
                continue;
            }

            let annotations = self.parse_annotations()?;
            if self.is_eof() {
                if annotations.attribute_lists.is_empty() && annotations.modifiers.is_empty() {
                    break;
                }
                return Err(ParseError::invalid("dangling attributes or modifiers at end of file"));
            }

            statements.push(self.parse_declaration(annotations)?);
        }
        Ok(ValkyrieRoot { statements })
    }

    fn parse_expression_bp(&mut self, min_bp: u8) -> Result<TermExpression, ParseError> {
        let mut lhs = self.parse_expression_prefix()?;

        loop {
            if let Some((left_bp, _right_bp)) = self.expression_postfix_binding_power() {
                if left_bp < min_bp {
                    break;
                }
                if !expression_can_participate_in_value_composition(&lhs) {
                    return Err(ParseError::invalid_at("该控制流表达式不能继续参与调用、成员访问或下标组合", lhs.span().clone()));
                }
                lhs = self.parse_expression_postfix(lhs)?;
                continue;
            }

            if self.check_token_keyword(Keyword::As) {
                let (left_bp, _right_bp) = (45, 46);
                if left_bp < min_bp {
                    break;
                }
                if !expression_can_participate_in_value_composition(&lhs) {
                    return Err(ParseError::invalid_at("该控制流表达式不能继续参与类型转换", lhs.span().clone()));
                }
                let start = lhs.span().start;
                self.bump();
                let ty = self.parse_intersection_type_expression()?;
                let end = self.previous().span.end;
                lhs = TermExpression::As(Box::new(TermAsExpression { base: lhs, target: ty, span: span(start, end) }));
                continue;
            }

            if let Some(op) = self.compound_assignment_operator() {
                let (left_bp, right_bp) = (10, 9);
                if left_bp < min_bp {
                    break;
                }
                if !expression_can_participate_in_value_composition(&lhs) {
                    return Err(ParseError::invalid_at("该控制流表达式不能作为赋值目标继续组合", lhs.span().clone()));
                }
                let start = lhs.span().start;
                self.bump();
                let target = lhs.clone();
                let rhs = self.parse_value_expression_bp(right_bp)?;
                let end = rhs.span().end;
                let combined =
                    TermExpression::Binary(Box::new(TermBinaryExpression { operator: op, lhs: target.clone(), rhs, span: span(start, end) }));
                lhs = TermExpression::Assign { target: Box::new(target), value: Box::new(combined), span: span(start, end) };
                continue;
            }

            if self.check_symbol(TokenKind::Equal) {
                let (left_bp, right_bp) = (10, 9);
                if left_bp < min_bp {
                    break;
                }
                if !expression_can_participate_in_value_composition(&lhs) {
                    return Err(ParseError::invalid_at("该控制流表达式不能作为赋值目标继续组合", lhs.span().clone()));
                }
                let start = lhs.span().start;
                self.bump();
                let value = self.parse_value_expression_bp(right_bp)?;
                let end = value.span().end;
                lhs = TermExpression::Assign { target: Box::new(lhs), value: Box::new(value), span: span(start, end) };
                continue;
            }

            let Some((op, left_bp, right_bp)) = self.expression_infix_binding_power()
            else {
                break;
            };
            if left_bp < min_bp {
                break;
            }
            if !expression_can_participate_in_value_composition(&lhs) {
                return Err(ParseError::invalid_at("该控制流表达式不能继续参与中缀组合", lhs.span().clone()));
            }

            let start = lhs.span().start;
            self.bump();
            let rhs = self.parse_value_expression_bp(right_bp)?;
            let end = rhs.span().end;
            lhs = TermExpression::Binary(Box::new(TermBinaryExpression { operator: op, lhs, rhs, span: span(start, end) }));
        }

        Ok(lhs)
    }

    fn parse_value_expression_bp(&mut self, min_bp: u8) -> Result<TermExpression, ParseError> {
        let expression = self.parse_expression_bp(min_bp)?;
        ensure_value_expression(&expression)?;
        Ok(expression)
    }

    fn parse_expression_prefix(&mut self) -> Result<TermExpression, ParseError> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Keyword(Keyword::True) | TokenKind::Keyword(Keyword::False) => {
                let value = token.kind == TokenKind::Keyword(Keyword::True);
                self.bump();
                Ok(TermExpression::Literal { literal: LiteralExpression::Bool(value), span: token.span })
            }
            TokenKind::Apostrophe => self.parse_labeled_loop_expression(),
            TokenKind::Keyword(Keyword::Raise) => self.parse_raise_expression(),
            TokenKind::Keyword(Keyword::Catch) => self.parse_catch_expression(),
            TokenKind::Keyword(Keyword::If) => self.parse_if_expression(),
            TokenKind::Keyword(Keyword::Loop) => self.parse_loop_expression(),
            TokenKind::Keyword(Keyword::While) => self.parse_while_expression(),
            TokenKind::Keyword(Keyword::Case) => self.parse_case_expression(),
            TokenKind::Keyword(Keyword::Match) => self.parse_match_expression(),
            TokenKind::Keyword(Keyword::Until) => self.parse_until_expression(),
            TokenKind::Keyword(Keyword::Try) => self.parse_try_expression(),
            TokenKind::Keyword(Keyword::Micro) => self.parse_lambda_expression(),
            TokenKind::At => self.parse_macro_invoke_expression(),
            TokenKind::Keyword(Keyword::Null) => {
                self.bump();
                Ok(TermExpression::Literal { literal: LiteralExpression::Null, span: token.span })
            }
            TokenKind::Identifier => {
                let path = self.parse_name_path()?;
                let span = path.span.clone();
                Ok(TermExpression::Name { path, span })
            }
            // Soft keywords as names (`flags`, `type`, …) plus Some/None/self.
            // Control-flow introducers (if/match/loop/…) are matched above.
            TokenKind::Keyword(keyword) => {
                self.bump();
                let path = NamePath { parts: vec![keyword.as_str().to_string()], span: token.span.clone() };
                Ok(TermExpression::Name { path, span: token.span })
            }
            TokenKind::StringLiteral => {
                let span = token.span.clone();
                let raw = self.slice(span.clone()).to_string();
                self.bump();
                if template_literal::is_template_string_literal(&raw) {
                    self.parse_template_literal_expression(&raw, span)
                }
                else {
                    Ok(TermExpression::Literal { literal: LiteralExpression::String(parse_string_literal(&raw)?), span })
                }
            }
            TokenKind::IntegerLiteral => {
                self.bump();
                Ok(TermExpression::Literal {
                    literal: LiteralExpression::Integer(self.slice(token.span.clone()).to_string()),
                    span: token.span,
                })
            }
            TokenKind::FloatLiteral => {
                self.bump();
                Ok(TermExpression::Literal { literal: LiteralExpression::Float(self.slice(token.span.clone()).to_string()), span: token.span })
            }
            TokenKind::Minus | TokenKind::Bang => {
                let start = token.span.start;
                let op = match token.kind {
                    TokenKind::Minus => UnaryOperator::Neg,
                    TokenKind::Bang => UnaryOperator::Not,
                    _ => unreachable!(),
                };
                self.bump();
                let rhs = self.parse_value_expression_bp(80)?;
                let end = rhs.span().end;
                Ok(TermExpression::Unary(Box::new(TermUnaryExpression { operator: op, base: rhs, span: span(start, end) })))
            }
            TokenKind::LParen => self.parse_parenthesized_expression(),
            TokenKind::LBrace => self.parse_block_expression(),
            TokenKind::LBracket => self.parse_array_expression(),
            TokenKind::LAngle if self.peek_xml_markup_start() => self.parse_xml_markup_expression(),
            TokenKind::Keyword(Keyword::Class)
                if self.nth_is_symbol(1, TokenKind::Colon)
                    || self.nth_is_symbol(1, TokenKind::LBrace)
                    || self.nth_is_symbol(1, TokenKind::LParen) =>
            {
                self.parse_anonymous_class_expression()
            }
            TokenKind::Keyword(Keyword::Structure)
                if self.nth_is_symbol(1, TokenKind::Colon)
                    || self.nth_is_symbol(1, TokenKind::LBrace)
                    || self.nth_is_symbol(1, TokenKind::LParen) =>
            {
                self.parse_anonymous_structure_expression()
            }
            TokenKind::Keyword(keyword) => {
                let name = keyword.as_str().to_string();
                self.bump();
                let span = token.span.clone();
                let path = NamePath { parts: vec![name], span: span.clone() };
                Ok(TermExpression::Name { path, span })
            }
            _ => Err(self.error_here("expected expression")),
        }
    }

    fn parse_parenthesized_expression(&mut self) -> Result<TermExpression, ParseError> {
        let open = self.expect_symbol(TokenKind::LParen)?;
        if self.match_symbol(TokenKind::RParen) {
            return Ok(TermExpression::Literal { literal: LiteralExpression::Unit, span: span(open.span.start, self.previous().span.end) });
        }

        let first = self.parse_value_expression_bp(0)?;
        if self.match_symbol(TokenKind::Comma) {
            let mut items = vec![first];
            loop {
                if self.check_symbol(TokenKind::RParen) {
                    break;
                }
                items.push(self.parse_value_expression_bp(0)?);
                if !self.match_symbol(TokenKind::Comma) {
                    break;
                }
            }
            let close = self.expect_symbol(TokenKind::RParen)?;
            return Ok(TermExpression::Tuple { items, span: span(open.span.start, close.span.end) });
        }

        let close = self.expect_symbol(TokenKind::RParen)?;
        Ok(with_term_span(first, span(open.span.start, close.span.end)))
    }

    fn parse_array_expression(&mut self) -> Result<TermExpression, ParseError> {
        let open = self.expect_symbol(TokenKind::LBracket)?;
        let items = self.parse_comma_separated_until(TokenKind::RBracket, |parser| parser.parse_value_expression_bp(0))?;
        let close = self.expect_symbol(TokenKind::RBracket)?;
        Ok(TermExpression::Array { items, span: span(open.span.start, close.span.end) })
    }

    fn parse_block_expression(&mut self) -> Result<TermExpression, ParseError> {
        let start = self.current().span.start;
        let body = self.parse_block_body()?;
        let end = body.span.end;
        Ok(TermExpression::Block { body: Box::new(body), is_unsafe: false, span: span(start, end) })
    }

    fn parse_macro_invoke_expression(&mut self) -> Result<TermExpression, ParseError> {
        let start = self.expect_symbol(TokenKind::At)?.span.start;
        let path = self.parse_name_path()?;
        let mut args = Vec::new();
        if self.match_symbol(TokenKind::LParen) {
            args = self.parse_comma_separated_until(TokenKind::RParen, |parser| parser.parse_expression_bp(0))?;
            self.expect_symbol(TokenKind::RParen)?;
        }
        let end = self.previous().span.end;
        Ok(TermExpression::MacroInvoke { path, args, span: span(start, end) })
    }

    /// 判断当前 token 是否为新语句的起始。
    ///
    /// 用于换行隐式终止：当表达式后紧跟标识符（含关键字）或字面量时，无需 `;` 即可结束当前语句。
    /// 值类型关键字（如 `i32`、`utf8`）也视为新语句起始。
    fn is_statement_start(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Identifier
                | TokenKind::Keyword(_)
                | TokenKind::Apostrophe
                | TokenKind::StringLiteral
                | TokenKind::IntegerLiteral
                | TokenKind::FloatLiteral
        )
    }

    fn compound_assignment_operator(&self) -> Option<BinaryOperator> {
        match self.current().kind {
            TokenKind::PlusEqual => Some(BinaryOperator::Add),
            TokenKind::MinusEqual => Some(BinaryOperator::Sub),
            TokenKind::StarEqual => Some(BinaryOperator::Mul),
            TokenKind::SlashEqual => Some(BinaryOperator::Div),
            TokenKind::PercentEqual => Some(BinaryOperator::Rem),
            _ => None,
        }
    }

    fn expression_infix_binding_power(&self) -> Option<(BinaryOperator, u8, u8)> {
        match self.current().kind {
            // `|>` 管道操作符优先级最低（低于逻辑或），左结合。
            TokenKind::PipeGt => Some((BinaryOperator::Pipe, 10, 11)),
            // 区间运算符，介于管道与逻辑或之间，左结合。
            TokenKind::DotDot => Some((BinaryOperator::Range, 15, 16)),
            TokenKind::DotDotEq => Some((BinaryOperator::RangeInclusive, 15, 16)),
            TokenKind::DotDotLt => Some((BinaryOperator::RangeTo, 15, 16)),
            TokenKind::OrOr => Some((BinaryOperator::Or, 20, 21)),
            TokenKind::AndAnd => Some((BinaryOperator::And, 30, 31)),
            TokenKind::Star => Some((BinaryOperator::Mul, 60, 61)),
            TokenKind::Slash => Some((BinaryOperator::Div, 60, 61)),
            TokenKind::Percent => Some((BinaryOperator::Rem, 60, 61)),
            TokenKind::Plus => Some((BinaryOperator::Add, 50, 51)),
            TokenKind::Minus => Some((BinaryOperator::Sub, 50, 51)),
            TokenKind::EqEq => Some((BinaryOperator::Eq, 40, 41)),
            TokenKind::NotEq => Some((BinaryOperator::Ne, 40, 41)),
            TokenKind::LAngle => Some((BinaryOperator::Lt, 40, 41)),
            TokenKind::RAngle => Some((BinaryOperator::Gt, 40, 41)),
            TokenKind::LessEq => Some((BinaryOperator::Le, 40, 41)),
            TokenKind::GreaterEq => Some((BinaryOperator::Ge, 40, 41)),
            // `<<` 和 `>>` 移位运算符，优先级介于算术和比较之间。
            TokenKind::Shl => Some((BinaryOperator::Shl, 45, 46)),
            TokenKind::Shr => Some((BinaryOperator::Shr, 45, 46)),
            // 按位运算符，优先级介于比较和逻辑运算之间（C 语言约定）。
            TokenKind::Ampersand => Some((BinaryOperator::BitAnd, 38, 39)),
            TokenKind::Caret => Some((BinaryOperator::Power, 36, 37)),
            TokenKind::Pipe => Some((BinaryOperator::BitOr, 34, 35)),
            _ => None,
        }
    }

    fn parse_comma_separated_until<T>(
        &mut self,
        end: TokenKind,
        mut parse: impl FnMut(&mut Self) -> Result<T, ParseError>,
    ) -> Result<Vec<T>, ParseError> {
        let mut items = Vec::new();
        if self.check_symbol(end) {
            return Ok(items);
        }
        loop {
            items.push(parse(self)?);
            if !self.match_symbol(TokenKind::Comma) {
                break;
            }
            if self.check_symbol(end) {
                break;
            }
        }
        Ok(items)
    }

    fn parse_separated_while<T>(
        &mut self,
        mut parse: impl FnMut(&mut Self) -> Result<T, ParseError>,
        separators: &[TokenKind],
    ) -> Result<Vec<T>, ParseError> {
        let mut items = vec![parse(self)?];
        while separators.iter().any(|separator| self.match_symbol(*separator)) {
            items.push(parse(self)?);
        }
        Ok(items)
    }

    fn expect_implicit_or_explicit_terminator(&mut self, next_item_keywords: &[&str]) -> Result<(), ParseError> {
        if self.match_symbol(TokenKind::Semicolon) {
            return Ok(());
        }

        if self.check_symbol(TokenKind::RBrace)
            || self.is_eof()
            || self.check_symbol(TokenKind::LBracket)
            || self.current_keyword_text().is_some_and(|text| DECLARATION_MODIFIERS.contains(&text) || next_item_keywords.contains(&text))
            || self.current_keyword_text().is_some_and(|text| next_item_keywords.contains(&text))
            || self.current_identifier_text().is_some_and(|text| DECLARATION_MODIFIERS.contains(&text) || next_item_keywords.contains(&text))
        {
            return Ok(());
        }

        Err(self.error_here("expected ';' or declaration boundary"))
    }

    fn current(&self) -> &Token {
        &self.tokens[self.index]
    }

    /// 查看下一个 token（不消费当前 token）。
    fn peek(&self) -> &Token {
        let next_index = (self.index + 1).min(self.tokens.len().saturating_sub(1));
        &self.tokens[next_index]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.index.saturating_sub(1)]
    }

    fn bump(&mut self) -> &Token {
        let current = self.index;
        if !matches!(self.tokens[current].kind, TokenKind::Eof) {
            self.index += 1;
        }
        &self.tokens[current]
    }

    fn is_eof(&self) -> bool {
        matches!(self.current().kind, TokenKind::Eof)
    }

    fn current_identifier_text(&self) -> Option<&str> {
        (self.current().kind == TokenKind::Identifier).then(|| self.slice(self.current().span.clone()))
    }

    fn current_keyword_text(&self) -> Option<&'static str> {
        match self.current().kind {
            TokenKind::Keyword(keyword) => Some(keyword.as_str()),
            _ => None,
        }
    }

    fn current_modifier_text(&self) -> Option<&str> {
        self.current_identifier_text().or_else(|| self.current_keyword_text())
    }

    fn check_token_keyword(&self, keyword: Keyword) -> bool {
        self.current().kind == TokenKind::Keyword(keyword)
    }

    fn match_token_keyword(&mut self, keyword: Keyword) -> bool {
        if self.check_token_keyword(keyword) {
            self.bump();
            true
        }
        else {
            false
        }
    }

    fn expect_token_keyword(&mut self, keyword: Keyword) -> Result<Token, ParseError> {
        if self.check_token_keyword(keyword) {
            return Ok(self.bump().clone());
        }
        Err(ParseError::invalid_at(format!("expected keyword `{}`", keyword.as_str()), self.current().span.clone()))
    }

    fn check_identifier_text_eq(&self, text: &str) -> bool {
        self.current_identifier_text().is_some_and(|current| current == text)
    }

    fn match_identifier_text_eq(&mut self, text: &str) -> bool {
        if self.check_identifier_text_eq(text) {
            self.bump();
            true
        }
        else {
            false
        }
    }

    fn expect_identifier_text_eq(&mut self, text: &str) -> Result<Token, ParseError> {
        if self.check_identifier_text_eq(text) {
            return Ok(self.bump().clone());
        }
        Err(self.error_here(format!("expected identifier `{}`", text)))
    }

    #[allow(dead_code)]
    #[deprecated(note = "use tokenized keyword checks instead: TokenKind::Keyword(...), expect_token_keyword, or match_token_keyword")]
    fn check_keyword(&self, text: &str) -> bool {
        Keyword::from_str(text).is_some_and(|keyword| self.current().kind == TokenKind::Keyword(keyword))
            || self.current_identifier_text().is_some_and(|current| current == text)
    }

    #[allow(dead_code)]
    #[deprecated(note = "use tokenized keyword checks instead: TokenKind::Keyword(...), expect_token_keyword, or match_token_keyword")]
    fn match_keyword(&mut self, text: &str) -> bool {
        if Keyword::from_str(text).is_some_and(|keyword| self.current().kind == TokenKind::Keyword(keyword))
            || self.current_identifier_text().is_some_and(|current| current == text)
        {
            self.bump();
            true
        }
        else {
            false
        }
    }

    #[allow(dead_code)]
    #[deprecated(note = "use tokenized keyword checks instead: TokenKind::Keyword(...), expect_token_keyword, or match_token_keyword")]
    fn expect_keyword(&mut self, text: &str) -> Result<Token, ParseError> {
        if Keyword::from_str(text).is_some_and(|keyword| self.current().kind == TokenKind::Keyword(keyword))
            || self.current_identifier_text().is_some_and(|current| current == text)
        {
            return Ok(self.bump().clone());
        }
        Err(self.error_here(format!("expected keyword '{}'", text)))
    }

    fn expect_identifier_text(&mut self) -> Result<&str, ParseError> {
        if matches!(self.current().kind, TokenKind::Identifier) {
            let span = self.bump().span.clone();
            return Ok(self.slice(span));
        }
        Err(self.error_here("expected identifier"))
    }

    /// 对象字段 / 构造字段起始：普通标识符、反引号名，或关键字后跟 `:` / `=`（如 `type: utf8`）。
    fn check_field_name_start(&self) -> bool {
        match self.current().kind {
            TokenKind::Identifier | TokenKind::BacktickSymbol => true,
            TokenKind::Keyword(_) => matches!(self.peek().kind, TokenKind::Colon | TokenKind::Equal),
            _ => false,
        }
    }

    /// 属性命名参数起始：`name = "x"` / `case: "camelCase"`（关键字键须后跟 `:` 或 `=`）。
    fn check_attribute_named_argument_start(&self) -> bool {
        let has_separator = self.nth_is_symbol(1, TokenKind::Equal) || self.nth_is_symbol(1, TokenKind::Colon);
        match self.current().kind {
            TokenKind::Identifier | TokenKind::Keyword(_) => has_separator,
            _ => false,
        }
    }

    fn parse_label_name(&mut self) -> Result<String, ParseError> {
        self.expect_symbol(TokenKind::Apostrophe)?;
        Ok(self.expect_identifier_text()?.to_string())
    }

    #[allow(dead_code)]
    fn expect_keyword_text(&mut self) -> Result<String, ParseError> {
        if let TokenKind::Keyword(keyword) = self.current().kind {
            self.bump();
            return Ok(keyword.as_str().to_string());
        }
        if let Some(text) = self.current_identifier_text() {
            let owned = text.to_string();
            self.bump();
            return Ok(owned);
        }
        Err(self.error_here("expected keyword"))
    }

    fn expect_member_name_text(&mut self) -> Result<String, ParseError> {
        match self.current().kind {
            TokenKind::Identifier => {
                let span = self.bump().span.clone();
                Ok(self.slice(span).to_string())
            }
            TokenKind::Keyword(keyword) => {
                self.bump();
                Ok(keyword.as_str().to_string())
            }
            TokenKind::BacktickSymbol => self.expect_name_text(),
            _ => Err(self.error_here("expected member name")),
        }
    }

    fn expect_type_name_text(&mut self) -> Result<String, ParseError> {
        match self.current().kind {
            TokenKind::Identifier => {
                let span = self.bump().span.clone();
                Ok(self.slice(span).to_string())
            }
            TokenKind::Keyword(keyword) => {
                self.bump();
                Ok(keyword.as_str().to_string())
            }
            TokenKind::BacktickSymbol => self.expect_name_text(),
            _ => Err(self.error_here("expected type name")),
        }
    }

    /// 期望一个名称文本，接受普通标识符或反引号包裹的标识符。
    ///
    /// 反引号包裹的标识符（如 `` `any` ``、`` `bool` ``）允许使用关键字作为名称。
    /// 返回的文本已剥离反引号。
    fn expect_name_text(&mut self) -> Result<String, ParseError> {
        if matches!(self.current().kind, TokenKind::BacktickSymbol) {
            let span = self.bump().span.clone();
            let text = self.slice(span);
            let trimmed = text.trim_start_matches('`').trim_end_matches('`');
            return Ok(trimmed.to_string());
        }
        if matches!(self.current().kind, TokenKind::Identifier) {
            let span = self.bump().span.clone();
            return Ok(self.slice(span).to_string());
        }
        Err(self.error_here("expected identifier or backtick name"))
    }

    fn check_symbol(&self, symbol: TokenKind) -> bool {
        self.current().kind == symbol
            // 嵌套泛型中 `>>` 应视为两个 `>`，此处允许 `Shr` 匹配 `RAngle`。
            || (symbol == TokenKind::RAngle && self.current().kind == TokenKind::Shr)
    }

    fn nth_is_symbol(&self, offset: usize, symbol: TokenKind) -> bool {
        self.tokens.get(self.index + offset).is_some_and(|token| token.kind == symbol)
    }

    fn nth_is_identifier(&self, offset: usize) -> bool {
        self.tokens.get(self.index + offset).is_some_and(|token| token.kind == TokenKind::Identifier)
    }

    fn nth_is_type_name(&self, offset: usize) -> bool {
        self.tokens
            .get(self.index + offset)
            .is_some_and(|token| matches!(token.kind, TokenKind::Identifier | TokenKind::Keyword(_) | TokenKind::BacktickSymbol))
    }

    fn match_symbol(&mut self, symbol: TokenKind) -> bool {
        if self.check_symbol(symbol) {
            self.bump();
            true
        }
        else {
            false
        }
    }

    fn expect_symbol(&mut self, symbol: TokenKind) -> Result<Token, ParseError> {
        if self.check_symbol(symbol) {
            // 嵌套泛型中 `>>` (Shr) 需拆分为两个 `>` (RAngle)，
            // 此处将当前 `>>` token 替换为第一个 `>`，并插入第二个 `>` 供外层泛型消费。
            if symbol == TokenKind::RAngle && self.current().kind == TokenKind::Shr {
                let start = self.current().span.start;
                self.tokens[self.index] = Token { kind: TokenKind::RAngle, span: span(start, start + 1) };
                self.tokens.insert(self.index + 1, Token { kind: TokenKind::RAngle, span: span(start + 1, start + 2) });
            }
            return Ok(self.bump().clone());
        }
        Err(self.error_here(format!("expected symbol {:?}", symbol)))
    }

    fn slice(&self, source_span: Range<usize>) -> &str {
        &self.source[source_span.start as usize..source_span.end as usize]
    }

    fn error_here(&self, message: impl Into<String>) -> ParseError {
        let current = self.current();
        let token_text = if matches!(current.kind, TokenKind::Eof) { "<eof>" } else { self.slice(current.span.clone()) };
        ParseError::invalid_at(format!("{} near '{}'", message.into(), token_text), current.span.clone())
    }
}

/// Entry point that parses source text into `ValkyrieRoot`.
pub struct AstParser;

impl AstParser {
    /// Parses a source file from disk.
    pub fn parse_path(path: &PathBuf) -> Result<ValkyrieRoot, ParseError> {
        let source = std::fs::read_to_string(path)?;
        Self::parse_root(&source)
    }

    /// Parses source text into a parser root.
    pub fn parse_root(source: &str) -> Result<ValkyrieRoot, ParseError> {
        let tokens = Lexer::tokenize(source)?;
        Self::parse_tokens(source, tokens)
    }

    /// Parses source text using a precomputed token stream (cache restore path).
    pub fn parse_tokens(source: &str, tokens: Vec<Token>) -> Result<ValkyrieRoot, ParseError> {
        Parser::new(source, tokens).parse_root()
    }

    /// Parses `.vx` source and re-parses widget view bodies that contain `<% %>` meta.
    pub fn parse_vx_root(source: &str) -> Result<ValkyrieRoot, ParseError> {
        let mut root = Self::parse_root(source)?;
        fixup_vx_widget_view_markup(&mut root, source);
        Ok(root)
    }
}

#[cfg(test)]
mod tests {
    use super::AstParser;
    use crate::transitional::cst::valkyrie::ast::{ClassLikeKind, FunctionStatement, RootStatement};

    #[test]
    fn parses_compound_assignment_expression() {
        let source = "micro demo() { self.current += 1 }";
        AstParser::parse_root(source).expect("compound assignment should parse");
    }

    #[test]
    fn parses_long_else_if_ladder_without_recursive_descent() {
        let mut source = String::from("micro demo() { if flag0 { value = 0 }");
        for index in 1..=128 {
            source.push_str(&format!(" else if flag{index} {{ value = {index} }}"));
        }
        source.push_str(" else { value = 129 } }");

        AstParser::parse_root(&source).expect("long else-if ladder should parse");
    }

    #[test]
    fn parses_bare_anonymous_class_expression() {
        let source = "micro demo() { class { x: 1 } }";
        AstParser::parse_root(source).expect("bare anonymous class should parse");
    }

    #[test]
    fn rejects_anonymous_class_with_colon_but_no_trait_bound() {
        let source = "micro demo() { class: {} }";
        let err = AstParser::parse_root(source).expect_err("class: {} requires at least one trait bound");
        let message = err.to_string();
        assert!(message.contains("trait bound"), "unexpected error: {message}");
    }

    #[test]
    fn parses_empty_parenthesized_anonymous_class_expression() {
        let source = "micro demo() { class() {} }";
        AstParser::parse_root(source).expect("empty parenthesized anonymous class should parse");
    }

    #[test]
    fn parses_parenthesized_anonymous_class_expression() {
        let source = "micro demo() { class(Animal) { x: 1 } }";
        AstParser::parse_root(source).expect("parenthesized anonymous class should parse");
    }

    #[test]
    fn parses_bare_anonymous_structure_expression() {
        let source = "micro demo() { structure { x: 1 } }";
        AstParser::parse_root(source).expect("bare anonymous structure should parse");
    }

    #[test]
    fn rejects_anonymous_structure_with_colon_but_no_trait_bound() {
        let source = "micro demo() { structure: {} }";
        let err = AstParser::parse_root(source).expect_err("structure: {} requires at least one trait bound");
        let message = err.to_string();
        assert!(message.contains("trait bound"), "unexpected error: {message}");
    }

    #[test]
    fn parses_widget_body_let_and_micro_with_bracket_attributes() {
        let source = r#"widget ThemeSwitcher {
    [property]
    let theme;

    [property]
    let mode = "auto";

    [event]
    micro theme_change(theme: string) { }

    [memoize]
    let visible_themes = filter_themes(themes, query);
}"#;
        let root = AstParser::parse_vx_root(source).expect("widget ABI attributes should parse");
        let widget = root
            .statements
            .iter()
            .find_map(|stmt| match stmt {
                RootStatement::Class(class) if class.kind == ClassLikeKind::Widget => Some(class),
                _ => None,
            })
            .expect("widget declaration");
        let FunctionStatement::Let(theme) = &widget.body.script_statements[0]
        else {
            panic!("expected property let");
        };
        assert!(theme.annotations.attributes().any(|attr| attr.name.parts.last().is_some_and(|name| name == "property")));
        assert!(theme.initializer.is_none());

        let event_micro = widget.body.methods.iter().find(|method| method.name.as_str() == "theme_change").expect("event micro");
        assert!(event_micro.annotations.attributes().any(|attr| attr.name.parts.last().is_some_and(|name| name == "event")));
        assert!(event_micro.body.as_ref().is_some_and(|body| body.statements.is_empty() && body.tail_expression.is_none()));
    }
}
