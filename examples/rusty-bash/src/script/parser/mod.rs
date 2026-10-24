//! Bash recursive-descent parser (minimal subset).

use super::{
    BashError,
    ast::{BashRedirect, BashStmt},
    lexer::{Token, TokenKind},
};

/// Bash parser.
pub struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    index: usize,
}

impl<'a> Parser<'a> {
    /// Parse tokens into statements.
    pub fn parse(source: &'a str, tokens: Vec<Token>) -> Result<Vec<BashStmt>, BashError> {
        let mut parser = Self { source, tokens, index: 0 };
        parser.parse_list()
    }

    fn parse_list(&mut self) -> Result<Vec<BashStmt>, BashError> {
        let mut statements = Vec::new();
        self.skip_separators();
        while !self.check(TokenKind::Eof)
            && !self.check(TokenKind::Fi)
            && !self.check(TokenKind::Else)
            && !self.check(TokenKind::Elif)
            && !self.check(TokenKind::Done)
            && !self.check(TokenKind::RightBrace)
            && !self.check(TokenKind::Then)
            && !self.check(TokenKind::Do)
        {
            if let Some(stmt) = self.parse_and_or()? {
                statements.push(stmt);
            }
            self.skip_separators();
        }
        Ok(statements)
    }

    fn parse_and_or(&mut self) -> Result<Option<BashStmt>, BashError> {
        let Some(mut left) = self.parse_pipeline()?
        else {
            return Ok(None);
        };
        while matches!(self.peek_kind(), Some(TokenKind::AndAnd) | Some(TokenKind::OrOr)) {
            let op = if self.match_kind(TokenKind::AndAnd) {
                "&&".to_string()
            }
            else if self.match_kind(TokenKind::OrOr) {
                "||".to_string()
            }
            else {
                return Err(BashError::UnexpectedToken);
            };
            self.skip_newlines();
            let right = self.parse_pipeline()?.ok_or(BashError::UnexpectedToken)?;
            left = BashStmt::AndOr { left: Box::new(left), op, right: Box::new(right) };
        }
        Ok(Some(left))
    }

    fn parse_pipeline(&mut self) -> Result<Option<BashStmt>, BashError> {
        let Some(first) = self.parse_command()?
        else {
            return Ok(None);
        };
        if !self.check(TokenKind::Pipe) {
            return Ok(Some(first));
        }
        let mut stages = vec![first];
        while self.match_kind(TokenKind::Pipe) {
            self.skip_newlines();
            let stage = self.parse_command()?.ok_or(BashError::UnexpectedToken)?;
            stages.push(stage);
        }
        Ok(Some(BashStmt::Pipeline { stages }))
    }

    fn parse_command(&mut self) -> Result<Option<BashStmt>, BashError> {
        if self.match_kind(TokenKind::If) {
            return Ok(Some(self.parse_if()?));
        }
        if self.match_kind(TokenKind::While) {
            return Ok(Some(self.parse_while()?));
        }
        if self.match_kind(TokenKind::For) {
            return Ok(Some(self.parse_for()?));
        }
        if self.match_kind(TokenKind::Function) {
            return Ok(Some(self.parse_function_keyword()?));
        }
        if self.match_kind(TokenKind::Return) {
            let code = self.take_word().and_then(|text| text.parse::<i64>().ok());
            return Ok(Some(BashStmt::Return(code)));
        }
        if self.match_kind(TokenKind::Break) {
            return Ok(Some(BashStmt::Break));
        }
        if self.match_kind(TokenKind::Continue) {
            return Ok(Some(BashStmt::Continue));
        }
        if self.match_kind(TokenKind::LeftBrace) {
            let body = self.parse_list()?;
            self.expect(TokenKind::RightBrace)?;
            return Ok(Some(BashStmt::Group(body)));
        }

        // `name() { ... }`
        if self.check(TokenKind::Word) && self.check_at(1, TokenKind::LeftParen) && self.check_at(2, TokenKind::RightParen) {
            let name = self.expect_word()?;
            self.expect(TokenKind::LeftParen)?;
            self.expect(TokenKind::RightParen)?;
            self.skip_newlines();
            self.expect(TokenKind::LeftBrace)?;
            let body = self.parse_list()?;
            self.expect(TokenKind::RightBrace)?;
            return Ok(Some(BashStmt::FunctionDef { name, body }));
        }

        // `NAME=value` / `export NAME=value`
        if self.check(TokenKind::Word) && self.text_at(0) == Some("export") {
            self.advance();
            let name = self.expect_word()?;
            let value = if self.match_kind(TokenKind::Equal) { Some(self.read_assign_value()) } else { None };
            return Ok(Some(BashStmt::Export { name, value }));
        }

        if self.check(TokenKind::Word) && self.check_at(1, TokenKind::Equal) {
            let name = self.expect_word()?;
            if !is_ident(&name) {
                // Treat as a normal command word; put `=` back by reparsing — rare.
                return Ok(Some(self.parse_simple_command_starting(name)?));
            }
            self.expect(TokenKind::Equal)?;
            let value = self.read_assign_value();
            return Ok(Some(BashStmt::Assign { name, value }));
        }

        if self.is_wordish() {
            return Ok(Some(self.parse_simple_command()?));
        }

        Ok(None)
    }

    fn parse_simple_command(&mut self) -> Result<BashStmt, BashError> {
        let first = self.take_wordish().ok_or(BashError::UnexpectedToken)?;
        self.parse_simple_command_starting(first)
    }

    fn parse_simple_command_starting(&mut self, first: String) -> Result<BashStmt, BashError> {
        let mut words = vec![first];
        let mut redirects = Vec::new();
        loop {
            if self.match_kind(TokenKind::Greater) {
                let path = self.take_wordish().ok_or(BashError::UnexpectedToken)?;
                redirects.push(BashRedirect::Write { path, append: false });
                continue;
            }
            if self.match_kind(TokenKind::GreaterGreater) {
                let path = self.take_wordish().ok_or(BashError::UnexpectedToken)?;
                redirects.push(BashRedirect::Write { path, append: true });
                continue;
            }
            if self.match_kind(TokenKind::Less) {
                let path = self.take_wordish().ok_or(BashError::UnexpectedToken)?;
                redirects.push(BashRedirect::Read { path });
                continue;
            }
            if let Some(word) = self.take_wordish() {
                words.push(word);
                continue;
            }
            break;
        }
        Ok(BashStmt::Command { words, redirects })
    }

    fn parse_if(&mut self) -> Result<BashStmt, BashError> {
        self.parse_if_arm(true)
    }

    /// Parse one `if`/`elif` arm. Only the outermost arm consumes the closing `fi`.
    fn parse_if_arm(&mut self, consume_fi: bool) -> Result<BashStmt, BashError> {
        let condition = Box::new(self.parse_and_or()?.ok_or(BashError::UnexpectedToken)?);
        self.skip_separators();
        self.expect(TokenKind::Then)?;
        self.skip_separators();
        let then_body = self.parse_list()?;
        let mut else_body = Vec::new();
        if self.match_kind(TokenKind::Elif) {
            else_body.push(self.parse_if_arm(false)?);
        }
        else if self.match_kind(TokenKind::Else) {
            self.skip_separators();
            else_body = self.parse_list()?;
        }
        if consume_fi {
            self.expect(TokenKind::Fi)?;
        }
        Ok(BashStmt::If { condition, then_body, else_body })
    }

    fn parse_while(&mut self) -> Result<BashStmt, BashError> {
        let condition = Box::new(self.parse_and_or()?.ok_or(BashError::UnexpectedToken)?);
        self.skip_separators();
        self.expect(TokenKind::Do)?;
        self.skip_separators();
        let body = self.parse_list()?;
        self.expect(TokenKind::Done)?;
        Ok(BashStmt::While { condition, body })
    }

    fn parse_for(&mut self) -> Result<BashStmt, BashError> {
        let var = self.expect_word()?;
        self.expect(TokenKind::In)?;
        let mut items = Vec::new();
        while self.is_wordish() {
            items.push(self.take_wordish().unwrap());
        }
        self.skip_separators();
        self.expect(TokenKind::Do)?;
        self.skip_separators();
        let body = self.parse_list()?;
        self.expect(TokenKind::Done)?;
        Ok(BashStmt::For { var, items, body })
    }

    fn parse_function_keyword(&mut self) -> Result<BashStmt, BashError> {
        let name = self.expect_word()?;
        if self.match_kind(TokenKind::LeftParen) {
            self.expect(TokenKind::RightParen)?;
        }
        self.skip_newlines();
        self.expect(TokenKind::LeftBrace)?;
        let body = self.parse_list()?;
        self.expect(TokenKind::RightBrace)?;
        Ok(BashStmt::FunctionDef { name, body })
    }

    fn read_assign_value(&mut self) -> String {
        self.take_wordish().unwrap_or_default()
    }

    fn skip_separators(&mut self) {
        while matches!(self.peek_kind(), Some(TokenKind::Newline) | Some(TokenKind::Semicolon)) {
            self.advance();
        }
    }

    fn skip_newlines(&mut self) {
        while self.match_kind(TokenKind::Newline) {}
    }

    fn is_wordish(&self) -> bool {
        matches!(
            self.peek_kind(),
            Some(TokenKind::Word)
                | Some(TokenKind::String)
                | Some(TokenKind::If)
                | Some(TokenKind::Then)
                | Some(TokenKind::Else)
                | Some(TokenKind::Elif)
                | Some(TokenKind::Fi)
                | Some(TokenKind::While)
                | Some(TokenKind::Do)
                | Some(TokenKind::Done)
                | Some(TokenKind::For)
                | Some(TokenKind::In)
                | Some(TokenKind::Function)
                | Some(TokenKind::Return)
                | Some(TokenKind::Break)
                | Some(TokenKind::Continue)
        )
    }

    fn take_wordish(&mut self) -> Option<String> {
        if !self.is_wordish() {
            return None;
        }
        let token = self.advance();
        Some(self.source[token.span.clone()].to_string())
    }

    fn take_word(&mut self) -> Option<String> {
        if self.check(TokenKind::Word) {
            let token = self.advance();
            Some(self.source[token.span.clone()].to_string())
        }
        else {
            None
        }
    }

    fn expect_word(&mut self) -> Result<String, BashError> {
        self.take_word().ok_or(BashError::ExpectedName)
    }

    fn text_at(&self, offset: usize) -> Option<&str> {
        let token = self.tokens.get(self.index + offset)?;
        Some(&self.source[token.span.clone()])
    }

    fn check(&self, kind: TokenKind) -> bool {
        self.peek_kind() == Some(kind)
    }

    fn check_at(&self, offset: usize, kind: TokenKind) -> bool {
        self.tokens.get(self.index + offset).map(|token| token.kind) == Some(kind)
    }

    fn match_kind(&mut self, kind: TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        }
        else {
            false
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Result<(), BashError> {
        if self.match_kind(kind) { Ok(()) } else { Err(BashError::ExpectedToken(kind)) }
    }

    fn peek_kind(&self) -> Option<TokenKind> {
        self.tokens.get(self.index).map(|token| token.kind)
    }

    fn advance(&mut self) -> Token {
        let token = self.tokens[self.index].clone();
        if token.kind != TokenKind::Eof {
            self.index += 1;
        }
        token
    }
}

fn is_ident(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(ch) if ch.is_ascii_alphabetic() || ch == '_' => chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_'),
        _ => false,
    }
}
