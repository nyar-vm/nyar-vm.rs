//! X-Grammar 词法分析。

mod token;

pub use token::{Token, TokenKind};

pub struct Lexer<'a> {
    source: &'a str,
    offset: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    pub fn tokenize(source: &'a str) -> Vec<Token> {
        let mut lexer = Self { source, offset: 0, tokens: Vec::new() };
        while lexer.offset < source.len() {
            if lexer.at_content_boundary() {
                lexer.skip_content_trivia();
            }
            if lexer.offset >= source.len() {
                break;
            }
            let start = lexer.offset;
            if lexer.peek_starts_with("<!--") {
                if lexer.scan_comment().is_ok() {
                    lexer.tokens.push(Token { kind: TokenKind::Comment, span: start..lexer.offset });
                }
                continue;
            }
            if lexer.peek_starts_with("<%") {
                lexer.scan_directive();
                lexer.tokens.push(Token { kind: TokenKind::Directive, span: start..lexer.offset });
                continue;
            }
            if lexer.peek_char() == Some('<') {
                lexer.offset += 1;
                lexer.tokens.push(Token { kind: TokenKind::Lt, span: start..lexer.offset });
                lexer.lex_tag_interior();
                continue;
            }
            lexer.scan_text_until_lt();
            if lexer.offset > start {
                lexer.tokens.push(Token { kind: TokenKind::Text, span: start..lexer.offset });
            }
        }
        lexer.tokens.push(Token::eof(source.len()));
        lexer.tokens
    }

    fn lex_tag_interior(&mut self) {
        self.skip_inline_whitespace();
        if self.peek_char() == Some('/') {
            let start = self.offset;
            self.offset += 1;
            self.tokens.push(Token { kind: TokenKind::Slash, span: start..self.offset });
        }
        if self.peek_is_name_char() {
            self.lex_identifier();
        }
        loop {
            self.skip_inline_whitespace();
            if self.peek_char() == Some('>') {
                let start = self.offset;
                self.offset += 1;
                self.tokens.push(Token { kind: TokenKind::Gt, span: start..self.offset });
                break;
            }
            if self.peek_starts_with("/>") {
                let start = self.offset;
                self.offset += 1;
                self.tokens.push(Token { kind: TokenKind::Slash, span: start..self.offset });
                self.expect_char('>');
                self.tokens.push(Token { kind: TokenKind::Gt, span: self.offset - 1..self.offset });
                break;
            }
            if !self.peek_is_name_char() {
                break;
            }
            self.lex_identifier();
            self.skip_inline_whitespace();
            if self.peek_char() == Some('=') {
                let start = self.offset;
                self.offset += 1;
                self.tokens.push(Token { kind: TokenKind::Eq, span: start..self.offset });
                self.skip_inline_whitespace();
                self.lex_attribute_value();
            }
        }
    }

    fn lex_identifier(&mut self) {
        let start = self.offset;
        while self.peek_is_name_char() {
            self.offset += self.peek_char().unwrap().len_utf8();
        }
        self.tokens.push(Token { kind: TokenKind::Identifier, span: start..self.offset });
    }

    fn lex_attribute_value(&mut self) {
        let start = self.offset;
        match self.peek_char() {
            Some('"' | '\'') => {
                let quote = self.peek_char().unwrap();
                self.offset += 1;
                while let Some(ch) = self.peek_char() {
                    if ch == quote {
                        self.offset += 1;
                        break;
                    }
                    self.offset += ch.len_utf8();
                }
                self.tokens.push(Token { kind: TokenKind::StringLiteral, span: start..self.offset });
            }
            Some('{') => {
                self.scan_braced_expr();
                self.tokens.push(Token { kind: TokenKind::BracedExpr, span: start..self.offset });
            }
            _ => {
                while self.offset < self.source.len() {
                    let ch = self.peek_char().unwrap();
                    if ch.is_whitespace() || ch == '>' || ch == '/' {
                        break;
                    }
                    self.offset += ch.len_utf8();
                }
                if start < self.offset {
                    self.tokens.push(Token { kind: TokenKind::Identifier, span: start..self.offset });
                }
            }
        }
    }

    fn scan_directive(&mut self) {
        self.offset += 2;
        while self.offset + 1 < self.source.len() {
            if self.source[self.offset..].starts_with("%>") {
                self.offset += 2;
                return;
            }
            self.offset += self.peek_char().map(|ch| ch.len_utf8()).unwrap_or(1);
        }
    }

    fn scan_comment(&mut self) -> Result<(), ()> {
        self.offset += 4;
        while self.offset + 2 < self.source.len() {
            if self.source[self.offset..].starts_with("-->") {
                self.offset += 3;
                return Ok(());
            }
            self.offset += self.peek_char().map(|ch| ch.len_utf8()).unwrap_or(1);
        }
        Err(())
    }

    fn scan_text_until_lt(&mut self) {
        while self.offset < self.source.len() && self.peek_char() != Some('<') {
            self.offset += self.peek_char().map(|ch| ch.len_utf8()).unwrap_or(1);
        }
    }

    fn scan_braced_expr(&mut self) {
        self.expect_char('{');
        let mut depth = 1usize;
        while self.offset < self.source.len() {
            let ch = self.peek_char().unwrap();
            if ch == '{' {
                depth += 1;
            }
            else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    self.offset += 1;
                    return;
                }
            }
            self.offset += ch.len_utf8();
        }
    }

    fn at_content_boundary(&self) -> bool {
        self.tokens
            .last()
            .map(|token| matches!(token.kind, TokenKind::Gt | TokenKind::Comment | TokenKind::Directive | TokenKind::Text | TokenKind::Eof))
            .unwrap_or(true)
    }

    fn skip_content_trivia(&mut self) {
        loop {
            if self.peek_starts_with("<!--") {
                let start = self.offset;
                if self.scan_comment().is_ok() {
                    self.tokens.push(Token { kind: TokenKind::Comment, span: start..self.offset });
                    continue;
                }
            }
            if self.peek_char().is_some_and(|ch| ch.is_whitespace()) {
                self.offset += self.peek_char().unwrap().len_utf8();
                continue;
            }
            break;
        }
    }

    fn skip_inline_whitespace(&mut self) {
        while self.peek_char().is_some_and(|ch| ch.is_whitespace()) {
            self.offset += self.peek_char().unwrap().len_utf8();
        }
    }

    fn peek_is_name_char(&self) -> bool {
        self.peek_char().is_some_and(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '-' | ':' | '@'))
    }

    fn peek_starts_with(&self, literal: &str) -> bool {
        self.source[self.offset..].starts_with(literal)
    }

    fn peek_char(&self) -> Option<char> {
        self.source[self.offset..].chars().next()
    }

    fn expect_char(&mut self, expected: char) {
        if self.peek_char() == Some(expected) {
            self.offset += expected.len_utf8();
        }
    }
}
