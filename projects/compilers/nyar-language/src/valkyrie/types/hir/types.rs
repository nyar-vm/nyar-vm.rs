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

/// 泛型形参种类（类型或函数种类）。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum HirKind {
    /// 类型形参。
    Type,
    /// 函数种类 `domain -> codomain`。
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

/// 结构类型（row）中的方法签名。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RowMethodType {
    /// 方法名。
    pub name: Identifier,
    /// 参数类型列表。
    pub params: Vec<ValkyrieType>,
    /// 返回类型。
    pub return_type: ValkyrieType,
}

/// 结构类型（row）的方法集合。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RowType {
    /// 方法签名列表。
    pub methods: Vec<RowMethodType>,
}

/// 函数类型签名。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FunctionType {
    /// 参数类型列表。
    pub params: Vec<ValkyrieType>,
    /// 返回类型。
    pub return_type: ValkyrieType,
}

/// 类型 lambda（`micro(T) -> U`）的内部表示。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TypeLambda {
    /// 绑定的泛型形参。
    pub params: Vec<GenericType>,
    /// lambda 体类型。
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
    /// 8 位整数。
    Integer8 {
        /// 是否为有符号整数。
        signed: bool,
    },
    /// 16 位整数。
    Integer16 {
        /// 是否为有符号整数。
        signed: bool,
    },
    /// 32 位整数。
    Integer32 {
        /// 是否为有符号整数。
        signed: bool,
    },
    /// 64 位整数。
    Integer64 {
        /// 是否为有符号整数。
        signed: bool,
    },
    /// 128 位整数。
    Integer128 {
        /// 是否为有符号整数。
        signed: bool,
    },
    /// `f32`
    Float32,
    /// `f64`
    Float64,
    /// `char`, the Unicode character
    Character,
    /// UTF-8 字符串类型。
    Utf8,
    /// UTF-16 字符串类型。
    Utf16,
    /// 未绑定声明的表面名（泛型形参、尚未解析符号）。不得单独充当名义身份。
    Named(Identifier),
    /// 待绑定的限定类型路径；绑定阶段按 `namespace` + `name` 冻结为 [`Nominal`]，禁止压成点号字符串。
    QualifiedPath {
        /// 类型路径前缀（不含最终短名）。
        namespace: Vec<String>,
        /// 类型短名。
        name: Identifier,
    },
    /// 已绑定 `ItemId` 的名义类型；`arguments` 为完成代入的类型实参。
    /// Resolver/HIR 成功路径必须使用本变体，禁止把短名改写成限定名再塞回 [`Self::Named`]。
    Nominal {
        /// 名义声明身份。
        declaration: nyar_types::ItemId,
        /// 类型实参列表。
        arguments: Vec<ValkyrieType>,
    },
    /// 类型应用 `F<A, B, ...>`。
    Apply(Box<ValkyrieType>, Vec<ValkyrieType>),
    /// `T<X>`
    Generic(GenericType),
    /// 函数类型。
    Function(Box<FunctionType>),
    /// 元组类型。
    Tuple(Vec<ValkyrieType>),
    /// 结构 row 类型。
    Row(RowType),
    /// `[T]` 堆数组。
    Array(Box<ValkyrieType>),
    /// `[T; N]` 定长数组。
    FixedArray {
        /// 元素类型。
        element: Box<ValkyrieType>,
        /// 固定长度。
        length: usize,
    },
    /// `micro(T) -> U`
    TypeLambda(Box<TypeLambda>),
    /// trait 对象类型。
    TraitObject(TraitObject),
    /// 关联类型投影。
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

impl ValkyrieType {
    /// 诊断用限定路径显示；只投影 `QualifiedPath`，不参与语义匹配。
    pub fn qualified_path_label(namespace: &[String], name: &Identifier) -> String {
        if namespace.is_empty() { name.to_string() } else { format!("{}::{}", namespace.join("::"), name) }
    }

    /// 名义声明身份；仅 [`Self::Nominal`]（及历史上仍包在 `Apply` 里的名义基）携带。
    pub fn nominal_declaration(&self) -> Option<nyar_types::ItemId> {
        match self {
            Self::Nominal { declaration, .. } => Some(*declaration),
            Self::Apply(base, _) => base.nominal_declaration(),
            _ => None,
        }
    }

    /// 名义类型实参；非名义形状返回空切片。
    pub fn nominal_arguments(&self) -> &[ValkyrieType] {
        match self {
            Self::Nominal { arguments, .. } => arguments.as_slice(),
            Self::Apply(_, arguments) => arguments.as_slice(),
            _ => &[],
        }
    }
}

/// 泛型参数由所属声明和源码参数序号唯一标识，拼写只供解析与诊断。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GenericBinderId {
    /// 声明身份（函数、名义类型、trait、imply 或局部泛型声明）。
    pub owner: nyar_types::ItemId,
    /// 在所属声明内的源码顺序。
    pub ordinal: u32,
}

/// 泛型形参表面表示。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GenericType {
    /// 泛型声明的身份；仅解析中的表面节点可以为空。
    pub binder: Option<GenericBinderId>,
    /// 形参名。
    pub name: Identifier,
    /// 形参种类。
    pub kind: HirKind,
    /// trait 约束列表。
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
    /// 非可变参数。
    None,
    /// `..name` ? absorbs extra positional arguments.
    PositionalRest,
    /// `...name` ? absorbs extra keyword arguments.
    KeywordRest,
}

/// 函数或方法形参。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirParam {
    /// 参数标识符。
    pub name: HirIdentifier,
    /// 参数类型。
    pub ty: ValkyrieType,
    /// 调用点绑定模式。
    pub binding_kind: HirParameterBindingKind,
    /// Whether the parameter uses `mut` (including `mut self`).
    #[cfg_attr(feature = "serde", serde(default))]
    pub is_mutable: bool,
    /// 默认值表达式（若有）。
    pub default: Option<HirExpr>,
    /// 可变参数种类。
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

/// 提取器模式（构造器或数组切片）。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum HirExtractorPattern {
    /// 构造器提取器。
    Constructor {
        /// 构造器路径。
        name: crate::NamePath,
        /// 规范化被调路径。
        canonical_callee: crate::NamePath,
        /// 字段子模式。
        fields: Vec<HirPattern>,
        /// 解析后的调用信息。
        resolved: Option<HirResolvedCall>,
    },
    /// 数组切片提取器。
    Array {
        /// 规范化被调路径。
        canonical_callee: crate::NamePath,
        /// 前缀元素模式。
        prefix: Vec<HirPattern>,
        /// 剩余绑定（若有）。
        rest: Option<HirIdentifier>,
        /// 后缀元素模式。
        suffix: Vec<HirPattern>,
        /// 解析后的调用信息。
        resolved: Option<HirResolvedCall>,
    },
}

/// 模式静态可反驳性分类。
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
    /// 判断模式是否可能匹配失败。
    pub fn is_refutable(self) -> bool {
        matches!(self, PatternRefutability::Refutable | PatternRefutability::Unknown)
    }
}

/// HIR 模式匹配节点。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum HirPattern {
    /// 通配符 `_`。
    Wildcard,
    /// 变量绑定。
    Variable(HirIdentifier),
    /// 元组模式。
    Tuple(Vec<HirPattern>),
    /// 字面量模式。
    Literal(HirLiteral),
    /// 范围模式。
    Range {
        /// 起始字面量（可选）。
        start: Option<HirLiteral>,
        /// 结束字面量（可选）。
        end: Option<HirLiteral>,
        /// 结束边界是否包含。
        inclusive_end: bool,
    },
    /// 提取器模式。
    Extractor(HirExtractorPattern),
    /// 或模式 `a | b`。
    Or(Vec<HirPattern>),
    /// 名称路径模式。
    Name(crate::NamePath),
    /// 类型测试模式。
    Type(crate::NamePath),
    /// 带类型注解的绑定。
    TypedBind {
        /// 绑定标识符。
        identifier: HirIdentifier,
        /// 类型路径。
        ty: crate::NamePath,
    },
    /// 对象/结构体字段模式。
    Object {
        /// 类型路径（可选）。
        name: Option<crate::NamePath>,
        /// 命名字段子模式。
        fields: Vec<(Identifier, HirPattern)>,
        /// 剩余字段绑定（若有）。
        rest: Option<HirIdentifier>,
    },
    /// `else` 兜底分支。
    Else,
    /// `whole @ subpat` ? bind whole scrutinee slice and destructure sub-pattern.
    Bind {
        /// 整体绑定标识符。
        identifier: HirIdentifier,
        /// 子模式。
        pattern: Box<HirPattern>,
    },
    /// `mut pat` ? mutable reference binding.
    Mut(Box<HirPattern>),
    /// `pin [mut] pat` ? pinned mutable reference binding.
    Pin {
        /// 是否为 `mut pin`。
        mutable: bool,
        /// 子模式。
        pattern: Box<HirPattern>,
    },
}

impl HirPattern {
    /// 静态推断模式的可反驳性。
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

/// HIR 字面量值。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum HirLiteral {
    /// 64 位整数。
    Integer64(i64),
    /// 64 位浮点数。
    Float64(ordered_float::OrderedFloat<f64>),
    /// 字符串字面量。
    String(HirStringLiteral),
    /// 布尔值。
    Bool(bool),
    /// 单元值 `()`。
    Unit,
}
