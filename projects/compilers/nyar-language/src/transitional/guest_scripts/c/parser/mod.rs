//! C recursive-descent parser (minimal subset).

use super::{
    CError,
    ast::{CExpr, CFunction, CItem, CStmt, CVarDecl},
    lexer::{Token, TokenKind},
};

/// Recursive-descent parser.
pub struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    index: usize,
}

impl<'a> Parser<'a> {
    /// Parse a translation unit into top-level items.
    pub fn parse(source: &'a str, tokens: Vec<Token>) -> Result<Vec<CItem>, CError> {
        let mut parser = Self { source, tokens, index: 0 };
        parser.parse_translation_unit()
    }

    fn parse_translation_unit(&mut self) -> Result<Vec<CItem>, CError> {
        let mut items = Vec::new();
        while !self.check(TokenKind::Eof) {
            items.push(self.parse_external_declaration()?);
        }
        Ok(items)
    }

    fn parse_external_declaration(&mut self) -> Result<CItem, CError> {
        let ty = self.parse_type_name()?;
        let name = self.expect_ident()?;
        if self.match_kind(TokenKind::LeftParen) {
            let mut params = Vec::new();
            if !self.check(TokenKind::RightParen) {
                if self.match_kind(TokenKind::Void) {
                    // `void` parameter list
                }
                else {
                    loop {
                        let _param_ty = self.parse_type_name()?;
                        // Allow unnamed parameters in prototypes we still treat as definitions.
                        if self.check(TokenKind::Ident) {
                            params.push(self.expect_ident()?);
                        }
                        else {
                            params.push(format!("_{}", params.len()));
                        }
                        if !self.match_kind(TokenKind::Comma) {
                            break;
                        }
                    }
                }
            }
            self.expect(TokenKind::RightParen)?;
            let body = self.parse_compound_statement()?;
            Ok(CItem::Function(CFunction { return_type: ty, name, params, body }))
        }
        else {
            let init = if self.match_kind(TokenKind::Equal) { Some(self.parse_expression()?) } else { None };
            self.expect(TokenKind::Semicolon)?;
            Ok(CItem::GlobalVar(CVarDecl { ty, name, init }))
        }
    }

    fn parse_compound_statement(&mut self) -> Result<Vec<CStmt>, CError> {
        self.expect(TokenKind::LeftBrace)?;
        let mut statements = Vec::new();
        while !self.check(TokenKind::RightBrace) && !self.check(TokenKind::Eof) {
            statements.push(self.parse_statement()?);
        }
        self.expect(TokenKind::RightBrace)?;
        Ok(statements)
    }

    fn parse_statement(&mut self) -> Result<CStmt, CError> {
        if self.check(TokenKind::LeftBrace) {
            return Ok(CStmt::Block(self.parse_compound_statement()?));
        }
        if self.match_kind(TokenKind::Return) {
            let value = if self.check(TokenKind::Semicolon) { None } else { Some(self.parse_expression()?) };
            self.expect(TokenKind::Semicolon)?;
            return Ok(CStmt::Return(value));
        }
        if self.match_kind(TokenKind::If) {
            self.expect(TokenKind::LeftParen)?;
            let condition = self.parse_expression()?;
            self.expect(TokenKind::RightParen)?;
            let then_branch = Box::new(self.parse_statement()?);
            let else_branch = if self.match_kind(TokenKind::Else) { Some(Box::new(self.parse_statement()?)) } else { None };
            return Ok(CStmt::If { condition, then_branch, else_branch });
        }
        if self.match_kind(TokenKind::While) {
            self.expect(TokenKind::LeftParen)?;
            let condition = self.parse_expression()?;
            self.expect(TokenKind::RightParen)?;
            let body = Box::new(self.parse_statement()?);
            return Ok(CStmt::While { condition, body });
        }
        if self.match_kind(TokenKind::For) {
            self.expect(TokenKind::LeftParen)?;
            let init = if self.check(TokenKind::Semicolon) {
                self.advance();
                None
            }
            else if self.is_type_start() {
                Some(Box::new(self.parse_declaration_statement()?))
            }
            else {
                let expr = self.parse_expression()?;
                self.expect(TokenKind::Semicolon)?;
                Some(Box::new(CStmt::Expr(expr)))
            };
            let condition = if self.check(TokenKind::Semicolon) { None } else { Some(self.parse_expression()?) };
            self.expect(TokenKind::Semicolon)?;
            let step = if self.check(TokenKind::RightParen) { None } else { Some(self.parse_expression()?) };
            self.expect(TokenKind::RightParen)?;
            let body = Box::new(self.parse_statement()?);
            return Ok(CStmt::For { init, condition, step, body });
        }
        if self.match_kind(TokenKind::Break) {
            self.expect(TokenKind::Semicolon)?;
            return Ok(CStmt::Break);
        }
        if self.match_kind(TokenKind::Continue) {
            self.expect(TokenKind::Semicolon)?;
            return Ok(CStmt::Continue);
        }
        if self.is_type_start() {
            return self.parse_declaration_statement();
        }
        let expr = self.parse_expression()?;
        self.expect(TokenKind::Semicolon)?;
        Ok(CStmt::Expr(expr))
    }

    fn parse_declaration_statement(&mut self) -> Result<CStmt, CError> {
        let ty = self.parse_type_name()?;
        let name = self.expect_ident()?;
        let init = if self.match_kind(TokenKind::Equal) { Some(self.parse_expression()?) } else { None };
        self.expect(TokenKind::Semicolon)?;
        Ok(CStmt::Decl(CVarDecl { ty, name, init }))
    }

    fn parse_expression(&mut self) -> Result<CExpr, CError> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<CExpr, CError> {
        let left = self.parse_logical_or()?;
        if self.match_kind(TokenKind::Equal) {
            let value = self.parse_assignment()?;
            return match left {
                CExpr::Ident(name) => Ok(CExpr::Assign { name, value: Box::new(value) }),
                _ => Err(CError::InvalidAssignTarget),
            };
        }
        Ok(left)
    }

    fn parse_logical_or(&mut self) -> Result<CExpr, CError> {
        let mut left = self.parse_logical_and()?;
        while self.match_kind(TokenKind::PipePipe) {
            let right = self.parse_logical_and()?;
            left = CExpr::Binary { op: "||".to_string(), left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_logical_and(&mut self) -> Result<CExpr, CError> {
        let mut left = self.parse_equality()?;
        while self.match_kind(TokenKind::AmpAmp) {
            let right = self.parse_equality()?;
            left = CExpr::Binary { op: "&&".to_string(), left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<CExpr, CError> {
        let mut left = self.parse_relational()?;
        while matches!(self.peek_kind(), Some(TokenKind::EqualEqual) | Some(TokenKind::BangEqual)) {
            let op = if self.match_kind(TokenKind::EqualEqual) { "==" } else { "!=" }.to_string();
            let right = self.parse_relational()?;
            left = CExpr::Binary { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_relational(&mut self) -> Result<CExpr, CError> {
        let mut left = self.parse_additive()?;
        while matches!(
            self.peek_kind(),
            Some(TokenKind::Less) | Some(TokenKind::LessEqual) | Some(TokenKind::Greater) | Some(TokenKind::GreaterEqual)
        ) {
            let op = match self.advance().kind {
                TokenKind::Less => "<",
                TokenKind::LessEqual => "<=",
                TokenKind::Greater => ">",
                TokenKind::GreaterEqual => ">=",
                _ => return Err(CError::UnexpectedToken),
            }
            .to_string();
            let right = self.parse_additive()?;
            left = CExpr::Binary { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<CExpr, CError> {
        let mut left = self.parse_multiplicative()?;
        while matches!(self.peek_kind(), Some(TokenKind::Plus) | Some(TokenKind::Minus)) {
            let op = if self.match_kind(TokenKind::Plus) { "+" } else { "-" }.to_string();
            let right = self.parse_multiplicative()?;
            left = CExpr::Binary { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<CExpr, CError> {
        let mut left = self.parse_unary()?;
        while matches!(self.peek_kind(), Some(TokenKind::Star) | Some(TokenKind::Slash) | Some(TokenKind::Percent)) {
            let op = match self.advance().kind {
                TokenKind::Star => "*",
                TokenKind::Slash => "/",
                TokenKind::Percent => "%",
                _ => return Err(CError::UnexpectedToken),
            }
            .to_string();
            let right = self.parse_unary()?;
            left = CExpr::Binary { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<CExpr, CError> {
        if self.match_kind(TokenKind::Bang) {
            let operand = self.parse_unary()?;
            return Ok(CExpr::Unary { op: "!".to_string(), operand: Box::new(operand) });
        }
        if self.match_kind(TokenKind::Minus) {
            let operand = self.parse_unary()?;
            return Ok(CExpr::Unary { op: "-".to_string(), operand: Box::new(operand) });
        }
        if self.match_kind(TokenKind::Plus) {
            return self.parse_unary();
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<CExpr, CError> {
        let mut expr = self.parse_primary()?;
        if self.match_kind(TokenKind::LeftParen) {
            let name = match expr {
                CExpr::Ident(name) => name,
                other => return Err(CError::InvalidCallTarget(other)),
            };
            let mut args = Vec::new();
            if !self.check(TokenKind::RightParen) {
                args.push(self.parse_expression()?);
                while self.match_kind(TokenKind::Comma) {
                    args.push(self.parse_expression()?);
                }
            }
            self.expect(TokenKind::RightParen)?;
            expr = CExpr::Call { name, args };
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<CExpr, CError> {
        if self.match_kind(TokenKind::IntLiteral) {
            let text = self.previous_text();
            let value = text.parse::<i64>().map_err(|_| CError::InvalidNumber)?;
            return Ok(CExpr::Int(value));
        }
        if self.match_kind(TokenKind::FloatLiteral) {
            let text = self.previous_text();
            let value = text.parse::<f64>().map_err(|_| CError::InvalidNumber)?;
            return Ok(CExpr::Float(value));
        }
        if self.match_kind(TokenKind::StringLiteral) {
            return Ok(CExpr::String(unquote_string(self.previous_text())));
        }
        if self.match_kind(TokenKind::CharLiteral) {
            return Ok(CExpr::Char(decode_char_literal(self.previous_text())?));
        }
        if self.match_kind(TokenKind::Ident) {
            return Ok(CExpr::Ident(self.previous_text().to_string()));
        }
        if self.match_kind(TokenKind::LeftParen) {
            let expr = self.parse_expression()?;
            self.expect(TokenKind::RightParen)?;
            return Ok(expr);
        }
        Err(CError::UnexpectedToken)
    }

    fn parse_type_name(&mut self) -> Result<String, CError> {
        let kind = self.peek_kind().ok_or(CError::UnexpectedToken)?;
        let name = match kind {
            TokenKind::Int => "int",
            TokenKind::Void => "void",
            TokenKind::Char => "char",
            TokenKind::Float => "float",
            TokenKind::Double => "double",
            _ => return Err(CError::ExpectedType),
        };
        self.advance();
        Ok(name.to_string())
    }

    fn is_type_start(&self) -> bool {
        matches!(
            self.peek_kind(),
            Some(TokenKind::Int) | Some(TokenKind::Void) | Some(TokenKind::Char) | Some(TokenKind::Float) | Some(TokenKind::Double)
        )
    }

    fn expect_ident(&mut self) -> Result<String, CError> {
        if self.match_kind(TokenKind::Ident) { Ok(self.previous_text().to_string()) } else { Err(CError::ExpectedName) }
    }

    fn expect(&mut self, kind: TokenKind) -> Result<(), CError> {
        if self.match_kind(kind) { Ok(()) } else { Err(CError::ExpectedToken(kind)) }
    }

    fn match_kind(&mut self, kind: TokenKind) -> bool {
        if self.check(kind) {
            self.index += 1;
            true
        }
        else {
            false
        }
    }

    fn check(&self, kind: TokenKind) -> bool {
        self.peek_kind() == Some(kind)
    }

    fn peek_kind(&self) -> Option<TokenKind> {
        self.tokens.get(self.index).map(|token| token.kind)
    }

    fn advance(&mut self) -> Token {
        let token = self.tokens[self.index].clone();
        if !self.check(TokenKind::Eof) {
            self.index += 1;
        }
        token
    }

    fn previous_text(&self) -> &str {
        let token = &self.tokens[self.index - 1];
        &self.source[token.span.clone()]
    }
}

fn unquote_string(text: &str) -> String {
    if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
        let inner = &text[1..text.len() - 1];
        return unescape(inner);
    }
    text.to_string()
}

fn decode_char_literal(text: &str) -> Result<i64, CError> {
    if text.len() >= 3 && text.starts_with('\'') && text.ends_with('\'') {
        let inner = &text[1..text.len() - 1];
        let chars: Vec<char> = unescape(inner).chars().collect();
        if let Some(ch) = chars.first() {
            return Ok(i64::from(*ch as u32));
        }
    }
    Err(CError::InvalidNumber)
}

fn unescape(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some('\'') => out.push('\''),
                Some(other) => out.push(other),
                None => out.push('\\'),
            }
        }
        else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transitional::guest_scripts::c::CScript;

    #[test]
    fn parse_main_printf() {
        let source = r#"
#include <stdio.h>
int main(void) {
    int x = 1 + 2;
    printf("%d\n", x);
    return 0;
}
"#;
        let script = CScript::parse(source).expect("parse");
        assert_eq!(script.items.len(), 1);
    }
}
