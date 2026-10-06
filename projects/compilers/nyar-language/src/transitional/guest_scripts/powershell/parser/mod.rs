//! PowerShell recursive-descent parser (legend demo subset).

use super::{
    PowerShellError,
    ast::{PsExpr, PsStmt},
    lexer::{Lexer, Token, TokenKind},
};

/// Recursive-descent parser.
pub struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    index: usize,
}

impl<'a> Parser<'a> {
    /// Parse a PowerShell script into statements.
    pub fn parse(source: &'a str) -> Result<Vec<PsStmt>, PowerShellError> {
        let tokens = Lexer::tokenize(source);
        let mut parser = Self { source, tokens, index: 0 };
        let statements = parser.parse_statement_list(TokenKind::Eof)?;
        if statements.is_empty() {
            return Err(PowerShellError::EmptyScript);
        }
        Ok(statements)
    }

    fn parse_statement_list(&mut self, end: TokenKind) -> Result<Vec<PsStmt>, PowerShellError> {
        let mut statements = Vec::new();
        self.skip_separators();
        while !self.check(end) && !self.check(TokenKind::Eof) {
            statements.push(self.parse_statement()?);
            self.skip_separators();
        }
        Ok(statements)
    }

    fn parse_statement(&mut self) -> Result<PsStmt, PowerShellError> {
        if self.check(TokenKind::LeftBrace) {
            return Ok(PsStmt::Block(self.parse_block()?));
        }
        if self.match_kind(TokenKind::If) {
            return self.parse_if();
        }
        if self.match_kind(TokenKind::While) {
            return self.parse_while();
        }
        if self.match_kind(TokenKind::For) {
            return self.parse_for();
        }
        if self.match_kind(TokenKind::Function) {
            return self.parse_function();
        }
        if self.match_kind(TokenKind::Return) {
            let value = if self.at_statement_end() { None } else { Some(self.parse_expression()?) };
            return Ok(PsStmt::Return(value));
        }
        if self.check(TokenKind::Variable) {
            let checkpoint = self.index;
            let name = self.variable_name()?;
            if self.match_kind(TokenKind::Equal) {
                let value = self.parse_expression()?;
                return Ok(PsStmt::Assign { name, value });
            }
            self.index = checkpoint;
        }
        Ok(PsStmt::Expr(self.parse_expression()?))
    }

    fn parse_if(&mut self) -> Result<PsStmt, PowerShellError> {
        self.expect(TokenKind::LeftParen)?;
        let condition = self.parse_expression()?;
        self.expect(TokenKind::RightParen)?;
        let then_branch = self.parse_block()?;
        self.skip_newlines_only();
        let else_branch = if self.match_kind(TokenKind::Else) { self.parse_block()? } else { Vec::new() };
        Ok(PsStmt::If { condition, then_branch, else_branch })
    }

    fn parse_while(&mut self) -> Result<PsStmt, PowerShellError> {
        self.expect(TokenKind::LeftParen)?;
        let condition = self.parse_expression()?;
        self.expect(TokenKind::RightParen)?;
        let body = self.parse_block()?;
        Ok(PsStmt::While { condition, body })
    }

    fn parse_for(&mut self) -> Result<PsStmt, PowerShellError> {
        self.expect(TokenKind::LeftParen)?;
        let init = if self.check(TokenKind::Semicolon) { None } else { Some(Box::new(self.parse_for_clause_assign()?)) };
        self.expect(TokenKind::Semicolon)?;
        let condition = if self.check(TokenKind::Semicolon) { None } else { Some(self.parse_expression()?) };
        self.expect(TokenKind::Semicolon)?;
        let step = if self.check(TokenKind::RightParen) { None } else { Some(self.parse_assignment_expr()?) };
        self.expect(TokenKind::RightParen)?;
        let body = self.parse_block()?;
        Ok(PsStmt::For { init, condition, step, body })
    }

    fn parse_for_clause_assign(&mut self) -> Result<PsStmt, PowerShellError> {
        let name = self.variable_name()?;
        self.expect(TokenKind::Equal)?;
        let value = self.parse_expression()?;
        Ok(PsStmt::Assign { name, value })
    }

    fn parse_function(&mut self) -> Result<PsStmt, PowerShellError> {
        let name = self.expect_ident()?;
        self.expect(TokenKind::LeftParen)?;
        let mut params = Vec::new();
        if !self.check(TokenKind::RightParen) {
            loop {
                params.push(self.variable_name()?);
                if !self.match_kind(TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(TokenKind::RightParen)?;
        let body = self.parse_block()?;
        Ok(PsStmt::Function { name, params, body })
    }

    fn parse_block(&mut self) -> Result<Vec<PsStmt>, PowerShellError> {
        self.expect(TokenKind::LeftBrace)?;
        let body = self.parse_statement_list(TokenKind::RightBrace)?;
        self.expect(TokenKind::RightBrace)?;
        Ok(body)
    }

    fn parse_expression(&mut self) -> Result<PsExpr, PowerShellError> {
        self.parse_pipeline()
    }

    fn parse_pipeline(&mut self) -> Result<PsExpr, PowerShellError> {
        let mut expr = self.parse_or()?;
        while self.match_kind(TokenKind::Pipe) {
            let right = self.parse_pipeline_rhs()?;
            expr = PsExpr::Pipeline { left: Box::new(expr), right: Box::new(right) };
        }
        Ok(expr)
    }

    fn parse_pipeline_rhs(&mut self) -> Result<PsExpr, PowerShellError> {
        if self.check(TokenKind::Ident) {
            return self.parse_command_or_ident();
        }
        self.parse_or()
    }

    fn parse_assignment_expr(&mut self) -> Result<PsExpr, PowerShellError> {
        if self.check(TokenKind::Variable) {
            let checkpoint = self.index;
            let name = self.variable_name()?;
            if self.match_kind(TokenKind::Equal) {
                let value = self.parse_expression()?;
                return Ok(PsExpr::Binary { op: "=".to_string(), left: Box::new(PsExpr::Var(name)), right: Box::new(value) });
            }
            self.index = checkpoint;
        }
        self.parse_expression()
    }

    fn parse_or(&mut self) -> Result<PsExpr, PowerShellError> {
        let mut left = self.parse_and()?;
        while let Some(op) = self.match_ops(&[TokenKind::Or, TokenKind::Xor]) {
            let right = self.parse_and()?;
            left = PsExpr::Binary { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<PsExpr, PowerShellError> {
        let mut left = self.parse_comparison()?;
        while self.match_kind(TokenKind::And) {
            let right = self.parse_comparison()?;
            left = PsExpr::Binary { op: "-and".to_string(), left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<PsExpr, PowerShellError> {
        let mut left = self.parse_term()?;
        while let Some(op) = self.match_ops(&[
            TokenKind::Eq,
            TokenKind::Ne,
            TokenKind::Lt,
            TokenKind::Le,
            TokenKind::Gt,
            TokenKind::Ge,
            TokenKind::Like,
            TokenKind::NotLike,
        ]) {
            let right = self.parse_term()?;
            left = PsExpr::Binary { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_term(&mut self) -> Result<PsExpr, PowerShellError> {
        let mut left = self.parse_factor()?;
        while let Some(op) = self.match_ops(&[TokenKind::Plus, TokenKind::Minus]) {
            let right = self.parse_factor()?;
            left = PsExpr::Binary { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_factor(&mut self) -> Result<PsExpr, PowerShellError> {
        let mut left = self.parse_unary()?;
        while let Some(op) = self.match_ops(&[TokenKind::Star, TokenKind::Slash, TokenKind::Percent]) {
            let right = self.parse_unary()?;
            left = PsExpr::Binary { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<PsExpr, PowerShellError> {
        if self.match_kind(TokenKind::Not) {
            let operand = self.parse_unary()?;
            return Ok(PsExpr::Unary { op: "-not".to_string(), operand: Box::new(operand) });
        }
        if self.match_kind(TokenKind::Minus) {
            let operand = self.parse_unary()?;
            return Ok(PsExpr::Unary { op: "-".to_string(), operand: Box::new(operand) });
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<PsExpr, PowerShellError> {
        if self.match_kind(TokenKind::True) {
            return Ok(PsExpr::Bool(true));
        }
        if self.match_kind(TokenKind::False) {
            return Ok(PsExpr::Bool(false));
        }
        if self.match_kind(TokenKind::Null) {
            return Ok(PsExpr::Null);
        }
        if self.check(TokenKind::IntLiteral) {
            let text = self.lexeme().to_string();
            self.advance();
            let value = text.parse::<i64>().map_err(|_| PowerShellError::InvalidLiteral(text))?;
            return Ok(PsExpr::Int(value));
        }
        if self.check(TokenKind::FloatLiteral) {
            let text = self.lexeme().to_string();
            self.advance();
            let value = text.parse::<f64>().map_err(|_| PowerShellError::InvalidLiteral(text))?;
            return Ok(PsExpr::Float(value));
        }
        if self.check(TokenKind::StringLiteral) {
            let text = self.string_value()?;
            self.advance();
            return Ok(PsExpr::String(text));
        }
        if self.check(TokenKind::Variable) {
            let name = self.variable_name()?;
            return Ok(PsExpr::Var(name));
        }
        if self.match_kind(TokenKind::LeftParen) {
            let expr = if self.check(TokenKind::Ident) && self.looks_like_command_call() {
                self.parse_command_or_ident()?
            }
            else {
                self.parse_expression()?
            };
            self.expect(TokenKind::RightParen)?;
            return Ok(expr);
        }
        if self.check(TokenKind::Ident) {
            return self.parse_command_or_ident();
        }
        Err(PowerShellError::UnexpectedToken(self.lexeme().to_string()))
    }

    fn parse_command_or_ident(&mut self) -> Result<PsExpr, PowerShellError> {
        let name = self.expect_ident()?;
        if !self.looks_like_command_args() {
            return Ok(PsExpr::Ident(name));
        }
        let mut args = Vec::new();
        while self.looks_like_command_args() {
            args.push(self.parse_command_arg()?);
        }
        Ok(PsExpr::Call { name, args })
    }

    fn parse_command_arg(&mut self) -> Result<PsExpr, PowerShellError> {
        if self.match_kind(TokenKind::True) {
            return Ok(PsExpr::Bool(true));
        }
        if self.match_kind(TokenKind::False) {
            return Ok(PsExpr::Bool(false));
        }
        if self.match_kind(TokenKind::Null) {
            return Ok(PsExpr::Null);
        }
        if self.check(TokenKind::IntLiteral) {
            let text = self.lexeme().to_string();
            self.advance();
            let value = text.parse::<i64>().map_err(|_| PowerShellError::InvalidLiteral(text))?;
            return Ok(PsExpr::Int(value));
        }
        if self.check(TokenKind::FloatLiteral) {
            let text = self.lexeme().to_string();
            self.advance();
            let value = text.parse::<f64>().map_err(|_| PowerShellError::InvalidLiteral(text))?;
            return Ok(PsExpr::Float(value));
        }
        if self.check(TokenKind::StringLiteral) {
            let text = self.string_value()?;
            self.advance();
            return Ok(PsExpr::String(text));
        }
        if self.check(TokenKind::Variable) {
            return Ok(PsExpr::Var(self.variable_name()?));
        }
        if self.match_kind(TokenKind::LeftParen) {
            let expr = self.parse_expression()?;
            self.expect(TokenKind::RightParen)?;
            return Ok(expr);
        }
        if self.check(TokenKind::Ident) {
            // Bare-word command args are string-like for Write-Output etc.
            return Ok(PsExpr::String(self.expect_ident()?));
        }
        Err(PowerShellError::UnexpectedToken(self.lexeme().to_string()))
    }

    fn looks_like_command_call(&self) -> bool {
        let mut i = self.index + 1;
        while matches!(self.tokens.get(i).map(|t| t.kind), Some(TokenKind::Newline)) {
            i += 1;
        }
        matches!(
            self.tokens.get(i).map(|t| t.kind),
            Some(
                TokenKind::Variable
                    | TokenKind::IntLiteral
                    | TokenKind::FloatLiteral
                    | TokenKind::StringLiteral
                    | TokenKind::True
                    | TokenKind::False
                    | TokenKind::Null
                    | TokenKind::LeftParen
                    | TokenKind::Ident
            )
        )
    }

    fn looks_like_command_args(&self) -> bool {
        matches!(
            self.current_kind(),
            TokenKind::Variable
                | TokenKind::IntLiteral
                | TokenKind::FloatLiteral
                | TokenKind::StringLiteral
                | TokenKind::True
                | TokenKind::False
                | TokenKind::Null
                | TokenKind::LeftParen
                | TokenKind::Ident
        )
    }

    fn at_statement_end(&self) -> bool {
        matches!(self.current_kind(), TokenKind::Newline | TokenKind::Semicolon | TokenKind::RightBrace | TokenKind::Eof | TokenKind::Else)
    }

    fn skip_separators(&mut self) {
        while matches!(self.current_kind(), TokenKind::Newline | TokenKind::Semicolon) {
            self.advance();
        }
    }

    fn skip_newlines_only(&mut self) {
        while self.match_kind(TokenKind::Newline) {}
    }

    fn match_ops(&mut self, kinds: &[TokenKind]) -> Option<String> {
        let kind = self.current_kind();
        if kinds.contains(&kind) {
            let op = op_text(kind).to_string();
            self.advance();
            Some(op)
        }
        else {
            None
        }
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

    fn expect(&mut self, kind: TokenKind) -> Result<(), PowerShellError> {
        if self.match_kind(kind) { Ok(()) } else { Err(PowerShellError::UnexpectedToken(self.lexeme().to_string())) }
    }

    fn expect_ident(&mut self) -> Result<String, PowerShellError> {
        if self.check(TokenKind::Ident) {
            let name = self.lexeme().to_string();
            self.advance();
            Ok(name)
        }
        else {
            Err(PowerShellError::UnexpectedToken(self.lexeme().to_string()))
        }
    }

    fn variable_name(&mut self) -> Result<String, PowerShellError> {
        if !self.check(TokenKind::Variable) {
            return Err(PowerShellError::UnexpectedToken(self.lexeme().to_string()));
        }
        let text = self.lexeme();
        let name = text.strip_prefix('$').unwrap_or(text).to_string();
        self.advance();
        Ok(name)
    }

    fn string_value(&self) -> Result<String, PowerShellError> {
        let text = self.lexeme();
        if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
            Ok(unescape_string(&text[1..text.len() - 1]))
        }
        else {
            Err(PowerShellError::InvalidLiteral(text.to_string()))
        }
    }

    fn check(&self, kind: TokenKind) -> bool {
        self.current_kind() == kind
    }

    fn current_kind(&self) -> TokenKind {
        self.tokens.get(self.index).map(|token| token.kind).unwrap_or(TokenKind::Eof)
    }

    fn lexeme(&self) -> &str {
        self.tokens.get(self.index).map(|token| &self.source[token.span.clone()]).unwrap_or("")
    }

    fn advance(&mut self) {
        if !self.check(TokenKind::Eof) {
            self.index += 1;
        }
    }
}

fn op_text(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::Plus => "+",
        TokenKind::Minus => "-",
        TokenKind::Star => "*",
        TokenKind::Slash => "/",
        TokenKind::Percent => "%",
        TokenKind::Equal => "=",
        TokenKind::Eq => "-eq",
        TokenKind::Ne => "-ne",
        TokenKind::Lt => "-lt",
        TokenKind::Le => "-le",
        TokenKind::Gt => "-gt",
        TokenKind::Ge => "-ge",
        TokenKind::And => "-and",
        TokenKind::Or => "-or",
        TokenKind::Xor => "-xor",
        TokenKind::Like => "-like",
        TokenKind::NotLike => "-notlike",
        TokenKind::Not => "-not",
        _ => "",
    }
}

fn unescape_string(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        if ch == '`' || ch == '\\' {
            if let Some(escaped) = chars.next() {
                out.push(escaped);
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
    use crate::transitional::guest_scripts::powershell::PowerShellScript;

    #[test]
    fn parse_write_output() {
        let script = PowerShellScript::parse(r#"Write-Output "hello""#).expect("parse");
        assert!(matches!(
            &script.statements[0],
            PsStmt::Expr(PsExpr::Call { name, .. }) if name.eq_ignore_ascii_case("Write-Output")
        ));
    }

    #[test]
    fn parse_assign_if_while_function() {
        let source = r#"
$x = 1
if ($x -eq 1) { $x = $x + 1 } else { $x = 0 }
while ($x -lt 5) { $x = $x + 1 }
function Add($a, $b) { return $a + $b }
"#;
        let script = PowerShellScript::parse(source).expect("parse");
        assert!(matches!(script.statements[0], PsStmt::Assign { .. }));
        assert!(matches!(script.statements[1], PsStmt::If { .. }));
        assert!(matches!(script.statements[2], PsStmt::While { .. }));
        assert!(matches!(script.statements[3], PsStmt::Function { .. }));
    }

    #[test]
    fn parse_for_and_pipeline() {
        let source = r#"
$sum = 0
for ($i = 1; $i -le 3; $i = $i + 1) { $sum = $sum + $i }
$sum | Write-Output
"#;
        let script = PowerShellScript::parse(source).expect("parse");
        assert!(matches!(script.statements[1], PsStmt::For { .. }));
        assert!(matches!(&script.statements[2], PsStmt::Expr(PsExpr::Pipeline { .. })));
    }
}
