//! Bash lexer for the legend / legacy-vm subset.

mod token;

pub use token::{Token, TokenKind};

/// Tokenize Bash source.
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
            self.skip_spaces();
            if self.is_at_end() {
                break;
            }
            let start = self.offset;
            let ch = self.peek_char().unwrap();
            match ch {
                '\n' => {
                    self.advance_char();
                    self.push(TokenKind::Newline, start);
                }
                '#' => self.skip_line_comment(),
                ';' => {
                    self.advance_char();
                    self.push(TokenKind::Semicolon, start);
                }
                '|' => {
                    self.advance_char();
                    if self.peek_char() == Some('|') {
                        self.advance_char();
                        self.push(TokenKind::OrOr, start);
                    }
                    else {
                        self.push(TokenKind::Pipe, start);
                    }
                }
                '&' => {
                    self.advance_char();
                    if self.peek_char() == Some('&') {
                        self.advance_char();
                        self.push(TokenKind::AndAnd, start);
                    }
                    else {
                        self.push(TokenKind::Error, start);
                    }
                }
                '>' => {
                    self.advance_char();
                    if self.peek_char() == Some('>') {
                        self.advance_char();
                        self.push(TokenKind::GreaterGreater, start);
                    }
                    else {
                        self.push(TokenKind::Greater, start);
                    }
                }
                '<' => {
                    self.advance_char();
                    self.push(TokenKind::Less, start);
                }
                '=' => {
                    self.advance_char();
                    self.push(TokenKind::Equal, start);
                }
                '(' => {
                    self.advance_char();
                    self.push(TokenKind::LeftParen, start);
                }
                ')' => {
                    self.advance_char();
                    self.push(TokenKind::RightParen, start);
                }
                '{' => {
                    self.advance_char();
                    self.push(TokenKind::LeftBrace, start);
                }
                '}' => {
                    self.advance_char();
                    self.push(TokenKind::RightBrace, start);
                }
                '"' | '\'' => self.read_string(start, ch),
                _ => self.read_word(start),
            }
        }
        self.tokens.push(Token::eof(self.offset));
    }

    fn read_string(&mut self, start: usize, quote: char) {
        self.advance_char();
        let content_start = self.offset;
        while let Some(ch) = self.peek_char() {
            if ch == quote {
                break;
            }
            if ch == '\\' {
                self.advance_char();
                self.advance_char();
                continue;
            }
            self.advance_char();
        }
        let content_end = self.offset;
        if self.peek_char() == Some(quote) {
            self.advance_char();
        }
        // Store span over the unquoted content so Parser::text works.
        self.tokens.push(Token { kind: TokenKind::String, span: content_start..content_end });
        let _ = start;
    }

    fn read_word(&mut self, start: usize) {
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() || is_meta(ch) {
                break;
            }
            self.advance_char();
        }
        let text = &self.source[start..self.offset];
        let kind = match text {
            "if" => TokenKind::If,
            "then" => TokenKind::Then,
            "else" => TokenKind::Else,
            "elif" => TokenKind::Elif,
            "fi" => TokenKind::Fi,
            "while" => TokenKind::While,
            "do" => TokenKind::Do,
            "done" => TokenKind::Done,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "function" => TokenKind::Function,
            "return" => TokenKind::Return,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            _ => TokenKind::Word,
        };
        self.push(kind, start);
    }

    fn skip_spaces(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch == ' ' || ch == '\t' || ch == '\r' {
                self.advance_char();
            }
            else {
                break;
            }
        }
    }

    fn skip_line_comment(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch == '\n' {
                break;
            }
            self.advance_char();
        }
    }

    fn push(&mut self, kind: TokenKind, start: usize) {
        self.tokens.push(Token { kind, span: start..self.offset });
    }

    fn peek_char(&self) -> Option<char> {
        self.source[self.offset..].chars().next()
    }

    fn advance_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.offset += ch.len_utf8();
        Some(ch)
    }

    fn is_at_end(&self) -> bool {
        self.offset >= self.source.len()
    }
}

fn is_meta(ch: char) -> bool {
    matches!(ch, ';' | '|' | '&' | '>' | '<' | '(' | ')' | '{' | '}' | '=' | '#' | '"' | '\'')
}
