#![doc = include_str!("readme.md")]

use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

pub mod ast;
mod lexer;
mod parser;

pub use ast::{PsExpr, PsStmt};
pub use lexer::{Lexer, Token, TokenKind};
pub use parser::Parser;

/// PowerShell parse error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PowerShellError {
    /// Empty script.
    EmptyScript,
    /// Unexpected token.
    UnexpectedToken(String),
    /// Invalid literal.
    InvalidLiteral(String),
}

impl Display for PowerShellError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyScript => write!(f, "PowerShell script cannot be empty"),
            Self::UnexpectedToken(token) => write!(f, "unexpected PowerShell token: {token}"),
            Self::InvalidLiteral(text) => write!(f, "invalid PowerShell literal: {text}"),
        }
    }
}

impl std::error::Error for PowerShellError {}

/// Parsed PowerShell script.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PowerShellScript {
    /// Top-level statements.
    pub statements: Vec<PsStmt>,
}

impl PowerShellScript {
    /// Create an empty script.
    pub fn new() -> Self {
        Self { statements: Vec::new() }
    }

    /// Parse PowerShell source into the demo AST.
    pub fn parse(source: &str) -> Result<Self, PowerShellError> {
        Ok(Self { statements: Parser::parse(source)? })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fixture_line() {
        let script = PowerShellScript::parse(r#"Write-Output "legend powershell fixture""#).expect("parse");
        assert_eq!(script.statements.len(), 1);
    }
}
