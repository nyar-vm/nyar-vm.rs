//! Lua lexer for mid-subset (control flow + tables).

mod token;

pub use token::{Token, TokenKind};

/// Tokenize Lua source.
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
            self.skip_whitespace_and_comments();
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
            else if ch == '"' || ch == '\'' {
                self.read_string(start, ch);
            }
            else if ch == '=' {
                self.advance_char();
                if self.peek_char() == Some('=') {
                    self.advance_char();
                    self.push(TokenKind::EqualEqual, start);
                }
                else {
                    self.push(TokenKind::Equal, start);
                }
            }
            else if ch == '.' {
                self.advance_char();
                if self.peek_char() == Some('.') {
                    self.advance_char();
                    self.push(TokenKind::DotDot, start);
                }
                else {
                    self.push(TokenKind::Dot, start);
                }
            }
            else if matches!(ch, '+' | '-' | '*' | '/' | '%' | '^' | '#' | '(' | ')' | '{' | '}' | '[' | ']' | ',' | ';') {
                self.advance_char();
                let kind = match ch {
                    '+' => TokenKind::Plus,
                    '-' => TokenKind::Minus,
                    '*' => TokenKind::Star,
                    '/' => TokenKind::Slash,
                    '%' => TokenKind::Percent,
                    '^' => TokenKind::Caret,
                    '#' => TokenKind::Hash,
                    '(' => TokenKind::LeftParen,
                    ')' => TokenKind::RightParen,
                    '{' => TokenKind::LeftBrace,
                    '}' => TokenKind::RightBrace,
                    '[' => TokenKind::LeftBracket,
                    ']' => TokenKind::RightBracket,
                    ',' => TokenKind::Comma,
                    ';' => TokenKind::Semicolon,
                    _ => TokenKind::Error,
                };
                self.push(kind, start);
            }
            else if ch == '<' {
                self.advance_char();
                if self.peek_char() == Some('=') {
                    self.advance_char();
                    self.push(TokenKind::LessEqual, start);
                }
                else {
                    self.push(TokenKind::Less, start);
                }
            }
            else if ch == '>' {
                self.advance_char();
                if self.peek_char() == Some('=') {
                    self.advance_char();
                    self.push(TokenKind::GreaterEqual, start);
                }
                else {
                    self.push(TokenKind::Greater, start);
                }
            }
            else if ch == '~' {
                self.advance_char();
                if self.peek_char() == Some('=') {
                    self.advance_char();
                    self.push(TokenKind::TildeEqual, start);
                }
                else {
                    self.push(TokenKind::Error, start);
                }
            }
            else {
                self.advance_char();
                self.push(TokenKind::Error, start);
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
            "and" => TokenKind::And,
            "or" => TokenKind::Or,
            "not" => TokenKind::Not,
            "if" => TokenKind::If,
            "then" => TokenKind::Then,
            "else" => TokenKind::Else,
            "elseif" => TokenKind::ElseIf,
            "end" => TokenKind::End,
            "while" => TokenKind::While,
            "do" => TokenKind::Do,
            "repeat" => TokenKind::Repeat,
            "until" => TokenKind::Until,
            "for" => TokenKind::For,
            "function" => TokenKind::Function,
            "local" => TokenKind::Local,
            "return" => TokenKind::Return,
            "break" => TokenKind::Break,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "nil" => TokenKind::Nil,
            "print" => TokenKind::Print,
            _ => TokenKind::Name,
        };
        self.push(kind, start);
    }

    fn read_number(&mut self, start: usize) {
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_digit() || ch == '.' {
                self.advance_char();
            }
            else {
                break;
            }
        }
        self.push(TokenKind::Number, start);
    }

    fn read_string(&mut self, start: usize, quote: char) {
        self.advance_char();
        while let Some(ch) = self.peek_char() {
            if ch == quote {
                self.advance_char();
                self.push(TokenKind::String, start);
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

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            while let Some(ch) = self.peek_char() {
                if ch.is_whitespace() {
                    self.advance_char();
                }
                else {
                    break;
                }
            }
            if self.peek_char() == Some('-') && self.peek_char_at(1) == Some('-') {
                self.advance_char();
                self.advance_char();
                while let Some(ch) = self.peek_char() {
                    if ch == '\n' {
                        break;
                    }
                    self.advance_char();
                }
            }
            else {
                break;
            }
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
