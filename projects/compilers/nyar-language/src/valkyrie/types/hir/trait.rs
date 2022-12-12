//! Trait and associated type definitions for HIR.

use super::{GenericType, HirDocumentation, HirExpr, HirFunction, HirVisibility, ValkyrieType};
use crate::{Identifier, SourceSpan};

/// A trait in HIR.
///
/// Traits define shared behavior that types can implement. They can contain:
/// - Required methods that implementors must provide
/// - Default method implementations
/// - Associated types for flexible type relationships
/// - Super-traits for trait inheritance
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirTrait {
    /// The name of the trait.
    pub name: Identifier,
    /// Documentation for the trait.
    pub doc: HirDocumentation,
    /// Generic parameters for the trait.
    pub generics: Vec<GenericType>,
    /// Required methods that implementors must provide.
    pub methods: Vec<HirFunction>,
    /// Associated types defined by the trait.
    pub associated_types: Vec<HirAssociatedType>,
    /// Associated constants defined by the trait.
    pub associated_constants: Vec<HirAssociatedConst>,
    /// Super-traits that this trait extends.
    pub super_traits: Vec<ValkyrieType>,
    /// Default method implementations provided by the trait.
    pub default_methods: Vec<HirFunction>,
    /// Visibility of the trait.
    pub visibility: HirVisibility,
}

/// An associated type in a trait definition.
///
/// Associated types allow traits to define placeholder types that
/// implementing types can specify. They can also be generic associated
/// types (GATs) with their own type parameters.
///
/// # Example
///
/// ```v
/// trait Iterator {
///     type Item;
///     micro next(self) -> Self::Item?;
/// }
/// ```
///
/// # Generic Associated Types (GATs)
///
/// GATs allow associated types to have their own type parameters:
///
/// ```v
/// trait Container {
///     type Item<'a> where Self: 'a;
///     micro get<'a>(self, key: String) -> Self::Item<'a>;
/// }
///
/// trait LendingIterator {
///     type Item<'a> where Self: 'a;
///     micro next<'a>(&'a mut self) -> Option<Self::Item<'a>>;
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirAssociatedType {
    /// The name of the associated type.
    pub name: Identifier,
    /// Documentation for the associated type.
    pub doc: HirDocumentation,
    /// Type parameters for generic associated types (GATs).
    ///
    /// For example, in `type Item<'a>`, this would contain the lifetime
    /// parameter `'a`. For non-GAT associated types, this is empty.
    pub type_params: Vec<GenericType>,
    /// Bounds that the associated type must satisfy.
    pub bounds: Vec<ValkyrieType>,
    /// Default type for the associated type (optional).
    pub default: Option<ValkyrieType>,
    /// Source span for error reporting.
    pub span: SourceSpan,
}

impl HirAssociatedType {
    /// Creates a new associated type with the given name.
    pub fn new(name: Identifier, span: SourceSpan) -> Self {
        Self { name, doc: HirDocumentation::default(), type_params: Vec::new(), bounds: Vec::new(), default: None, span }
    }

    /// Creates a new associated type with documentation.
    pub fn with_doc(name: Identifier, doc: HirDocumentation, span: SourceSpan) -> Self {
        Self { name, doc, type_params: Vec::new(), bounds: Vec::new(), default: None, span }
    }

    /// Creates a generic associated type (GAT) with type parameters.
    pub fn with_type_params(name: Identifier, type_params: Vec<GenericType>, span: SourceSpan) -> Self {
        Self { name, doc: HirDocumentation::default(), type_params, bounds: Vec::new(), default: None, span }
    }

    /// Adds a bound to the associated type.
    pub fn with_bound(mut self, bound: ValkyrieType) -> Self {
        self.bounds.push(bound);
        self
    }

    /// Sets the default type for the associated type.
    pub fn with_default(mut self, default: ValkyrieType) -> Self {
        self.default = Some(default);
        self
    }

    /// Returns true if this is a generic associated type (GAT).
    pub fn is_gat(&self) -> bool {
        !self.type_params.is_empty()
    }

    /// Returns true if this associated type has a default.
    pub fn has_default(&self) -> bool {
        self.default.is_some()
    }
}

/// An associated constant in a trait definition.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirAssociatedConst {
    /// The name of the associated constant.
    pub name: Identifier,
    /// Documentation for the associated constant.
    pub doc: HirDocumentation,
    /// Declared type of the associated constant.
    pub const_type: ValkyrieType,
    /// Optional default value for the associated constant.
    pub default_value: Option<HirExpr>,
    /// Source span for error reporting.
    pub span: SourceSpan,
}

impl HirAssociatedConst {
    /// Creates a new associated constant with the given name and type.
    pub fn new(name: Identifier, const_type: ValkyrieType, span: SourceSpan) -> Self {
        Self { name, doc: HirDocumentation::default(), const_type, default_value: None, span }
    }

    /// Sets the default value for the associated constant.
    pub fn with_default(mut self, default_value: HirExpr) -> Self {
        self.default_value = Some(default_value);
        self
    }
}
