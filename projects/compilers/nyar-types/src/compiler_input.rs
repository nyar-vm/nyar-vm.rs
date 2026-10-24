//! Compiler 输入与声明索引的中立数据结构。
//!
//! 本模块承载 Legion / Resolver 与 Valkyrie Compiler 之间的**数据传输合同**：
//! 只拥有结构体形状与纯函数辅助方法，不拥有验证或语义选择逻辑。
//!
//! **生产权声明**：
//! - `CompilerSourceGroup` 的分配（dependency_key）、依赖拓扑排序、入口组身份由 Legion/Resolver 生产；
//! - `DeclarationKey` / `DeclarationKind` / `DeclarationIndexEntry` 由 nyar-language frontend 的 inventory 索引阶段生产；
//! - 结构化验证与错误返回（例如依赖拓扑校验、入口组存在校验、generated 组 producer 非空校验）由 nyar-language 的
//!   frontend lowering 阶段负责，本模块不返回任何语言特化错误类型。

use std::collections::BTreeSet;

// ---------------------------------------------------------------------------
// Compiler 输入结构
// ---------------------------------------------------------------------------

/// 一个具有独立 source span 边界的源文件。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CompilerSourceFile {
    /// 仓相对或逻辑文件身份，仅用于诊断与证据。
    pub path: String,
    /// 文件原文。
    pub source: String,
}

/// Resolver 交给 Compiler 的有序源码语义组。
///
/// 入口身份是显式事实，不由依赖顺序或「最后一组」推断：只有 `is_entry` 组
/// 能让其 `[export]` / `[main]` 声明成为闭包 seed。依赖组即使位于末尾也
/// 不产生入口，缺少 `[export]` / `[main]` 的入口组必须由 frontend 结构化失败。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CompilerSourceGroup {
    /// Resolver 分配的稳定依赖身份。
    pub dependency_key: String,
    /// 诊断和模块身份使用的名称。
    pub name: String,
    /// 生产源码闭包中的独立文件；每个文件必须单独经过 parser。
    pub sources: Vec<CompilerSourceFile>,
    /// 只允许引用已完成链接的直接依赖身份。
    pub direct_dependencies: Vec<String>,
    /// 是否为显式入口组；入口组的闭包 seed 只来自组内 `[export]` / `[main]`。
    pub is_entry: bool,
}

impl CompilerSourceGroup {
    /// 测试与单文件夹具用的最小源码组；默认不是入口组。
    pub fn single_source(dependency_key: &str, name: &str, source: &str) -> Self {
        Self::from_source_files(dependency_key, name, vec![CompilerSourceFile { path: format!("{name}.v"), source: source.into() }], Vec::new())
    }

    /// 多文件输入组；语义选择由 Compiler 生产，调用方不得注入裁剪或摘要。
    pub fn from_source_files(dependency_key: &str, name: &str, sources: Vec<CompilerSourceFile>, direct_dependencies: Vec<String>) -> Self {
        Self { dependency_key: dependency_key.into(), name: name.into(), sources, direct_dependencies, is_entry: false }
    }

    /// 覆盖直接依赖身份集合：仅供依赖顺序已固定的源码组夹具使用。
    #[doc(hidden)]
    pub fn with_direct_dependencies(mut self, direct_dependencies: Vec<String>) -> Self {
        self.direct_dependencies = direct_dependencies;
        self
    }

    /// 显式标记本组为入口组；只有入口组的 `[export]` / `[main]` 能产生闭包 seed。
    pub fn as_entry(mut self) -> Self {
        self.is_entry = true;
        self
    }
}

/// 由独立生产者生成、并作为正式 Compiler 输入的源码组。
///
/// 生成源码不是应用源码组的一部分：它拥有独立 producer 身份、源码身份与摘要，
/// 必须显式标记入口，并且只能追加在应用组之后。装配层不得把生成文本拼进应用组。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GeneratedSourceGroup {
    /// 生成器稳定身份（例：`asgard.awsl.wasm`）；空身份表示来源不可证明。
    pub producer: String,
    /// 生成源码组；必须显式标记入口。
    pub group: CompilerSourceGroup,
}

/// Compiler 的完整有序输入：应用源码组与独立生成源码组。
///
/// 生成组由 [`CompilerSourceInput::compose_groups`] 合成到唯一源码闭包，禁止调用方
/// 只提交应用组而静默丢弃生成源码；没有生成输入的调用方必须显式使用 [`Self::application`]。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CompilerSourceInput {
    /// 按依赖顺序排列的应用源码组。
    pub groups: Vec<CompilerSourceGroup>,
    /// 独立生产者生成、追加在应用组之后的入口源码组。
    pub generated: Vec<GeneratedSourceGroup>,
}

impl CompilerSourceInput {
    /// 只含应用源码的输入（无生成源码）。
    pub fn application(groups: Vec<CompilerSourceGroup>) -> Self {
        Self { groups, generated: Vec::new() }
    }

    /// 把生成组追加到应用组之后，返回唯一有序源码闭包。
    ///
    /// 本方法是纯组合逻辑，不返回语言特化错误类型。调用方应在
    /// 进入 Compiler 之前完成 producer 非空、入口身份、依赖拓扑等验证。
    pub fn compose_groups(&self) -> Vec<CompilerSourceGroup> {
        let mut composed = self.groups.clone();
        for generated in &self.generated {
            composed.push(generated.group.clone());
        }
        composed
    }
}

// ---------------------------------------------------------------------------
// Declaration 索引结构（Legion / nyar-language 之间的中立数据传输合同）
// ---------------------------------------------------------------------------

/// 预 HIR 声明种类；与后续 `ItemId` 分配一一对应，不依赖短名认亲。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DeclarationKind {
    /// 函数（`micro`）声明。
    Micro,
    /// 结构体声明。
    Structure,
    /// 类声明。
    Class,
    /// trait 声明。
    Trait,
    /// enum 声明。
    Enum,
    /// unite 声明。
    Union,
    /// flags 声明。
    Flags,
    /// 类型别名声明。
    TypeAlias,
    /// 类型函数声明。
    TypeFunction,
    /// 成员方法声明。
    Method,
    /// trait 实现方法声明。
    ImplyMethod,
}

impl DeclarationKind {
    /// 返回声明种类的稳定标识字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Micro => "micro",
            Self::Structure => "structure",
            Self::Class => "class",
            Self::Trait => "trait",
            Self::Enum => "enum",
            Self::Union => "union",
            Self::Flags => "flags",
            Self::TypeAlias => "type_alias",
            Self::TypeFunction => "type_function",
            Self::Method => "method",
            Self::ImplyMethod => "imply_method",
        }
    }

    /// 是否携带需要 lowering 的函数体。
    pub fn is_body_bearing(self) -> bool {
        matches!(self, Self::Micro | Self::TypeFunction | Self::Method | Self::ImplyMethod)
    }

    /// 是否为成员声明：成员身份只能由「拥有者聚合身份 + 选择名」产生，不进入短名索引。
    pub fn is_member(self) -> bool {
        matches!(self, Self::Method | Self::ImplyMethod)
    }
}

/// 显式入口注解身份：入口组 seed 的唯一事实来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EntryPointKind {
    /// `[export]`：产物导出入口。
    Export,
    /// `[main]`：程序入口。
    Main,
}

/// 名字引用路径（`std::collection::hash_utf8` → `["std","collection","hash_utf8"]`）。
pub type ReferencePath = Vec<String>;

/// 预 HIR 声明身份：依赖组 + 源文件 + namespace + 种类 + 名字；成员方法另带拥有者聚合名。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DeclarationKey {
    /// Resolver 分配的依赖组身份。
    pub dependency_key: String,
    /// 仓相对源码路径。
    pub source_path: String,
    /// 声明所在 namespace 段；`namespace a::b;` → `["a","b"]`，无 namespace → 空。
    pub namespace: Vec<String>,
    /// 顶层声明种类。
    pub kind: DeclarationKind,
    /// 成员方法所属的结构/类/trait 名；自由声明为 `None`。
    pub owner: Option<String>,
    /// 声明短名。
    pub name: String,
}

impl DeclarationKey {
    /// 稳定编码身份；只用于审核与诊断，身份匹配必须使用结构字段。
    pub fn encoded(&self) -> String {
        format!(
            "{}::{}::{}::{}::{}::{}",
            self.dependency_key,
            self.source_path,
            self.kind.as_str(),
            self.namespace.join("::"),
            self.owner.as_deref().unwrap_or(""),
            self.name
        )
    }
}

/// 声明的结构关联：一个声明在解析后还应指向哪个**声明身份**。
///
/// 成员身份只能由「拥有者聚合身份 + 选择名」产生，类型别名不是独立名义聚合，
/// 因此二者都必须先解析出被指向的声明身份，而不是按名字直接连接。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DeclarationLink {
    /// 成员声明指向其拥有者聚合的引用路径（`imply i64: Hash` → `["i64"]`）。
    MemberOwner(ReferencePath),
    /// 类型别名指向其目标聚合的引用路径（`type utf8 = Utf8Text` → `["Utf8Text"]`）。
    AliasTarget(ReferencePath),
}

/// 单条声明索引：不降低函数体，只记录该声明签名与函数体引用的名字路径。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DeclarationIndexEntry {
    /// 稳定声明身份。
    pub key: DeclarationKey,
    /// 结构关联引用：成员声明指向其拥有者聚合，类型别名指向其目标聚合；自由声明为 `None`。
    ///
    /// 关联必须在整个 inventory 索引完成后按**声明身份**解析，不能按「拥有者短名 + namespace」
    /// 直接连接：成员与其拥有者可以在不同依赖组（`imply i64: Hash` 在 std，`structure i64` 在 core）。
    pub link: Option<DeclarationLink>,
    /// 显式入口身份（`[export]` / `[main]`）；只有它能让入口组产生 seed。
    pub entry_point: Option<EntryPointKind>,
    /// 该声明签名与函数体内出现的名字路径（未解析）。
    pub references: BTreeSet<ReferencePath>,
    /// 该声明签名与函数体内出现的**成员选择名**（`x.get(…)`、`a + b`、`a[i]`）。
    ///
    /// 选择名不是自由名引用：它只能与拥有聚合的声明身份结合参与成员提升，
    /// 不进入声明解析，否则同名自由函数会被误拉进闭包。
    pub selectors: BTreeSet<String>,
    /// 文件级 `using` 路径；当前解析器不消费 import，这里只保留为诊断与后续绑定输入。
    pub import_paths: BTreeSet<String>,
}
