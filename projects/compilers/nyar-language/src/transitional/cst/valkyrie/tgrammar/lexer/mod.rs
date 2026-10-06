//! T-Grammar 词法分析。

mod token;

pub use token::{Token, TokenKind};

/// T-Grammar 词法分析器。
pub struct Lexer<'a> {
    source: &'a str,
    offset: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    /// 词法分析完整模板。
    pub fn tokenize(source: &'a str) -> Vec<Token> {
        let mut lexer = Self { source, offset: 0, tokens: Vec::new() };
        while lexer.offset < source.len() {
            lexer.skip_template_whitespace();
            if lexer.offset >= source.len() {
                break;
            }
            let start = lexer.offset;
            if lexer.peek_starts_with("<%") {
                lexer.scan_directive();
                lexer.tokens.push(Token { kind: TokenKind::Directive, span: start..lexer.offset });
            }
            else if lexer.peek_starts_with("<#") {
                if lexer.scan_comment().is_ok() {
                    lexer.tokens.push(Token { kind: TokenKind::Comment, span: start..lexer.offset });
                }
            }
            else {
                lexer.scan_text_until_special();
                if lexer.offset > start {
                    lexer.tokens.push(Token { kind: TokenKind::Text, span: start..lexer.offset });
                }
            }
        }
        lexer.tokens.push(Token::eof(source.len()));
        lexer.tokens
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
        self.offset += 2;
        while self.offset + 1 < self.source.len() {
            if self.source[self.offset..].starts_with("#>") {
                self.offset += 2;
                return Ok(());
            }
            self.offset += self.peek_char().map(|ch| ch.len_utf8()).unwrap_or(1);
        }
        Err(())
    }

    fn scan_text_until_special(&mut self) {
        while self.offset < self.source.len() {
            if self.peek_starts_with("<%") || self.peek_starts_with("<#") {
                break;
            }
            self.offset += self.peek_char().map(|ch| ch.len_utf8()).unwrap_or(1);
        }
    }

    fn skip_template_whitespace(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() {
                self.offset += ch.len_utf8();
            }
            else {
                break;
            }
        }
    }

    fn peek_starts_with(&self, literal: &str) -> bool {
        self.source[self.offset..].starts_with(literal)
    }

    fn peek_char(&self) -> Option<char> {
        self.source[self.offset..].chars().next()
    }
}
