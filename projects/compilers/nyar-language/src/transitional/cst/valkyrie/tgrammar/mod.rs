#![doc = include_str!("readme.md")]

mod ast;
mod lexer;
mod parser;

pub use ast::{TgIf, TgIfArm, TgKeyword, TgLoop, TgMatch, TgMatchArm, TgNode, TgRoot, TgTextPart};
pub use lexer::{Lexer, Token, TokenKind};
pub use parser::{TgParseError, parse_tgrammar_fragment, parse_tgrammar_template};
