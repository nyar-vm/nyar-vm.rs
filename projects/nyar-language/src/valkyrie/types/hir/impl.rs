//! Impl and derive definitions for HIR.

use super::{GenericType, HirAssociatedConstImpl, HirAssociatedTypeImpl, HirFunction, ValkyrieType};
use crate::{NamePath, SourceSpan};

/// A structured `where` constraint attached to an impl block.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirWhereConstraint {
    /// The constrained type.
    pub target: ValkyrieType,
    /// Trait bounds that the target must satisfy.
    pub bounds: Vec<NamePath>,
    /// Source span for error reporting.
    pub span: SourceSpan,
}

/// An impl block in HIR.
///
/// Impl blocks are used to implement traits for types or to add methods to existing types.
///
/// # Generic Impl Blocks
///
/// Impl blocks can have generic parameters, which is essential for implementing
/// traits for generic types:
///
/// ```v
/// impl<T: Clone> Clone for Box<T> {
///     micro clone(self) -> Self {
///         Box { value: self.value.clone() }
///     }
/// }
/// ```
///
/// # Associated Type Implementations
///
/// When implementing a trait with associated types, the impl block must specify
/// concrete types for each associated type:
///
/// ```v
/// impl Iterator for Counter {
///     type Item = i32
///
///     micro next(self) -> Self::Item? {
///         // implementation
///     }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirImpl {
    /// Generic parameters for the impl block.
    ///
    /// For example, in `impl<T: Clone> Clone for Box<T>`, this would contain
    /// the generic parameter `T` with its `Clone` bound.
    pub generics: Vec<GenericType>,
    /// Structured `where` constraints attached to the impl header.
    pub where_constraints: Vec<HirWhereConstraint>,
    /// The target type being implemented.
    pub target: ValkyrieType,
    /// The trait being implemented (None for inherent impl blocks).
    pub trait_path: Option<NamePath>,
    /// Methods defined in this impl block.
    pub methods: Vec<HirFunction>,
    /// Associated type implementations in this impl block.
    ///
    /// When implementing a trait with associated types, each associated type
    /// must be bound to a concrete type. This field stores those bindings.
    ///
    /// For example, in `impl Iterator for Counter { type Item = i32; ... }`,
    /// this would contain `HirAssociatedTypeImpl { name: "Item", concrete_type: i32 }`.
    pub associated_type_impls: Vec<HirAssociatedTypeImpl>,
    /// Associated constant implementations in this impl block.
    pub associated_const_impls: Vec<HirAssociatedConstImpl>,
}

/// Represents a derive macro invocation in HIR.
///
/// Derive macros automatically implement traits for types based on their structure.
/// When a type is annotated with `@derive(Trait1, Trait2)`, the compiler generates
/// appropriate trait implementations during compilation.
///
/// # Supported Derive Traits
///
/// Common derive traits include:
/// - `Eq` - Equality comparison
/// - `Hash` - Hashing for use in hash maps
/// - `Show` - String representation for debugging
/// - `Clone` - Explicit cloning capability
/// - `Default` - Default value construction
/// - `Serialize` / `Deserialize` - Serialization support
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
///
/// The derive macro system will generate implementations equivalent to:
///
/// ```v
/// impl Hash for Point {
///     micro hash(self) -> u64 { ... }
/// }
/// impl Eq for Point {
///     micro eq(self, other: Self) -> bool { ... }
/// }
/// impl Show for Point {
///     micro show(self) -> String { ... }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirDerive {
    /// The traits to derive for the target type.
    ///
    /// Each trait path will be resolved to its definition during compilation,
    /// and appropriate implementations will be generated.
    pub traits: Vec<NamePath>,
    /// Generated trait implementations.
    ///
    /// This field is populated during the compilation phase when derive macros
    /// are expanded. Each generated impl block corresponds to one derived trait.
    pub generated_impls: Vec<HirImpl>,
}

impl HirDerive {
    /// Creates a new derive directive with the specified traits.
    pub fn new(traits: Vec<NamePath>) -> Self {
        Self { traits, generated_impls: Vec::new() }
    }

    /// Adds a generated implementation for a derived trait.
    pub fn add_generated_impl(&mut self, impl_block: HirImpl) {
        self.generated_impls.push(impl_block);
    }

    /// Checks if all derives have been processed.
    pub fn is_complete(&self) -> bool {
        self.traits.len() == self.generated_impls.len()
    }

    /// Returns the traits that have not yet been processed.
    pub fn pending_traits(&self) -> impl Iterator<Item = &NamePath> {
        let processed_count = self.generated_impls.len();
        self.traits.iter().skip(processed_count)
    }
}
