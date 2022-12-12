//! Enum, variant, flags, and flag member definitions for HIR.

use super::{GenericType, HirDocumentation, HirExpr, HirField, HirVisibility};
use crate::Identifier;

/// An enum in HIR.
///
/// Enums define sum types with named variants. Each variant can
/// optionally contain fields.
///
/// # Unity Types
///
/// When `is_unity` is true, this represents a Unity type (similar to Rust's enum).
/// Unity types allow variants to be used as subtypes of the base type:
///
/// ```v
/// unite Option<T> {
///     Some { value: T },
///     None
/// }
///
/// // Variant subtyping: Some<i32> <: Option<i32>
/// let opt: Option<i32> = Some { value: 42 }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirEnum {
    /// The name of the enum.
    pub name: Identifier,
    /// Documentation for the enum.
    pub doc: HirDocumentation,
    /// Generic parameters for the enum.
    pub generics: Vec<GenericType>,
    /// The variants of the enum.
    pub variants: Vec<HirVariant>,
    /// Visibility of the enum.
    pub visibility: HirVisibility,
    /// Whether this is a Unity type (variant subtyping enabled).
    ///
    /// Unity types allow each variant to be implicitly converted to the base type.
    /// This enables patterns like `Some<T> <: Option<T>` and `None <: Option<T>`.
    pub is_unity: bool,
}

/// A variant in an enum.
///
/// Each variant represents one possible value of the enum type.
/// Variants can optionally contain named or positional fields.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirVariant {
    /// The name of the variant.
    pub name: Identifier,
    /// Documentation for the variant.
    pub doc: HirDocumentation,
    /// The fields contained in this variant.
    pub fields: Vec<HirField>,
    /// Optional constructor result type for GADT-style variants.
    ///
    /// When present, this variant refines the instantiated base `unite`
    /// type returned by the constructor, such as `Expr<f64>`.
    pub result_type: Option<super::ValkyrieType>,
    /// Optional explicit enum discriminator (`enums Color { Red = 2 }`).
    pub discriminator: Option<HirExpr>,
}

impl Default for HirVariant {
    fn default() -> Self {
        Self { name: Identifier::new(""), doc: HirDocumentation::default(), fields: Vec::new(), result_type: None, discriminator: None }
    }
}

/// A flags (bit flags) type in HIR.
///
/// Flags types define a set of named bit flags that can be combined using
/// bitwise operations. Each flag member represents a single bit value,
/// and members can be combined to represent multiple flags at once.
///
/// # Example
///
/// ```v
/// flags FilePermission {
///     READ = 1
///     WRITE = 2
///     EXECUTE = 4
///     ALL = READ | WRITE | EXECUTE
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirFlags {
    /// The name of the flags type.
    pub name: Identifier,
    /// Documentation for the flags type.
    pub doc: HirDocumentation,
    /// The individual flag members.
    pub members: Vec<HirFlagMember>,
    /// Visibility of the flags type.
    pub visibility: HirVisibility,
}

/// A member of a flags type.
///
/// Each flag member has a name and an associated value. The value can be
/// a simple literal or a combination of other flags using bitwise OR.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirFlagMember {
    /// The name of the flag member.
    pub name: Identifier,
    /// Documentation for the flag member.
    pub doc: HirDocumentation,
    /// The value expression for this flag.
    ///
    /// This can be a simple integer literal or a combination expression
    /// like `READ | WRITE` for combined flags.
    pub value: HirExpr,
    /// Whether this is a combined flag (e.g., `ALL = READ | WRITE | EXECUTE`).
    ///
    /// Combined flags are composed of other flag members using bitwise operations.
    pub is_combined: bool,
}

impl Default for HirEnum {
    fn default() -> Self {
        Self {
            name: Identifier::new(""),
            doc: HirDocumentation::default(),
            generics: Vec::new(),
            variants: Vec::new(),
            visibility: HirVisibility::default(),
            is_unity: false,
        }
    }
}

impl HirEnum {
    /// Creates a new enum with the given name.
    pub fn new(name: Identifier) -> Self {
        Self { name, ..Self::default() }
    }

    /// Creates a new Unity type with the given name.
    pub fn new_unity(name: Identifier) -> Self {
        Self { name, is_unity: true, ..Self::default() }
    }

    /// Returns true if this is a Unity type.
    pub fn is_unity(&self) -> bool {
        self.is_unity
    }

    /// Returns the variant with the given name, if it exists.
    pub fn find_variant(&self, name: &Identifier) -> Option<&HirVariant> {
        self.variants.iter().find(|v| &v.name == name)
    }

    /// Returns the names of all variants.
    pub fn variant_names(&self) -> Vec<&Identifier> {
        self.variants.iter().map(|v| &v.name).collect()
    }
}
