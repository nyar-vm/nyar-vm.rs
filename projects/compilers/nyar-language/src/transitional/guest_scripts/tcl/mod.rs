#![doc = include_str!("readme.md")]

use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

pub mod ast;
mod lexer;
mod parser;

pub use ast::{TclCommand, TclWord};
pub use lexer::{TclToken, split_tcl_list, tokenize_words};
pub use parser::Parser;

/// Tcl parse error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TclError {
    /// Empty script.
    EmptyScript,
    /// Empty command.
    EmptyCommand,
    /// Missing argument.
    MissingArgument,
    /// Unknown command name.
    UnknownCommand(String),
}

impl Display for TclError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyScript => write!(f, "Tcl script cannot be empty"),
            Self::EmptyCommand => write!(f, "empty Tcl command"),
            Self::MissingArgument => write!(f, "missing Tcl command argument"),
            Self::UnknownCommand(name) => write!(f, "unknown Tcl command: {name}"),
        }
    }
}

impl std::error::Error for TclError {}

/// Parsed Tcl script.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TclScript {
    /// Top-level commands.
    pub commands: Vec<TclCommand>,
}

impl TclScript {
    /// Parse Tcl source.
    pub fn parse(source: &str) -> Result<Self, TclError> {
        Ok(Self { commands: Parser::parse(source)? })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_puts() {
        let script = TclScript::parse("puts hello").expect("parse");
        assert!(matches!(script.commands[0], TclCommand::Puts { .. }));
    }

    #[test]
    fn parse_set_and_incr() {
        let script = TclScript::parse("set x 1\nincr x").expect("parse");
        assert_eq!(script.commands.len(), 2);
    }

    #[test]
    fn parse_proc_foreach_and_list() {
        let script = TclScript::parse("proc add {a b} { expr {$a + $b} }\nforeach x {a b} { puts $x }\nset xs [list a b]\n").expect("parse");
        assert!(matches!(script.commands[0], TclCommand::Proc { .. }));
        assert!(matches!(script.commands[1], TclCommand::Foreach { .. }));
        assert!(matches!(script.commands[2], TclCommand::Set { .. }));
    }
}
