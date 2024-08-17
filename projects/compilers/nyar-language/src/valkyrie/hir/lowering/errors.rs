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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// `.vx` source 暂不支持。
    VxUnsupported,
}

impl HirError {
    /// 确定性生成 variant 的 machine code。
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
            Self::LoweringFailed { .. } => "valkyrie.hir.lowering_failed",
            Self::VxUnsupported => "valkyrie.hir.vx_unsupported",
        }
    }
}

impl From<HirError> for DiagnosticRow {
    fn from(error: HirError) -> Self {
        let source = match &error {
            HirError::VxUnsupported => DiagnosticSource { id: ".vx".into(), revision: None },
            other => other_source(other).clone(),
        };
        let span = match &error {
            HirError::VxUnsupported => DiagnosticSpan { start: 0, end: 0 },
            other => other_span(other).clone(),
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
            HirError::LoweringFailed { scenario, details, .. } => {
                row = row.with_arg("scenario", DiagnosticArg::String(scenario.clone()));
                for (k, v) in details {
                    row = row.with_arg(k, DiagnosticArg::String(v.clone()));
                }
            }
            HirError::VxUnsupported => {}
        }

        row.with_internal_message(error.code().to_string())
    }
}

fn other_source(error: &HirError) -> &DiagnosticSource {
    match error {
        HirError::UnresolvedCall { source, .. } => source,
        HirError::UnresolvedConstructor { source, .. } => source,
        HirError::NominalAmbiguous { source, .. } => source,
        HirError::SumUniteAmbiguous { source, .. } => source,
        HirError::StructureMissingConstructor { source, .. } => source,
        HirError::SumUniteMissingDeclaration { source, .. } => source,
        HirError::InvalidDiscriminator { source, .. } => source,
        HirError::NoBodySourceInGroup { source, .. } => source,
        HirError::LoweringFailed { source, .. } => source,
        HirError::VxUnsupported => unreachable!(),
    }
}

fn other_span(error: &HirError) -> &DiagnosticSpan {
    match error {
        HirError::UnresolvedCall { span, .. } => span,
        HirError::UnresolvedConstructor { span, .. } => span,
        HirError::NominalAmbiguous { span, .. } => span,
        HirError::SumUniteAmbiguous { span, .. } => span,
        HirError::StructureMissingConstructor { span, .. } => span,
        HirError::SumUniteMissingDeclaration { span, .. } => span,
        HirError::InvalidDiscriminator { span, .. } => span,
        HirError::NoBodySourceInGroup { span, .. } => span,
        HirError::LoweringFailed { span, .. } => span,
        HirError::VxUnsupported => unreachable!(),
    }
}
