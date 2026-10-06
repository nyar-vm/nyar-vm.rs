//! Lua token definitions.

/// Lua token kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// End of file.
    Eof,
    /// Lexer error.
    Error,
    /// Identifier.
    Name,
    /// Numeric literal.
    Number,
    /// String literal.
    String,
    /// `local`
    Local,
    /// `if`
    If,
    /// `then`
    Then,
    /// `else`
    Else,
    /// `elseif`
    ElseIf,
    /// `end`
    End,
    /// `while`
    While,
    /// `do`
    Do,
    /// `repeat`
    Repeat,
    /// `until`
    Until,
    /// `for`
    For,
    /// `function`
    Function,
    /// `return`
    Return,
    /// `break`
    Break,
    /// `and`
    And,
    /// `or`
    Or,
    /// `not`
    Not,
    /// `true`
    True,
    /// `false`
    False,
    /// `nil`
    Nil,
    /// `print`
    Print,
    /// `(`
    LeftParen,
    /// `)`
    RightParen,
    /// `{`
    LeftBrace,
    /// `}`
    RightBrace,
    /// `[`
    LeftBracket,
    /// `]`
    RightBracket,
    /// `,`
    Comma,
    /// `;`
    Semicolon,
    /// `.`
    Dot,
    /// `..`
    DotDot,
    /// `=`
    Equal,
    /// `==`
    EqualEqual,
    /// `~=`
    TildeEqual,
    /// `<`
    Less,
    /// `<=`
    LessEqual,
    /// `>`
    Greater,
    /// `>=`
    GreaterEqual,
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
    /// `^`
    Caret,
    /// `#`
    Hash,
}

/// Lua token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// Token kind.
    pub kind: TokenKind,
    /// Source span.
    pub span: std::ops::Range<usize>,
}

impl Token {
    /// EOF token.
    pub fn eof(offset: usize) -> Self {
        Self { kind: TokenKind::Eof, span: offset..offset }
    }
}
