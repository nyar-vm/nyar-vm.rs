//! X-Grammar 内联 XML AST（Valkyrie 语言扩展）。

use std::ops::Range;

use crate::transitional::cst::valkyrie::tgrammar::TgRoot;

/// 标记根：有序子节点。
pub type XgRoot = Vec<XgNode>;

/// 标记节点。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum XgNode {
    /// 元素节点。
    Element(XgElement),
    /// 文本 / 插值片段。
    Text { parts: Vec<XgTextPart>, span: Range<usize> },
    /// T-Grammar meta 块（`<% %>` 混写）。
    Meta { nodes: TgRoot, span: Range<usize> },
}

/// 元素节点。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct XgElement {
    pub tag: String,
    pub attrs: Vec<(String, XgAttrValue)>,
    pub children: XgRoot,
    pub self_closing: bool,
    pub span: Range<usize>,
}

/// 属性值。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum XgAttrValue {
    /// 静态字符串。
    Literal(String),
    /// 动态表达式（`:name="expr"` 引号内原文）。
    Expression(String),
}

/// 文本片段。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum XgTextPart {
    Static(String),
    Expression(String),
}
