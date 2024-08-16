//! 跨阶段结构化诊断合同（语言无关）。
//!
//! 本模块定义编译器产出的结构化诊断行 [`DiagnosticRow`]，它是 Oak / Valkyrie frontend / Nyar / backend
//! 各阶段产出诊断的**唯一语言无关载体**。所有阶段必须通过它暴露诊断，禁止用阶段私有错误类型
//! 携带诊断事实后再 stringify 成 `ParseError::invalid(message)` 之类的 ad-hoc 格式。
//!
//! **所有权边界**：
//! - Oak 生产 `owner = DiagnosticOwner::OakParser`、`phase = DiagnosticPhase::Parsing` 的诊断；
//! - Valkyrie frontend 生产 `owner = DiagnosticOwner::ValkyrieFrontend`、`phase = DiagnosticPhase::Staging | Binding | Hir | Type` 的诊断；
//! - Nyar 生产 `owner = DiagnosticOwner::NyarCompiler`、`phase = DiagnosticPhase::Mir | Link | Representation | Backend | Emit` 的诊断；
//! - Backend contract 错误只能来自 owner = DiagnosticOwner::Backend；
//! - 禁止任何阶段把另一个 owner 的错误包装成自己的——owner 字段必须诚实记录事实生产者。
//!
//! **序列化边界**：
//! - [`DiagnosticRow`] 带 `serde::Serialize + serde::Deserialize`（feature-gated），是 Rust → TypeScript `@vmz/diagnostic` 的唯一 schema；
//! - `args` 必须是结构化 key-value（[`DiagnosticArg`] 枚举），不能是已格式化的字符串；
//! - 同一 [`DiagnosticRow`] 在不同 locale 下必须有相同的 code / args / primary_span / related_spans / severity / owner / phase，
//!   只有 human-facing 的展示文本由 `@vmz/diagnostic` catalog 根据 code 渲染。

use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// 阶段与所有权枚举（必须稳定、可序列化、不依赖语言实现）
// ---------------------------------------------------------------------------

/// 诊断所属编译阶段（pipeline phase）。
///
/// 与 [`CompileStage`]（语义 pipeline 阶段）分离：本类型面向诊断生产入口，
/// 覆盖 AST parsing、staging（template）、binding、type、HIR、MIR、link、representation、
/// backend contract、artifact emit。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DiagnosticPhase {
    /// Oak AST parsing / recovery（语法 / token / source 边界）。
    Parsing,
    /// TGrammar staging / template expansion（常量 / 架构分支 / 循环展开）。
    Staging,
    /// Symbol binding / overload selection（声明可见性、trait 解析、call identity）。
    Binding,
    /// HIR lowering（AST → HIR 结构冻结、identity 分配）。
    Hir,
    /// Type 系统（Kind / Substitution / Evidence / Instance 求解）。
    Type,
    /// Semantic MIR construction / lowering（HIR → Mir 操作）。
    Mir,
    /// Cross-package link（依赖导出、identity 一致性、linker contract）。
    Link,
    /// Representation planning / layout selection（carrier、layout、ABI 准备）。
    Representation,
    /// Backend contract（Wasm/Clr/Native target adaptor、ABI 不完整 / 缺失 carrier）。
    Backend,
    /// Artifact emit / partitioning（二进制构建、产物计划）。
    Emit,
}

/// 诊断所属的事实生产者 owner。
///
/// 这是**不可伪造**字段：生产侧必须如实记录是谁生产了这条诊断，
/// 禁止消费方（如 Compiler 汇总层）在不改变语义的情况下改写 owner。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DiagnosticOwner {
    /// Oak parser（语法、token、source recovery）。
    OakParser,
    /// Valkyrie language frontend（binding、HIR、type、semantic closure）。
    ValkyrieFrontend,
    /// Nyar compiler（MIR、link、representation）。
    NyarCompiler,
    /// Backend contract / target adaptor（ABI、carrier、layout）。
    Backend,
    /// Legion / package manager / workspace resolver（依赖拓扑、manifest、装配）。
    Legion,
}

/// 诊断严重级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DiagnosticSeverity {
    /// 阻断性错误：编译无法继续。
    Error,
    /// 警告：编译可继续但有风险。
    Warning,
    /// 提示信息：编译顺利但有建议。
    Info,
    /// 内联提示：编辑器级 hint（类型推断建议、可简化表达式等）。
    Hint,
}

// ---------------------------------------------------------------------------
// 源码身份与 span（语言无关）
// ---------------------------------------------------------------------------

/// 诊断引用的源码或 artifact 身份。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DiagnosticSource {
    /// 稳定 source identity（仓相对路径、package/source-id、artifact 身份）。
    pub id: String,
    /// 可选 revision/digest（证明身份）。
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub revision: Option<String>,
}

/// 一段源码范围（start/end 字节偏移，0-based 半开区间）。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DiagnosticSpan {
    /// 起始字节偏移（0-based，包含）。
    pub start: usize,
    /// 结束字节偏移（0-based，不包含）。
    pub end: usize,
}

// ---------------------------------------------------------------------------
// 结构化诊断参数（key-value，不预格式化）
// ---------------------------------------------------------------------------

/// 诊断的结构化参数值（不预格式化的 key-value 字典值）。
///
/// 禁止把参数预先拼成 "缺少 trait Hash 类型 K" 这样的句子——这会把语言耦合固化进诊断数据，
/// 导致下游 locale / catalog 无法替换展示文本。参数必须能被 `@vmz/diagnostic` 按类型
/// 正确渲染（TypeName 显示为加粗 / 带 ABI 标记）。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type", content = "value"))]
pub enum DiagnosticArg {
    /// 普通字符串参数（trait 名、符号名、路径片段）。
    String(String),
    /// 整数字面量（discriminator、arity、offset）。
    Int(i64),
    /// 类型名（`TypeName` 渲染为加粗 / 带 ABI 标记）。
    TypeName(String),
    /// 符号引用路径（`["std","collection","Hash"]`）。
    SymbolPath(Vec<String>),
    /// 关联的源码 span（related_span 场景）。
    Span(DiagnosticSpan),
    /// 聚合 / 结构体 / 类名（可能带 namespace）。
    Aggregate(String),
    /// callable 身份短名。
    CallableShort(String),
    /// 依赖组 / source group 身份。
    DependencyKey(String),
}

// ---------------------------------------------------------------------------
// 核心 DiagnosticRow
// ---------------------------------------------------------------------------

/// 语言无关的结构化诊断行（跨 Rust → TypeScript 边界的唯一 schema）。
///
/// 所有编译器阶段的诊断最终必须产出本类型；禁止阶段私有错误类型携带诊断事实后再 stringify。
/// `internal_message` 字段**仅供 Rust 内部调试 / miette 直接展示**——对外（JSON/NDJSON）
/// 下游应优先消费 `code + args + primary_span + related_spans` 并交给 `@vmz/diagnostic` 渲染。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DiagnosticRow {
    /// 稳定机器 code（如 `oak.parse.unexpected_token`、`valkyrie.hir.unresolved_call`、`backend.contract.missing_carrier`）。
    pub code: String,
    /// 严重级别。
    pub severity: DiagnosticSeverity,
    /// 所属编译阶段。
    pub phase: DiagnosticPhase,
    /// 事实生产者 owner（禁止伪造）。
    pub owner: DiagnosticOwner,
    /// 诊断引用的源码或 artifact 身份。
    pub source: DiagnosticSource,
    /// 主定位 span（必须指向 `source`）。
    pub primary_span: DiagnosticSpan,
    /// 相关联的其他 span（如 trait 定义处、冲突声明处）。
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Vec::is_empty"))]
    pub related_spans: Vec<DiagnosticSpan>,
    /// 结构化 key-value 参数（不预格式化）。
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "BTreeMap::is_empty"))]
    pub args: BTreeMap<String, DiagnosticArg>,
    /// 阶段私有附加 notes（可多段）。
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Vec::is_empty"))]
    pub notes: Vec<String>,
    /// 可选 help 提示（catalog 可根据 code 覆盖 / 本地化）。
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub help: Option<String>,
    /// 可选结构化因果链（禁止把底层 error 直接 stringify 进主消息）。
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub cause: Option<String>,
    /// Rust 内部调试用：human-facing 消息（对外 JSON 消费方**不得**依赖此字段——
    /// 应根据 code + args + spans 渲染）。
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub internal_message: Option<String>,
}

impl DiagnosticRow {
    /// 构造诊断行的唯一入口。字段完整性由构造器检查。
    pub fn new(
        code: impl Into<String>,
        severity: DiagnosticSeverity,
        phase: DiagnosticPhase,
        owner: DiagnosticOwner,
        source: DiagnosticSource,
        primary_span: DiagnosticSpan,
    ) -> Self {
        Self {
            code: code.into(),
            severity,
            phase,
            owner,
            source,
            primary_span,
            related_spans: Vec::new(),
            args: BTreeMap::new(),
            notes: Vec::new(),
            help: None,
            cause: None,
            internal_message: None,
        }
    }

    /// 添加相关联的其他 span（如 trait 定义处、冲突声明处）。
    pub fn with_related(mut self, span: DiagnosticSpan) -> Self {
        self.related_spans.push(span);
        self
    }

    /// 批量添加相关 span。
    pub fn with_related_all(mut self, spans: Vec<DiagnosticSpan>) -> Self {
        self.related_spans.extend(spans);
        self
    }

    /// 添加结构化参数（不预格式化）。
    pub fn with_arg(mut self, key: impl Into<String>, value: DiagnosticArg) -> Self {
        self.args.insert(key.into(), value);
        self
    }

    /// 添加 notes 条目。
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// 添加 help 提示。
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// 添加 cause 链（结构化因果，禁止把底层 error 直接 stringify 进主消息）。
    pub fn with_cause(mut self, cause: impl Into<String>) -> Self {
        self.cause = Some(cause.into());
        self
    }

    /// 设置 Rust 内部调试消息（对外消费者**不得**依赖）。
    pub fn with_internal_message(mut self, msg: impl Into<String>) -> Self {
        self.internal_message = Some(msg.into());
        self
    }

    /// 确定性排序 key：先按 code，再按 source.id + primary_span。
    pub fn stable_sort_key(&self) -> String {
        format!("{}|{}|{}|{}", self.code, self.source.id, self.primary_span.start, self.primary_span.end)
    }
}

// ---------------------------------------------------------------------------
// CompileResult：编译器顶层返回类型（语言无关）
// ---------------------------------------------------------------------------

/// 编译器顶层返回类型。
///
/// 成功载荷与失败诊断**分离**——成功时不携带任何诊断（诊断在编译前半段就已放行或被 Compiler 汇总），
/// 失败时只携带诊断行集合，由 CLI 层（@vmz/commander）选择渲染方式。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CompileResult<TArtifact = ()> {
    /// 成功产出。
    Success(TArtifact),
    /// 失败，携带所有结构化诊断行。
    Failure(Vec<DiagnosticRow>),
}

impl<T> CompileResult<T> {
    /// 判断是否成功产出。
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success(_))
    }

    /// 判断是否因诊断失败而中止。
    pub fn is_failure(&self) -> bool {
        matches!(self, Self::Failure(_))
    }

    /// 失败时返回全部诊断行引用；成功返回 None。
    pub fn diagnostics(&self) -> Option<&[DiagnosticRow]> {
        match self {
            Self::Success(_) => None,
            Self::Failure(rows) => Some(rows),
        }
    }

    /// 映射成功载荷类型（失败诊断保持不变）。
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> CompileResult<U> {
        match self {
            Self::Success(t) => CompileResult::Success(f(t)),
            Self::Failure(rows) => CompileResult::Failure(rows),
        }
    }
}

// ---------------------------------------------------------------------------
// 遗留类型（保留给现有 Pipeline trait / CanonicalMirError 等使用；待迁移）
// ---------------------------------------------------------------------------

/// 语义 pipeline 阶段（保留给现有 Pipeline trait 体系；**不要**与 [`DiagnosticPhase`] 混用）。
///
/// [`DiagnosticPhase`] 面向诊断生产入口（覆盖 Parsing / Staging 等），而本类型面向 Semantic MIR pipeline 的
/// 语义阶段划分（M1/M2/Link/Representation 等）。两者并存是渐进式迁移的结果——待所有阶段私有错误类型
/// 废弃后，本类型应被 [`DiagnosticPhase`] 替换。
#[deprecated(
    since = "S1-phase",
    note = "Use DiagnosticPhase + DiagnosticRow for all new diagnostic production. This type remains for legacy Pipeline trait / CanonicalMirError interop and should be migrated out."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompileStage {
    /// Package AST.
    Ast,
    /// HIR elaboration / overload / evidence solving.
    Hir,
    /// Package semantic MIR + SPI (M1).
    SemanticMir,
    /// Cross-package link with selected std adaptors.
    LinkTime,
    /// Validated semantic MIR (M2).
    ValidateMir,
    /// Sparse representation / layout planning.
    RepresentationPlan,
    /// Target-private plan.
    TargetExecutionPlan,
    /// Artifact emit.
    Emit,
}

/// 遗留结构化诊断行（使用 String message / severity；待全量迁移到 [`DiagnosticRow`] 后废弃）。
#[deprecated(
    since = "S1-phase",
    note = "Use DiagnosticRow instead. This legacy type is kept for SemanticClosureError / CanonicalMirError interop and should be migrated out."
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticRecord {
    /// 稳定 machine code（如 `SMIR006`）。
    pub code: String,
    /// 严重级别字符串（`error` / `warning`）。
    pub severity: String,
    /// 遗留 pipeline 阶段身份。
    pub stage: CompileStage,
    /// owning module / package symbol。
    pub module: String,
    /// 人类可读消息（本类型已弃用，不应依赖此字段）。
    pub message: String,
    /// 确定性排序 key。
    pub stable_sort_key: String,
}

/// 遗留 structured diagnostics 集合（待全量迁移到 `Vec<DiagnosticRow>` 后废弃）。
#[deprecated(
    since = "S1-phase",
    note = "Use Vec<DiagnosticRow> instead. This legacy type is kept for Pipeline trait interop."
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredDiagnosticSet {
    /// 遗留诊断记录列表。
    pub records: Vec<DiagnosticRecord>,
}

#[allow(deprecated)]
impl StructuredDiagnosticSet {
    /// 从非空 records 构造；空 Vec 返回 None。
    pub fn from_records(records: Vec<DiagnosticRecord>) -> Option<Self> {
        if records.is_empty() { None } else { Some(Self { records }) }
    }
}

/// 遗留 pipeline 阶段结果别名（待全量迁移到 `Result<T, Vec<DiagnosticRow>>` 后废弃）。
#[deprecated(
    since = "S1-phase",
    note = "Use Result<T, Vec<DiagnosticRow>> instead. This legacy type alias is kept for Pipeline trait interop."
)]
#[allow(deprecated)]
pub type StageResult<T> = Result<T, StructuredDiagnosticSet>;
