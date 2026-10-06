use super::*;

///
/// ```v
/// if condition { ... }
/// if c1 { ... } else if c2 { ... } else { ... }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IfStatement {
    /// The condition expression.
    pub condition: TermExpression,
    /// The then branch body.
    pub then_body: DeclarationBody,
    /// The optional else branch body.
    pub else_body: Option<DeclarationBody>,
    /// Source span of the expression.
    pub span: Range<usize>,
}

///
/// ```v
/// if let Some(x) = item { ... } else { ... }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IfLetStatement {
    /// The condition expression.
    pub pattern: PatternExpression,
    /// The condition expression.
    pub item: TermExpression,
    /// The then branch body.
    pub then_body: DeclarationBody,
    /// The optional else branch body.
    pub else_body: Option<DeclarationBody>,
    /// Source span of the expression.
    pub span: Range<usize>,
}
