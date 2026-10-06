//! VON 词法记号定义。

use std::ops::Range;

/// 词法记号类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `:`
    Colon,
    /// `,`
    Comma,
    /// 字符串字面量（含引号）。
    StringLiteral,
    /// 整数字面量。
    IntegerLiteral,
    /// 普通标识符。
    Identifier,
    /// `null`
    Null,
    /// `true`
    True,
    /// `false`
    False,
    /// 行注释（`#`）。
    LineComment,
    /// 空白。
    Whitespace,
    /// 输入结束。
    Eof,
}

impl TokenKind {
    /// 是否为 trivia。
    pub fn is_trivia(self) -> bool {
        matches!(self, Self::LineComment | Self::Whitespace)
    }
}

/// 词法记号。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// 记号类型。
    pub kind: TokenKind,
    /// 在源文本中的字节范围。
    pub span: Range<usize>,
}

impl Token {
    pub(crate) fn eof(span: usize) -> Self {
        Self { kind: TokenKind::Eof, span: span..span }
    }
}
