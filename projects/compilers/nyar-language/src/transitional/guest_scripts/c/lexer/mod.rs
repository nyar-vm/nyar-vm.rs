//! C lexer for the legend / legacy-vm subset.

mod token;

pub use token::{Token, TokenKind};

/// Tokenize C source.
pub fn tokenize(source: &str) -> Vec<Token> {
    let mut lexer = Lexer { source, offset: 0, tokens: Vec::new() };
    lexer.run();
    lexer.tokens
}

struct Lexer<'a> {
    source: &'a str,
    offset: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn run(&mut self) {
        while !self.is_at_end() {
            self.skip_trivia();
            if self.is_at_end() {
                break;
            }
            let start = self.offset;
            let ch = self.peek_char().unwrap();
            if ch.is_ascii_alphabetic() || ch == '_' {
                self.read_ident_or_keyword(start);
            }
            else if ch.is_ascii_digit() || (ch == '.' && self.peek_char_at(1).map(|c| c.is_ascii_digit()).unwrap_or(false)) {
                self.read_number(start);
            }
            else if ch == '"' {
                self.read_string(start);
            }
            else if ch == '\'' {
                self.read_char(start);
            }
            else if ch == '#' {
                // Skip preprocessor lines (`#include …`) for the demo subset.
                while let Some(c) = self.peek_char() {
                    if c == '\n' {
                        break;
                    }
                    self.advance_char();
                }
            }
            else {
                self.read_operator_or_punct(start, ch);
            }
        }
        self.tokens.push(Token::eof(self.offset));
    }

    fn read_ident_or_keyword(&mut self, start: usize) {
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                self.advance_char();
            }
            else {
                break;
            }
        }
        let text = &self.source[start..self.offset];
        let kind = match text {
            "int" => TokenKind::Int,
            "void" => TokenKind::Void,
            "char" => TokenKind::Char,
            "float" => TokenKind::Float,
            "double" => TokenKind::Double,
            "return" => TokenKind::Return,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "for" => TokenKind::For,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            _ => TokenKind::Ident,
        };
        self.push(kind, start);
    }

    fn read_number(&mut self, start: usize) {
        let mut is_float = false;
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_digit() {
                self.advance_char();
            }
            else if ch == '.' && !is_float {
                is_float = true;
                self.advance_char();
            }
            else {
                break;
            }
        }
        self.push(if is_float { TokenKind::FloatLiteral } else { TokenKind::IntLiteral }, start);
    }

    fn read_string(&mut self, start: usize) {
        self.advance_char();
        while let Some(ch) = self.peek_char() {
            if ch == '"' {
                self.advance_char();
                self.push(TokenKind::StringLiteral, start);
                return;
            }
            if ch == '\\' {
                self.advance_char();
                if !self.is_at_end() {
                    self.advance_char();
                }
            }
            else {
                self.advance_char();
            }
        }
        self.push(TokenKind::Error, start);
    }

    fn read_char(&mut self, start: usize) {
        self.advance_char();
        if self.peek_char() == Some('\\') {
            self.advance_char();
            if !self.is_at_end() {
                self.advance_char();
            }
        }
        else if !self.is_at_end() {
            self.advance_char();
        }
        if self.peek_char() == Some('\'') {
            self.advance_char();
            self.push(TokenKind::CharLiteral, start);
        }
        else {
            self.push(TokenKind::Error, start);
        }
    }

    fn read_operator_or_punct(&mut self, start: usize, ch: char) {
        self.advance_char();
        let kind = match ch {
            '(' => TokenKind::LeftParen,
            ')' => TokenKind::RightParen,
            '{' => TokenKind::LeftBrace,
            '}' => TokenKind::RightBrace,
            ',' => TokenKind::Comma,
            ';' => TokenKind::Semicolon,
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '%' => TokenKind::Percent,
            '/' => TokenKind::Slash,
            '!' => {
                if self.peek_char() == Some('=') {
                    self.advance_char();
                    TokenKind::BangEqual
                }
                else {
                    TokenKind::Bang
                }
            }
            '=' => {
                if self.peek_char() == Some('=') {
                    self.advance_char();
                    TokenKind::EqualEqual
                }
                else {
                    TokenKind::Equal
                }
            }
            '<' => {
                if self.peek_char() == Some('=') {
                    self.advance_char();
                    TokenKind::LessEqual
                }
                else {
                    TokenKind::Less
                }
            }
            '>' => {
                if self.peek_char() == Some('=') {
                    self.advance_char();
                    TokenKind::GreaterEqual
                }
                else {
                    TokenKind::Greater
                }
            }
            '&' => {
                if self.peek_char() == Some('&') {
                    self.advance_char();
                    TokenKind::AmpAmp
                }
                else {
                    TokenKind::Error
                }
            }
            '|' => {
                if self.peek_char() == Some('|') {
                    self.advance_char();
                    TokenKind::PipePipe
                }
                else {
                    TokenKind::Error
                }
            }
            _ => TokenKind::Error,
        };
        self.push(kind, start);
    }

    fn skip_trivia(&mut self) {
        loop {
            while let Some(ch) = self.peek_char() {
                if ch.is_whitespace() {
                    self.advance_char();
                }
                else {
                    break;
                }
            }
            if self.peek_char() == Some('/') && self.peek_char_at(1) == Some('/') {
                self.advance_char();
                self.advance_char();
                while let Some(ch) = self.peek_char() {
                    if ch == '\n' {
                        break;
                    }
                    self.advance_char();
                }
                continue;
            }
            if self.peek_char() == Some('/') && self.peek_char_at(1) == Some('*') {
                self.advance_char();
                self.advance_char();
                while !self.is_at_end() {
                    if self.peek_char() == Some('*') && self.peek_char_at(1) == Some('/') {
                        self.advance_char();
                        self.advance_char();
                        break;
                    }
                    self.advance_char();
                }
                continue;
            }
            break;
        }
    }

    fn push(&mut self, kind: TokenKind, start: usize) {
        self.tokens.push(Token { kind, span: start..self.offset });
    }

    fn peek_char(&self) -> Option<char> {
        self.source[self.offset..].chars().next()
    }

    fn peek_char_at(&self, index: usize) -> Option<char> {
        self.source[self.offset..].chars().nth(index)
    }

    fn advance_char(&mut self) {
        if let Some(ch) = self.peek_char() {
            self.offset += ch.len_utf8();
        }
    }

    fn is_at_end(&self) -> bool {
        self.offset >= self.source.len()
    }
}
