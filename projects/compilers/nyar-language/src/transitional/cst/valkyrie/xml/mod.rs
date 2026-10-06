#![doc = include_str!("readme.md")]

mod ast;
mod lexer;
mod parser;

pub use ast::{XgAttrValue, XgElement, XgNode, XgRoot, XgTextPart};
pub use lexer::{Lexer, Token, TokenKind};
pub use parser::{XgParseError, parse_xgrammar_markup, parse_xgrammar_template, parse_xgrammar_with_meta};
