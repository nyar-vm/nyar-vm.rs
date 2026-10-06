use crate::transitional::cst::valkyrie::{TypePath, ast::IdentifierNode};
use std::ops::Range;

/// 原始指针权限。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PointerKind {
    /// `◇T`
    ReadOnly,
    /// `◆T`
    Mutable,
}

/// 匿名 row 中的方法 requirement。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RowMethodTypeExpression {
    /// 方法名。
    pub name: IdentifierNode,
    /// 参数类型列表。
    pub params: Vec<TypeExpression>,
    /// 返回类型。
    pub return_type: Box<TypeExpression>,
    /// 源码跨度。
    pub span: Range<usize>,
}

/// Parsed type expression node.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TypeExpression {
    /// Namepath reference such as `package::module::TypeName`.
    Path(TypePath),
    /// 堆数组类型 `[T]`。
    Array {
        /// Element type.
        item: Box<TypeExpression>,
        /// Source span of the expression.
        span: Range<usize>,
    },
    /// 栈数组类型 `[T; N]`（值语义）。
    FixedArray {
        /// Element type.
        item: Box<TypeExpression>,
        /// Fixed length.
        length: u64,
        /// Source span of the expression.
        span: Range<usize>,
    },
    /// Tuple type such as `(i64, utf8)`.
    Tuple {
        /// Tuple item types.
        items: Vec<TypeExpression>,
        /// Source span of the expression.
        span: Range<usize>,
    },
    /// 匿名 row 类型，如 `{ fmt() -> utf8, clone() -> Self }`。
    Row {
        /// 匿名 row 中的方法 requirement。
        methods: Vec<RowMethodTypeExpression>,
        /// 源码跨度。
        span: Range<usize>,
    },
    /// 原始指针类型，如 `◇T` 或 `◆T`。
    Pointer {
        /// 指针权限。
        kind: PointerKind,
        /// 指向的内部类型。
        item: Box<TypeExpression>,
        /// 源码跨度。
        span: Range<usize>,
    },
    /// 关联类型绑定参数，如 `Iterator<Item = T>` 中的 `Item = T`。
    Associated {
        /// 关联类型名称。
        name: IdentifierNode,
        /// 绑定的类型表达式。
        ty: Box<TypeExpression>,
        /// 源码跨度。
        span: Range<usize>,
    },
    /// 可空类型 `T?`，等价于 `T | null`，表示 `T` 或 `null`。
    Nullable {
        /// 内部类型。
        item: Box<TypeExpression>,
        /// 源码跨度。
        span: Range<usize>,
    },
    /// 联合类型 `A | B | C`。
    Union {
        /// 联合成员类型（左结合扁平化）。
        items: Vec<TypeExpression>,
        /// 源码跨度。
        span: Range<usize>,
    },
    /// 交集类型 `A & B & C`。
    Intersection {
        /// 交集成员类型（左结合扁平化）。
        items: Vec<TypeExpression>,
        /// 源码跨度。
        span: Range<usize>,
    },
    /// 函数类型 `micro(P1, P2) -> R`。
    Function {
        /// 参数类型列表。
        params: Vec<TypeExpression>,
        /// 返回类型。
        return_type: Box<TypeExpression>,
        /// 源码跨度。
        span: Range<usize>,
    },
}

impl TypeExpression {
    /// Returns the source span of the expression.
    pub fn span(&self) -> &Range<usize> {
        match self {
            Self::Path(path) => &path.span,
            Self::Array { span, .. }
            | Self::FixedArray { span, .. }
            | Self::Tuple { span, .. }
            | Self::Row { span, .. }
            | Self::Pointer { span, .. }
            | Self::Associated { span, .. }
            | Self::Nullable { span, .. }
            | Self::Union { span, .. }
            | Self::Intersection { span, .. }
            | Self::Function { span, .. } => span,
        }
    }
}
