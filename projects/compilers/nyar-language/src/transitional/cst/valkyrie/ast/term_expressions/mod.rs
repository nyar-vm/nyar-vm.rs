use super::*;
use crate::transitional::cst::valkyrie::{tgrammar::TgRoot, xml::XgRoot};
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DereferenceKind {
    /// `expr.◇`
    ReadOnly,
    /// `expr.◆`
    Mutable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SubscriptKind {
    /// `a[1]`/ `a[-1]` 这类序数访问。
    Ordinal,
    /// `a⁅0⁆` / `a::[0]` 这类基数访问。
    Cardinal,
}

///
///
/// ```v
/// tensor[]
/// tensor[:,:]
/// tensor[1]
/// tensor[1:-1:1]
/// tensor[1:-1:1,-1:1:-1]
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TermSubscriptExpression {
    /// Whether the subscript uses ordinal or offset semantics.
    pub kind: SubscriptKind,
    /// Basement expression.
    pub base: TermExpression,
    /// Index expression.
    pub subscripts: Vec<SubscriptItem>,
    /// Source span of the expression.
    pub span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TermDereferenceExpression {
    /// 被解引用的基础表达式。
    pub base: TermExpression,
    /// 解引用权限。
    pub kind: DereferenceKind,
    /// Source span of the expression.
    pub span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SubscriptItem {
    /// ```v
    /// tensor[1]
    /// ```
    Index {
        term: TermExpression,
        /// Source span of the expression.
        span: Range<usize>,
    },
    /// ```v
    /// tensor[:]
    /// tensor[::]
    /// tensor[1:-1:1]
    /// ```
    Slice {
        start: Option<TermExpression>,
        end: Option<TermExpression>,
        step: Option<TermExpression>,
        /// Source span of the expression.
        span: Range<usize>,
    },
}

/// Parsed term expression node.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TermExpression {
    /// Name/path reference such as `std::console::write_line`.
    Name {
        /// Referenced path.
        path: NamePath,
        /// Source span of the expression.
        span: Range<usize>,
    },
    /// Literal value.
    Literal {
        /// Literal payload.
        literal: LiteralExpression,
        /// Source span of the expression.
        span: Range<usize>,
    },
    /// Prefix unary operator.
    Unary(Box<TermUnaryExpression>),
    /// Binary operator parsed by Pratt.
    Binary(Box<TermBinaryExpression>),
    /// Function or constructor call.
    Call(Box<TermCallExpression>),
    /// Member access such as `obj.field`.
    DotCall(Box<TermDotExpression>),
    /// Dereference access such as `ptr.◇` or `ptr.◆`.
    Dereference(Box<TermDereferenceExpression>),
    /// Subscript expression such as `items[i,j,k]`.
    Subscript(Box<TermSubscriptExpression>),
    /// Tuple literal or grouped multi-expression sequence.
    Tuple {
        /// Tuple items.
        items: Vec<TermExpression>,
        /// Source span of the expression.
        span: Range<usize>,
    },
    /// Array literal.
    Array {
        /// Array items.
        items: Vec<TermExpression>,
        /// Source span of the expression.
        span: Range<usize>,
    },
    /// Explicit cast expression such as `value as i32`.
    As(Box<TermAsExpression>),
    /// Shape assert expression such as `value is {x: i32}`.
    Is(Box<TermIsExpression>),
    /// Explicit generic application such as `value::<i32>`.
    Turbofish {
        /// Input expression.
        expr: Box<TermExpression>,
        /// Generic arguments.
        arguments: Vec<TypeExpression>,
        /// Source span of the expression.
        span: Range<usize>,
    },
    /// Assignment expression such as `self.field = value`.
    Assign {
        /// Target expression.
        target: Box<TermExpression>,
        /// Assigned value.
        value: Box<TermExpression>,
        /// Source span of the expression.
        span: Range<usize>,
    },
    /// `raise effect`
    Raise {
        /// Raised effect payload.
        value: Box<TermExpression>,
        /// Source span of the expression.
        span: Range<usize>,
    },
    /// `if condition { then } else { else }`
    If(Box<IfStatement>),
    /// `if condition { then } else { else }`
    IfLet(Box<IfLetStatement>),
    /// `loop { ... }`
    Loop(Box<LoopStatement>),
    /// `loop pattern in source { ... }`
    LoopIn(Box<LoopInStatement>),
    /// `while { ... }` 或 `loop pattern in source { ... }`
    While(Box<WhileStatement>),
    /// `while { ... }` 或 `loop pattern in source { ... }`
    WhileLet(Box<WhileLetStatement>),
    /// `until { ... }` 或 `loop pattern in source { ... }`
    Until(Box<UntilStatement>),
    /// `until { ... }` 或 `loop pattern in source { ... }`
    UntilNot(Box<UntilNotStatement>),
    /// `try { ... }` / `try? { ... }` / `try! { ... }`
    Try(Box<TryStatement>),
    /// `match scrutinee { case Pattern(binding): body default: body }`
    Match {
        /// 被匹配的表达式。
        scrutinee: Box<TermExpression>,
        /// 匹配分支列表。
        arms: Vec<ArmStatement>,
        /// 源码范围。
        span: Range<usize>,
    },
    /// `catch expr { ... }`
    Catch {
        /// Protected expression.
        expr: Box<TermExpression>,
        /// Handler arms.
        arms: Vec<ArmStatement>,
        /// Source span of the expression.
        span: Range<usize>,
    },
    /// `expr?` — nullable early-return propagation.
    TryPropagate {
        /// Operand expression.
        base: Box<TermExpression>,
        /// Source span of the expression.
        span: Range<usize>,
    },
    /// `@macro(args...)` compile-time macro invocation.
    MacroInvoke {
        /// Macro path.
        path: NamePath,
        /// Invocation arguments as AST nodes.
        args: Vec<TermExpression>,
        /// Source span of the expression.
        span: Range<usize>,
    },
    /// `expr.match { case ... }` — postfix scrutinee match (JetBrains style).
    PostfixMatch {
        /// Scrutinee expression to the left of `.match`.
        base: Box<TermExpression>,
        /// Match arms.
        arms: Vec<ArmStatement>,
        /// Source span of the expression.
        span: Range<usize>,
    },
    /// `expr.catch { case ... }` — postfix protected expression catch.
    PostfixCatch {
        /// Protected expression to the left of `.catch`.
        base: Box<TermExpression>,
        /// Handler arms.
        arms: Vec<ArmStatement>,
        /// Source span of the expression.
        span: Range<usize>,
    },
    /// 结构体构造表达式 `Type { field: value, ... }`。
    Construct {
        /// 类型路径。
        path: NamePath,
        /// 字段初始化列表（字段名, 字段值）。
        fields: Vec<(String, TermExpression)>,
        /// 源码范围。
        span: Range<usize>,
    },
    /// Lambda 表达式 `micro(params) -> return_type { body }`。
    Lambda {
        /// 参数列表。
        params: Vec<FunctionParameter>,
        /// 可选返回类型。
        return_type: Option<TypeExpression>,
        /// 函数体。
        body: Box<DeclarationBody>,
        /// 源码范围。
        span: Range<usize>,
    },
    /// 块表达式 `{ ... }`，内含语句序列和可选尾表达式。
    Block {
        /// 块体。
        body: Box<DeclarationBody>,
        /// 保留字段，当前语言不提供 `unsafe` 块。
        is_unsafe: bool,
        /// 源码范围。
        span: Range<usize>,
    },
    /// X-Grammar 内联 XML 标记（Valkyrie 语言扩展）。
    XmlMarkup {
        /// 解析后的标记树。
        nodes: XgRoot,
        /// 源码范围。
        span: Range<usize>,
    },
    /// T-Grammar 模板字符串字面量（`t"..."`）。
    Template {
        /// 解析后的模板节点树。
        nodes: TgRoot,
        /// 源码范围。
        span: Range<usize>,
    },
    /// 匿名 class / structure 表达式 `class { ... }` / `structure(Parent) { ... }`。
    AnonymousClass {
        /// `true` 表示匿名 structure（值类型），`false` 表示匿名 class。
        is_value_type: bool,
        /// 实现的 trait / 父类型列表。
        parents: Vec<InheritanceItem>,
        /// 内联类体。
        body: ObjectBody,
        /// 源码范围。
        span: Range<usize>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TermUnaryExpression {
    /// Operator.
    pub operator: UnaryOperator,
    /// Operand.
    pub base: TermExpression,
    /// Source span of the expression.
    pub span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TermBinaryExpression {
    /// Operator.
    pub operator: BinaryOperator,
    /// Left operand.
    pub lhs: TermExpression,
    /// Right operand.
    pub rhs: TermExpression,
    /// Source span of the expression.
    pub span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TermAsExpression {
    /// Input expression.
    pub base: TermExpression,
    /// Target type.
    pub target: TypeExpression,
    /// Source span of the expression.
    pub span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TermIsExpression {
    /// Input expression.
    pub base: TermExpression,
    /// Target type.
    pub target: PatternExpression,
    /// Source span of the expression.
    pub span: Range<usize>,
}

///
/// ```v
/// x.field
/// x.module::Class
/// x?.method::<T>() {}
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TermDotExpression {
    /// Object expression.
    pub base: TermExpression,
    /// Member name.
    pub caller: NamePath,
    pub arguments: TermArguments,
    /// Source span of the expression.
    pub span: Range<usize>,
}

///
///
/// ```v
/// field::<T>() { }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TermCallExpression {
    /// Callee expression.
    pub callee: TermExpression,
    /// Call arguments.
    pub args: TermArguments,
    /// Source span of the expression.
    pub span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TermCallArgument {
    /// When `Some`, the argument was written as `name = value`.
    pub key: Option<String>,
    /// Argument value expression.
    pub value: TermExpression,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TermArguments {
    pub arguments: Vec<TermCallArgument>,
}

impl TermArguments {
    /// Positional and keyword call arguments in source order.
    pub fn new(arguments: Vec<TermCallArgument>) -> Self {
        Self { arguments }
    }

    /// A single positional argument.
    pub fn positional(value: TermExpression) -> Self {
        Self { arguments: vec![TermCallArgument { key: None, value }] }
    }
}

impl TermExpression {
    /// Returns the source span of the expression.
    pub fn span(&self) -> &Range<usize> {
        match self {
            Self::Name { span, .. }
            | Self::Literal { span, .. }
            | Self::Tuple { span, .. }
            | Self::Array { span, .. }
            | Self::Turbofish { span, .. }
            | Self::Assign { span, .. }
            | Self::Raise { span, .. }
            | Self::Catch { span, .. }
            | Self::PostfixMatch { span, .. }
            | Self::PostfixCatch { span, .. }
            | Self::TryPropagate { span, .. }
            | Self::MacroInvoke { span, .. }
            | Self::Match { span, .. }
            | Self::Construct { span, .. }
            | Self::Lambda { span, .. }
            | Self::Block { span, .. }
            | Self::XmlMarkup { span, .. }
            | Self::Template { span, .. }
            | Self::AnonymousClass { span, .. } => span,
            Self::Unary(inner) => &inner.span,
            Self::Binary(inner) => &inner.span,
            Self::Call(inner) => &inner.span,
            Self::DotCall(inner) => &inner.span,
            Self::Dereference(inner) => &inner.span,
            Self::Subscript(inner) => &inner.span,
            Self::As(inner) => &inner.span,
            Self::Is(inner) => &inner.span,
            Self::If(inner) => &inner.span,
            Self::IfLet(inner) => &inner.span,
            Self::Loop(inner) => &inner.span,
            Self::LoopIn(inner) => &inner.span,
            Self::While(inner) => &inner.span,
            Self::WhileLet(inner) => &inner.span,
            Self::Until(inner) => &inner.span,
            Self::UntilNot(inner) => &inner.span,
            Self::Try(inner) => &inner.span,
        }
    }
}
