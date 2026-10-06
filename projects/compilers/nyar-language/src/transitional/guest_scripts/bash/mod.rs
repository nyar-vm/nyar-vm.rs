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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_echo() {
        let script = BashScript::parse("echo hello").expect("parse");
        assert_eq!(script.statements.len(), 1);
    }

    #[test]
    fn parse_if_assign() {
        let source = r#"
x=1
if [ $x -eq 1 ]; then
  echo ok
else
  echo no
fi
"#;
        let script = BashScript::parse(source).expect("parse");
        assert!(script.statements.len() >= 2);
        assert!(matches!(script.statements[1], BashStmt::If { .. }));
    }

    #[test]
    fn parse_while_for_function() {
        let source = r#"
greet() {
  printf "%s" "$1"
}
while [ $x -lt 1 ]; do
  x=1
done
for item in a b; do
  greet $item
done
"#;
        let script = BashScript::parse(source).expect("parse");
        assert!(script.statements.iter().any(|stmt| matches!(stmt, BashStmt::FunctionDef { .. })));
        assert!(script.statements.iter().any(|stmt| matches!(stmt, BashStmt::While { .. })));
        assert!(script.statements.iter().any(|stmt| matches!(stmt, BashStmt::For { .. })));
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(BashScript::parse("   ").unwrap_err(), BashError::EmptyScript);
    }

    #[test]
    fn parse_elif_and_and_or() {
        let source = r#"
if false; then
  echo a
elif true; then
  echo b || echo c
fi
"#;
        let script = BashScript::parse(source).expect("parse");
        assert!(matches!(script.statements[0], BashStmt::If { .. }));
    }
}
