//! T-Grammar 词法记号。

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// 静态文本（可含 `{expr}`，由 parser 二次切分）。
    Text,
    /// `<% ... %>` 指令块。
    Directive,
    /// `<# ... #>` 注释。
    Comment,
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
