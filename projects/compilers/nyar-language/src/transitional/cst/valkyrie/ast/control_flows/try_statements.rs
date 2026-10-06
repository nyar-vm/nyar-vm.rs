use super::*;

/// `try { ... }` 或 `try? Type { ... }` 语句。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TryStatement {
    /// 是否为 `try?` 形式。
    pub is_optional: bool,
    /// 是否为 `try!` 形式。
    pub is_forced: bool,
    /// 可选显式结果类型。
    pub result_type: Option<crate::transitional::cst::valkyrie::ast::TypeExpression>,
    /// try 体。
    pub body: DeclarationBody,
    /// 源码跨度。
    pub span: Range<usize>,
}
