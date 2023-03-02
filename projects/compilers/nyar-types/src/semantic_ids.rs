//! Canonical Semantic MIR 的稳定语义身份。
//!
//! 这些 id 是 Semantic MIR 与稀疏 RepresentationPlan 的键，
//! **不是** Wasm 类型下标、CLR token、JVM CP 下标，也不是 Rust `dyn` / `impl Trait`。
//!
//! **禁止：** 用 `function@block:index` 当长期身份、把 EvidenceId 挂在 LegacyCall 上、
//! CallLayout 上帝表，以及解析后的字符串 / `as_str()` 语义分派。

use std::{fmt, marker::PhantomData, num::NonZeroU32};

/// 为 [`SemanticId`] 打品牌，使不同语义域在编译期不可混用。
pub trait IdKind {
    /// [`Display`] 所用的诊断标签；不是运行时分派键。
    const NAME: &'static str;
}

/// 所有带品牌 id 类型共享的稠密、从 1 起编号的语义身份。
///
/// 底层存储为 `NonZeroU32`（保留 0）。表查找使用 [`SemanticId::index`]（从 0 起）。
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SemanticId<K: IdKind> {
    raw: NonZeroU32,
    _kind: PhantomData<fn() -> K>,
}

impl<K: IdKind> SemanticId<K> {
    /// 由从 0 起的稠密表下标构造（`0` → 第一个槽位）。
    pub fn from_index(index: u32) -> Option<Self> {
        NonZeroU32::new(index.saturating_add(1)).map(|raw| Self { raw, _kind: PhantomData })
    }

    /// 由非零原始身份构造。
    pub fn from_raw(raw: NonZeroU32) -> Self {
        Self { raw, _kind: PhantomData }
    }

    /// 用于表查找的从 0 起稠密下标。
    pub fn index(&self) -> u32 {
        self.raw.get() - 1
    }

    /// 非零原始身份。
    pub fn raw(self) -> NonZeroU32 {
        self.raw
    }
}

#[cfg(feature = "serde")]
impl<K: IdKind> serde::Serialize for SemanticId<K> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(self.index())
    }
}

#[cfg(feature = "serde")]
impl<'de, K: IdKind> serde::Deserialize<'de> for SemanticId<K> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let index = u32::deserialize(deserializer)?;
        Self::from_index(index).ok_or_else(|| serde::de::Error::custom(format!("invalid {} index {index}", K::NAME)))
    }
}

impl<K: IdKind> fmt::Debug for SemanticId<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", K::NAME, self.index())
    }
}

impl<K: IdKind> fmt::Display for SemanticId<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", K::NAME, self.index())
    }
}

/// 稳定的 SSA 指令身份（优化器必须保留或显式改写）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstructionIdKind;
impl IdKind for InstructionIdKind {
    const NAME: &'static str = "InstructionId";
}
pub type InstructionId = SemanticId<InstructionIdKind>;

/// 稳定的 SSA 值身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MirValueIdKind;
impl IdKind for MirValueIdKind {
    const NAME: &'static str = "MirValueId";
}
pub type MirValueId = SemanticId<MirValueIdKind>;

/// 已声明的语言项（函数 / 方法 / 构造器 / trait 方法 /
/// std adaptor 入口 / intrinsic 声明）。由包链接器分配。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemIdKind;
impl IdKind for ItemIdKind {
    const NAME: &'static str = "ItemId";
}
pub type ItemId = SemanticId<ItemIdKind>;

/// 程序类型表中的语义类型身份（名义基类型 / 类型参数槽）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeIdKind;
impl IdKind for TypeIdKind {
    const NAME: &'static str = "TypeId";
}
pub type TypeId = SemanticId<TypeIdKind>;

/// 替换后的应用类型身份（`TypeId` × 类型实参）。
/// 不同于 ADT 特化的 [`NominalInstanceId`]。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeInstanceIdKind;
impl IdKind for TypeInstanceIdKind {
    const NAME: &'static str = "TypeInstanceId";
}
pub type TypeInstanceId = SemanticId<TypeInstanceIdKind>;

/// 已链接的项实例（替换后的函数 / 方法 / adaptor 绑定）。
/// 链接器旁表将其关联到 [`ItemId`] + [`SubstitutionId`] + evidence 环境。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemInstanceIdKind;
impl IdKind for ItemInstanceIdKind {
    const NAME: &'static str = "ItemInstanceId";
}
pub type ItemInstanceId = SemanticId<ItemInstanceIdKind>;

/// 具体的名义 ADT 实例（`NominalType × Substitution`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NominalInstanceIdKind;
impl IdKind for NominalInstanceIdKind {
    const NAME: &'static str = "NominalInstanceId";
}
pub type NominalInstanceId = SemanticId<NominalInstanceIdKind>;

/// 由可扩展运算符注册表分配的运算符身份。
///
/// **不是封闭 enum。** 内置与用户定义运算符在解析/链接时从注册表获得稳定 id。
/// 结合性、优先级与词素只存旁表（[`OperatorRegistration`]）；MIR 只存
/// `OperatorId`，从不存 `"infix =="` 这类显示字符串。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OperatorIdKind;
impl IdKind for OperatorIdKind {
    const NAME: &'static str = "OperatorId";
}
pub type OperatorId = SemanticId<OperatorIdKind>;

/// 字节码 / 宿主能力表中的已校验导入槽。
/// 字节码只存此下标，从不存宿主函数名字符串。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImportIndexKind;
impl IdKind for ImportIndexKind {
    const NAME: &'static str = "ImportIndex";
}
pub type ImportIndex = SemanticId<ImportIndexKind>;

/// 名义 sum 内已声明的变体身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VariantIdKind;
impl IdKind for VariantIdKind {
    const NAME: &'static str = "VariantId";
}
pub type VariantId = SemanticId<VariantIdKind>;

/// 结构体 / 聚合 / 变体载荷内已声明的字段身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldIdKind;
impl IdKind for FieldIdKind {
    const NAME: &'static str = "FieldId";
}
pub type FieldId = SemanticId<FieldIdKind>;

/// 泛型替换身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SubstitutionIdKind;
impl IdKind for SubstitutionIdKind {
    const NAME: &'static str = "SubstitutionId";
}
pub type SubstitutionId = SemanticId<SubstitutionIdKind>;

/// 效应操作位点身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EffectSiteIdKind;
impl IdKind for EffectSiteIdKind {
    const NAME: &'static str = "EffectSiteId";
}
pub type EffectSiteId = SemanticId<EffectSiteIdKind>;

/// 效应边身份（`Handled` 或 `Propagate`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EffectEdgeIdKind;
impl IdKind for EffectEdgeIdKind {
    const NAME: &'static str = "EffectEdgeId";
}
pub type EffectEdgeId = SemanticId<EffectEdgeIdKind>;

/// 源码 / 合成 provenance 身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProvenanceIdKind;
impl IdKind for ProvenanceIdKind {
    const NAME: &'static str = "ProvenanceId";
}
pub type ProvenanceId = SemanticId<ProvenanceIdKind>;

/// 运算符结合性（仅句法分类 —— 不是运算符身份）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum OperatorFixity {
    /// 前缀运算符（`!x`、`-x`）。
    Prefix,
    /// 中缀运算符（`a + b`）。
    Infix,
    /// 后缀运算符（`x!`）。
    Postfix,
}

/// 运算符注册表旁表中的一行（解析 / 链接阶段）。
///
/// 将可扩展 [`OperatorId`] 映射到词素 + 结合性 + 优先级 + 已解析 callee。
/// 重复的 `(fixity, lexeme)` 或重复的 `OperatorId` 分配必须失败关闭。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OperatorRegistration {
    /// MIR / 重载所引用的稳定运算符身份。
    pub id: OperatorId,
    /// 仅用于诊断的源词素（`+`、`==`、`>>=`）。解析后不是分派键。
    pub lexeme: String,
    /// 前缀 / 中缀 / 后缀。
    pub fixity: OperatorFixity,
    /// 结合强度（数值越大绑得越紧；具体刻度由语言前端拥有）。
    pub precedence: u16,
    /// 重载 / type-class 选择后的已解析实现（若链接时已知）。
    pub callee: Option<ItemInstanceId>,
}

/// 语言与 emitter 共享的有限内置 / intrinsic 操作。
///
/// 注册一次；后端消费 id，而不是 `builtin.array.push` 这类路径字符串。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum IntrinsicId {
    /// `builtin.array.push` / 向可增长数组存储 push。
    ArrayPush,
    /// 数组 / 列表长度。
    ArrayLen,
    /// 数组存储的下标读取。
    ArrayGet,
    /// 数组存储的下标写入。
    ArraySet,
    /// 引用解引用。
    RefDeref,
    /// 可空 / 引用空检查（表面名 `is_null` 仅迁移期）。
    IsNull,
    /// 可空解包（表面名 `unwrap_null` 仅迁移期）。
    UnwrapNull,
}

impl IntrinsicId {
    /// 写入 `CallIntrinsic` 操作数的稳定稠密下标（与 VM 分派表对齐；不是诊断路径）。
    pub fn bytecode_index(self) -> u32 {
        match self {
            Self::ArrayPush => 0,
            Self::ArrayLen => 1,
            Self::ArrayGet => 2,
            Self::ArraySet => 3,
            Self::RefDeref => 4,
            Self::IsNull => 5,
            Self::UnwrapNull => 6,
        }
    }

    /// 由 `CallIntrinsic` 操作数还原；未知下标 → `None`（verify / 解释器 fail-closed）。
    pub fn from_bytecode_index(index: u32) -> Option<Self> {
        match index {
            0 => Some(Self::ArrayPush),
            1 => Some(Self::ArrayLen),
            2 => Some(Self::ArrayGet),
            3 => Some(Self::ArraySet),
            4 => Some(Self::RefDeref),
            5 => Some(Self::IsNull),
            6 => Some(Self::UnwrapNull),
            _ => None,
        }
    }

    /// 诊断 / 注册表路径段（不是运行时分派键）。
    pub fn diagnostic_path(self) -> &'static str {
        match self {
            Self::ArrayPush => "builtin.array.push",
            Self::ArrayLen => "builtin.array.length",
            Self::ArrayGet => "builtin.array.get",
            Self::ArraySet => "builtin.array.set",
            Self::RefDeref => "builtin.ref.deref",
            Self::IsNull => "builtin.null.is_null",
            Self::UnwrapNull => "builtin.null.unwrap",
        }
    }

    /// 迁移期：由已进入 MIR / executable 的符号路径段映到 [`IntrinsicId`]。
    ///
    /// 仅识别：
    /// - 语言 builtin 路径 `builtin.array.*` / `builtin.ref.deref`
    /// - 私有 `[intrinsic(...)]` 种子名（`__array_len` 等，可带限定前缀）
    ///
    /// **禁止**按 `Array.get` / 类型名末段猜 —— 那会把 `HashMap.get` 等绑错。
    pub fn resolve_from_segments(parts: &[&str]) -> Option<Self> {
        if parts.len() == 3 && parts[0] == "builtin" {
            return match (parts[1], parts[2]) {
                ("array", "push") => Some(Self::ArrayPush),
                ("array", "length") | ("array", "len") => Some(Self::ArrayLen),
                ("array", "get") => Some(Self::ArrayGet),
                ("array", "set") => Some(Self::ArraySet),
                ("ref", "deref") => Some(Self::RefDeref),
                ("null", "is_null") | ("null", "isnull") => Some(Self::IsNull),
                ("null", "unwrap") | ("null", "unwrap_null") => Some(Self::UnwrapNull),
                _ => None,
            };
        }
        match parts.last().copied() {
            Some("__array_len") => Some(Self::ArrayLen),
            Some("__array_get") => Some(Self::ArrayGet),
            Some("__array_set") => Some(Self::ArraySet),
            Some("__ref_deref") => Some(Self::RefDeref),
            // 迁移期单段表面名：MIR 仍可能写出 `is_null` / `unwrap_null`。
            Some("is_null") => Some(Self::IsNull),
            Some("unwrap_null") => Some(Self::UnwrapNull),
            _ => None,
        }
    }
}

impl fmt::Display for IntrinsicId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.diagnostic_path())
    }
}

/// 属性身份：由可扩展属性注册表分配。
///
/// **不是封闭 enum。** 内建 `[export]` / `[main]` 与用户自定义属性均获得稳定 id；
/// 名称只留在 [`AttributeRegistration`] 旁表，解析后不得再用 `as_str() == "export"` 分派。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AttributeIdKind;
impl IdKind for AttributeIdKind {
    const NAME: &'static str = "AttributeId";
}
pub type AttributeId = SemanticId<AttributeIdKind>;

/// 属性注册表旁表中的一行（解析阶段）。
///
/// 重复的属性名或重复的 [`AttributeId`] 分配必须失败关闭。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AttributeRegistration {
    /// MIR / 规划所引用的稳定属性身份。
    pub id: AttributeId,
    /// 源属性简单名（`export`、`main`、用户自定义）；解析后仅诊断，不是分派键。
    pub name: String,
}

/// 内建属性的播种槽位（语言前端注册表先登记这些；其后才是用户属性）。
///
/// 槽位是注册约定，不是语言语义封闭集合。
pub mod builtin_attribute {
    use super::{AttributeId, AttributeRegistration};

    /// `[export]`。
    pub fn export() -> AttributeId {
        AttributeId::from_index(0).expect("export attribute id")
    }

    /// `@main` / `[main]`。
    pub fn main() -> AttributeId {
        AttributeId::from_index(1).expect("main attribute id")
    }

    /// `[test]`。
    pub fn test() -> AttributeId {
        AttributeId::from_index(2).expect("test attribute id")
    }

    /// `[benchmark]`。
    pub fn benchmark() -> AttributeId {
        AttributeId::from_index(3).expect("benchmark attribute id")
    }

    /// 内建属性的初始注册行（供前端 `AttributeRegistry` 播种）。
    pub fn seed_registrations() -> [AttributeRegistration; 4] {
        [
            AttributeRegistration { id: export(), name: "export".into() },
            AttributeRegistration { id: main(), name: "main".into() },
            AttributeRegistration { id: test(), name: "test".into() },
            AttributeRegistration { id: benchmark(), name: "benchmark".into() },
        ]
    }

    /// 由简单名查找已播种的内建 [`AttributeId`]；未知名返回 `None`（应交注册表 intern）。
    pub fn lookup_seed(name: &str) -> Option<AttributeId> {
        match name {
            "export" => Some(export()),
            "main" => Some(main()),
            "test" => Some(test()),
            "benchmark" => Some(benchmark()),
            _ => None,
        }
    }
}

/// 校验映射到 [`ImportIndex`] 之前的外部导入能力声明。
///
/// 模块 / 导出的**链接名**仅用于互通；VM 执行使用 [`ImportIndex`]。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ImportCapability {
    /// 宿主 / 模块链接名。
    pub module_name: String,
    /// 该模块内的导出链接名。
    pub export_name: String,
}

impl ImportCapability {
    /// 构造一对能力名。
    pub fn new(module_name: impl Into<String>, export_name: impl Into<String>) -> Self {
        Self { module_name: module_name.into(), export_name: export_name.into() }
    }
}

impl fmt::Display for ImportCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{}", self.module_name, self.export_name)
    }
}

/// trait/imply evidence 绑定的稳定身份（语义证明，不是运行时 witness）。
///
/// 字符串键只作诊断 / provenance 载体。
/// 调用点不得为 lowering 对 `as_str()` 分支；应按本 id 查旁表。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EvidenceId(String);

impl EvidenceId {
    /// 由预归一化的稳定键构造。
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// 由 trait、实现类型与操作身份生成确定性键。
    pub fn from_parts(trait_id: &str, implementing_type: &str, operation: &str) -> Self {
        if operation.is_empty() {
            Self(format!("evidence:{trait_id}@{implementing_type}"))
        }
        else {
            Self(format!("evidence:{trait_id}@{implementing_type}#{operation}"))
        }
    }

    /// 借用稳定键。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EvidenceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl AsRef<str> for EvidenceId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// 参数化函数声明的稳定身份（不是特化后的物理体）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GenericFunctionId(String);

impl GenericFunctionId {
    /// 由预归一化的稳定键构造。
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// 借用稳定键。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GenericFunctionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// SSA 值如何被定义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MirValueDefinition {
    /// 某条指令的结果槽。
    InstructionResult {
        /// 定义该值的指令。
        instruction: InstructionId,
        /// 该指令内的结果下标。
        result_index: u32,
    },
    /// 基本块参数。
    BlockParameter {
        /// 所属块下标（BlockId 落地前为函数内稠密 id）。
        block_index: u32,
        /// 参数下标。
        parameter_index: u32,
    },
    /// 函数参数。
    FunctionParameter {
        /// 参数下标。
        parameter_index: u32,
    },
}

/// 稀疏的、与目标无关的表示计划。
///
/// 以稳定语义 id 为键；不得复制 CFG，也不得用 `function@block:index` 当长期 identity。
///
/// 键只能是稳定语义 id。禁止 `function@block:index`。
pub mod layout_choice {
    use super::{EffectSiteId, EvidenceId, InstructionId, MirValueId, NominalInstanceId};
    use std::collections::BTreeMap;

    /// 可调用 / apply 位点的布局选择（不是语言范畴）。
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum InvokeLowering {
        /// 对已知项的直接调用。
        Direct,
        /// 类型化 witness 调用。
        TypedWitness,
        /// 共享操作表分派。
        SharedOperationTable,
        /// 特化体。
        Specialized,
        /// 类型化间接 / 函数引用。
        TypedReference,
    }

    /// 值载体表示。
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum ValueRepresentation {
        /// 编译期身份 / 已擦除。
        CompileTimeIdentity,
        /// 特化标量 / 聚合载体。
        Specialized,
        /// 物化的 GC / 托管对象。
        Reified,
        /// 装箱的擦除载体。
        ErasedBoxed,
    }

    /// Evidence 布局选择。
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum EvidenceLayout {
        /// 静态消除。
        Erased,
        /// 显式运行时 witness。
        ExplicitWitness,
        /// 共享类型化操作表。
        SharedOperationTable,
        /// 装箱的 evidence 束。
        Boxed,
    }

    /// ADT 布局选择。
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum AdtRepresentation {
        /// 内联标量 / 带标签载荷（细节在私有计划中）。
        TaggedPayload,
        /// 类型化聚合。
        TypedAggregate,
        /// 装箱值。
        Boxed,
    }

    /// 效应延续布局选择。
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum EffectRepresentation {
        /// 私有计划中的直接状态机编码。
        DirectStateMachine,
        /// 类型化延续对象。
        TypedContinuation,
        /// 装箱帧。
        BoxedFrame,
    }

    /// 与目标无关的稀疏计划。
    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct RepresentationPlan {
        /// 按指令的 invoke / apply 降低。
        pub invoke_lowerings: BTreeMap<InstructionId, InvokeLowering>,
        /// 按值的载体表示。
        pub value_reps: BTreeMap<MirValueId, ValueRepresentation>,
        /// 按 evidence 的布局。
        pub evidence_layouts: BTreeMap<EvidenceId, EvidenceLayout>,
        /// 按名义实例的 ADT 布局。
        pub adt_reps: BTreeMap<NominalInstanceId, AdtRepresentation>,
        /// 按效应位点的延续布局。
        pub effect_reps: BTreeMap<EffectSiteId, EffectRepresentation>,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_id_is_stable_and_distinct() {
        let a = EvidenceId::from_parts("Comparable", "Int32", "compare");
        let b = EvidenceId::from_parts("Comparable", "Int32", "compare");
        let c = EvidenceId::from_parts("Comparable", "Utf8", "compare");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.as_str(), "evidence:Comparable@Int32#compare");
    }

    #[test]
    fn instruction_id_is_dense_and_nonzero() {
        let id = InstructionId::from_index(0).expect("index 0");
        assert_eq!(id.index(), 0);
        assert!(InstructionId::from_index(u32::MAX).is_some() || InstructionId::from_index(u32::MAX).is_none());
    }

    #[test]
    fn representation_plan_keys_are_stable_ids() {
        let mut plan = layout_choice::RepresentationPlan::default();
        let insn = InstructionId::from_index(3).unwrap();
        plan.invoke_lowerings.insert(insn, layout_choice::InvokeLowering::Direct);
        assert!(plan.invoke_lowerings.contains_key(&insn));
    }

    #[test]
    fn item_and_type_instance_ids_are_opaque() {
        let item = ItemId::from_index(0).expect("item");
        let ty = TypeInstanceId::from_index(1).expect("type instance");
        let import = ImportIndex::from_index(0).expect("import");
        assert_eq!(item.index(), 0);
        assert_eq!(ty.index(), 1);
        assert_eq!(import.index(), 0);
    }

    #[test]
    fn branded_ids_are_distinct_types() {
        fn expects_instruction(_: InstructionId) {}
        expects_instruction(InstructionId::from_index(0).unwrap());
        assert_eq!(InstructionIdKind::NAME, "InstructionId");
        assert_eq!(ItemIdKind::NAME, "ItemId");
    }

    #[test]
    fn operator_registry_uses_extensible_opaque_ids() {
        let op = OperatorId::from_index(7).expect("operator");
        assert_eq!(op.index(), 7);
        let reg = OperatorRegistration {
            id: op,
            lexeme: "+".to_string(),
            fixity: OperatorFixity::Infix,
            precedence: 6,
            callee: ItemInstanceId::from_index(0),
        };
        assert_eq!(reg.lexeme, "+");
        assert_eq!(reg.fixity, OperatorFixity::Infix);
    }

    #[test]
    fn intrinsic_and_attribute_ids_are_extensible() {
        assert_eq!(IntrinsicId::ArrayPush.diagnostic_path(), "builtin.array.push");
        assert_eq!(IntrinsicId::ArrayPush.bytecode_index(), 0);
        assert_eq!(IntrinsicId::from_bytecode_index(2), Some(IntrinsicId::ArrayGet));
        assert_eq!(IntrinsicId::from_bytecode_index(99), None);
        assert_eq!(
            IntrinsicId::resolve_from_segments(&["builtin", "array", "push"]),
            Some(IntrinsicId::ArrayPush)
        );
        assert_eq!(IntrinsicId::resolve_from_segments(&["__array_len"]), Some(IntrinsicId::ArrayLen));
        assert_eq!(IntrinsicId::resolve_from_segments(&["marker", "__ref_deref"]), Some(IntrinsicId::RefDeref));
        // 禁止按类型名末段猜
        assert_eq!(IntrinsicId::resolve_from_segments(&["Array", "get"]), None);
        assert_eq!(IntrinsicId::resolve_from_segments(&["HashMap", "get"]), None);
        assert_eq!(IntrinsicId::resolve_from_segments(&["is_null"]), Some(IntrinsicId::IsNull));
        assert_eq!(IntrinsicId::resolve_from_segments(&["unwrap_null"]), Some(IntrinsicId::UnwrapNull));
        assert_eq!(
            IntrinsicId::resolve_from_segments(&["builtin", "null", "is_null"]),
            Some(IntrinsicId::IsNull)
        );
        assert_eq!(builtin_attribute::export().index(), 0);
        assert_eq!(builtin_attribute::main().index(), 1);
        let seeds = builtin_attribute::seed_registrations();
        assert_eq!(seeds[0].name, "export");
        assert_eq!(builtin_attribute::lookup_seed("export"), Some(builtin_attribute::export()));
        assert_eq!(builtin_attribute::lookup_seed("custom_attr"), None);
        let cap = ImportCapability::new("env", "console_log");
        assert_eq!(cap.to_string(), "env::console_log");
    }
}
