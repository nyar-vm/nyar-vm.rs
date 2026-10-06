//! Lua recursive-descent parser (mid-subset: control flow + tables).

use super::{
    LuaError,
    ast::{LuaExpr, LuaLValue, LuaNode, LuaStmt, LuaTableField},
    lexer::{Token, TokenKind},
};

pub struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    index: usize,
}

impl<'a> Parser<'a> {
    /// Parse Lua source into a block.
    pub fn parse(source: &'a str, tokens: Vec<Token>) -> Result<Vec<LuaStmt>, LuaError> {
        let mut parser = Self { source, tokens, index: 0 };
        parser.parse_block()
    }

    fn parse_block(&mut self) -> Result<Vec<LuaStmt>, LuaError> {
        let mut statements = Vec::new();
        while !self.check(TokenKind::Eof)
            && !self.check(TokenKind::End)
            && !self.check(TokenKind::Else)
            && !self.check(TokenKind::ElseIf)
            && !self.check(TokenKind::Until)
        {
            while self.match_kind(TokenKind::Semicolon) {}
            if self.check(TokenKind::Eof)
                || self.check(TokenKind::End)
                || self.check(TokenKind::Else)
                || self.check(TokenKind::ElseIf)
                || self.check(TokenKind::Until)
            {
                break;
            }
            if let Some(stmt) = self.parse_statement()? {
                statements.push(stmt);
            }
        }
        Ok(statements)
    }

    fn parse_statement(&mut self) -> Result<Option<LuaStmt>, LuaError> {
        if self.match_kind(TokenKind::Local) {
            return Ok(Some(self.parse_local()?));
        }
        if self.match_kind(TokenKind::If) {
            return Ok(Some(self.parse_if()?));
        }
        if self.match_kind(TokenKind::While) {
            return Ok(Some(self.parse_while()?));
        }
        if self.match_kind(TokenKind::Repeat) {
            return Ok(Some(self.parse_repeat()?));
        }
        if self.match_kind(TokenKind::For) {
            return Ok(Some(self.parse_for_numeric()?));
        }
        if self.match_kind(TokenKind::Break) {
            return Ok(Some(LuaNode::Break));
        }
        if self.match_kind(TokenKind::Return) {
            let value = if self.is_expression_start() { Some(self.parse_expression()?) } else { None };
            return Ok(Some(LuaNode::Return(value)));
        }
        if self.match_kind(TokenKind::Function) {
            return Ok(Some(self.parse_function_def()?));
        }

        Ok(Some(self.parse_assign_or_expr_stmt()?))
    }

    fn parse_assign_or_expr_stmt(&mut self) -> Result<LuaStmt, LuaError> {
        let first = self.parse_postfix()?;
        if self.check(TokenKind::Equal) || self.check(TokenKind::Comma) {
            let mut targets = vec![expr_to_lvalue(first)?];
            while self.match_kind(TokenKind::Comma) {
                let next = self.parse_postfix()?;
                targets.push(expr_to_lvalue(next)?);
            }
            self.expect(TokenKind::Equal)?;
            let mut values = vec![self.parse_expression()?];
            while self.match_kind(TokenKind::Comma) {
                values.push(self.parse_expression()?);
            }
            return Ok(LuaNode::Assign { targets, values });
        }
        Ok(LuaNode::ExprStmt(first))
    }

    fn parse_local(&mut self) -> Result<LuaStmt, LuaError> {
        if self.match_kind(TokenKind::Function) {
            return self.parse_function_def();
        }
        let mut names = vec![self.expect_name()?];
        while self.match_kind(TokenKind::Comma) {
            names.push(self.expect_name()?);
        }
        let mut values = Vec::new();
        if self.match_kind(TokenKind::Equal) {
            values.push(self.parse_expression()?);
            while self.match_kind(TokenKind::Comma) {
                values.push(self.parse_expression()?);
            }
        }
        Ok(LuaNode::LocalDecl { names, values })
    }

    fn parse_if(&mut self) -> Result<LuaStmt, LuaError> {
        let condition = self.parse_expression()?;
        self.expect(TokenKind::Then)?;
        let then_block = self.parse_block()?;
        let mut else_block = Vec::new();
        if self.match_kind(TokenKind::ElseIf) {
            let nested = self.parse_if()?;
            else_block.push(nested);
        }
        else if self.match_kind(TokenKind::Else) {
            else_block = self.parse_block()?;
        }
        self.expect(TokenKind::End)?;
        Ok(LuaNode::If { condition, then_block, else_block })
    }

    fn parse_while(&mut self) -> Result<LuaStmt, LuaError> {
        let condition = self.parse_expression()?;
        self.expect(TokenKind::Do)?;
        let body = self.parse_block()?;
        self.expect(TokenKind::End)?;
        Ok(LuaNode::While { condition, body })
    }

    fn parse_repeat(&mut self) -> Result<LuaStmt, LuaError> {
        let body = self.parse_block()?;
        self.expect(TokenKind::Until)?;
        let condition = self.parse_expression()?;
        Ok(LuaNode::Repeat { body, condition })
    }

    fn parse_for_numeric(&mut self) -> Result<LuaStmt, LuaError> {
        let name = self.expect_name()?;
        self.expect(TokenKind::Equal)?;
        let start = self.parse_expression()?;
        self.expect(TokenKind::Comma)?;
        let limit = self.parse_expression()?;
        let step = if self.match_kind(TokenKind::Comma) { Some(self.parse_expression()?) } else { None };
        self.expect(TokenKind::Do)?;
        let body = self.parse_block()?;
        self.expect(TokenKind::End)?;
        Ok(LuaNode::ForNumeric { name, start, limit, step, body })
    }

    fn parse_function_def(&mut self) -> Result<LuaStmt, LuaError> {
        let name = self.expect_name()?;
        self.expect(TokenKind::LeftParen)?;
        let mut params = Vec::new();
        if !self.check(TokenKind::RightParen) {
            params.push(self.expect_name()?);
            while self.match_kind(TokenKind::Comma) {
                params.push(self.expect_name()?);
            }
        }
        self.expect(TokenKind::RightParen)?;
        let body = self.parse_block()?;
        self.expect(TokenKind::End)?;
        Ok(LuaNode::FunctionDef { name, params, body })
    }

    fn parse_expression(&mut self) -> Result<LuaExpr, LuaError> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<LuaExpr, LuaError> {
        let mut left = self.parse_and()?;
        while self.match_kind(TokenKind::Or) {
            let right = self.parse_and()?;
            left = LuaExpr::Binary { op: "or".to_string(), left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<LuaExpr, LuaError> {
        let mut left = self.parse_comparison()?;
        while self.match_kind(TokenKind::And) {
            let right = self.parse_comparison()?;
            left = LuaExpr::Binary { op: "and".to_string(), left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<LuaExpr, LuaError> {
        let mut left = self.parse_concat()?;
        while matches!(
            self.peek_kind(),
            Some(TokenKind::EqualEqual)
                | Some(TokenKind::TildeEqual)
                | Some(TokenKind::Less)
                | Some(TokenKind::LessEqual)
                | Some(TokenKind::Greater)
                | Some(TokenKind::GreaterEqual)
        ) {
            let op = match self.advance().kind {
                TokenKind::EqualEqual => "==",
                TokenKind::TildeEqual => "~=",
                TokenKind::Less => "<",
                TokenKind::LessEqual => "<=",
                TokenKind::Greater => ">",
                TokenKind::GreaterEqual => ">=",
                _ => return Err(LuaError::UnexpectedToken),
            }
            .to_string();
            let right = self.parse_concat()?;
            left = LuaExpr::Binary { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_concat(&mut self) -> Result<LuaExpr, LuaError> {
        let mut left = self.parse_additive()?;
        while self.match_kind(TokenKind::DotDot) {
            let right = self.parse_additive()?;
            left = LuaExpr::Binary { op: "..".to_string(), left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<LuaExpr, LuaError> {
        let mut left = self.parse_multiplicative()?;
        while matches!(self.peek_kind(), Some(TokenKind::Plus) | Some(TokenKind::Minus)) {
            let op = if self.match_kind(TokenKind::Plus) { "+" } else { "-" }.to_string();
            let right = self.parse_multiplicative()?;
            left = LuaExpr::Binary { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<LuaExpr, LuaError> {
        let mut left = self.parse_unary()?;
        while matches!(self.peek_kind(), Some(TokenKind::Star) | Some(TokenKind::Slash) | Some(TokenKind::Percent)) {
            let op = match self.advance().kind {
                TokenKind::Star => "*",
                TokenKind::Slash => "/",
                TokenKind::Percent => "%",
                _ => return Err(LuaError::UnexpectedToken),
            }
            .to_string();
            let right = self.parse_unary()?;
            left = LuaExpr::Binary { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<LuaExpr, LuaError> {
        if self.match_kind(TokenKind::Not) {
            let operand = self.parse_unary()?;
            return Ok(LuaExpr::Unary { op: "not".to_string(), operand: Box::new(operand) });
        }
        if self.match_kind(TokenKind::Minus) {
            let operand = self.parse_unary()?;
            return Ok(LuaExpr::Unary { op: "-".to_string(), operand: Box::new(operand) });
        }
        if self.match_kind(TokenKind::Hash) {
            let operand = self.parse_unary()?;
            return Ok(LuaExpr::Unary { op: "#".to_string(), operand: Box::new(operand) });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<LuaExpr, LuaError> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.match_kind(TokenKind::LeftParen) {
                let name = match expr {
                    LuaExpr::Ident(name) => name,
                    other => return Err(LuaError::InvalidCallTarget(other)),
                };
                let mut args = Vec::new();
                if !self.check(TokenKind::RightParen) {
                    args.push(self.parse_expression()?);
                    while self.match_kind(TokenKind::Comma) {
                        args.push(self.parse_expression()?);
                    }
                }
                self.expect(TokenKind::RightParen)?;
                expr = LuaExpr::Call { name, args };
            }
            else if self.match_kind(TokenKind::Dot) {
                let name = self.expect_name()?;
                expr = LuaExpr::Field { table: Box::new(expr), name };
            }
            else if self.match_kind(TokenKind::LeftBracket) {
                let key = self.parse_expression()?;
                self.expect(TokenKind::RightBracket)?;
                expr = LuaExpr::Index { table: Box::new(expr), key: Box::new(key) };
            }
            else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<LuaExpr, LuaError> {
        if self.match_kind(TokenKind::Number) {
            let text = self.previous_text();
            let value = text.parse::<f64>().map_err(|_| LuaError::InvalidNumber)?;
            return Ok(LuaExpr::Number(value));
        }
        if self.match_kind(TokenKind::String) {
            return Ok(LuaExpr::String(unquote_string(self.previous_text())));
        }
        if self.match_kind(TokenKind::True) {
            return Ok(LuaExpr::Bool(true));
        }
        if self.match_kind(TokenKind::False) {
            return Ok(LuaExpr::Bool(false));
        }
        if self.match_kind(TokenKind::Nil) {
            return Ok(LuaExpr::Nil);
        }
        if self.match_kind(TokenKind::Name) | self.match_kind(TokenKind::Print) {
            return Ok(LuaExpr::Ident(self.previous_text().to_string()));
        }
        if self.match_kind(TokenKind::LeftBrace) {
            return self.parse_table_constructor();
        }
        if self.match_kind(TokenKind::LeftParen) {
            let expr = self.parse_expression()?;
            self.expect(TokenKind::RightParen)?;
            return Ok(expr);
        }
        Err(LuaError::UnexpectedToken)
    }

    fn parse_table_constructor(&mut self) -> Result<LuaExpr, LuaError> {
        let mut fields = Vec::new();
        while !self.check(TokenKind::RightBrace) && !self.check(TokenKind::Eof) {
            if self.match_kind(TokenKind::LeftBracket) {
                let key = self.parse_expression()?;
                self.expect(TokenKind::RightBracket)?;
                self.expect(TokenKind::Equal)?;
                let value = self.parse_expression()?;
                fields.push(LuaTableField::Indexed { key, value });
            }
            else if matches!(self.peek_kind(), Some(TokenKind::Name) | Some(TokenKind::Print))
                && matches!(self.peek_kind_at(1), Some(TokenKind::Equal))
            {
                let key = self.expect_name()?;
                self.expect(TokenKind::Equal)?;
                let value = self.parse_expression()?;
                fields.push(LuaTableField::Record { key, value });
            }
            else {
                fields.push(LuaTableField::Array(self.parse_expression()?));
            }
            if !self.match_kind(TokenKind::Comma) && !self.match_kind(TokenKind::Semicolon) {
                break;
            }
        }
        self.expect(TokenKind::RightBrace)?;
        Ok(LuaExpr::Table { fields })
    }

    fn is_expression_start(&self) -> bool {
        matches!(
            self.peek_kind(),
            Some(TokenKind::Number)
                | Some(TokenKind::String)
                | Some(TokenKind::True)
                | Some(TokenKind::False)
                | Some(TokenKind::Nil)
                | Some(TokenKind::Name)
                | Some(TokenKind::Print)
                | Some(TokenKind::Not)
                | Some(TokenKind::Minus)
                | Some(TokenKind::Hash)
                | Some(TokenKind::LeftParen)
                | Some(TokenKind::LeftBrace)
        )
    }

    fn expect_name(&mut self) -> Result<String, LuaError> {
        if self.match_kind(TokenKind::Name) | self.match_kind(TokenKind::Print) {
            Ok(self.previous_text().to_string())
        }
        else {
            Err(LuaError::ExpectedName)
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Result<(), LuaError> {
        if self.match_kind(kind) { Ok(()) } else { Err(LuaError::ExpectedToken(kind)) }
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

    fn peek_kind_at(&self, offset: usize) -> Option<TokenKind> {
        self.tokens.get(self.index + offset).map(|token| token.kind)
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

fn expr_to_lvalue(expr: LuaExpr) -> Result<LuaLValue, LuaError> {
    match expr {
        LuaExpr::Ident(name) => Ok(LuaLValue::Name(name)),
        LuaExpr::Index { table, key } => Ok(LuaLValue::Index { table: *table, key: *key }),
        LuaExpr::Field { table, name } => Ok(LuaLValue::Field { table: *table, name }),
        _ => Err(LuaError::InvalidAssignTarget),
    }
}

fn unquote_string(text: &str) -> String {
    if text.len() >= 2 {
        let quote = text.as_bytes()[0];
        if (quote == b'"' || quote == b'\'') && text.as_bytes()[text.len() - 1] == quote {
            return text[1..text.len() - 1].to_string();
        }
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transitional::guest_scripts::lua::LuaScript;

    #[test]
    fn parse_local_and_print() {
        let script = LuaScript::parse("local x = 1 + 2\nprint(x)").expect("parse");
        assert_eq!(script.statements.len(), 2);
    }

    #[test]
    fn parse_if_while() {
        let source = r#"
if x > 0 then
  print("ok")
end
while x < 5 do
  x = x + 1
end
"#;
        let script = LuaScript::parse(source).expect("parse");
        assert!(script.statements.len() >= 2);
    }

    #[test]
    fn parse_repeat_for_tables() {
        let source = r#"
local t = {1, 2, name = "lua"}
local a, b = 1, 2
repeat
  a = a + 1
until a > 2
for i = 1, 3 do
  t[i] = i
end
print(t.name .. #t)
"#;
        let script = LuaScript::parse(source).expect("parse");
        assert!(script.statements.len() >= 5);
    }

    #[test]
    fn parse_multi_assign_and_break() {
        let source = r#"
a, b = 10, 20
while true do
  break
end
"#;
        let script = LuaScript::parse(source).expect("parse");
        assert!(matches!(script.statements[0], LuaNode::Assign { .. }));
        assert!(matches!(script.statements[1], LuaNode::While { .. }));
    }
}
