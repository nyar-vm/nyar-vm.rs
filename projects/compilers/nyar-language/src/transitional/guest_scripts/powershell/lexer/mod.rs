//! PowerShell lexer (legend demo subset).

mod token;

pub use token::{Token, TokenKind};

/// PowerShell lexer.
pub struct Lexer<'a> {
    source: &'a str,
    offset: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    /// Tokenize source into a token stream (trailing EOF included).
    pub fn tokenize(source: &'a str) -> Vec<Token> {
        let mut lexer = Self { source, offset: 0, tokens: Vec::new() };
        while lexer.offset < source.len() {
            lexer.skip_spaces();
            if lexer.offset >= source.len() {
                break;
            }
            let Some(ch) = lexer.peek_char()
            else {
                break;
            };
            if ch == '\r' {
                lexer.offset += 1;
                continue;
            }
            if ch == '\n' {
                let start = lexer.offset;
                lexer.offset += 1;
                lexer.tokens.push(Token { kind: TokenKind::Newline, span: start..lexer.offset });
                continue;
            }
            if ch == '#' {
                lexer.skip_line_comment();
                continue;
            }
            match ch {
                '(' => lexer.push_simple(TokenKind::LeftParen, 1),
                ')' => lexer.push_simple(TokenKind::RightParen, 1),
                '{' => lexer.push_simple(TokenKind::LeftBrace, 1),
                '}' => lexer.push_simple(TokenKind::RightBrace, 1),
                ',' => lexer.push_simple(TokenKind::Comma, 1),
                ';' => lexer.push_simple(TokenKind::Semicolon, 1),
                '+' => lexer.push_simple(TokenKind::Plus, 1),
                '*' => lexer.push_simple(TokenKind::Star, 1),
                '/' => lexer.push_simple(TokenKind::Slash, 1),
                '%' => lexer.push_simple(TokenKind::Percent, 1),
                '|' => lexer.push_simple(TokenKind::Pipe, 1),
                '=' => lexer.push_simple(TokenKind::Equal, 1),
                '"' => lexer.read_string(),
                '$' => lexer.read_dollar(),
                '-' => lexer.read_minus_or_op(),
                _ if ch.is_ascii_digit() => lexer.read_number(),
                _ if is_ident_start(ch) => lexer.read_ident(),
                _ => {
                    lexer.offset += ch.len_utf8();
                }
            }
        }
        lexer.tokens.push(Token::eof(source.len()));
        lexer.tokens
    }

    fn push_simple(&mut self, kind: TokenKind, len: usize) {
        let start = self.offset;
        self.offset += len;
        self.tokens.push(Token { kind, span: start..self.offset });
    }

    fn skip_spaces(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch == ' ' || ch == '\t' {
                self.offset += ch.len_utf8();
            }
            else {
                break;
            }
        }
    }

    fn skip_line_comment(&mut self) {
        while let Some(ch) = self.peek_char() {
            self.offset += ch.len_utf8();
            if ch == '\n' {
                break;
            }
        }
    }

    fn read_string(&mut self) {
        let start = self.offset;
        self.offset += 1;
        while let Some(ch) = self.peek_char() {
            self.offset += ch.len_utf8();
            if ch == '"' {
                break;
            }
            if ch == '\\' || ch == '`' {
                if let Some(escaped) = self.peek_char() {
                    self.offset += escaped.len_utf8();
                }
            }
        }
        self.tokens.push(Token { kind: TokenKind::StringLiteral, span: start..self.offset });
    }

    fn read_dollar(&mut self) {
        let start = self.offset;
        self.offset += 1;
        let mut name = String::new();
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                name.push(ch);
                self.offset += ch.len_utf8();
            }
            else {
                break;
            }
        }
        let kind = match name.to_ascii_lowercase().as_str() {
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "null" => TokenKind::Null,
            _ => TokenKind::Variable,
        };
        self.tokens.push(Token { kind, span: start..self.offset });
    }

    fn read_minus_or_op(&mut self) {
        let start = self.offset;
        self.offset += 1;
        let rest = &self.source[self.offset..];
        let ops = [
            ("notlike", TokenKind::NotLike),
            ("like", TokenKind::Like),
            ("xor", TokenKind::Xor),
            ("and", TokenKind::And),
            ("not", TokenKind::Not),
            ("or", TokenKind::Or),
            ("eq", TokenKind::Eq),
            ("ne", TokenKind::Ne),
            ("lt", TokenKind::Lt),
            ("le", TokenKind::Le),
            ("gt", TokenKind::Gt),
            ("ge", TokenKind::Ge),
        ];
        for (text, kind) in ops {
            if rest.len() >= text.len() && rest[..text.len()].eq_ignore_ascii_case(text) {
                let after = rest.as_bytes().get(text.len()).copied();
                if after.is_none_or(|b| !is_ident_continue_byte(b)) {
                    self.offset += text.len();
                    self.tokens.push(Token { kind, span: start..self.offset });
                    return;
                }
            }
        }
        self.tokens.push(Token { kind: TokenKind::Minus, span: start..self.offset });
    }

    fn read_number(&mut self) {
        let start = self.offset;
        let mut is_float = false;
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_digit() {
                self.offset += 1;
            }
            else if ch == '.' && !is_float {
                is_float = true;
                self.offset += 1;
            }
            else {
                break;
            }
        }
        self.tokens.push(Token { kind: if is_float { TokenKind::FloatLiteral } else { TokenKind::IntLiteral }, span: start..self.offset });
    }

    fn read_ident(&mut self) {
        let start = self.offset;
        while let Some(ch) = self.peek_char() {
            if is_ident_continue(ch) {
                self.offset += ch.len_utf8();
            }
            else {
                break;
            }
        }
        let text = &self.source[start..self.offset];
        let kind = match text.to_ascii_lowercase().as_str() {
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "for" => TokenKind::For,
            "function" => TokenKind::Function,
            "return" => TokenKind::Return,
            _ => TokenKind::Ident,
        };
        self.tokens.push(Token { kind, span: start..self.offset });
    }

    fn peek_char(&self) -> Option<char> {
        self.source[self.offset..].chars().next()
    }
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}

fn is_ident_continue_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_write_output() {
        let tokens = Lexer::tokenize(r#"Write-Output "hello""#);
        assert_eq!(tokens[0].kind, TokenKind::Ident);
        assert_eq!(tokens[1].kind, TokenKind::StringLiteral);
    }

    #[test]
    fn tokenizes_comparison_ops() {
        let tokens = Lexer::tokenize("$x -eq 1");
        assert_eq!(tokens[0].kind, TokenKind::Variable);
        assert_eq!(tokens[1].kind, TokenKind::Eq);
        assert_eq!(tokens[2].kind, TokenKind::IntLiteral);
    }
}
