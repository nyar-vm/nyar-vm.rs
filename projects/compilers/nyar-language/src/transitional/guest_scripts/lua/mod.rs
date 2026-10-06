#![doc = include_str!("readme.md")]

use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

pub mod ast;
mod lexer;
mod parser;

pub use ast::{LuaExpr, LuaLValue, LuaNode, LuaStmt, LuaTableField};
pub use lexer::{Token, TokenKind, tokenize};
pub use parser::Parser;

/// Lua parse error.
#[derive(Debug, Clone, PartialEq)]
pub enum LuaError {
    /// Empty script.
    EmptyScript,
    /// Unexpected token.
    UnexpectedToken,
    /// Expected identifier.
    ExpectedName,
    /// Expected specific token.
    ExpectedToken(TokenKind),
    /// Invalid number literal.
    InvalidNumber,
    /// Invalid assignment target.
    InvalidAssignTarget,
    /// Invalid call target.
    InvalidCallTarget(LuaExpr),
}

impl Display for LuaError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyScript => write!(f, "Lua script cannot be empty"),
            Self::UnexpectedToken => write!(f, "unexpected token"),
            Self::ExpectedName => write!(f, "expected identifier"),
            Self::ExpectedToken(kind) => write!(f, "expected token {kind:?}"),
            Self::InvalidNumber => write!(f, "invalid number literal"),
            Self::InvalidAssignTarget => write!(f, "invalid assignment target"),
            Self::InvalidCallTarget(_) => write!(f, "invalid call target"),
        }
    }
}

impl std::error::Error for LuaError {}

/// Parsed Lua script.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LuaScript {
    /// Top-level statements.
    pub statements: Vec<LuaStmt>,
}

impl LuaScript {
    /// Parse Lua source.
    pub fn parse(source: &str) -> Result<Self, LuaError> {
        let trimmed = source.trim();
        if trimmed.is_empty() {
            return Err(LuaError::EmptyScript);
        }
        let tokens = tokenize(source);
        let statements = Parser::parse(source, tokens)?;
        if statements.is_empty() {
            return Err(LuaError::EmptyScript);
        }
        Ok(Self { statements })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_script() {
        let script = LuaScript::parse("print('hello')").expect("parse");
        assert_eq!(script.statements.len(), 1);
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(LuaScript::parse("   ").unwrap_err(), LuaError::EmptyScript);
    }
}
