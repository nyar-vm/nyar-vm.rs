use super::{DeclarationBody, LiteralExpression, NamePath, TermExpression};
use crate::transitional::cst::valkyrie::TypeExpression;
use nyar_types::Identifier;
use std::ops::Range;

/// `match`/`catch` 的单个 `case` 分支。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ArmStatement {
    Case(CaseArm),
    Type(TypeArm),
    Else(ElseArm),
}

/// ```v
/// case pattern if guard: ...
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CaseArm {
    /// 分支模式。
    pub pattern: Option<PatternExpression>,
    /// 分支守卫。
    pub guard: Option<TermExpression>,
    /// 分支体。
    pub body: DeclarationBody,
    /// 源码范围。
    pub span: Range<usize>,
}

/// `match`/`catch` 的单个 `type` 分支。
///
/// ```v
/// type Point if input: ...
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TypeArm {
    pub typing: TypeExpression,
    /// 分支守卫。
    pub guard: Option<TermExpression>,
    /// 分支体。
    pub body: DeclarationBody,
    /// 源码范围。
    pub span: Range<usize>,
}

/// `match`/`catch` 的单个 `else` 分支。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ElseArm {
    /// 源码范围。
    pub span: Range<usize>,
}

/// `match` 分支模式。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PatternExpression {
    /// `value` — 变量绑定模式。
    Variable {
        /// 绑定变量名。
        name: String,
        /// 源码范围。
        span: Range<usize>,
    },
    /// `Foo` / `Foo::Bar` — 裸名字模式。
    Name {
        /// 名字路径。
        path: NamePath,
        /// 源码范围。
        span: Range<usize>,
    },
    /// `_` — 通配模式。
    Wildcard {
        /// 源码范围。
        span: Range<usize>,
    },
    /// `1` / `true` — 字面量模式。
    Literal {
        /// 字面量值。
        literal: LiteralExpression,
        /// 源码范围。
        span: Range<usize>,
    },
    /// `(a, b)` — 元组模式。
    Tuple(Box<TuplePattern>),
    /// `TypeName(binding1, binding2)` — extractor 模式。
    Extract(Box<ExtractPattern>),
    /// `TypeName { field1, field2 }` 或 `{ field1, field2 }` — 原始对象模式。
    Object(Box<ObjectPattern>),
    /// `[head, ..tail, last]` — 数组 extractor 模式。
    Array(Box<ArrayPattern>),
    /// `1..=10` / `..<100` — 范围模式。
    Range {
        /// 起始字面量；缺失时表示无下界。
        start: Option<LiteralExpression>,
        /// 结束字面量；缺失时表示无上界。
        end: Option<LiteralExpression>,
        /// 上界是否包含在内。
        inclusive_end: bool,
        /// 源码范围。
        span: Range<usize>,
    },
    /// `value as TypeName` — 带类型约束的绑定模式。
    TypedBind {
        /// 绑定变量名。
        name: String,
        /// 约束类型路径。
        ty: NamePath,
        /// 源码范围。
        span: Range<usize>,
    },
    /// `A | B | C` — 或模式。
    Or(Box<PatternOrExpression>),
    /// `whole <- subpat` — 同时绑定整体与子模式。
    Bind {
        /// 整体绑定名。
        name: String,
        /// 子模式。
        pattern: Box<PatternExpression>,
        /// 源码范围。
        span: Range<usize>,
    },
    /// `mut pat` — 可变借用绑定（等价 Rust `ref mut`）。
    Mut {
        /// 内部模式。
        pattern: Box<PatternExpression>,
        /// 源码范围。
        span: Range<usize>,
    },
    /// `pin [mut] pat` — pinned 可变借用。
    Pin {
        /// 是否带 `mut`。
        mutable: bool,
        /// 内部模式。
        pattern: Box<PatternExpression>,
        /// 源码范围。
        span: Range<usize>,
    },
}

/// `[x, y, <Pattern>]`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ArrayPattern {
    /// `..` 之前的前缀子模式。
    pub prefix: Vec<PatternExpression>,
    /// `..rest` 的可选绑定名；裸 `..` 为 `None`。
    pub rest: Option<Identifier>,
    /// `..` 之后的后缀子模式。
    pub suffix: Vec<PatternExpression>,
    /// 源码范围。
    pub span: Range<usize>,
}

/// `(x, y, <Pattern>)`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TuplePattern {
    /// 元组子模式列表。
    pub items: Vec<PatternExpression>,
    /// 源码范围。
    pub span: Range<usize>,
}

/// `Extractable(x, y, <Pattern>)`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ExtractPattern {
    /// 构造器名称路径。
    pub name: NamePath,
    /// 构造器子模式列表。
    pub fields: Vec<PatternExpression>,
    /// 源码范围。
    pub span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ObjectPattern {
    /// 可选的具名类型路径；缺失时表示匿名结构模式。
    pub name: Option<NamePath>,
    /// 需要从对象中匹配的字段模式列表。
    pub fields: Vec<MatchObjectField>,
    /// `...rest` 的可选绑定名。
    pub rest: Option<Identifier>,
    /// 源码范围。
    pub span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PatternOrExpression {
    /// 各个候选子模式。
    pub patterns: Vec<PatternExpression>,
    /// 源码范围。
    pub span: Range<usize>,
}

/// `match` / `case` object pattern 的字段子模式。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MatchObjectField {
    /// 字段名称。
    pub name: String,
    /// 字段对应的子模式；`case { foo }` 会被补成同名变量模式。
    pub pattern: PatternExpression,
    /// 源码范围。
    pub span: Range<usize>,
}

impl PatternExpression {
    /// 返回模式的源码范围。
    pub fn span(&self) -> &Range<usize> {
        match self {
            Self::Variable { span, .. }
            | Self::Wildcard { span }
            | Self::Literal { span, .. }
            | Self::Range { span, .. }
            | Self::Name { span, .. }
            | Self::TypedBind { span, .. }
            | Self::Bind { span, .. }
            | Self::Mut { span, .. }
            | Self::Pin { span, .. } => span,
            Self::Or(pattern) => &pattern.span,
            Self::Tuple(pattern) => &pattern.span,
            Self::Extract(pattern) => &pattern.span,
            Self::Object(pattern) => &pattern.span,
            Self::Array(pattern) => &pattern.span,
        }
    }
}
