//! VON 词法分析。

mod token;

pub use token::{Token, TokenKind};

use std::ops::Range;

use super::{error::VonParseError, lexical};

/// 将源文本切分为词法记号流。
pub struct Lexer<'a> {
    source: &'a str,
    offset: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    /// 不含 trivia 的 token 流（AST / VonValue 解析）。
    pub fn tokenize_clean(source: &'a str) -> Result<Vec<Token>, VonParseError> {
        let all = Self::tokenize_lossless(source)?;
        Ok(all.into_iter().filter(|t| !t.kind.is_trivia()).collect())
    }

    /// 含 trivia 的完整 token 流（CST / formatter）。
    pub fn tokenize_lossless(source: &'a str) -> Result<Vec<Token>, VonParseError> {
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
        lexer.tokens.push(Token::eof(eof));
        Ok(lexer.tokens)
    }

    /// 同 [`tokenize_clean`]。
    pub fn tokenize(source: &'a str) -> Result<Vec<Token>, VonParseError> {
        Self::tokenize_clean(source)
    }

    fn lex_trivia(&mut self) -> Result<bool, VonParseError> {
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
            self.tokens.push(Token { kind: TokenKind::LineComment, span: span(start, self.offset) });
            return Ok(true);
        }
        let Some(ch) = self.peek_char()
        else {
            return Ok(false);
        };
        if ch.is_whitespace() {
            self.offset += ch.len_utf8();
            self.tokens.push(Token { kind: TokenKind::Whitespace, span: span(start, self.offset) });
            return Ok(true);
        }
        Ok(false)
    }

    #[allow(dead_code)]
    fn skip_trivia(&mut self) {
        while self.lex_trivia().unwrap_or(false) {}
    }

    fn lex_token(&mut self) -> Result<(), VonParseError> {
        let start = self.offset;
        let Some(ch) = self.peek_char()
        else {
            return Ok(());
        };

        let kind = match ch {
            '{' => {
                self.offset += 1;
                TokenKind::LBrace
            }
            '}' => {
                self.offset += 1;
                TokenKind::RBrace
            }
            '[' => {
                self.offset += 1;
                TokenKind::LBracket
            }
            ']' => {
                self.offset += 1;
                TokenKind::RBracket
            }
            ':' => {
                self.offset += 1;
                TokenKind::Colon
            }
            ',' => {
                self.offset += 1;
                TokenKind::Comma
            }
            '"' => {
                self.lex_string()?;
                TokenKind::StringLiteral
            }
            '-' | '0'..='9' => {
                self.lex_number()?;
                TokenKind::IntegerLiteral
            }
            _ if lexical::is_identifier_start(ch) => self.lex_identifier(start)?,
            _ => return Err(self.error(start, format!("unexpected character '{ch}'"))),
        };

        self.tokens.push(Token { kind, span: span(start, self.offset) });
        Ok(())
    }

    fn lex_identifier(&mut self, start: usize) -> Result<TokenKind, VonParseError> {
        while self.peek_char().is_some_and(lexical::is_identifier_continue) {
            self.offset += self.peek_char().unwrap().len_utf8();
        }
        let text = &self.source[start..self.offset];
        let kind = match text {
            "null" => TokenKind::Null,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            _ => TokenKind::Identifier,
        };
        Ok(kind)
    }

    fn lex_string(&mut self) -> Result<(), VonParseError> {
        let start = self.offset;
        self.expect_char('"')?;
        loop {
            match self.next_char() {
                Some('"') => return Ok(()),
                Some('\\') => {
                    let _ = self.next_char().ok_or_else(|| self.error(start, "unterminated string escape"))?;
                }
                Some(_) => {}
                None => return Err(self.error(start, "unterminated string")),
            }
        }
    }

    fn lex_number(&mut self) -> Result<(), VonParseError> {
        if self.peek_char() == Some('-') {
            self.offset += 1;
            if !self.peek_char().is_some_and(|ch| ch.is_ascii_digit()) {
                return Err(self.error(self.offset.saturating_sub(1), "expected digit after '-'"));
            }
        }
        while self.peek_char().is_some_and(|ch| ch.is_ascii_digit()) {
            self.offset += 1;
        }
        Ok(())
    }

    fn peek_char(&self) -> Option<char> {
        self.source[self.offset..].chars().next()
    }

    fn next_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.offset += ch.len_utf8();
        Some(ch)
    }

    fn expect_char(&mut self, expected: char) -> Result<(), VonParseError> {
        match self.next_char() {
            Some(ch) if ch == expected => Ok(()),
            Some(_) => Err(self.error(self.offset.saturating_sub(1), format!("expected '{expected}'"))),
            None => Err(self.error(self.offset, format!("expected '{expected}'"))),
        }
    }

    fn error(&self, position: usize, message: impl Into<String>) -> VonParseError {
        VonParseError::new(position, message)
    }
}

fn span(start: usize, end: usize) -> Range<usize> {
    start..end
}
