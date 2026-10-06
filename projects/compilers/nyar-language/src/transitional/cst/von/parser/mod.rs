use std::collections::BTreeMap;

use super::{
    error::VonParseError,
    lexer::{Lexer, Token, TokenKind},
    lexical,
    value::VonValue,
};

/// 基于 token 流的 VON 解析器。
pub struct VonParser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    index: usize,
}

impl<'a> VonParser<'a> {
    /// 解析 VON 文本为 `VonValue`。
    pub fn parse(source: &'a str) -> Result<VonValue, VonParseError> {
        let tokens = Lexer::tokenize(source)?;
        let mut parser = Self { source, tokens, index: 0 };
        let value = parser.parse_value()?;
        if !parser.is_at_end() {
            return Err(parser.error("unexpected trailing input"));
        }
        Ok(value)
    }

    fn parse_value(&mut self) -> Result<VonValue, VonParseError> {
        match self.peek_kind() {
            TokenKind::LBrace => self.parse_object(),
            TokenKind::LBracket => self.parse_array(),
            TokenKind::StringLiteral => self.parse_string_value(),
            TokenKind::IntegerLiteral => self.parse_number_value(),
            TokenKind::Null => {
                self.advance();
                Ok(VonValue::Null)
            }
            TokenKind::True => {
                self.advance();
                Ok(VonValue::Bool(true))
            }
            TokenKind::False => {
                self.advance();
                Ok(VonValue::Bool(false))
            }
            TokenKind::Identifier => {
                let identifier = self.parse_identifier_text()?;
                Ok(VonValue::String(identifier))
            }
            TokenKind::Eof => Err(self.error("unexpected end of input")),
            _ => Err(self.error(format!("unexpected token {:?}", self.peek_kind()))),
        }
    }

    fn parse_object(&mut self) -> Result<VonValue, VonParseError> {
        self.expect(TokenKind::LBrace)?;
        let mut members = BTreeMap::new();
        loop {
            if self.check(TokenKind::RBrace) {
                self.advance();
                break;
            }

            let key = self.parse_key()?;
            self.expect(TokenKind::Colon)?;
            let value = self.parse_value()?;
            members.insert(key, value);

            if self.consume_if(TokenKind::Comma) {
                continue;
            }
            if self.check(TokenKind::RBrace) {
                self.advance();
                break;
            }
            return Err(self.error("expected ',' or '}'"));
        }
        Ok(VonValue::Object(members))
    }

    fn parse_array(&mut self) -> Result<VonValue, VonParseError> {
        self.expect(TokenKind::LBracket)?;
        let mut items = Vec::new();
        loop {
            if self.check(TokenKind::RBracket) {
                self.advance();
                break;
            }
            items.push(self.parse_value()?);
            if self.consume_if(TokenKind::Comma) {
                continue;
            }
            if self.check(TokenKind::RBracket) {
                self.advance();
                break;
            }
            return Err(self.error("expected ',' or ']'"));
        }
        Ok(VonValue::Array(items))
    }

    fn parse_key(&mut self) -> Result<String, VonParseError> {
        match self.peek_kind() {
            TokenKind::StringLiteral => self.parse_string_value().map(|value| match value {
                VonValue::String(text) => text,
                _ => unreachable!("string literal must decode to string"),
            }),
            TokenKind::Identifier => self.parse_identifier_text(),
            _ => Err(self.error("expected object key")),
        }
    }

    fn parse_string_value(&mut self) -> Result<VonValue, VonParseError> {
        let token = self.advance();
        let start = token.span.start;
        let raw = &self.source[token.span];
        let inner =
            raw.strip_prefix('"').and_then(|text| text.strip_suffix('"')).ok_or_else(|| self.error_at(start, "invalid string literal"))?;
        let decoded = lexical::decode_string_literal(inner).map_err(|message| self.error_at(start, message))?;
        Ok(VonValue::String(decoded))
    }

    fn parse_number_value(&mut self) -> Result<VonValue, VonParseError> {
        let token = self.advance();
        let start = token.span.start;
        let text = &self.source[token.span];
        let number = text.parse::<i64>().map_err(|_| self.error_at(start, "invalid integer"))?;
        Ok(VonValue::Number(number))
    }

    fn parse_identifier_text(&mut self) -> Result<String, VonParseError> {
        let token = self.advance();
        Ok(self.source[token.span].to_string())
    }

    fn expect(&mut self, kind: TokenKind) -> Result<(), VonParseError> {
        if self.check(kind) {
            self.advance();
            Ok(())
        }
        else {
            Err(self.error(format!("expected {:?}", kind)))
        }
    }

    fn consume_if(&mut self, kind: TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        }
        else {
            false
        }
    }

    fn check(&self, kind: TokenKind) -> bool {
        self.peek_kind() == kind
    }

    fn peek_kind(&self) -> TokenKind {
        self.tokens.get(self.index).map(|token| token.kind).unwrap_or(TokenKind::Eof)
    }

    fn advance(&mut self) -> Token {
        let token = self.tokens[self.index].clone();
        if !self.is_at_end() {
            self.index += 1;
        }
        token
    }

    fn is_at_end(&self) -> bool {
        self.peek_kind() == TokenKind::Eof
    }

    fn error(&self, message: impl Into<String>) -> VonParseError {
        let position = self.tokens.get(self.index).map(|token| token.span.start).unwrap_or(self.source.len());
        VonParseError::new(position, message)
    }

    fn error_at(&self, position: usize, message: impl Into<String>) -> VonParseError {
        VonParseError::new(position, message)
    }
}
