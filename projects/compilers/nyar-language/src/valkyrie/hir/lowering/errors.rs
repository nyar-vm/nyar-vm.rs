//! HIR lowering 阶段的结构化错误（模块内部）。
//!
//! 本模块定义 HirError——HIR lowering 逻辑块的**唯一错误类型**。它取代 lowering/mod.rs
//! 里原先散布的 ParseError::invalid(...) ad-hoc 字符串错误。HirError 不拥有诊断合同：
//! 它实现 `Into<DiagnosticRow>` 时自动填充 code / phase=Hir / owner=ValkyrieFrontend，
//! 把结构化字段映射到 args BTreeMap。
//!
//! # 为什么不放进 nyar-types
//! nyar-types 只拥有中立诊断合同（DiagnosticRow）。phase-specific error 类型属于产出它们
//! 的模块内部实现细节——HirError 的 variant 命名、code 生成规则、args 映射逻辑都是 HIR
//! lowering 逻辑块的语义知识，不应污染中立 crate。
//!
//! # 迁移顺序
//! 1. 定义 HirError variant（本文件）
//! 2. 在 lowering/mod.rs 的 lowering 阶段替换 ParseError::invalid → return Err(HirError::...)
//! 3. 编译验证 + 跑既有测试确认零新回归
//! 4. 再推进 ResolveError / StageError / TypeError

use nyar_types::{
    DiagnosticArg, DiagnosticOwner, DiagnosticPhase, DiagnosticRow, DiagnosticSeverity, DiagnosticSource, DiagnosticSpan,
};

/// HIR lowering 阶段的结构化失败原因。
///
/// 每个 variant 对应 lowering 逻辑块里**一个语义场景**，携带结构化字段（不是已格式化的 String）。
/// 转换成 DiagnosticRow 时：
/// - phase 固定为 DiagnosticPhase::Hir
/// - owner 固定为 DiagnosticOwner::ValkyrieFrontend
/// - severity 固定为 DiagnosticSeverity::Error
/// - code 由 variant 确定性生成
/// - variant 字段映射到 args BTreeMap
// 该错误类型承载上游 `ParseError`（不可 `Clone`），因此不派生 `Clone` / `PartialEq`：
// 失败事实由各自生产者持有，HIR 只读转发。
#[derive(Debug)]
pub enum HirError {
    /// SMIR003: call contract 未解析（选择名 + 调用上下文）。
    UnresolvedCall {
        /// 未解析的选择名（如 `as_range`、`+`、`[]`）。
        selector: String,
        /// 失败所在的源码片段 / 函数名。
        context: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// SMIR003: constructor contract 未解析。
    UnresolvedConstructor {
        /// 未解析的构造名。
        name: String,
        /// 失败所在的源码片段。
        context: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 名义类型短名在作用域内歧义，无法冻结 declaration identity。
    NominalAmbiguous {
        /// 歧义名义类型名。
        name: String,
        /// 候选声明数量。
        candidate_count: usize,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// sum/unite 声明在作用域内歧义。
    SumUniteAmbiguous {
        /// 歧义声明名。
        name: String,
        /// 候选声明数量。
        candidate_count: usize,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 结构缺少 constructor_declaration，无法绑定名义类型。
    StructureMissingConstructor {
        /// 结构聚合全名（namespace::name）。
        qualified_name: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// sum/unite 缺少 declaration，无法绑定名义类型。
    SumUniteMissingDeclaration {
        /// 声明名。
        name: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 分配 discriminator 失败：必须是非负整数字面量 / 不能重复 tag。
    InvalidDiscriminator {
        /// 所属聚合名。
        aggregate: String,
        /// 失败场景：`not_non_negative_integer` / `duplicate_tag`。
        scenario: String,
        /// 涉及的变体名（None 表示 aggregate 级问题）。
        variant: Option<String>,
        /// 具体值（None 表示未知）。
        value: Option<String>,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 源码组没有 body-lowering 可处理的源文件。
    NoBodySourceInGroup {
        /// 空组依赖身份。
        dependency_key: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// MIR 校验拒绝 MIR 自己产出的结构化诊断。
    ///
    /// 该变体只承载 MIR lowering 已冻结的结构化诊断事实，不在 HIR 侧重新拼装场景字符串。
    MirDiagnosticRejected {
        /// MIR lowering 产出的原始诊断。
        diagnostic: crate::mir::MirDiagnostic,
    },
    /// Semantic MIR 合同校验失败（由 `SemanticMirContractError` 生产）。
    MirContractFailed(crate::valkyrie::mir::validation::SemanticMirContractError),
    /// MIR 控制流图校验失败。
    ///
    /// 控制流校验器当前仍以 `ParseError` 生产失败事实，本变体原样转发其结构化错误，
    /// 不由 HIR 重新解释或字符串化。
    MirControlFlowRejected(crate::valkyrie::frontend::ParseError),
    /// MIR 函数缺少 entry block。
    MirEntryBlockMissing {
        /// 函数符号。
        function: String,
    },
    /// dispatch block 引用了函数未冻结的 state 值。
    MirUnknownStateValue {
        /// 函数符号。
        function: String,
        /// dispatch block 标签。
        block: String,
    },
    /// frontend 解析 / 暂存阶段失败。
    ///
    /// 原样转发 frontend 生产的 `ParseError`（parser、staging 边界的事实所有者），
    /// HIR 不重新拼装解析消息。
    FrontendRejected(crate::valkyrie::frontend::ParseError),
    /// TGrammar 模板展开失败。
    TemplateExpansionFailed {
        /// 触发失败的模板构造。
        construct: TemplateConstruct,
        /// 结构化失败原因。
        failure: TemplateFailure,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// `match` 源码恢复失败。
    MatchRecoveryFailed {
        /// 恢复阶段。
        stage: MatchRecoveryStage,
        /// 涉及的 pattern 文本（仅 pattern 恢复阶段携带）。
        pattern: Option<String>,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 可打断控制流的操作出现在不允许它的上下文。
    ControlFlowContextViolation {
        /// 违规操作。
        operation: ControlFlowOp,
        /// 该操作要求的上下文。
        required: RequiredContext,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// `fallthrough` 所在的 `case` arm 没有后继 arm。
    FallthroughWithoutNextArm {
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// `fallthrough` 不在 `case` statement 体系内。
    FallthroughOutsideCaseChain {
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// `fallthrough` 非法引用上一 arm 的 pattern 绑定。
    FallthroughBindingLeak {
        /// 被非法引用的绑定名。
        name: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// `break` / `continue` 找不到目标 loop。
    LoopTargetMissing {
        /// 跳转操作。
        operation: LoopJumpOperation,
        /// `break` 是否携带值。
        carries_value: bool,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 目标 loop 当前不接受 `break` 值。
    BreakValueRejected {
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// `break` 值类型与目标 loop 已收敛的结果类型不兼容。
    BreakValueTypeMismatch {
        /// 实际值类型。
        actual: String,
        /// 目标 loop 已收敛的结果类型。
        expected: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// `?` 的操作数不实现可传播协议（nullable / `Option` / `Result`）。
    TryPropagateOperandUnsupported {
        /// 操作数类型。
        operand_type: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// `?` 所在函数的返回类型与操作数协议不匹配，且不在 `try` 作用域内。
    TryPropagateReturnMismatch {
        /// 操作数要求的协议。
        protocol: PropagateProtocol,
        /// 当前函数返回类型。
        return_type: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// `await` / `awake` / `block` 的操作数不是 `Future<T>` / `Promise<T>`。
    FutureControlOperandMismatch {
        /// 违规操作。
        operation: ControlFlowOp,
        /// 操作数类型。
        operand_type: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// `return` / `yield` 缺少上下文要求的 carried value。
    ControlFlowValueRequired {
        /// 缺少值的操作。
        operation: ControlFlowOp,
        /// 上下文期望的类型。
        expected_type: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// `return expr` 的值类型与函数返回类型不兼容。
    ReturnValueTypeMismatch {
        /// 实际值类型。
        actual: String,
        /// 函数返回类型。
        expected: String,
        /// 所在函数名（未知时为空）。
        function: Option<String>,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// `yield expr` 的值类型与生成器元素类型不兼容。
    YieldValueTypeMismatch {
        /// 实际值类型。
        actual: String,
        /// 函数返回类型。
        expected_return: String,
        /// 生成器 yield 元素类型。
        item_type: String,
        /// 所在函数名（未知时为空）。
        function: Option<String>,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// `yield from` 的操作数不是生成器协议。
    YieldFromOperandNotGenerator {
        /// 操作数类型。
        operand_type: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// `yield from` 的元素类型与当前函数的 yield 元素类型不兼容。
    YieldFromItemTypeMismatch {
        /// 被委托生成器的元素类型。
        source_item: String,
        /// 当前函数返回类型。
        expected_return: String,
        /// 当前函数 yield 元素类型。
        expected_item: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 单段小写裸名字模式与变量绑定歧义。
    AmbiguousBareNamePattern {
        /// 歧义模式名。
        name: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 生成源码组缺少显式 producer 身份。
    SourceGroupProducerMissing {
        /// 涉及的源码组依赖身份（未知时为 `None`）。
        dependency_key: Option<String>,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 生成源码组没有显式标记为入口。
    SourceGroupNotEntry {
        /// 源码组依赖身份。
        dependency_key: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 源码组依赖身份为空或与已提供组重复。
    SourceGroupIdentityInvalid {
        /// 请求的依赖身份（为空时携带空串）。
        dependency_key: String,
        /// 身份是否为空（否则为重复）。
        empty: bool,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 源码组依赖的组尚未在之前提供。
    SourceGroupDependencyUnavailable {
        /// 消费依赖的组。
        group: String,
        /// 缺失的依赖组。
        dependency: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 组内源码文件身份为空或重复。
    SourceFileIdentityInvalid {
        /// 所属组依赖身份。
        group: String,
        /// 请求的文件路径（为空时携带空串）。
        path: String,
        /// 身份是否为空（否则为重复）。
        empty: bool,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 入口组没有任何源码文件。
    EntryGroupWithoutFiles {
        /// 入口组依赖身份。
        group: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// Compiler 源码快照没有入口组（或缺少显式入口组）。
    SourceInputEntryGroupMissing {
        /// 是否缺少显式入口标记（否则快照完全为空）。
        explicit: bool,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 读取源码文件失败。
    SourceReadFailed {
        /// 请求的文件路径。
        path: String,
        /// 底层 IO 失败种类。
        kind: std::io::ErrorKind,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 语义源码组计划为空，无法确定链接根。
    EmptySemanticSourceGroupPlan {
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// frontend 拒绝源文件；原样转发其结构化解析失败。
    SourceFileRejected {
        /// 所属组依赖身份。
        dependency_key: String,
        /// 被拒绝的文件路径。
        path: String,
        /// frontend 生产的结构化解析失败。
        error: crate::valkyrie::frontend::ParseError,
    },
    /// 依赖导出的语义事实不可用。
    DependencyExportUnavailable {
        /// 消费依赖的组。
        group: String,
        /// 缺失的依赖组。
        dependency: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 同一依赖身份被重复注册语义导出。
    DependencyExportIdentityCollision {
        /// 冲突的依赖身份。
        dependency_key: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 声明 / 实例 / 字段身份空间耗尽或索引非法。
    DeclarationIndexExhausted {
        /// 耗尽的索引空间。
        space: IndexSpace,
        /// 正在注册的声明所有者。
        owner: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 已分配的声明 / 实例 / 字段身份被重复注册。
    DeclarationIdentityDuplicated {
        /// 冲突的索引空间。
        space: IndexSpace,
        /// 声明所有者。
        owner: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 依赖声明的身份注册必须由完整 Compiler 源码闭包拥有。
    DependencyDeclarationsRequireSourceClosure {
        /// 已提供的依赖导出数量。
        imported_export_count: usize,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 命名 `union` 尚未进入 HIR lowering。
    UnionNotLowered {
        /// union 声明名。
        union_name: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 必须在宏/模板展开后消失的 TGrammar 模板残留到 HIR lowering。
    UnexpandedTemplateItem {
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 根条目类型不属于 HIR lowering 的输入面。
    UnsupportedRootItem {
        /// Oak 根条目的稳定种类名。
        item_kind: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 下游 compile pipeline 拒绝已 lowering 的输入。
    ///
    /// 原样转发下游已冻结的诊断 code 与 module 身份，不在 HIR 侧重新拼装失败文本。
    PipelineRejected {
        /// 下游诊断 code 列表。
        codes: Vec<String>,
        /// owning module 符号。
        module: String,
    },
    /// HIR lowering 失败（通用 fallback，携带已解析的结构化信息）。
    LoweringFailed {
        /// 场景标识。
        scenario: String,
        /// 附加细节（禁止预格式化——放入 args）。
        details: Vec<(String, String)>,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// 类型 lowering 时遇到无法解析的类型短名。
    ///
    /// 既不是当前作用域内已声明的 type 别名，也不是语言内建（`bool`/`utf8`/`unit`/`i64` 等）。
    /// 这种名字以前被错误地 fallback 成 `ValkyrieType::Named` 传递到 canonical 阶段，
    /// 但 canonical 已对 Named 直接返回 CAN003——现在在 lowering 阶段就结构化失败。
    TypeNotFound {
        /// 未解析的类型短名。
        name: String,
        /// 诊断引用的源码或 artifact 身份。
        source: DiagnosticSource,
        /// 诊断主定位 span。
        span: DiagnosticSpan,
    },
    /// `.vx` source 暂不支持。
    VxUnsupported,
    /// 源码组语义闭包失败：入口声明缺失、HostProvider 未解析等。
    /// 直接携带上游 `SemanticClosureError`，不在 lowering 阶段降级成字符串。
    SourceGroupClosureFailed(crate::valkyrie::semantic_closure::SemanticClosureError),
    /// DeriveInjector 注入失败：字段类型缺 trait、冲突等。
    /// 直接携带上游 `DeriveError` 列表，不在 lowering 阶段拼接成字符串。
    DeriveResolutionFailed {
        /// 结构化 derive 错误列表。
        errors: Vec<crate::valkyrie::derive::DeriveError>,
    },
}

/// TGrammar 模板构造类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateConstruct {
    /// 元表达式 `<% ... %>`。
    MetaExpression,
    /// `loop _ in start..end` 常量范围展开。
    Loop,
    /// 按 `arch` 选择分支的 `match`。
    Match,
    /// 块内模板片段展开。
    BlockFragment,
}

/// TGrammar 模板展开的结构化失败原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateFailure {
    /// 元表达式为空。
    EmptyExpression,
    /// 表达式后仍有无法求值的语法。
    TrailingTokens,
    /// 表达式在未读完时结束。
    IncompleteExpression,
    /// 括号未闭合。
    UnclosedParenthesis,
    /// 出现无法求值的 token。
    UnevaluableToken,
    /// 整数字面量无法解析成 `i64`。
    IntegerLiteralUnparsable,
    /// 比较两侧类型不兼容。
    IncompatibleComparisonOperands,
    /// `loop` 头部短于 `_ in start..end`。
    LoopHeaderTooShort,
    /// `loop` 绑定名不是丢弃绑定 `_`。
    LoopBindingNotDiscard,
    /// `loop` 头部缺少 `in` 关键字。
    LoopMissingInKeyword,
    /// `loop` 头部在范围表达式之后仍有无法求值的语法。
    LoopHeaderTrailingTokens,
    /// `loop` 缺少范围表达式。
    LoopRangeMissing,
    /// `loop` 范围不是 `start..end` 形态。
    RangeShapeUnsupported,
    /// 范围端点不是整数字面量。
    RangeEndpointNotInteger,
    /// 范围起点大于终点。
    LoopRangeInvalid {
        /// 起点。
        start: i64,
        /// 终点。
        end: i64,
    },
    /// `match` 的 scrutinee 不是 `arch`。
    NonArchScrutinee,
    /// 展开结果不能作为块内语句保留。
    BlockFragmentUnsupportedNode {
        /// 无法保留的节点类别（稳定短名，不是 `Debug` 输出）。
        kind: String,
    },
    /// 元表达式重新解析后不是单个 `micro` 声明。
    FragmentNotMicroDeclaration {
        /// 重新解析产出的顶层项数量。
        produced_items: usize,
    },
}

/// `match` 源码恢复阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchRecoveryStage {
    /// 从源码 span 恢复 `match` 块内部文本。
    BlockInterior,
    /// `match` 块没有可恢复的 arm。
    Arms,
    /// 恢复 arm 的 pattern。
    Pattern,
    /// arm body 为空。
    ArmBody,
    /// arm body 不是表达式。
    ArmBodyExpression,
}

/// 可能打断控制流连续性的操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlFlowOp {
    /// `await`。
    Await,
    /// `awake`。
    Awake,
    /// `block`。
    Block,
    /// `yield`。
    Yield,
    /// `yield from`。
    YieldFrom,
    /// `resume`。
    Resume,
    /// `?`。
    TryPropagate,
    /// `fallthrough`。
    Fallthrough,
    /// `break`。
    Break,
    /// `continue`。
    Continue,
    /// `return`。
    Return,
}

/// 控制流操作要求的合法上下文。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequiredContext {
    /// 必须位于 `catch arm body`。
    CatchArmBody,
    /// 不得位于 guard 中。
    OutsideGuard,
    /// 必须位于 async 作用域。
    AsyncScope,
    /// 必须位于允许阻塞的作用域。
    BlockingScope,
    /// 必须位于生成器或等价 handler 作用域。
    GeneratorScope,
}

/// loop 跳转操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopJumpOperation {
    /// `break`。
    Break,
    /// `continue`。
    Continue,
}

/// `?` 操作数要求的传播协议。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropagateProtocol {
    /// nullable 类型。
    Nullable,
    /// `Option<T>`。
    Option,
    /// `Result<T, E>`。
    Result,
}

/// 声明身份索引空间。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexSpace {
    /// `ItemId` 声明空间。
    Item,
    /// `ItemInstanceId` 实例空间。
    ItemInstance,
    /// `FieldId` 字段空间。
    Field,
}

impl HirError {
    /// Lowering 阶段通用 fallback（来自非法状态 / 前置条件失败）。
    pub fn lowering_failed(message: impl Into<String>) -> Self {
        Self::LoweringFailed {
            scenario: message.into(),
            details: Vec::new(),
            source: nyar_types::DiagnosticSource { id: "<unknown>".into(), revision: None },
            span: nyar_types::DiagnosticSpan { start: 0, end: 0 },
        }
    }

    /// 带显式 span 的 lowering fallback。
    pub fn lowering_failed_at(message: impl Into<String>, span: nyar_types::DiagnosticSpan) -> Self {
        Self::LoweringFailed {
            scenario: message.into(),
            details: Vec::new(),
            source: nyar_types::DiagnosticSource { id: "<unknown>".into(), revision: None },
            span,
        }
    }

    /// TGrammar 模板展开失败；span 指向触发失败的模板 token 流。
    pub(crate) fn template_expansion_failed(construct: TemplateConstruct, failure: TemplateFailure, span: DiagnosticSpan) -> Self {
        Self::TemplateExpansionFailed {
            construct,
            failure,
            source: DiagnosticSource { id: "<template>".into(), revision: None },
            span,
        }
    }

    /// `match` 源码恢复失败；span 指向被恢复的 `match` 块。
    pub(crate) fn match_recovery_failed(stage: MatchRecoveryStage, pattern: Option<String>, span: DiagnosticSpan) -> Self {
        Self::MatchRecoveryFailed {
            stage,
            pattern,
            source: DiagnosticSource { id: "<match>".into(), revision: None },
            span,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::UnresolvedCall { .. } => "valkyrie.hir.unresolved_call",
            Self::UnresolvedConstructor { .. } => "valkyrie.hir.unresolved_constructor",
            Self::NominalAmbiguous { .. } => "valkyrie.hir.nominal_ambiguous",
            Self::SumUniteAmbiguous { .. } => "valkyrie.hir.sum_unite_ambiguous",
            Self::StructureMissingConstructor { .. } => "valkyrie.hir.structure_missing_constructor",
            Self::SumUniteMissingDeclaration { .. } => "valkyrie.hir.sum_unite_missing_declaration",
            Self::InvalidDiscriminator { .. } => "valkyrie.hir.invalid_discriminator",
            Self::NoBodySourceInGroup { .. } => "valkyrie.hir.no_body_source",
            Self::MirDiagnosticRejected { .. } => "valkyrie.mir.diagnostic_rejected",
            Self::MirContractFailed(_) => "valkyrie.mir.contract_failed",
            Self::MirControlFlowRejected(_) => "valkyrie.mir.control_flow_rejected",
            Self::MirEntryBlockMissing { .. } => "valkyrie.mir.entry_block_missing",
            Self::MirUnknownStateValue { .. } => "valkyrie.mir.unknown_state_value",
            Self::FrontendRejected(_) => "valkyrie.frontend.rejected",
            Self::TemplateExpansionFailed { .. } => "valkyrie.hir.template_expansion_failed",
            Self::MatchRecoveryFailed { .. } => "valkyrie.hir.match_recovery_failed",
            Self::ControlFlowContextViolation { .. } => "valkyrie.hir.control_flow_context_violation",
            Self::FallthroughWithoutNextArm { .. } => "valkyrie.hir.fallthrough_without_next_arm",
            Self::FallthroughOutsideCaseChain { .. } => "valkyrie.hir.fallthrough_outside_case_chain",
            Self::FallthroughBindingLeak { .. } => "valkyrie.hir.fallthrough_binding_leak",
            Self::LoopTargetMissing { .. } => "valkyrie.hir.loop_target_missing",
            Self::BreakValueRejected { .. } => "valkyrie.hir.break_value_rejected",
            Self::BreakValueTypeMismatch { .. } => "valkyrie.hir.break_value_type_mismatch",
            Self::TryPropagateOperandUnsupported { .. } => "valkyrie.hir.try_propagate_operand_unsupported",
            Self::TryPropagateReturnMismatch { .. } => "valkyrie.hir.try_propagate_return_mismatch",
            Self::FutureControlOperandMismatch { .. } => "valkyrie.hir.future_control_operand_mismatch",
            Self::ControlFlowValueRequired { .. } => "valkyrie.hir.control_flow_value_required",
            Self::ReturnValueTypeMismatch { .. } => "valkyrie.hir.return_value_type_mismatch",
            Self::YieldValueTypeMismatch { .. } => "valkyrie.hir.yield_value_type_mismatch",
            Self::YieldFromOperandNotGenerator { .. } => "valkyrie.hir.yield_from_operand_not_generator",
            Self::YieldFromItemTypeMismatch { .. } => "valkyrie.hir.yield_from_item_type_mismatch",
            Self::AmbiguousBareNamePattern { .. } => "valkyrie.hir.ambiguous_bare_name_pattern",
            Self::SourceGroupProducerMissing { .. } => "valkyrie.hir.source_group_producer_missing",
            Self::SourceGroupNotEntry { .. } => "valkyrie.hir.source_group_not_entry",
            Self::SourceGroupIdentityInvalid { .. } => "valkyrie.hir.source_group_identity_invalid",
            Self::SourceGroupDependencyUnavailable { .. } => "valkyrie.hir.source_group_dependency_unavailable",
            Self::SourceFileIdentityInvalid { .. } => "valkyrie.hir.source_file_identity_invalid",
            Self::EntryGroupWithoutFiles { .. } => "valkyrie.hir.entry_group_without_files",
            Self::SourceInputEntryGroupMissing { .. } => "valkyrie.hir.source_input_entry_group_missing",
            Self::SourceReadFailed { .. } => "valkyrie.hir.source_read_failed",
            Self::EmptySemanticSourceGroupPlan { .. } => "valkyrie.hir.empty_semantic_source_group_plan",
            Self::SourceFileRejected { .. } => "valkyrie.hir.source_file_rejected",
            Self::DependencyExportUnavailable { .. } => "valkyrie.hir.dependency_export_unavailable",
            Self::DependencyExportIdentityCollision { .. } => "valkyrie.hir.dependency_export_identity_collision",
            Self::DeclarationIndexExhausted { .. } => "valkyrie.hir.declaration_index_exhausted",
            Self::DeclarationIdentityDuplicated { .. } => "valkyrie.hir.declaration_identity_duplicated",
            Self::DependencyDeclarationsRequireSourceClosure { .. } => "valkyrie.hir.dependency_declarations_require_source_closure",
            Self::UnionNotLowered { .. } => "valkyrie.hir.union_not_lowered",
            Self::UnexpandedTemplateItem { .. } => "valkyrie.hir.unexpanded_template_item",
            Self::UnsupportedRootItem { .. } => "valkyrie.hir.unsupported_root_item",
            Self::PipelineRejected { .. } => "valkyrie.hir.pipeline_rejected",
            Self::LoweringFailed { .. } => "valkyrie.hir.lowering_failed",
            Self::TypeNotFound { .. } => "valkyrie.hir.type_not_found",
            Self::VxUnsupported => "valkyrie.hir.vx_unsupported",
            Self::SourceGroupClosureFailed(_) => "valkyrie.hir.source_group_closure_failed",
            Self::DeriveResolutionFailed { .. } => "valkyrie.hir.derive_resolution_failed",
        }
    }
}

impl From<HirError> for DiagnosticRow {
    fn from(error: HirError) -> Self {
        let source = match &error {
            HirError::VxUnsupported => DiagnosticSource { id: ".vx".into(), revision: None },
            other => other_source(other),
        };
        let span = match &error {
            HirError::VxUnsupported => DiagnosticSpan { start: 0, end: 0 },
            other => other_span(other),
        };

        let mut row = DiagnosticRow::new(
            error.code(),
            DiagnosticSeverity::Error,
            DiagnosticPhase::Hir,
            DiagnosticOwner::ValkyrieFrontend,
            source,
            span,
        );

        match &error {
            HirError::UnresolvedCall { selector, context, .. } => {
                row = row
                    .with_arg("selector", DiagnosticArg::String(selector.clone()))
                    .with_arg("context", DiagnosticArg::String(context.clone()));
            }
            HirError::UnresolvedConstructor { name, context, .. } => {
                row = row
                    .with_arg("name", DiagnosticArg::Aggregate(name.clone()))
                    .with_arg("context", DiagnosticArg::String(context.clone()));
            }
            HirError::NominalAmbiguous { name, candidate_count, .. } => {
                row = row
                    .with_arg("name", DiagnosticArg::TypeName(name.clone()))
                    .with_arg("candidate_count", DiagnosticArg::Int(*candidate_count as i64));
            }
            HirError::SumUniteAmbiguous { name, candidate_count, .. } => {
                row = row
                    .with_arg("name", DiagnosticArg::Aggregate(name.clone()))
                    .with_arg("candidate_count", DiagnosticArg::Int(*candidate_count as i64));
            }
            HirError::StructureMissingConstructor { qualified_name, .. } => {
                row = row.with_arg("aggregate", DiagnosticArg::Aggregate(qualified_name.clone()));
            }
            HirError::SumUniteMissingDeclaration { name, .. } => {
                row = row.with_arg("aggregate", DiagnosticArg::Aggregate(name.clone()));
            }
            HirError::InvalidDiscriminator { aggregate, scenario, variant, value, .. } => {
                row = row
                    .with_arg("aggregate", DiagnosticArg::Aggregate(aggregate.clone()))
                    .with_arg("scenario", DiagnosticArg::String(scenario.clone()));
                if let Some(v) = variant {
                    row = row.with_arg("variant", DiagnosticArg::CallableShort(v.clone()));
                }
                if let Some(v) = value {
                    row = row.with_arg("value", DiagnosticArg::String(v.clone()));
                }
            }
            HirError::NoBodySourceInGroup { dependency_key, .. } => {
                row = row.with_arg("dependency_key", DiagnosticArg::DependencyKey(dependency_key.clone()));
            }
            HirError::MirDiagnosticRejected { diagnostic } => {
                row = mir_diagnostic_args(row, diagnostic);
            }
            HirError::MirContractFailed(error) => {
                row = row
                    .with_arg("code", DiagnosticArg::String(error.code.to_string()))
                    .with_arg("function", DiagnosticArg::CallableShort(error.function.clone()))
                    .with_arg("location", DiagnosticArg::String(error.location.clone()))
                    .with_cause(error.detail.clone());
            }
            HirError::MirControlFlowRejected(error) => {
                row = row.with_cause(error.to_string());
            }
            HirError::MirEntryBlockMissing { function } => {
                row = row.with_arg("function", DiagnosticArg::CallableShort(function.clone()));
            }
            HirError::MirUnknownStateValue { function, block } => {
                row = row
                    .with_arg("function", DiagnosticArg::CallableShort(function.clone()))
                    .with_arg("block", DiagnosticArg::String(block.clone()));
            }
            HirError::FrontendRejected(error) => {
                row = row.with_cause(error.to_string());
            }
            HirError::TemplateExpansionFailed { construct, failure, .. } => {
                row = row
                    .with_arg("construct", DiagnosticArg::String(template_construct_name(*construct).to_string()))
                    .with_arg("failure", DiagnosticArg::String(template_failure_name(failure).to_string()));
                if let TemplateFailure::LoopRangeInvalid { start, end } = failure {
                    row = row
                        .with_arg("range_start", DiagnosticArg::Int(*start))
                        .with_arg("range_end", DiagnosticArg::Int(*end));
                }
                if let TemplateFailure::BlockFragmentUnsupportedNode { kind } = failure {
                    row = row.with_arg("node_kind", DiagnosticArg::String(kind.clone()));
                }
                if let TemplateFailure::FragmentNotMicroDeclaration { produced_items } = failure {
                    row = row.with_arg("produced_items", DiagnosticArg::Int(*produced_items as i64));
                }
            }
            HirError::MatchRecoveryFailed { stage, pattern, .. } => {
                row = row.with_arg("stage", DiagnosticArg::String(match_recovery_stage_name(*stage).to_string()));
                if let Some(pattern) = pattern {
                    row = row.with_arg("pattern", DiagnosticArg::String(pattern.clone()));
                }
            }
            HirError::ControlFlowContextViolation { operation, required, .. } => {
                row = row
                    .with_arg("operation", DiagnosticArg::String(control_flow_op_name(*operation).to_string()))
                    .with_arg("required", DiagnosticArg::String(control_flow_context_name(*required).to_string()));
            }
            HirError::FallthroughBindingLeak { name, .. } => {
                row = row.with_arg("name", DiagnosticArg::String(name.clone()));
            }
            HirError::LoopTargetMissing { operation, carries_value, .. } => {
                row = row
                    .with_arg("operation", DiagnosticArg::String(loop_jump_operation_name(*operation).to_string()))
                    .with_arg("carries_value", DiagnosticArg::String(carries_value.to_string()));
            }
            HirError::BreakValueTypeMismatch { actual, expected, .. } => {
                row = row
                    .with_arg("actual", DiagnosticArg::String(actual.clone()))
                    .with_arg("expected", DiagnosticArg::String(expected.clone()));
            }
            HirError::TryPropagateOperandUnsupported { operand_type, .. } => {
                row = row.with_arg("operand_type", DiagnosticArg::String(operand_type.clone()));
            }
            HirError::TryPropagateReturnMismatch { protocol, return_type, .. } => {
                row = row
                    .with_arg("protocol", DiagnosticArg::String(propagate_protocol_name(*protocol).to_string()))
                    .with_arg("return_type", DiagnosticArg::String(return_type.clone()));
            }
            HirError::FutureControlOperandMismatch { operation, operand_type, .. } => {
                row = row
                    .with_arg("operation", DiagnosticArg::String(control_flow_op_name(*operation).to_string()))
                    .with_arg("operand_type", DiagnosticArg::String(operand_type.clone()));
            }
            HirError::ControlFlowValueRequired { operation, expected_type, .. } => {
                row = row
                    .with_arg("operation", DiagnosticArg::String(control_flow_op_name(*operation).to_string()))
                    .with_arg("expected_type", DiagnosticArg::String(expected_type.clone()));
            }
            HirError::ReturnValueTypeMismatch { actual, expected, function, .. } => {
                row = row
                    .with_arg("actual", DiagnosticArg::String(actual.clone()))
                    .with_arg("expected", DiagnosticArg::String(expected.clone()));
                if let Some(function) = function {
                    row = row.with_arg("function", DiagnosticArg::CallableShort(function.clone()));
                }
            }
            HirError::YieldValueTypeMismatch { actual, expected_return, item_type, function, .. } => {
                row = row
                    .with_arg("actual", DiagnosticArg::String(actual.clone()))
                    .with_arg("expected_return", DiagnosticArg::String(expected_return.clone()))
                    .with_arg("item_type", DiagnosticArg::String(item_type.clone()));
                if let Some(function) = function {
                    row = row.with_arg("function", DiagnosticArg::CallableShort(function.clone()));
                }
            }
            HirError::YieldFromOperandNotGenerator { operand_type, .. } => {
                row = row.with_arg("operand_type", DiagnosticArg::String(operand_type.clone()));
            }
            HirError::YieldFromItemTypeMismatch { source_item, expected_return, expected_item, .. } => {
                row = row
                    .with_arg("source_item", DiagnosticArg::String(source_item.clone()))
                    .with_arg("expected_return", DiagnosticArg::String(expected_return.clone()))
                    .with_arg("expected_item", DiagnosticArg::String(expected_item.clone()));
            }
            HirError::AmbiguousBareNamePattern { name, .. } => {
                row = row.with_arg("name", DiagnosticArg::String(name.clone()));
            }
            HirError::SourceGroupProducerMissing { dependency_key, .. } => {
                if let Some(dependency_key) = dependency_key {
                    row = row.with_arg("dependency_key", DiagnosticArg::DependencyKey(dependency_key.clone()));
                }
            }
            HirError::SourceGroupNotEntry { dependency_key, .. } => {
                row = row.with_arg("dependency_key", DiagnosticArg::DependencyKey(dependency_key.clone()));
            }
            HirError::SourceGroupIdentityInvalid { dependency_key, empty, .. } => {
                row = row
                    .with_arg("dependency_key", DiagnosticArg::DependencyKey(dependency_key.clone()))
                    .with_arg("empty", DiagnosticArg::String(empty.to_string()));
            }
            HirError::SourceGroupDependencyUnavailable { group, dependency, .. } => {
                row = row
                    .with_arg("group", DiagnosticArg::DependencyKey(group.clone()))
                    .with_arg("dependency", DiagnosticArg::DependencyKey(dependency.clone()));
            }
            HirError::SourceFileIdentityInvalid { group, path, empty, .. } => {
                row = row
                    .with_arg("group", DiagnosticArg::DependencyKey(group.clone()))
                    .with_arg("path", DiagnosticArg::String(path.clone()))
                    .with_arg("empty", DiagnosticArg::String(empty.to_string()));
            }
            HirError::EntryGroupWithoutFiles { group, .. } => {
                row = row.with_arg("group", DiagnosticArg::DependencyKey(group.clone()));
            }
            HirError::SourceInputEntryGroupMissing { explicit, .. } => {
                row = row.with_arg("explicit", DiagnosticArg::String(explicit.to_string()));
            }
            HirError::SourceReadFailed { path, kind, .. } => {
                row = row
                    .with_arg("path", DiagnosticArg::String(path.clone()))
                    .with_arg("kind", DiagnosticArg::String(io_error_kind_name(*kind).to_string()));
            }
            HirError::EmptySemanticSourceGroupPlan { .. } | HirError::UnexpandedTemplateItem { .. } => {}
            HirError::SourceFileRejected { dependency_key, path, error } => {
                row = row
                    .with_arg("dependency_key", DiagnosticArg::DependencyKey(dependency_key.clone()))
                    .with_arg("path", DiagnosticArg::String(path.clone()))
                    .with_cause(error.to_string());
            }
            HirError::DependencyExportUnavailable { group, dependency, .. } => {
                row = row
                    .with_arg("group", DiagnosticArg::DependencyKey(group.clone()))
                    .with_arg("dependency", DiagnosticArg::DependencyKey(dependency.clone()));
            }
            HirError::DependencyExportIdentityCollision { dependency_key, .. } => {
                row = row.with_arg("dependency_key", DiagnosticArg::DependencyKey(dependency_key.clone()));
            }
            HirError::DeclarationIndexExhausted { space, owner, .. } => {
                row = row
                    .with_arg("space", DiagnosticArg::String(index_space_name(*space).to_string()))
                    .with_arg("owner", DiagnosticArg::CallableShort(owner.clone()));
            }
            HirError::DeclarationIdentityDuplicated { space, owner, .. } => {
                row = row
                    .with_arg("space", DiagnosticArg::String(index_space_name(*space).to_string()))
                    .with_arg("owner", DiagnosticArg::CallableShort(owner.clone()));
            }
            HirError::DependencyDeclarationsRequireSourceClosure { imported_export_count, .. } => {
                row = row.with_arg("imported_export_count", DiagnosticArg::Int(*imported_export_count as i64));
            }
            HirError::UnionNotLowered { union_name, .. } => {
                row = row.with_arg("union", DiagnosticArg::Aggregate(union_name.clone()));
            }
            HirError::UnsupportedRootItem { item_kind, .. } => {
                row = row.with_arg("item_kind", DiagnosticArg::String(item_kind.clone()));
            }
            HirError::PipelineRejected { codes, module } => {
                row = row
                    .with_arg("module", DiagnosticArg::String(module.clone()))
                    .with_arg("codes", DiagnosticArg::Aggregate(codes.join(",")));
            }
            HirError::FallthroughWithoutNextArm { .. } | HirError::FallthroughOutsideCaseChain { .. } | HirError::BreakValueRejected { .. } => {}
            HirError::LoweringFailed { scenario, details, .. } => {
                row = row.with_arg("scenario", DiagnosticArg::String(scenario.clone()));
                for (k, v) in details {
                    row = row.with_arg(k, DiagnosticArg::String(v.clone()));
                }
            }
            HirError::TypeNotFound { name, .. } => {
                row = row.with_arg("name", DiagnosticArg::TypeName(name.clone()));
            }
            HirError::VxUnsupported => {}
            HirError::SourceGroupClosureFailed(e) => {
                row = row.with_arg("details", DiagnosticArg::String(e.to_string()));
            }
            HirError::DeriveResolutionFailed { errors } => {
                row = row.with_arg("count", DiagnosticArg::Int(errors.len() as i64));
                for (i, err) in errors.iter().enumerate() {
                    row = row.with_arg(&format!("derive_{i}"), DiagnosticArg::String(err.to_string()));
                }
            }
        }

        row.with_internal_message(error.code().to_string())
    }
}

/// 把 MIR 生产的结构化诊断渲染为人类可读文本（仅用于 `Display`）。
fn mir_diagnostic_message(diagnostic: &crate::mir::MirDiagnostic) -> String {
    use crate::mir::MirDiagnostic;
    match diagnostic {
        MirDiagnostic::PatternLoweringFailed { reason, .. } => format!("MIR lowering failed: {reason}"),
        MirDiagnostic::UnsupportedExpression { span, kind } => {
            format!("MIR lowering rejected unsupported HIR expression `{kind}` at source span {span:?}")
        }
        MirDiagnostic::UnresolvedValueType { context } => format!("MIR lowering missing value type fact: {context}"),
        MirDiagnostic::UnresolvedVariantIdentity { sum_type, variant } => {
            format!("MIR lowering unresolved variant identity `{sum_type}::{variant}`")
        }
        MirDiagnostic::UnresolvedNominalIdentity { type_name } => format!("MIR lowering unresolved nominal identity `{type_name}`"),
        MirDiagnostic::UnresolvedFieldIdentity { field } => format!("MIR lowering unresolved field identity `{field}`"),
        MirDiagnostic::UnresolvedCallableIdentity { symbol } => format!("MIR lowering unresolved callable identity `{symbol}`"),
    }
}

/// frontend 失败事实的缺省 source 身份。
fn frontend_source(_error: &crate::valkyrie::frontend::ParseError) -> DiagnosticSource {
    DiagnosticSource { id: "<frontend>".into(), revision: None }
}

/// 从 frontend `ParseError` 提取可定位的源码 span。
fn frontend_span(error: &crate::valkyrie::frontend::ParseError) -> DiagnosticSpan {
    match error {
        crate::valkyrie::frontend::ParseError::Invalid { span: Some(range), .. } => {
            DiagnosticSpan { start: range.start, end: range.end }
        }
        _ => empty_span(),
    }
}

/// 模板构造的稳定短名（用于诊断参数）。
fn template_construct_name(construct: TemplateConstruct) -> &'static str {
    match construct {
        TemplateConstruct::MetaExpression => "meta_expression",
        TemplateConstruct::Loop => "loop",
        TemplateConstruct::Match => "match",
        TemplateConstruct::BlockFragment => "block_fragment",
    }
}

/// 模板失败原因的稳定短名（用于诊断参数）。
fn template_failure_name(failure: &TemplateFailure) -> &'static str {
    match failure {
        TemplateFailure::EmptyExpression => "empty_expression",
        TemplateFailure::TrailingTokens => "trailing_tokens",
        TemplateFailure::IncompleteExpression => "incomplete_expression",
        TemplateFailure::UnclosedParenthesis => "unclosed_parenthesis",
        TemplateFailure::UnevaluableToken => "unevaluable_token",
        TemplateFailure::IntegerLiteralUnparsable => "integer_literal_unparsable",
        TemplateFailure::IncompatibleComparisonOperands => "incompatible_comparison_operands",
        TemplateFailure::LoopHeaderTooShort => "loop_header_too_short",
        TemplateFailure::LoopBindingNotDiscard => "loop_binding_not_discard",
        TemplateFailure::LoopMissingInKeyword => "loop_missing_in_keyword",
        TemplateFailure::LoopHeaderTrailingTokens => "loop_header_trailing_tokens",
        TemplateFailure::LoopRangeMissing => "loop_range_missing",
        TemplateFailure::RangeShapeUnsupported => "range_shape_unsupported",
        TemplateFailure::RangeEndpointNotInteger => "range_endpoint_not_integer",
        TemplateFailure::LoopRangeInvalid { .. } => "loop_range_invalid",
        TemplateFailure::NonArchScrutinee => "non_arch_scrutinee",
        TemplateFailure::BlockFragmentUnsupportedNode { .. } => "block_fragment_unsupported_node",
        TemplateFailure::FragmentNotMicroDeclaration { .. } => "fragment_not_micro_declaration",
    }
}

/// 模板失败原因的人类可读描述（仅用于 `Display`）。
fn template_failure_message(failure: &TemplateFailure) -> String {
    match failure {
        TemplateFailure::EmptyExpression => "meta expression is empty",
        TemplateFailure::TrailingTokens => "meta expression has unevaluable trailing tokens",
        TemplateFailure::IncompleteExpression => "meta expression ended before it is complete",
        TemplateFailure::UnclosedParenthesis => "meta expression parenthesis is not closed",
        TemplateFailure::UnevaluableToken => "meta expression contains an unevaluable token",
        TemplateFailure::IntegerLiteralUnparsable => "integer literal cannot be parsed as `i64`",
        TemplateFailure::IncompatibleComparisonOperands => "comparison operands have incompatible types",
        TemplateFailure::LoopHeaderTooShort => "`loop` header must be `_ in start..end`",
        TemplateFailure::LoopBindingNotDiscard => "`loop` only supports the discard binding `_`",
        TemplateFailure::LoopMissingInKeyword => "`loop` header must contain `in`",
        TemplateFailure::LoopHeaderTrailingTokens => "`loop` header has unevaluable syntax after the range",
        TemplateFailure::LoopRangeMissing => "`loop` is missing a range expression",
        TemplateFailure::RangeShapeUnsupported => "range endpoints must be integer literals",
        TemplateFailure::RangeEndpointNotInteger => "range endpoints must be integer literals",
        TemplateFailure::LoopRangeInvalid { start, end } => return format!("`loop` range `{start}..{end}` is invalid"),
        TemplateFailure::NonArchScrutinee => "`match` only supports the `arch` scrutinee",
        TemplateFailure::BlockFragmentUnsupportedNode { kind } => {
            return format!("expanded `{kind}` cannot be kept inside a block")
        }
        TemplateFailure::FragmentNotMicroDeclaration { produced_items } => {
            return format!("expanded meta expression produced {produced_items} top-level items instead of one `micro` declaration")
        }
    }
    .to_string()
}

/// match 恢复阶段的稳定短名（用于诊断参数）。
fn match_recovery_stage_name(stage: MatchRecoveryStage) -> &'static str {
    match stage {
        MatchRecoveryStage::BlockInterior => "block_interior",
        MatchRecoveryStage::Arms => "arms",
        MatchRecoveryStage::Pattern => "pattern",
        MatchRecoveryStage::ArmBody => "arm_body",
        MatchRecoveryStage::ArmBodyExpression => "arm_body_expression",
    }
}

/// 控制流操作的稳定短名（用于诊断参数）。
fn control_flow_op_name(operation: ControlFlowOp) -> &'static str {
    match operation {
        ControlFlowOp::Await => "await",
        ControlFlowOp::Awake => "awake",
        ControlFlowOp::Block => "block",
        ControlFlowOp::Yield => "yield",
        ControlFlowOp::YieldFrom => "yield_from",
        ControlFlowOp::Resume => "resume",
        ControlFlowOp::TryPropagate => "try_propagate",
        ControlFlowOp::Fallthrough => "fallthrough",
        ControlFlowOp::Break => "break",
        ControlFlowOp::Continue => "continue",
        ControlFlowOp::Return => "return",
    }
}

/// 控制流上下文的稳定短名（用于诊断参数）。
fn control_flow_context_name(context: RequiredContext) -> &'static str {
    match context {
        RequiredContext::CatchArmBody => "catch_arm_body",
        RequiredContext::OutsideGuard => "outside_guard",
        RequiredContext::AsyncScope => "async_scope",
        RequiredContext::BlockingScope => "blocking_scope",
        RequiredContext::GeneratorScope => "generator_scope",
    }
}

/// loop 跳转操作的稳定短名（用于诊断参数）。
fn loop_jump_operation_name(operation: LoopJumpOperation) -> &'static str {
    match operation {
        LoopJumpOperation::Break => "break",
        LoopJumpOperation::Continue => "continue",
    }
}

/// `?` 传播协议的稳定短名（用于诊断参数）。
fn propagate_protocol_name(protocol: PropagateProtocol) -> &'static str {
    match protocol {
        PropagateProtocol::Nullable => "nullable",
        PropagateProtocol::Option => "option",
        PropagateProtocol::Result => "result",
    }
}

/// IO 失败种类的稳定短名（用于诊断参数）。
fn io_error_kind_name(kind: std::io::ErrorKind) -> &'static str {
    match kind {
        std::io::ErrorKind::NotFound => "not_found",
        std::io::ErrorKind::PermissionDenied => "permission_denied",
        std::io::ErrorKind::InvalidData => "invalid_data",
        std::io::ErrorKind::InvalidInput => "invalid_input",
        _ => "other",
    }
}

/// 声明索引空间的稳定短名（用于诊断参数）。
fn index_space_name(space: IndexSpace) -> &'static str {
    match space {
        IndexSpace::Item => "item",
        IndexSpace::ItemInstance => "item_instance",
        IndexSpace::Field => "field",
    }
}

/// MIR 侧失败事实的缺省 source 身份（MIR 结构本身不携带源码身份）。
fn mir_source() -> DiagnosticSource {
    DiagnosticSource { id: "<mir>".into(), revision: None }
}

/// 无源码定位时的空 span。
fn empty_span() -> DiagnosticSpan {
    DiagnosticSpan { start: 0, end: 0 }
}

/// 从 MIR 诊断中提取可定位的源码 span。
fn mir_diagnostic_span(diagnostic: &crate::mir::MirDiagnostic) -> DiagnosticSpan {
    match diagnostic {
        crate::mir::MirDiagnostic::UnsupportedExpression { span, .. } => {
            DiagnosticSpan { start: span.span.start as usize, end: span.span.end as usize }
        }
        _ => empty_span(),
    }
}

/// 把 MIR 诊断的结构化字段映射为诊断参数。
fn mir_diagnostic_args(row: DiagnosticRow, diagnostic: &crate::mir::MirDiagnostic) -> DiagnosticRow {
    use crate::mir::MirDiagnostic;
    match diagnostic {
        MirDiagnostic::PatternLoweringFailed { reason, .. } => row.with_arg("reason", DiagnosticArg::String(reason.clone())),
        MirDiagnostic::UnsupportedExpression { span, kind } => row
            .with_arg("kind", DiagnosticArg::String(kind.clone()))
            .with_arg("span", DiagnosticArg::Span(DiagnosticSpan { start: span.span.start as usize, end: span.span.end as usize })),
        MirDiagnostic::UnresolvedValueType { context } => row.with_arg("context", DiagnosticArg::String(context.clone())),
        MirDiagnostic::UnresolvedVariantIdentity { sum_type, variant } => row
            .with_arg("sum_type", DiagnosticArg::Aggregate(sum_type.clone()))
            .with_arg("variant", DiagnosticArg::CallableShort(variant.clone())),
        MirDiagnostic::UnresolvedNominalIdentity { type_name } => row.with_arg("type_name", DiagnosticArg::TypeName(type_name.clone())),
        MirDiagnostic::UnresolvedFieldIdentity { field } => row.with_arg("field", DiagnosticArg::String(field.clone())),
        MirDiagnostic::UnresolvedCallableIdentity { symbol } => row.with_arg("symbol", DiagnosticArg::CallableShort(symbol.clone())),
    }
}

fn other_source(error: &HirError) -> DiagnosticSource {
    match error {
        HirError::UnresolvedCall { source, .. } => source.clone(),
        HirError::UnresolvedConstructor { source, .. } => source.clone(),
        HirError::NominalAmbiguous { source, .. } => source.clone(),
        HirError::SumUniteAmbiguous { source, .. } => source.clone(),
        HirError::StructureMissingConstructor { source, .. } => source.clone(),
        HirError::SumUniteMissingDeclaration { source, .. } => source.clone(),
        HirError::InvalidDiscriminator { source, .. } => source.clone(),
        HirError::NoBodySourceInGroup { source, .. } => source.clone(),
        HirError::MirDiagnosticRejected { .. } => mir_source(),
        HirError::MirContractFailed(_) => mir_source(),
        HirError::MirControlFlowRejected(_) => mir_source(),
        HirError::MirEntryBlockMissing { .. } => mir_source(),
        HirError::MirUnknownStateValue { .. } => mir_source(),
        HirError::FrontendRejected(error) => frontend_source(error),
        HirError::TemplateExpansionFailed { source, .. } => source.clone(),
        HirError::MatchRecoveryFailed { source, .. } => source.clone(),
        HirError::ControlFlowContextViolation { source, .. } => source.clone(),
        HirError::FallthroughWithoutNextArm { source, .. } => source.clone(),
        HirError::FallthroughOutsideCaseChain { source, .. } => source.clone(),
        HirError::FallthroughBindingLeak { source, .. } => source.clone(),
        HirError::LoopTargetMissing { source, .. } => source.clone(),
        HirError::BreakValueRejected { source, .. } => source.clone(),
        HirError::BreakValueTypeMismatch { source, .. } => source.clone(),
        HirError::TryPropagateOperandUnsupported { source, .. } => source.clone(),
        HirError::TryPropagateReturnMismatch { source, .. } => source.clone(),
        HirError::FutureControlOperandMismatch { source, .. } => source.clone(),
        HirError::ControlFlowValueRequired { source, .. } => source.clone(),
        HirError::ReturnValueTypeMismatch { source, .. } => source.clone(),
        HirError::YieldValueTypeMismatch { source, .. } => source.clone(),
        HirError::YieldFromOperandNotGenerator { source, .. } => source.clone(),
        HirError::YieldFromItemTypeMismatch { source, .. } => source.clone(),
        HirError::AmbiguousBareNamePattern { source, .. } => source.clone(),
        HirError::SourceGroupProducerMissing { source, .. } => source.clone(),
        HirError::SourceGroupNotEntry { source, .. } => source.clone(),
        HirError::SourceGroupIdentityInvalid { source, .. } => source.clone(),
        HirError::SourceGroupDependencyUnavailable { source, .. } => source.clone(),
        HirError::SourceFileIdentityInvalid { source, .. } => source.clone(),
        HirError::EntryGroupWithoutFiles { source, .. } => source.clone(),
        HirError::SourceInputEntryGroupMissing { source, .. } => source.clone(),
        HirError::SourceReadFailed { source, .. } => source.clone(),
        HirError::EmptySemanticSourceGroupPlan { source, .. } => source.clone(),
        HirError::SourceFileRejected { error, .. } => frontend_source(error),
        HirError::DependencyExportUnavailable { source, .. } => source.clone(),
        HirError::DependencyExportIdentityCollision { source, .. } => source.clone(),
        HirError::DeclarationIndexExhausted { source, .. } => source.clone(),
        HirError::DeclarationIdentityDuplicated { source, .. } => source.clone(),
        HirError::DependencyDeclarationsRequireSourceClosure { source, .. } => source.clone(),
        HirError::UnionNotLowered { source, .. } => source.clone(),
        HirError::UnexpandedTemplateItem { source, .. } => source.clone(),
        HirError::UnsupportedRootItem { source, .. } => source.clone(),
        HirError::PipelineRejected { .. } => DiagnosticSource { id: "<pipeline>".into(), revision: None },
        HirError::LoweringFailed { source, .. } => source.clone(),
        HirError::TypeNotFound { source, .. } => source.clone(),
        HirError::VxUnsupported => unreachable!(),
        HirError::SourceGroupClosureFailed(_) => unreachable!(),
        HirError::DeriveResolutionFailed { .. } => unreachable!(),
    }
}

fn other_span(error: &HirError) -> DiagnosticSpan {
    match error {
        HirError::UnresolvedCall { span, .. } => span.clone(),
        HirError::UnresolvedConstructor { span, .. } => span.clone(),
        HirError::NominalAmbiguous { span, .. } => span.clone(),
        HirError::SumUniteAmbiguous { span, .. } => span.clone(),
        HirError::StructureMissingConstructor { span, .. } => span.clone(),
        HirError::SumUniteMissingDeclaration { span, .. } => span.clone(),
        HirError::InvalidDiscriminator { span, .. } => span.clone(),
        HirError::NoBodySourceInGroup { span, .. } => span.clone(),
        HirError::MirDiagnosticRejected { diagnostic } => mir_diagnostic_span(diagnostic),
        HirError::MirContractFailed(_) => empty_span(),
        HirError::MirControlFlowRejected(_) => empty_span(),
        HirError::MirEntryBlockMissing { .. } => empty_span(),
        HirError::MirUnknownStateValue { .. } => empty_span(),
        HirError::FrontendRejected(error) => frontend_span(error),
        HirError::TemplateExpansionFailed { span, .. } => span.clone(),
        HirError::MatchRecoveryFailed { span, .. } => span.clone(),
        HirError::ControlFlowContextViolation { span, .. } => span.clone(),
        HirError::FallthroughWithoutNextArm { span, .. } => span.clone(),
        HirError::FallthroughOutsideCaseChain { span, .. } => span.clone(),
        HirError::FallthroughBindingLeak { span, .. } => span.clone(),
        HirError::LoopTargetMissing { span, .. } => span.clone(),
        HirError::BreakValueRejected { span, .. } => span.clone(),
        HirError::BreakValueTypeMismatch { span, .. } => span.clone(),
        HirError::TryPropagateOperandUnsupported { span, .. } => span.clone(),
        HirError::TryPropagateReturnMismatch { span, .. } => span.clone(),
        HirError::FutureControlOperandMismatch { span, .. } => span.clone(),
        HirError::ControlFlowValueRequired { span, .. } => span.clone(),
        HirError::ReturnValueTypeMismatch { span, .. } => span.clone(),
        HirError::YieldValueTypeMismatch { span, .. } => span.clone(),
        HirError::YieldFromOperandNotGenerator { span, .. } => span.clone(),
        HirError::YieldFromItemTypeMismatch { span, .. } => span.clone(),
        HirError::AmbiguousBareNamePattern { span, .. } => span.clone(),
        HirError::SourceGroupProducerMissing { span, .. } => span.clone(),
        HirError::SourceGroupNotEntry { span, .. } => span.clone(),
        HirError::SourceGroupIdentityInvalid { span, .. } => span.clone(),
        HirError::SourceGroupDependencyUnavailable { span, .. } => span.clone(),
        HirError::SourceFileIdentityInvalid { span, .. } => span.clone(),
        HirError::EntryGroupWithoutFiles { span, .. } => span.clone(),
        HirError::SourceInputEntryGroupMissing { span, .. } => span.clone(),
        HirError::SourceReadFailed { span, .. } => span.clone(),
        HirError::EmptySemanticSourceGroupPlan { span, .. } => span.clone(),
        HirError::SourceFileRejected { error, .. } => frontend_span(error),
        HirError::DependencyExportUnavailable { span, .. } => span.clone(),
        HirError::DependencyExportIdentityCollision { span, .. } => span.clone(),
        HirError::DeclarationIndexExhausted { span, .. } => span.clone(),
        HirError::DeclarationIdentityDuplicated { span, .. } => span.clone(),
        HirError::DependencyDeclarationsRequireSourceClosure { span, .. } => span.clone(),
        HirError::UnionNotLowered { span, .. } => span.clone(),
        HirError::UnexpandedTemplateItem { span, .. } => span.clone(),
        HirError::UnsupportedRootItem { span, .. } => span.clone(),
        HirError::PipelineRejected { .. } => empty_span(),
        HirError::LoweringFailed { span, .. } => span.clone(),
        HirError::TypeNotFound { span, .. } => span.clone(),
        HirError::VxUnsupported => unreachable!(),
        HirError::SourceGroupClosureFailed(_) => unreachable!(),
        HirError::DeriveResolutionFailed { .. } => unreachable!(),
    }
}

impl From<crate::valkyrie::frontend::ParseError> for HirError {
    fn from(error: crate::valkyrie::frontend::ParseError) -> Self {
        Self::FrontendRejected(error)
    }
}

impl From<crate::valkyrie::semantic_closure::SemanticClosureError> for HirError {
    fn from(error: crate::valkyrie::semantic_closure::SemanticClosureError) -> Self {
        Self::SourceGroupClosureFailed(error)
    }
}

impl std::error::Error for HirError {}

impl miette::Diagnostic for HirError {}

impl std::fmt::Display for HirError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnresolvedCall { selector, context, .. } => {
                write!(f, "unresolved call `{selector}` in {context}")
            }
            Self::UnresolvedConstructor { name, context, .. } => {
                write!(f, "unresolved constructor `{name}` in {context}")
            }
            Self::NominalAmbiguous { name, candidate_count, .. } => {
                write!(f, "nominal `{name}` is ambiguous ({candidate_count} candidates)")
            }
            Self::SumUniteAmbiguous { name, candidate_count, .. } => {
                write!(f, "sum/unite `{name}` is ambiguous ({candidate_count} candidates)")
            }
            Self::StructureMissingConstructor { qualified_name, .. } => {
                write!(f, "structure `{qualified_name}` missing constructor_declaration")
            }
            Self::SumUniteMissingDeclaration { name, .. } => {
                write!(f, "sum/unite `{name}` missing declaration")
            }
            Self::InvalidDiscriminator { aggregate, scenario, variant, value, .. } => {
                write!(f, "invalid discriminator in `{aggregate}` ({scenario})");
                if let Some(v) = variant {
                    write!(f, " variant={v}")?;
                }
                if let Some(v) = value {
                    write!(f, " value={v}")?;
                }
                Ok(())
            }
            Self::NoBodySourceInGroup { dependency_key, .. } => {
                write!(f, "no body source in group `{dependency_key}`")
            }
            Self::MirDiagnosticRejected { diagnostic } => write!(f, "{}", mir_diagnostic_message(diagnostic)),
            Self::MirContractFailed(error) => {
                write!(f, "{} {} at {}: {}", error.code, error.function, error.location, error.detail)
            }
            Self::MirControlFlowRejected(error) => write!(f, "{error}"),
            Self::MirEntryBlockMissing { function } => {
                write!(f, "control-flow schedule validation failed: function `{function}` has no entry block")
            }
            Self::MirUnknownStateValue { function, block } => write!(
                f,
                "control-flow schedule validation failed: dispatch block `{block}` of function `{function}` references an unknown state value"
            ),
            Self::FrontendRejected(error) => write!(f, "{error}"),
            Self::TemplateExpansionFailed { construct, failure, .. } => {
                write!(f, "TGrammar {} expansion failed: {}", template_construct_name(*construct), template_failure_message(failure))
            }
            Self::MatchRecoveryFailed { stage, pattern, .. } => {
                write!(f, "match recovery failed at stage `{}`", match_recovery_stage_name(*stage))?;
                if let Some(pattern) = pattern {
                    write!(f, " for pattern `{pattern}`")?;
                }
                Ok(())
            }
            Self::ControlFlowContextViolation { operation, required, .. } => {
                write!(f, "`{}` is only allowed in `{}`", control_flow_op_name(*operation), control_flow_context_name(*required))
            }
            Self::FallthroughWithoutNextArm { .. } => write!(f, "the last `case` arm cannot use `fallthrough`"),
            Self::FallthroughOutsideCaseChain { .. } => write!(f, "`fallthrough` is only allowed inside a `case` chain"),
            Self::FallthroughBindingLeak { name, .. } => {
                write!(f, "`fallthrough` does not inherit the previous arm pattern bindings, yet `{name}` is referenced")
            }
            Self::LoopTargetMissing { operation, carries_value, .. } => {
                write!(f, "`{}`", loop_jump_operation_name(*operation))?;
                if *carries_value {
                    write!(f, " with a value")?;
                }
                write!(f, " has no target loop")
            }
            Self::BreakValueRejected { .. } => write!(f, "the target loop does not accept a `break` value"),
            Self::BreakValueTypeMismatch { actual, expected, .. } => {
                write!(f, "`break` value type `{actual}` is incompatible with the target loop value type `{expected}`")
            }
            Self::TryPropagateOperandUnsupported { operand_type, .. } => {
                write!(f, "`?` cannot be applied to `{operand_type}`")
            }
            Self::TryPropagateReturnMismatch { protocol, return_type, .. } => {
                write!(f, "`?` on a `{}` operand requires a matching enclosing return type, got `{return_type}`", propagate_protocol_name(*protocol))
            }
            Self::FutureControlOperandMismatch { operation, operand_type, .. } => {
                write!(f, "`{}` requires a `Future<T>` or `Promise<T>` operand, got `{operand_type}`", control_flow_op_name(*operation))
            }
            Self::ControlFlowValueRequired { operation, expected_type, .. } => {
                write!(f, "`{}` requires a carried value of type `{expected_type}`", control_flow_op_name(*operation))
            }
            Self::ReturnValueTypeMismatch { actual, expected, function, .. } => {
                write!(f, "`return` value type `{actual}` is incompatible with the function return type `{expected}`")?;
                match function {
                    Some(function) => write!(f, " in function `{function}`"),
                    None => Ok(()),
                }
            }
            Self::YieldValueTypeMismatch { actual, expected_return, item_type, function, .. } => {
                write!(f, "`yield` value type `{actual}` is incompatible with the `{item_type}` item of return type `{expected_return}`")?;
                match function {
                    Some(function) => write!(f, " in function `{function}`"),
                    None => Ok(()),
                }
            }
            Self::YieldFromOperandNotGenerator { operand_type, .. } => {
                write!(f, "`yield from` operand type `{operand_type}` is not a generator protocol")
            }
            Self::YieldFromItemTypeMismatch { source_item, expected_return, expected_item, .. } => write!(
                f,
                "`yield from` item type `{source_item}` is incompatible with the `{expected_item}` item of return type `{expected_return}`"
            ),
            Self::AmbiguousBareNamePattern { name, .. } => write!(
                f,
                "bare lowercase single-segment pattern `{name}` is ambiguous with a variable binding, use a variant form instead"
            ),
            Self::SourceGroupProducerMissing { dependency_key, .. } => {
                write!(f, "generated source group has no producer identity")?;
                match dependency_key {
                    Some(dependency_key) => write!(f, " (group `{dependency_key}`)"),
                    None => Ok(()),
                }
            }
            Self::SourceGroupNotEntry { dependency_key, .. } => {
                write!(f, "generated source group `{dependency_key}` must be explicitly marked as entry")
            }
            Self::SourceGroupIdentityInvalid { dependency_key, empty, .. } => {
                if *empty {
                    write!(f, "source group dependency identity is empty")
                }
                else {
                    write!(f, "source group dependency identity `{dependency_key}` is duplicated")
                }
            }
            Self::SourceGroupDependencyUnavailable { group, dependency, .. } => {
                write!(f, "source group `{group}` depends on `{dependency}`, which was not provided before it")
            }
            Self::SourceFileIdentityInvalid { group, path, empty, .. } => {
                if *empty {
                    write!(f, "source group `{group}` contains a source file with an empty path")
                }
                else {
                    write!(f, "source group `{group}` contains duplicated source file `{path}`")
                }
            }
            Self::EntryGroupWithoutFiles { group, .. } => write!(f, "entry source group `{group}` declares no source file"),
            Self::SourceInputEntryGroupMissing { explicit, .. } => {
                if *explicit {
                    write!(f, "compiler source snapshot has no explicitly marked entry group")
                }
                else {
                    write!(f, "compiler source snapshot has no entry group")
                }
            }
            Self::SourceReadFailed { path, kind, .. } => {
                write!(f, "failed to read source file `{path}` ({})", io_error_kind_name(*kind))
            }
            Self::EmptySemanticSourceGroupPlan { .. } => write!(f, "semantic source group plan is empty"),
            Self::SourceFileRejected { dependency_key, path, error } => {
                write!(f, "source group `{dependency_key}` file `{path}` was rejected: {error}")
            }
            Self::DependencyExportUnavailable { group, dependency, .. } => {
                write!(f, "source group `{group}` requires the semantic export of `{dependency}`, which is unavailable")
            }
            Self::DependencyExportIdentityCollision { dependency_key, .. } => {
                write!(f, "dependency `{dependency_key}` registered its semantic export twice")
            }
            Self::DeclarationIndexExhausted { space, owner, .. } => {
                write!(f, "`{}` space is exhausted while registering `{owner}`", index_space_name(*space))
            }
            Self::DeclarationIdentityDuplicated { space, owner, .. } => {
                write!(f, "`{}` identity of `{owner}` was already registered", index_space_name(*space))
            }
            Self::DependencyDeclarationsRequireSourceClosure { imported_export_count, .. } => write!(
                f,
                "dependency declaration registration requires the full compiler source closure, got {imported_export_count} imported exports"
            ),
            Self::UnionNotLowered { union_name, .. } => write!(f, "named union `{union_name}` is not lowered into HIR"),
            Self::UnexpandedTemplateItem { .. } => write!(f, "TGrammar template item reached HIR lowering without expansion"),
            Self::UnsupportedRootItem { item_kind, .. } => {
                write!(f, "root item kind `{item_kind}` is not supported by HIR lowering")
            }
            Self::PipelineRejected { codes, module } => {
                write!(f, "compile pipeline rejected module `{module}`")?;
                if !codes.is_empty() {
                    write!(f, " ({})", codes.join(","))?;
                }
                Ok(())
            }
            Self::LoweringFailed { scenario, details, source, span, .. } => {
                write!(f, "lowering failed: {scenario}")?;
                if !details.is_empty() {
                    write!(f, " (details={})", details.len())?;
                }
                write!(f, " source={} span={}..{}", source.id, span.start, span.end)
            }
            Self::TypeNotFound { name, .. } => write!(f, "unknown type name `{name}`: not a module type alias or language builtin"),
            Self::VxUnsupported => write!(f, ".vx source is not supported"),
            Self::SourceGroupClosureFailed(e) => write!(f, "source group closure failed: {e}"),
            Self::DeriveResolutionFailed { errors } => {
                write!(f, "derive injection failed: {} errors", errors.len())?;
                for err in errors {
                    write!(f, "; {err}")?;
                }
                Ok(())
            }
        }
    }
}

