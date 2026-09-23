use super::{HirExpr, HirIdentifier, HirResolvedCall};
use crate::{Identifier, SourceSpan};
use std::cmp::Ordering;

/// Access level for HIR items.
///
/// This enum represents the four access levels in Valkyrie:
/// - `Public`: Visible to all code
/// - `Protected`: Visible to the current class and its subclasses
/// - `Internal`: Visible within the current module
/// - `Private`: Visible only within the current class/file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AccessLevel {
    /// Public access - visible to all code.
    ///
    /// Items declared with `public` modifier are accessible from any module or class.
    /// This is the most permissive access level.
    Public,

    /// Protected access - visible to current class and subclasses.
    ///
    /// Items declared with `protected` modifier are accessible within:
    /// - The class that defines them
    /// - All subclasses (direct and indirect)
    ///
    /// This is useful for implementation details that should be hidden
    /// from external code but accessible to subclasses.
    Protected,

    /// Internal access - visible within the current module.
    ///
    /// Items declared with `internal` modifier are accessible from any code
    /// within the same module, but not from other modules.
    ///
    /// This is useful for module-internal APIs that should not be exposed
    /// to external consumers.
    Internal,

    /// Private access - visible only within the current class/file.
    ///
    /// Items declared with `private` modifier are accessible only within:
    /// - The class that defines them (for class members)
    /// - The current file (for top-level items)
    ///
    /// This is the most restrictive access level.
    #[default]
    Private,
}

impl AccessLevel {
    fn rank(&self) -> u8 {
        match self {
            AccessLevel::Private => 0,
            AccessLevel::Internal => 1,
            AccessLevel::Protected => 2,
            AccessLevel::Public => 3,
        }
    }

    /// Returns true if this is public access.
    pub fn is_public(&self) -> bool {
        matches!(self, AccessLevel::Public)
    }

    /// Returns true if this is protected access.
    pub fn is_protected(&self) -> bool {
        matches!(self, AccessLevel::Protected)
    }

    /// Returns true if this is internal access.
    pub fn is_internal(&self) -> bool {
        matches!(self, AccessLevel::Internal)
    }

    /// Returns true if this is private access.
    pub fn is_private(&self) -> bool {
        matches!(self, AccessLevel::Private)
    }

    /// Returns the access level as a string for display purposes.
    pub fn as_str(&self) -> &'static str {
        match self {
            AccessLevel::Public => "public",
            AccessLevel::Protected => "protected",
            AccessLevel::Internal => "internal",
            AccessLevel::Private => "private",
        }
    }
}

impl PartialOrd for AccessLevel {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AccessLevel {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rank().cmp(&other.rank())
    }
}

/// Visibility of HIR items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirVisibility {
    /// The access level of the item.
    pub access: AccessLevel,
}

impl HirVisibility {
    /// Creates a new visibility with the given access level.
    pub fn new(access: AccessLevel) -> Self {
        Self { access }
    }

    /// Creates a public visibility.
    pub fn public() -> Self {
        Self { access: AccessLevel::Public }
    }

    /// Creates a protected visibility.
    pub fn protected() -> Self {
        Self { access: AccessLevel::Protected }
    }

    /// Creates an internal visibility.
    pub fn internal() -> Self {
        Self { access: AccessLevel::Internal }
    }

    /// Creates a private visibility.
    pub fn private() -> Self {
        Self { access: AccessLevel::Private }
    }

    /// Returns true if the item is public.
    pub fn is_public(&self) -> bool {
        self.access.is_public()
    }

    /// Returns true if the item is protected.
    pub fn is_protected(&self) -> bool {
        self.access.is_protected()
    }

    /// Returns true if the item is internal.
    pub fn is_internal(&self) -> bool {
        self.access.is_internal()
    }

    /// Returns true if the item is private.
    pub fn is_private(&self) -> bool {
        self.access.is_private()
    }

    /// Returns the access level.
    pub fn access_level(&self) -> AccessLevel {
        self.access
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum HirKind {
    Type,
    Function(Box<HirKind>, Box<HirKind>),
}

/// An associated type projection like `Self::Item` or `<T as Trait>::Item`.
///
/// This represents accessing an associated type from a trait implementation.
/// For Generic Associated Types (GATs), the `type_args` field contains the
/// type arguments passed to the GAT's type parameters.
///
/// # Examples
///
/// Simple associated type:
/// ```v
/// trait Iterator {
///     type Item
///     micro next(self) -> Self::Item?
/// }
///
/// micro process<T: Iterator>(iter: T) -> T::Item {
///     iter.next()?
/// }
/// ```
///
/// Generic Associated Type (GAT):
/// ```v
/// trait LendingIterator {
///     type Item<'a> where Self: 'a
///     micro next<'a>(&'a mut self) -> Option<Self::Item<'a>>
/// }
///
/// // Here Self::Item<'a> would have type_args = [HirType::Lifetime("'a")]
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AssociatedType {
    /// The base type (e.g., `Self` or a type parameter).
    pub base: ValkyrieType,
    /// The name of the associated type.
    pub name: Identifier,
    /// Type arguments for Generic Associated Types (GATs).
    ///
    /// For example, in `T::Item<'a>`, this would contain the lifetime
    /// argument `'a`. For non-GAT associated types, this is empty.
    pub type_arguments: Vec<ValkyrieType>,
}

/// A trait object type representing dynamic dispatch.
///
/// Trait objects use witness tables for dynamic method dispatch.
/// At runtime, a trait object is represented as a fat pointer:
/// - data pointer: points to the actual data
/// - witness table pointer: points to the witness table for the trait implementation
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TraitObject {
    /// The trait being implemented.
    pub trait_path: Identifier,
    /// Type parameters for the trait (if any).
    pub type_arguments: Vec<ValkyrieType>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RowMethodType {
    pub name: Identifier,
    pub params: Vec<ValkyrieType>,
    pub return_type: ValkyrieType,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RowType {
    pub methods: Vec<RowMethodType>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FunctionType {
    pub params: Vec<ValkyrieType>,
    pub return_type: ValkyrieType,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TypeLambda {
    pub params: Vec<GenericType>,
    pub body: ValkyrieType,
}

/// Type model of Valkyrie language.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ValkyrieType {
    /// `void`, the zero type in ADT model
    Void,
    /// `unit`, the one type in ADT model
    Unit,
    /// `bool`
    Boolean,
    Integer8 {
        signed: bool,
    },
    Integer16 {
        signed: bool,
    },
    Integer32 {
        signed: bool,
    },
    Integer64 {
        signed: bool,
    },
    Integer128 {
        signed: bool,
    },
    /// `f32`
    Float32,
    /// `f64`
    Float64,
    /// `char`, the Unicode character
    Character,
    Utf8,
    Utf16,
    Named(Identifier),
    Apply(Box<ValkyrieType>, Vec<ValkyrieType>),
    /// `T<X>`
    Generic(GenericType),
    Function(Box<FunctionType>),
    Tuple(Vec<ValkyrieType>),
    Row(RowType),
    /// `[T]` 堆数组。
    Array(Box<ValkyrieType>),
    /// `[T; N]` 定长数组。
    FixedArray {
        element: Box<ValkyrieType>,
        length: usize,
    },
    /// `micro(T) -> U`
    TypeLambda(Box<TypeLambda>),
    TraitObject(TraitObject),
    Associated(Box<AssociatedType>),
    /// `auto`, automatic infer type
    AutoType,
    /// `Self`, the type slot
    SelfType,
    /// `T?`, an explicit nullable type. It is not an anonymous union and is
    /// never represented by a nominal type named `null`.
    Nullable(Box<ValkyrieType>),
    /// `A | B | C` anonymous union type
    Union(Vec<ValkyrieType>),
    /// `A & B & C` anonymous intersection type
    Intersection(Vec<ValkyrieType>),
}

impl Default for ValkyrieType {
    fn default() -> Self {
        ValkyrieType::Unit
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GenericType {
    pub name: Identifier,
    pub kind: HirKind,
    pub bounds: Vec<Identifier>,
}

/// An associated type implementation in an impl block.
///
/// When implementing a trait with associated types, the concrete type
/// for each associated type must be specified in the impl block.
///
/// # Example
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
///
/// In this example, `HirAssociatedTypeImpl` would represent the
/// `type Item = i32` declaration within the impl block.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirAssociatedTypeImpl {
    /// The name of the associated type being implemented.
    pub name: Identifier,
    /// The concrete type that the associated type is bound to.
    pub concrete_type: ValkyrieType,
    /// Type arguments for Generic Associated Types (GATs).
    ///
    /// For example, in `type Item<'a> = &'a T`, this would contain the
    /// lifetime argument `'a`. For non-GAT associated types, this is empty.
    pub type_args: Vec<ValkyrieType>,
    /// Source span for error reporting.
    pub span: SourceSpan,
}

impl HirAssociatedTypeImpl {
    /// Creates a new associated type implementation.
    pub fn new(name: Identifier, concrete_type: ValkyrieType, span: SourceSpan) -> Self {
        Self { name, concrete_type, type_args: Vec::new(), span }
    }

    /// Creates a GAT implementation with type arguments.
    pub fn with_type_args(name: Identifier, concrete_type: ValkyrieType, type_args: Vec<ValkyrieType>, span: SourceSpan) -> Self {
        Self { name, concrete_type, type_args, span }
    }

    /// Returns true if this is a GAT implementation with type arguments.
    pub fn is_gat_imply(&self) -> bool {
        !self.type_args.is_empty()
    }
}

/// An associated constant implementation in an impl block.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirAssociatedConstImpl {
    /// The name of the associated constant being implemented.
    pub name: Identifier,
    /// Optional explicit constant type annotation.
    pub const_type: Option<ValkyrieType>,
    /// The concrete value assigned to the associated constant.
    pub value: HirExpr,
    /// Source span for error reporting.
    pub span: SourceSpan,
}

/// Call-site binding mode for a parameter (`<` / `>` markers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum HirParameterBindingKind {
    /// May only be passed positionally (before `<`).
    PositionalOnly,
    /// May be passed positionally or by name (default when no markers).
    #[default]
    PositionalOrKeyword,
    /// May only be passed by name (after `>`).
    KeywordOnly,
}

/// Variadic parameter kind (`..` positional rest, `...` keyword rest).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum HirVariadicKind {
    #[default]
    None,
    /// `..name` ? absorbs extra positional arguments.
    PositionalRest,
    /// `...name` ? absorbs extra keyword arguments.
    KeywordRest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirParam {
    pub name: HirIdentifier,
    pub ty: ValkyrieType,
    pub binding_kind: HirParameterBindingKind,
    /// Whether the parameter uses `mut` (including `mut self`).
    #[cfg_attr(feature = "serde", serde(default))]
    pub is_mutable: bool,
    pub default: Option<HirExpr>,
    pub variadic: HirVariadicKind,
}

impl Default for HirParam {
    fn default() -> Self {
        Self {
            name: HirIdentifier {
                name: Identifier::new(""),
                shadow_index: 0,
                span: crate::types::SourceSpan::new(crate::types::SourceID::default(), 0, 0),
            },
            ty: ValkyrieType::Unit,
            binding_kind: HirParameterBindingKind::default(),
            is_mutable: false,
            default: None,
            variadic: HirVariadicKind::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum HirExtractorPattern {
    Constructor {
        name: crate::NamePath,
        canonical_callee: crate::NamePath,
        fields: Vec<HirPattern>,
        resolved: Option<HirResolvedCall>,
    },
    Array {
        canonical_callee: crate::NamePath,
        prefix: Vec<HirPattern>,
        rest: Option<HirIdentifier>,
        suffix: Vec<HirPattern>,
        resolved: Option<HirResolvedCall>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PatternRefutability {
    /// Pattern always matches (irrefutable) - safe for `let` bindings
    Irrefutable,
    /// Pattern may fail to match (refutable) - requires `if let`, `match`, etc.
    Refutable,
    /// Cannot determine statically (e.g., extractor with unknown semantics)
    Unknown,
}

impl PatternRefutability {
    pub fn is_refutable(self) -> bool {
        matches!(self, PatternRefutability::Refutable | PatternRefutability::Unknown)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum HirPattern {
    Wildcard,
    Variable(HirIdentifier),
    Tuple(Vec<HirPattern>),
    Literal(HirLiteral),
    Range {
        start: Option<HirLiteral>,
        end: Option<HirLiteral>,
        inclusive_end: bool,
    },
    Extractor(HirExtractorPattern),
    Or(Vec<HirPattern>),
    Name(crate::NamePath),
    Type(crate::NamePath),
    TypedBind {
        identifier: HirIdentifier,
        ty: crate::NamePath,
    },
    Object {
        name: Option<crate::NamePath>,
        fields: Vec<(Identifier, HirPattern)>,
        rest: Option<HirIdentifier>,
    },
    Else,
    /// `whole @ subpat` ? bind whole scrutinee slice and destructure sub-pattern.
    Bind {
        identifier: HirIdentifier,
        pattern: Box<HirPattern>,
    },
    /// `mut pat` ? mutable reference binding.
    Mut(Box<HirPattern>),
    /// `pin [mut] pat` ? pinned mutable reference binding.
    Pin {
        mutable: bool,
        pattern: Box<HirPattern>,
    },
}

impl HirPattern {
    pub fn refutability(&self) -> PatternRefutability {
        use PatternRefutability::*;
        match self {
            HirPattern::Wildcard | HirPattern::Variable(_) | HirPattern::Bind { .. } | HirPattern::Mut(_) | HirPattern::Pin { .. } => {
                Irrefutable
            }
            HirPattern::Tuple(items) => {
                if items.iter().any(|item| item.refutability().is_refutable()) {
                    Refutable
                }
                else {
                    Irrefutable
                }
            }
            HirPattern::Literal(_) | HirPattern::Range { .. } | HirPattern::Extractor(_) | HirPattern::Name(_) | HirPattern::Type(_) => {
                Refutable
            }
            HirPattern::Or(items) => {
                if items.iter().any(|item| item.refutability().is_refutable()) {
                    Refutable
                }
                else {
                    Irrefutable
                }
            }
            HirPattern::TypedBind { .. } => Refutable,
            HirPattern::Object { fields, rest, .. } => {
                let fields_refutable = fields.iter().any(|(_, p)| p.refutability().is_refutable());
                let rest_refutable = rest.is_some();
                if fields_refutable || rest_refutable { Refutable } else { Irrefutable }
            }
            HirPattern::Else => Refutable,
        }
    }
}

/// HIR 字符串片段。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum HirStringSegment {
    /// 纯文本。
    Text(String),
    /// 插值片段。
    Interpolation {
        /// 插值表达式。
        expr: HirExpr,
        /// 是否为 Fluent 插值。
        is_fluent: bool,
    },
}

/// HIR 字符串字面量。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirStringLiteral {
    /// DSL 前缀。
    pub prefix: Option<Identifier>,
    /// 引号个数。
    pub quote_count: u8,
    /// 字符串片段列表。
    pub segments: Vec<HirStringSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum HirLiteral {
    Integer64(i64),
    Float64(ordered_float::OrderedFloat<f64>),
    String(HirStringLiteral),
    Bool(bool),
    Unit,
}
