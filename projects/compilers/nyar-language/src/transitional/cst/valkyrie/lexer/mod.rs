//! 词法分析入口。

pub mod string_delim;
pub mod token;
pub mod token_codec;

pub use token::{Keyword, Token, TokenKind};
pub use token_codec::{decode_tokens, encode_tokens};

use crate::transitional::cst::valkyrie::{
    lexer::string_delim::{MAX_QUOTE_COUNT, find_closing_quote_run, opening_quote_run},
    parser::ParseError,
};
use std::ops::Range;

/// 将源文本切分为词法记号流。
pub struct Lexer<'a> {
    source: &'a str,
    offset: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    /// 词法分析并返回 **不含** trivia 的 token 序列（供 AST parser）。
    pub fn tokenize_clean(source: &'a str) -> Result<Vec<Token>, ParseError> {
        let all = Self::tokenize_lossless(source)?;
        Ok(all.into_iter().filter(|t| !t.kind.is_trivia()).collect())
    }

    /// 词法分析并返回完整 token 序列（含 trivia）；供 CST / formatter。
    pub fn tokenize_lossless(source: &'a str) -> Result<Vec<Token>, ParseError> {
        let mut lexer = Self { source, offset: 0, tokens: Vec::new() };
        while lexer.offset < source.len() {
            if lexer.lex_trivia()? {
                continue;
            }
            if lexer.offset >= source.len() {
                break;
            }
            lexer.lex_token()?;
        }
        let eof = source.len();
        lexer.tokens.push(Token { kind: TokenKind::Eof, span: span(eof, eof) });
        Ok(lexer.tokens)
    }

    /// 同 [`tokenize_clean`]（AST 解析默认入口）。
    pub fn tokenize(source: &'a str) -> Result<Vec<Token>, ParseError> {
        Self::tokenize_clean(source)
    }

    /// 尝试 lex 一段 trivia；若成功返回 `true`。
    fn lex_trivia(&mut self) -> Result<bool, ParseError> {
        let start = self.offset;
        let Some(remaining) = self.source.get(self.offset..)
        else {
            return Ok(false);
        };
        if remaining.starts_with('#') {
            while let Some(ch) = self.peek_char() {
                self.offset += ch.len_utf8();
                if ch == '\n' {
                    break;
                }
            }
            self.push_token(TokenKind::LineComment, start, self.offset);
            return Ok(true);
        }
        if remaining.starts_with('⍝') {
            self.offset += '⍝'.len_utf8();
            while let Some(ch) = self.peek_char() {
                self.offset += ch.len_utf8();
                if ch == '\n' {
                    break;
                }
            }
            self.push_token(TokenKind::LineComment, start, self.offset);
            return Ok(true);
        }
        if remaining.starts_with("<%") {
            self.offset += 2;
            while let Some(ch) = self.peek_char() {
                if ch == '%' && self.source.get(self.offset + 1..).is_some_and(|s| s.starts_with('>')) {
                    self.offset += 2;
                    break;
                }
                self.offset += ch.len_utf8();
            }
            self.push_token(TokenKind::TemplateDirective, start, self.offset);
            return Ok(true);
        }
        let Some(ch) = self.peek_char()
        else {
            return Ok(false);
        };
        if ch.is_whitespace() {
            self.offset += ch.len_utf8();
            self.push_token(TokenKind::Whitespace, start, self.offset);
            return Ok(true);
        }
        Ok(false)
    }

    #[allow(dead_code)]
    fn skip_trivia(&mut self) {
        while self.lex_trivia().unwrap_or(false) {}
    }

    fn lex_token(&mut self) -> Result<(), ParseError> {
        let start = self.offset;
        let Some(ch) = self.peek_char()
        else {
            return Ok(());
        };

        if let Some(prefix_skip) = self.prefixed_string_prefix_len() {
            self.lex_string(start, prefix_skip)?;
            return Ok(());
        }
        if is_identifier_start(ch) {
            self.lex_identifier(start);
            return Ok(());
        }
        if ch.is_ascii_digit() {
            self.lex_number(start);
            return Ok(());
        }
        if ch == '"' || (ch == '\'' && self.starts_with("''")) {
            self.lex_string(start, 0)?;
            return Ok(());
        }
        if ch == '`' {
            self.lex_backtick_symbol(start)?;
            return Ok(());
        }

        match ch {
            '(' => self.push_token(TokenKind::LParen, start, start + 1),
            ')' => self.push_token(TokenKind::RParen, start, start + 1),
            '{' => self.push_token(TokenKind::LBrace, start, start + 1),
            '}' => self.push_token(TokenKind::RBrace, start, start + 1),
            '[' => self.push_token(TokenKind::LBracket, start, start + 1),
            ']' => self.push_token(TokenKind::RBracket, start, start + 1),
            '⁅' => self.push_token(TokenKind::LOffsetBracket, start, start + ch.len_utf8()),
            '⁆' => self.push_token(TokenKind::ROffsetBracket, start, start + ch.len_utf8()),
            '<' => {
                if self.starts_with("<-") {
                    self.push_token(TokenKind::LArrow, start, start + 2);
                }
                else if self.starts_with("<=") {
                    self.push_token(TokenKind::LessEq, start, start + 2);
                }
                else if self.starts_with("<<") {
                    self.push_token(TokenKind::Shl, start, start + 2);
                }
                else {
                    self.push_token(TokenKind::LAngle, start, start + 1);
                }
            }
            '>' => {
                if self.starts_with(">=") {
                    self.push_token(TokenKind::GreaterEq, start, start + 2);
                }
                else if self.starts_with(">>") {
                    self.push_token(TokenKind::Shr, start, start + 2);
                }
                else {
                    self.push_token(TokenKind::RAngle, start, start + 1);
                }
            }
            ',' => self.push_token(TokenKind::Comma, start, start + 1),
            '\'' => self.push_token(TokenKind::Apostrophe, start, start + 1),
            ':' => {
                if self.starts_with("::") {
                    self.push_token(TokenKind::DoubleColon, start, start + 2);
                }
                else {
                    self.push_token(TokenKind::Colon, start, start + 1);
                }
            }
            ';' => self.push_token(TokenKind::Semicolon, start, start + 1),
            '=' => {
                if self.starts_with("==") {
                    self.push_token(TokenKind::EqEq, start, start + 2);
                }
                else if self.starts_with("=>") {
                    self.push_token(TokenKind::FatArrow, start, start + 2);
                }
                else {
                    self.push_token(TokenKind::Equal, start, start + 1);
                }
            }
            '-' => {
                if self.starts_with("->") {
                    self.push_token(TokenKind::Arrow, start, start + 2);
                }
                else if self.starts_with("-=") {
                    self.push_token(TokenKind::MinusEqual, start, start + 2);
                }
                else {
                    self.push_token(TokenKind::Minus, start, start + 1);
                }
            }
            '.' => {
                if self.starts_with("...") {
                    self.push_token(TokenKind::Ellipsis, start, start + 3);
                }
                else if self.starts_with("..=") {
                    self.push_token(TokenKind::DotDotEq, start, start + 3);
                }
                else if self.starts_with("..<") {
                    self.push_token(TokenKind::DotDotLt, start, start + 3);
                }
                else if self.starts_with("..") {
                    self.push_token(TokenKind::DotDot, start, start + 2);
                }
                else {
                    self.push_token(TokenKind::Dot, start, start + 1);
                }
            }
            '+' => {
                if self.starts_with("+=") {
                    self.push_token(TokenKind::PlusEqual, start, start + 2);
                }
                else {
                    self.push_token(TokenKind::Plus, start, start + 1);
                }
            }
            '*' => {
                if self.starts_with("*=") {
                    self.push_token(TokenKind::StarEqual, start, start + 2);
                }
                else {
                    self.push_token(TokenKind::Star, start, start + 1);
                }
            }
            '/' => {
                if self.starts_with("/=") {
                    self.push_token(TokenKind::SlashEqual, start, start + 2);
                }
                else {
                    self.push_token(TokenKind::Slash, start, start + 1);
                }
            }
            '%' => {
                if self.starts_with("%=") {
                    self.push_token(TokenKind::PercentEqual, start, start + 2);
                }
                else {
                    self.push_token(TokenKind::Percent, start, start + 1);
                }
            }
            '&' => {
                if self.starts_with("&&") {
                    self.push_token(TokenKind::AndAnd, start, start + 2);
                }
                else {
                    self.push_token(TokenKind::Ampersand, start, start + 1);
                }
            }
            '|' => {
                if self.starts_with("||") {
                    self.push_token(TokenKind::OrOr, start, start + 2);
                }
                else if self.starts_with("|>") {
                    self.push_token(TokenKind::PipeGt, start, start + 2);
                }
                else {
                    self.push_token(TokenKind::Pipe, start, start + 1);
                }
            }
            '~' => self.push_token(TokenKind::Tilde, start, start + 1),
            '^' => self.push_token(TokenKind::Caret, start, start + 1),
            '@' => self.push_token(TokenKind::At, start, start + 1),
            '◇' => self.push_token(TokenKind::HollowDiamond, start, start + ch.len_utf8()),
            '◆' => self.push_token(TokenKind::SolidDiamond, start, start + ch.len_utf8()),
            '!' => {
                if self.starts_with("!=") {
                    self.push_token(TokenKind::NotEq, start, start + 2);
                }
                else {
                    self.push_token(TokenKind::Bang, start, start + 1);
                }
            }
            '?' => {
                if self.starts_with("?.") {
                    self.push_token(TokenKind::QuestionDot, start, start + 2);
                }
                else {
                    self.push_token(TokenKind::Question, start, start + 1);
                }
            }
            _ => {
                return Err(ParseError::invalid(format!("unexpected character '{}' at {}", ch, start)));
            }
        }
        Ok(())
    }

    fn lex_identifier(&mut self, start: usize) {
        self.offset += self.peek_char().unwrap().len_utf8();
        while let Some(ch) = self.peek_char() {
            if !is_identifier_continue(ch) {
                break;
            }
            self.offset += ch.len_utf8();
        }
        let text = &self.source[start..self.offset];
        let kind = Keyword::from_str(text).map(TokenKind::Keyword).unwrap_or(TokenKind::Identifier);
        self.push_token(kind, start, self.offset);
    }

    fn lex_number(&mut self, start: usize) {
        self.offset += self.peek_char().unwrap().len_utf8();

        // 检查十六进制、二进制、八进制前缀。
        if self.peek_char() == Some('x') || self.peek_char() == Some('X') {
            self.offset += 'x'.len_utf8();
            while let Some(ch) = self.peek_char() {
                if !ch.is_ascii_hexdigit() {
                    break;
                }
                self.offset += ch.len_utf8();
            }
            self.push_token(TokenKind::IntegerLiteral, start, self.offset);
            return;
        }
        if self.peek_char() == Some('b') || self.peek_char() == Some('B') {
            self.offset += 'b'.len_utf8();
            while let Some(ch) = self.peek_char() {
                if ch != '0' && ch != '1' {
                    break;
                }
                self.offset += ch.len_utf8();
            }
            self.push_token(TokenKind::IntegerLiteral, start, self.offset);
            return;
        }
        if self.peek_char() == Some('o') || self.peek_char() == Some('O') {
            self.offset += 'o'.len_utf8();
            while let Some(ch) = self.peek_char() {
                if !('0'..='7').contains(&ch) {
                    break;
                }
                self.offset += ch.len_utf8();
            }
            self.push_token(TokenKind::IntegerLiteral, start, self.offset);
            return;
        }

        // 十进制整数部分。
        while let Some(ch) = self.peek_char() {
            if !ch.is_ascii_digit() {
                break;
            }
            self.offset += ch.len_utf8();
        }

        // 浮点数小数部分；`1..` / `1..=` / `1..<` 仍应保留给 range token。
        if self.peek_char() == Some('.') && !self.starts_with("..") {
            self.offset += '.'.len_utf8();
            while let Some(ch) = self.peek_char() {
                if !ch.is_ascii_digit() {
                    break;
                }
                self.offset += ch.len_utf8();
            }
            // 指数部分。
            if self.peek_char() == Some('e') || self.peek_char() == Some('E') {
                self.offset += 'e'.len_utf8();
                if self.peek_char() == Some('+') || self.peek_char() == Some('-') {
                    self.offset += 1;
                }
                while let Some(ch) = self.peek_char() {
                    if !ch.is_ascii_digit() {
                        break;
                    }
                    self.offset += ch.len_utf8();
                }
            }
            self.push_token(TokenKind::FloatLiteral, start, self.offset);
            return;
        }

        // 指数部分（无小数点）。
        if self.peek_char() == Some('e') || self.peek_char() == Some('E') {
            self.offset += 'e'.len_utf8();
            if self.peek_char() == Some('+') || self.peek_char() == Some('-') {
                self.offset += 1;
            }
            while let Some(ch) = self.peek_char() {
                if !ch.is_ascii_digit() {
                    break;
                }
                self.offset += ch.len_utf8();
            }
            self.push_token(TokenKind::FloatLiteral, start, self.offset);
            return;
        }

        self.push_token(TokenKind::IntegerLiteral, start, self.offset);
    }

    /// 词法分析反引号包裹的符号字面量。
    ///
    /// 读取从开始反引号到结束反引号之间的内容，生成 `BacktickSymbol` token。
    /// span 覆盖整个 `` `symbol` `` 文本（含反引号），解析器负责剥离反引号。
    fn lex_backtick_symbol(&mut self, start: usize) -> Result<(), ParseError> {
        self.offset += '`'.len_utf8();
        while let Some(ch) = self.peek_char() {
            if ch == '`' {
                self.offset += '`'.len_utf8();
                self.tokens.push(Token { kind: TokenKind::BacktickSymbol, span: span(start, self.offset) });
                return Ok(());
            }
            self.offset += ch.len_utf8();
        }
        Err(ParseError::invalid("unterminated backtick symbol literal"))
    }

    fn prefixed_string_prefix_len(&self) -> Option<usize> {
        let ch = self.peek_char()?;
        if ch != 'r' && ch != 't' {
            return None;
        }
        let after = self.offset + ch.len_utf8();
        opening_quote_run(self.source, after)?;
        Some(ch.len_utf8())
    }

    fn lex_string(&mut self, start: usize, prefix_skip: usize) -> Result<(), ParseError> {
        self.offset += prefix_skip;
        let (delim, quote_count) =
            opening_quote_run(self.source, self.offset).ok_or_else(|| ParseError::invalid("expected string delimiter"))?;
        if quote_count > MAX_QUOTE_COUNT {
            return Err(ParseError::invalid(format!("string delimiter count exceeds {MAX_QUOTE_COUNT}")));
        }
        self.offset += quote_count * delim.len_utf8();

        if quote_count == 2 {
            self.tokens.push(Token { kind: TokenKind::StringLiteral, span: span(start, self.offset) });
            return Ok(());
        }

        let end = find_closing_quote_run(self.source, self.offset, delim, quote_count)
            .ok_or_else(|| ParseError::invalid(format!("unterminated string literal with {quote_count} quote delimiter(s)")))?;
        self.offset = end;
        self.tokens.push(Token { kind: TokenKind::StringLiteral, span: span(start, self.offset) });
        Ok(())
    }

    fn push_token(&mut self, kind: TokenKind, start: usize, end: usize) {
        self.offset = end;
        self.tokens.push(Token { kind, span: span(start, end) });
    }

    fn peek_char(&self) -> Option<char> {
        self.source.get(self.offset..)?.chars().next()
    }

    fn starts_with(&self, text: &str) -> bool {
        self.source.get(self.offset..).is_some_and(|value| value.starts_with(text))
    }
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch.is_alphabetic()
}

fn is_identifier_continue(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

fn span(start: usize, end: usize) -> Range<usize> {
    start..end
}
