//! 后端私有可执行降低所用的聚合布局合同。
//!
//! 这些类型描述值/引用存储与字段偏移。它们不是
//! 语言级 IR，也不是跨前端的语义总线。

use std::collections::{BTreeMap, BTreeSet};

use crate::NyarType;

/// 可执行指令与后端引用的稳定布局标识。
///
/// 本模块中仍为稠密 `u32` 表下标。语义消费者应优先与 [`crate::FieldId`] /
/// 类型 id 配对；不透明 `LayoutId` newtype 可在 RepresentationPlan 不再依赖
/// 裸下标时再落地。
pub type LayoutId = u32;

/// Physical storage class for aggregate values at lowering time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageKind {
    /// Inline value aggregate: `structure`, tuple, `[T; N]`.
    Value,
    /// Heap / GC reference aggregate: `class`, dynamic `[T]`, trait objects.
    Reference,
}

impl StorageKind {
    /// Returns `true` when this is an inline value aggregate.
    pub fn is_value(self) -> bool {
        matches!(self, Self::Value)
    }

    /// Returns `true` when this is a heap / GC reference aggregate.
    pub fn is_reference(self) -> bool {
        matches!(self, Self::Reference)
    }
}

/// One field slot inside an aggregate layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldLayout {
    /// Field name.
    pub name: String,
    /// Platform field type.
    pub ty: NyarType,
    /// Byte offset within the aggregate.
    pub offset: u32,
    /// Field size in bytes.
    pub size: u32,
    /// Field alignment in bytes.
    pub align: u32,
}

/// 匿名聚合（tuple / 定长数组）的结构化布局键。
///
/// 以平台类型身份区分合成布局，不再用 `__tuple_...` / `__fixedarray_...`
/// 字符串认亲；名义类型恒走 [`AggregateLayoutPlan::declaration_to_layout`]。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LayoutKey {
    /// 匿名 tuple，按元素类型身份区分。
    Tuple(Vec<NyarType>),
    /// 定长数组，按元素类型身份与长度区分。
    FixedArray {
        /// 元素类型。
        element: Box<NyarType>,
        /// 元素个数。
        length: usize,
    },
}

/// Memory layout for one aggregate type (structure, tuple, or fixed array).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregateLayout {
    /// Layout identifier.
    pub id: LayoutId,
    /// Type simple name；合成布局只作诊断显示，不参与身份匹配。
    pub name: String,
    /// Dot-separated namespace (empty for global).
    pub namespace: String,
    /// 合成 tuple / fixedarray 的结构键；名义布局为 `None`。
    pub key: Option<LayoutKey>,
    /// Value vs reference storage class.
    pub storage: StorageKind,
    /// Total size in bytes.
    pub size: u32,
    /// Aggregate alignment in bytes.
    pub align: u32,
    /// Field slots in declaration order.
    pub fields: Vec<FieldLayout>,
}

/// Module-wide layout plan consumed by executable lowering and backends.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AggregateLayoutPlan {
    /// Registered layouts.
    pub layouts: Vec<AggregateLayout>,
    /// 合成 tuple / fixedarray 结构键到布局 id 的映射；名义类型不得写入。
    pub synthetic_layouts: BTreeMap<LayoutKey, LayoutId>,
    /// 结构声明到布局的稳定绑定；语义消费者不得从名称反查布局。
    pub declaration_to_layout: BTreeMap<crate::ItemId, LayoutId>,
    /// 按 declaration 登记的 value 存储结构；禁止靠短名判断 `MirStorageKind::Value`。
    pub value_type_declarations: BTreeSet<crate::ItemId>,
}

/// Sum type variant layout for nominal lowering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SumVariantLayout {
    /// Semantic MIR 冻结的 variant 身份。
    pub id: crate::VariantId,
    /// Variant name.
    pub name: String,
    /// Discriminant tag.
    pub tag: u32,
    /// Optional payload type.
    pub payload_type: Option<NyarType>,
}

/// Sum / enum discriminant layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SumTypeLayout {
    /// Semantic MIR 冻结的 sum nominal 身份。
    pub nominal: crate::NominalInstanceId,
    /// Sum type name.
    pub name: String,
    /// Whether this is a unite-style sum.
    pub is_unite: bool,
    /// Tag width in bits/bytes as used by the backend plan.
    pub tag_width: u32,
    /// Variant layouts.
    pub variants: Vec<SumVariantLayout>,
}

/// Flags bitmask nominal layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlagsLayout {
    /// Flags type name.
    pub name: String,
}

/// Fixed static field name holding the unique singleton instance.
pub const SINGLETON_INSTANCE_FIELD: &str = "INSTANCE";

/// Eager singleton accessor method name (`instance()`).
pub const SINGLETON_EAGER_ACCESSOR: &str = "instance";

/// Lazy singleton accessor method name (`get_instance()`).
pub const SINGLETON_LAZY_ACCESSOR: &str = "get_instance";

/// Method name recognized as the singleton constructor.
pub const SINGLETON_CONSTRUCTOR_NAME: &str = "init";

/// Method name recognized as the singleton finalizer.
pub const SINGLETON_FINALIZER_NAME: &str = "finalize";

/// Lazy singleton unload accessor method name (`unload()`).
pub const SINGLETON_UNLOAD_ACCESSOR: &str = "unload";

/// Describes the lifecycle contract for a singleton's global instance.
///
/// This plan only carries activation mode and fixed symbol names. Concrete
/// field offsets, field types, and receiver shape still come from the
/// singleton's merged aggregate layout.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SingletonInstancePlan {
    /// Singleton type simple name.
    pub name: String,
    /// Dot-separated namespace (empty for global).
    pub namespace: String,
    /// Static field holding the unique instance. Fixed to [`SINGLETON_INSTANCE_FIELD`] by convention.
    pub instance_field: String,
    /// When `false`, available during fragment activation.
    /// When `true`, materialized on first access and supports `unload`.
    pub is_lazy: bool,
    /// Symbol name of the user-defined constructor (`init`), if any.
    pub constructor_symbol: Option<String>,
    /// Symbol name of the user-defined finalizer (`finalize`), if any.
    pub finalizer_symbol: Option<String>,
}

impl SingletonInstancePlan {
    /// Returns the dot-separated qualified name `namespace.name`.
    pub fn qualified_name(&self) -> String {
        if self.namespace.is_empty() { self.name.clone() } else { format!("{}.{}", self.namespace, self.name) }
    }

    /// Returns the accessor method name based on lazy mode.
    pub fn accessor_method(&self) -> &'static str {
        if self.is_lazy { SINGLETON_LAZY_ACCESSOR } else { SINGLETON_EAGER_ACCESSOR }
    }

    /// Returns whether this plan exposes the lazy deactivation transition.
    pub fn supports_unload(&self) -> bool {
        self.is_lazy
    }

    /// Returns the function symbol for the constructor, if any.
    pub fn constructor_mir_symbol(&self) -> Option<String> {
        self.constructor_symbol.clone()
    }

    /// Returns the function symbol for the finalizer, if any.
    pub fn finalizer_mir_symbol(&self) -> Option<String> {
        self.finalizer_symbol.clone()
    }
}

// 已删除 `layout_key_for_nyar_type` / `nyar_type_layout_key_component`：
// 它们把 `NyarType::Named` 短名直接当成 layout 键，是「按名称回查布局」的来源。
// 名义类型只能经已冻结声明身份解析；合成 tuple / fixedarray 键由前端
// 以 [`LayoutKey`] 的结构化类型身份生产。
