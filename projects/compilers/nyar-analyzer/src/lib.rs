#![doc = include_str!("readme.md")]
#![warn(missing_docs)]

/// 前端无关源码格式化 / printer 基础设施（平台层）。
pub mod format;
/// 前端无关语法高亮基础设施（平台层）。
pub mod highlight;
/// 前端无关报告 / SSG / hydrate 岛契约（平台层）。
pub mod report;

use nyar_optimizer::ReferenceManagement;
use nyar_types::{CapabilityTag, ExternalImportLink, Identifier, NamePath, QualifiedName};

/// 中性入口约定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryContract {
    /// 入口符号。
    pub symbol: QualifiedName,
    /// 是否需要宿主包装入口。
    pub requires_wrapper: bool,
}

/// 中性导入约定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportContract {
    /// 待解析的导入路径。
    pub path: NamePath,
    /// 导入绑定到的本地声明。
    pub local_name: QualifiedName,
}

/// 中性导出约定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportContract {
    /// 对外暴露的导出名。
    pub exported_name: Identifier,
    /// 对应本地符号。
    pub local_name: QualifiedName,
    /// CLR / Unity 分区名（如 `unity.runtime`）；`None` 表示默认分区。
    pub partition: Option<String>,
}

/// 运行时需求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRequirement {
    /// 需求键。
    pub key: String,
    /// 需求值。
    pub value: String,
}

/// 单个函数的分析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionAnalysis {
    /// 函数符号。
    pub symbol: QualifiedName,
    /// 是否为外部声明。
    pub is_external: bool,
    /// 是否可能挂起。
    pub can_suspend: bool,
    /// 是否由显式 `.await` 推导为 async 函数。
    pub is_async: bool,
    /// 是否需要宿主交互。
    pub uses_host_interop: bool,
    /// 若函数绑定了外部导入，则保留它的稳定链接描述。
    pub external_import_link: Option<ExternalImportLink>,
    /// 若函数通过 `[host_provider(X)]` 声明为某 host_contract 的实现，则记录目标契约符号。
    pub host_provider_for: Option<QualifiedName>,
    /// 当前函数是否显式要求引用对象管理策略。
    pub reference_management_hint: Option<ReferenceManagement>,
}

/// 单个模块的中性事实集合。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProgramFacts {
    /// 逻辑模块名。
    pub module_name: QualifiedName,
    /// 入口约定列表。
    pub entries: Vec<EntryContract>,
    /// 导入列表。
    pub imports: Vec<ImportContract>,
    /// 导出列表。
    pub exports: Vec<ExportContract>,
    /// 函数分析结果。
    pub functions: Vec<FunctionAnalysis>,
    /// 名义类型定义（enums / unite / flags）。
    pub type_definitions: Vec<TypeDefinitionFact>,
    /// 能力标签。
    pub capabilities: Vec<CapabilityTag>,
    /// 前端显式判定的引用对象管理策略；为空时由目标侧默认值补全。
    pub reference_management: Option<ReferenceManagement>,
    /// 运行时需求。
    pub runtime_requirements: Vec<RuntimeRequirement>,
}

/// 名义类型种类。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeDefinitionKind {
    /// `enums` 声明。
    Enum,
    /// `unite` 声明。
    Unite,
    /// `flags` 声明。
    Flags,
}

/// 变体布局摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeVariantFact {
    /// 变体名。
    pub name: String,
    /// 判别 tag。
    pub tag: i32,
    /// 是否携带 payload。
    pub has_payload: bool,
}

/// 模块级名义类型定义事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDefinitionFact {
    /// 类型名。
    pub name: String,
    /// 种类。
    pub kind: TypeDefinitionKind,
    /// 变体列表（enums/unite）。
    pub variants: Vec<TypeVariantFact>,
    /// flags 成员 `(name, value)`。
    pub flag_members: Vec<(String, i64)>,
}

impl ProgramFacts {
    /// 返回第一个入口约定。
    pub fn primary_entry(&self) -> Option<&EntryContract> {
        self.entries.first()
    }

    /// 判断事实集合是否声明了指定能力。
    pub fn requires_capability(&self, expected: &str) -> bool {
        self.capabilities.iter().any(|capability| capability.as_str() == expected)
    }

    /// 返回是否包含宿主导入。
    pub fn uses_imports(&self) -> bool {
        !self.imports.is_empty()
    }

    /// 基于一组操作符号，返回最细粒度可见的引用对象管理提示。
    pub fn reference_management_for_operations(&self, operations: &[QualifiedName]) -> Option<ReferenceManagement> {
        operations.iter().find_map(|operation| {
            self.functions.iter().find(|function| function.symbol == *operation).and_then(|function| function.reference_management_hint)
        })
    }
}
