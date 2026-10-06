use super::*;

///
/// ```v
/// until x > 0 { ... }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UntilStatement {
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

///
/// ```v
/// until not Some(x) = item { ... }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UntilNotStatement {
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
