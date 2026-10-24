#![doc = include_str!("readme.md")]

use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

pub mod ast;
mod lexer;
mod parser;

pub use ast::{BashRedirect, BashStmt};
pub use lexer::{Token, TokenKind, tokenize};
pub use parser::Parser;

/// Bash parse error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BashError {
    /// Empty script.
    EmptyScript,
    /// Unexpected token.
    UnexpectedToken,
    /// Expected identifier.
    ExpectedName,
    /// Expected specific token.
    ExpectedToken(TokenKind),
}

impl Display for BashError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyScript => write!(f, "Bash script cannot be empty"),
            Self::UnexpectedToken => write!(f, "unexpected token"),
            Self::ExpectedName => write!(f, "expected identifier"),
            Self::ExpectedToken(kind) => write!(f, "expected token {kind:?}"),
        }
    }
}

impl std::error::Error for BashError {}

/// Parsed Bash script.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BashScript {
    /// Top-level statements.
    pub statements: Vec<BashStmt>,
}

impl BashScript {
    /// Parse Bash source.
    pub fn parse(source: &str) -> Result<Self, BashError> {
        let trimmed = source.trim();
        if trimmed.is_empty() {
            return Err(BashError::EmptyScript);
        }
        let tokens = tokenize(source);
        let statements = Parser::parse(source, tokens)?;
        if statements.is_empty() {
            return Err(BashError::EmptyScript);
        }
        Ok(Self { statements })
    }
}
