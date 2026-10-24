//! Bash token definitions.

use std::ops::Range;

/// Bash token kind for the legend subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// End of file.
    Eof,
    /// Lexer error.
    Error,
    /// Bare word / number / `$var` text.
    Word,
    /// Quoted string content (quotes stripped).
    String,
    /// Newline.
    Newline,
    /// `;`
    Semicolon,
    /// `|`
    Pipe,
    /// `&&`
    AndAnd,
    /// `||`
    OrOr,
    /// `>`
    Greater,
    /// `>>`
    GreaterGreater,
    /// `<`
    Less,
    /// `=`
    Equal,
    /// `(`
    LeftParen,
    /// `)`
    RightParen,
    /// `{`
    LeftBrace,
    /// `}`
    RightBrace,
    /// `if`
    If,
    /// `then`
    Then,
    /// `else`
    Else,
    /// `elif`
    Elif,
    /// `fi`
    Fi,
    /// `while`
    While,
    /// `do`
    Do,
    /// `done`
    Done,
    /// `for`
    For,
    /// `in`
    In,
    /// `function`
    Function,
    /// `return`
    Return,
    /// `break`
    Break,
    /// `continue`
    Continue,
}

/// Bash token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// Token kind.
    pub kind: TokenKind,
    /// Source span.
    pub span: Range<usize>,
}

impl Token {
    /// EOF token.
    pub fn eof(offset: usize) -> Self {
        Self { kind: TokenKind::Eof, span: offset..offset }
    }
}
