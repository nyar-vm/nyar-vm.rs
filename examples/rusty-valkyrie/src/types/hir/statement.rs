//! Statement, block, attribute, and argument definitions for HIR.

use super::{HirExpr, HirPattern};
use crate::{Identifier, NamePath, SourceSpan};

/// A block in HIR.
///
/// A block contains a sequence of statements and an optional final expression.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirBlock {
    /// The statements in the block.
    pub statements: Vec<HirStatement>,
    /// The optional final expression.
    pub expr: Option<Box<HirExpr>>,
    /// The source span for error reporting.
    pub span: SourceSpan,
}

/// A statement in HIR.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirStatement {
    /// The kind of the statement.
    pub kind: HirStatementKind,
    /// The source span for error reporting.
    pub span: SourceSpan,
}

/// The kind of a statement.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum HirStatementKind {
    /// A let binding statement.
    Let {
        /// Whether the binding is mutable.
        is_mutable: bool,
        /// The pattern to bind.
        pattern: HirPattern,
        /// The optional initializer expression.
        initializer: Option<Box<HirExpr>>,
        /// The optional type annotation.
        ty: Option<super::ValkyrieType>,
    },
    /// An expression statement.
    Expr(Box<HirExpr>),
}

/// A match arm in HIR.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirMatchArm {
    /// The pattern to match.
    pub pattern: HirPattern,
    /// The optional guard expression.
    pub guard: Option<Box<HirExpr>>,
    /// The body expression.
    pub body: Box<HirExpr>,
}

/// An attribute in HIR.
///
/// Attributes are metadata annotations that can be applied to various HIR items
/// such as structs, enums, functions, and fields. They follow the syntax `@name(args)`.
///
/// # Common Attributes
///
/// - `@derive(Trait1, Trait2)` - Automatically implement specified traits
/// - `@inline` - Hint for inlining functions
/// - `@deprecated` - Mark items as deprecated
///
/// # Example
///
/// ```v
/// @derive(Hash, Eq, Show)
/// structure Point {
///     x: i32
///     y: i32
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirAttribute {
    /// The name of the attribute (e.g., "derive", "inline").
    pub name: NamePath,
    /// Arguments passed to the attribute.
    pub arguments: Vec<HirArgument>,
}

impl HirAttribute {
    /// Creates a new attribute with the given name and no arguments.
    pub fn new(name: NamePath) -> Self {
        Self { name, arguments: Vec::new() }
    }

    /// Creates a new attribute with the given name and arguments.
    pub fn with_arguments(name: NamePath, arguments: Vec<HirArgument>) -> Self {
        Self { name, arguments }
    }

    /// Checks if this is a `@derive` attribute.
    pub fn is_derive(&self) -> bool {
        self.name.parts().first().map(|id| id.as_str() == "derive").unwrap_or(false)
    }

    /// Extracts derive trait paths from this attribute.
    ///
    /// Returns an empty vector if this is not a derive attribute,
    /// or if the arguments cannot be resolved to trait paths.
    pub fn extract_derive_traits(&self) -> Vec<NamePath> {
        if !self.is_derive() {
            return Vec::new();
        }

        self.arguments.iter().filter_map(|arg| Self::extract_path_from_expr(&arg.value)).collect()
    }

    /// Extracts a NamePath from an HirExpr if it represents a path.
    fn extract_path_from_expr(expr: &HirExpr) -> Option<NamePath> {
        match &expr.kind {
            super::HirExprKind::Path(path) => Some(path.clone()),
            _ => None,
        }
    }
}

/// An argument in an attribute.
///
/// Arguments can be positional (key is None) or named (key is Some).
///
/// # Examples
///
/// - `@derive(Hash, Eq)` - positional arguments `Hash` and `Eq`
/// - `@config(timeout = 30)` - named argument with key `timeout`
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirArgument {
    /// Optional key for named arguments.
    pub key: Option<Identifier>,
    /// The argument value expression.
    pub value: Box<HirExpr>,
}
