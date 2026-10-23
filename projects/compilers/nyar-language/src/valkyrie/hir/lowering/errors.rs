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

