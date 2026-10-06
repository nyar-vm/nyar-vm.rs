//! T-Grammar 内联 meta 标记 AST（`<% %>` 元计算）。

use std::ops::Range;

/// 模板根：有序子节点。
pub type TgRoot = Vec<TgNode>;

/// 模板节点。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TgNode {
    /// 静态文本与 `{expr}` 插值。
    Text { parts: Vec<TgTextPart>, span: Range<usize> },
    /// 单行 meta 语句 `<% stmt %>`（内部原文供 Valkyrie 解析）。
    Stmt { body: String, span: Range<usize> },
    /// 条件块 `<% if %>` … `<% end %>`。
    If(TgIf),
    /// 循环块 `<% loop %>` … `<% end %>`。
    Loop(TgLoop),
    /// 匹配块 `<% match %>` … `<% end %>`。
    Match(TgMatch),
    /// 模板注释 `<# ... #>`（不输出）。
    Comment { span: Range<usize> },
}

/// `if` 分支。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TgIfArm {
    /// 条件表达式原文；`None` 表示 `else` 分支。
    pub condition: Option<String>,
    pub body: TgRoot,
    pub span: Range<usize>,
}

/// `if` 块。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TgIf {
    pub arms: Vec<TgIfArm>,
    pub span: Range<usize>,
}

/// `loop` 块。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TgLoop {
    /// `loop` 头原文（如 `i in items`）。
    pub header: String,
    pub body: TgRoot,
    pub span: Range<usize>,
}

/// `match` 分支。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TgMatchArm {
    /// 模式原文；`None` 表示 `else` 分支。
    pub pattern: Option<String>,
    pub body: TgRoot,
    pub span: Range<usize>,
}

/// `match` 块。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TgMatch {
    pub scrutinee: String,
    pub arms: Vec<TgMatchArm>,
    pub span: Range<usize>,
}

/// 文本片段。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TgTextPart {
    Static(String),
    Expression(String),
}

/// T-Grammar 块级关键词（`<% kw %>` 指令头）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TgKeyword {
    If,
    ElseIf,
    Else,
    End,
    Loop,
    Match,
    Case,
}
