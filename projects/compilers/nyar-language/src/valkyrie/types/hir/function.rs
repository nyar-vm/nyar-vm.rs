//! Function definitions for HIR.

use super::{GenericType, HirAttribute, HirBlock, HirParam, HirVisibility, ValkyrieType};
use crate::{Identifier, NamePath, SourceSpan};

/// A function in HIR.
///
/// Functions are the primary unit of computation in Valkyrie.
/// They can be standalone or methods within a struct/class.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirFunction {
    /// The name of the function.
    pub name: Identifier,
    /// Declaring namespace from `namespace foo;` / `namespace foo { ... }` at the definition site.
    #[cfg_attr(feature = "serde", serde(default))]
    pub declaring_namespace: NamePath,
    /// Documentation for the function.
    pub doc: super::HirDocumentation,
    /// Attributes applied to the function (e.g., `@inline`, `@deprecated`).
    pub annotations: Vec<HirAttribute>,
    /// Generic parameters for the function.
    pub generics: Vec<GenericType>,
    /// Parameters of the function.
    pub params: Vec<HirParam>,
    /// The return type of the function.
    pub return_type: ValkyrieType,
    /// The function body block.
    pub body: HirBlock,
    /// The source span for error reporting.
    pub span: SourceSpan,
    /// Visibility of the function.
    pub visibility: HirVisibility,
    /// Whether this function is abstract (has no body implementation).
    ///
    /// Abstract functions are declared without a body in abstract classes
    /// and must be implemented by concrete subclasses.
    pub is_abstract: bool,
    /// Whether this function is final (cannot be overridden).
    ///
    /// Final methods cannot be overridden by subclasses.
    /// This is useful for methods that should have fixed behavior.
    pub is_final: bool,
    /// Whether this function is virtual (can be overridden by subclasses).
    ///
    /// Virtual functions use dynamic dispatch through the witness table,
    /// allowing subclasses to provide their own implementation.
    /// In Valkyrie, methods are implicitly virtual unless marked `final`,
    /// but an explicit `virtual` modifier can be used for clarity.
    pub is_virtual: bool,
    /// Whether this function overrides a parent class or trait method.
    ///
    /// Override functions must match the signature of the parent method
    /// and are verified during type checking.
    pub is_override: bool,
}

impl HirFunction {
    /// Returns true if this function can be overridden by subclasses.
    ///
    /// A function is overridable if it is virtual (implicitly or explicitly)
    /// or abstract, and not final or static.
    pub fn is_overridable(&self) -> bool {
        (self.is_virtual || self.is_abstract) && !self.is_final
    }

    /// Returns true if this function can be used to override a parent method.
    ///
    /// A function can override if it is marked `override` and is not final
    /// or static.
    pub fn can_override(&self) -> bool {
        self.is_override && !self.is_final
    }

    /// Returns true if this function is a valid override target.
    ///
    /// A function is a valid override target if it is marked `override`,
    /// not abstract (abstract methods are declarations, not implementations),
    /// and not final.
    pub fn is_valid_override(&self) -> bool {
        self.is_override && !self.is_abstract && !self.is_final
    }
}
