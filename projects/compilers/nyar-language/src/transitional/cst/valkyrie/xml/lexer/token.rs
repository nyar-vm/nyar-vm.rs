//! X-Grammar 词法记号。

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// 元素间文本（可含 `{expr}`，由 parser 二次切分）。
    Text,
    /// `<!-- ... -->`。
    Comment,
    /// `<% ... %>` meta 指令。
    Directive,
    /// `<`。
    Lt,
    /// `/`。
    Slash,
    /// `>`。
    Gt,
    /// `=`。
    Eq,
    /// 引号字符串属性值。
    StringLiteral,
    /// 标识符（标签名 / 属性名 / 无引号属性值）。
    Identifier,
    /// `{...}` 属性表达式。
    BracedExpr,
    /// 输入结束。
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Range<usize>,
}

impl Token {
    pub(crate) fn eof(offset: usize) -> Self {
        Self { kind: TokenKind::Eof, span: offset..offset }
    }
}
