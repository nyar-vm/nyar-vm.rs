#![doc = include_str!("readme.md")]

use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

pub mod ast;
mod lexer;
mod parser;

pub use ast::{CExpr, CFunction, CItem, CStmt, CVarDecl};
pub use lexer::{Token, TokenKind, tokenize};
pub use parser::Parser;

/// C parse error.
#[derive(Debug, Clone, PartialEq)]
pub enum CError {
    /// Empty translation unit.
    EmptyScript,
    /// Unexpected token.
    UnexpectedToken,
    /// Expected identifier.
    ExpectedName,
    /// Expected a type name.
    ExpectedType,
    /// Expected specific token.
    ExpectedToken(TokenKind),
    /// Invalid numeric literal.
    InvalidNumber,
    /// Invalid assignment target.
    InvalidAssignTarget,
    /// Invalid call target.
    InvalidCallTarget(CExpr),
}

impl Display for CError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyScript => write!(f, "C translation unit cannot be empty"),
            Self::UnexpectedToken => write!(f, "unexpected token"),
            Self::ExpectedName => write!(f, "expected identifier"),
            Self::ExpectedType => write!(f, "expected type name"),
            Self::ExpectedToken(kind) => write!(f, "expected token {kind:?}"),
            Self::InvalidNumber => write!(f, "invalid number literal"),
            Self::InvalidAssignTarget => write!(f, "invalid assignment target"),
            Self::InvalidCallTarget(_) => write!(f, "invalid call target"),
        }
    }
}

impl std::error::Error for CError {}

/// Parsed C translation unit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CScript {
    /// Top-level items.
    pub items: Vec<CItem>,
}

impl CScript {
    /// Parse C source.
    pub fn parse(source: &str) -> Result<Self, CError> {
        let trimmed = source.trim();
        if trimmed.is_empty() {
            return Err(CError::EmptyScript);
        }
        let tokens = tokenize(source);
        let items = Parser::parse(source, tokens)?;
        if items.is_empty() {
            return Err(CError::EmptyScript);
        }
        Ok(Self { items })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_main() {
        let script = CScript::parse("int main(void) { return 0; }").expect("parse");
        assert_eq!(script.items.len(), 1);
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(CScript::parse("   ").unwrap_err(), CError::EmptyScript);
    }
}
