use super::*;

///
/// ```v
/// loop (x, y) in points if x > 0 { ... }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LoopStatement {
    /// Loop body.
    pub body: DeclarationBody,
    /// Source span of the expression.
    pub span: Range<usize>,
}

///
/// ```v
/// loop point in points { ... }
/// loop (x, y) in points if x > 0 { ... }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LoopInStatement {
    /// Optional loop label.
    pub label: Option<Identifier>,
    /// Optional binding pattern for iterator loops.
    pub pattern: Option<PatternExpression>,
    /// Optional source expression for iterator loops.
    pub iterator: Option<TermExpression>,
    /// Optional condition for while-style loops.
    pub condition: Option<TermExpression>,
    /// Loop body.
    pub body: DeclarationBody,
    /// Source span of the expression.
    pub span: Range<usize>,
}
