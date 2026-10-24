//! PowerShell token definitions (legend demo subset).

use std::ops::Range;

/// PowerShell token kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// End of file.
    Eof,
    /// Identifier / command name.
    Ident,
    /// `$name` variable (span includes `$`).
    Variable,
    /// Integer literal.
    IntLiteral,
    /// Floating literal.
    FloatLiteral,
    /// Double-quoted string.
    StringLiteral,
    /// `if`
    If,
    /// `else`
    Else,
    /// `while`
    While,
    /// `for`
    For,
    /// `function`
    Function,
    /// `return`
    Return,
    /// `$true`
    True,
    /// `$false`
    False,
    /// `$null`
    Null,
    /// `(`
    LeftParen,
    /// `)`
    RightParen,
    /// `{`
    LeftBrace,
    /// `}`
    RightBrace,
    /// `,`
    Comma,
    /// `;`
    Semicolon,
    /// Newline (statement separator).
    Newline,
    /// `=`
    Equal,
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `%`
    Percent,
    /// `|`
    Pipe,
    /// `-eq`
    Eq,
    /// `-ne`
    Ne,
    /// `-lt`
    Lt,
    /// `-le`
    Le,
    /// `-gt`
    Gt,
    /// `-ge`
    Ge,
    /// `-and`
    And,
    /// `-or`
    Or,
    /// `-xor`
    Xor,
    /// `-like`
    Like,
    /// `-notlike`
    NotLike,
    /// `-not`
    Not,
}

/// PowerShell token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// Token kind.
    pub kind: TokenKind,
    /// Source span.
    pub span: Range<usize>,
}

impl Token {
    /// EOF token.
    pub(crate) fn eof(offset: usize) -> Self {
        Self { kind: TokenKind::Eof, span: offset..offset }
    }
}
