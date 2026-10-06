use super::*;

///
/// ```v
/// while x > 0 { ... }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WhileStatement {
    /// Optional loop label.
    pub label: Option<Identifier>,
    /// Optional condition for while-style loops.
    pub condition: Option<TermExpression>,
    /// Loop body.
    pub body: DeclarationBody,
    /// Source span of the expression.
    pub span: Range<usize>,
}

///
/// ```v
/// while let Some(x) = item { ... }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WhileLetStatement {
    /// Optional loop label.
    pub label: Option<Identifier>,
    /// Pattern to match each iteration value against.
    pub pattern: PatternExpression,
    /// Scrutinee expression (`while let pat = expr`).
    pub scrutinee: TermExpression,
    /// Optional guard (`while let pat = expr if guard`).
    pub guard: Option<TermExpression>,
    /// Loop body.
    pub body: DeclarationBody,
    /// Source span of the expression.
    pub span: Range<usize>,
}
