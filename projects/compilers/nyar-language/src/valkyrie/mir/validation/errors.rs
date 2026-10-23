//! MIR 校验阶段的结构化失败原因。
//!
//! 本模块定义 `MirValidationError`——MIR 校验逻辑块的**唯一错误类型**。每个 variant 对应
//! 校验器里一个具体语义场景，携带结构化字段（不是已格式化的 `String`）。诊断 code、
//! source、span 与参数映射都由本类型自己生产，不再借用 HIR 错误类型。

use nyar_types::{DiagnosticArg, DiagnosticOwner, DiagnosticPhase, DiagnosticRow, DiagnosticSeverity, DiagnosticSource, DiagnosticSpan};

use super::{diagnostics::display_type, semantic_contract::SemanticMirContractError};
use crate::{
    mir::{MirDiagnostic, MirEffectKind},
    types::hir::ValkyrieType,
};

/// MIR 校验阶段的结构化失败原因。
///
/// 生产者是 `module_validation` / `function_validation` / `control_flow` / `effect_validation`，
/// 消费边界只读取本类型携带的事实，不重新解释或字符串化。
#[derive(Debug, Clone)]
pub enum MirValidationError {
    /// MIR lowering 产出的结构化诊断被校验层拒绝。
    DiagnosticRejected {
        /// MIR lowering 产出的原始诊断。
        diagnostic: MirDiagnostic,
    },
    /// Semantic MIR 合同校验失败。
    ContractFailed(SemanticMirContractError),
    /// MIR 函数缺少 entry block。
    EntryBlockMissing {
        /// 函数符号。
        function: String,
    },
    /// dispatch block 引用了函数未冻结的 state 值。
    UnknownStateValue {
        /// 函数符号。
        function: String,
        /// dispatch block 标签。
        block: String,
    },
    /// `Jump` 终结符跳向不存在的目标块。
    JumpTargetMissing {
        /// 函数符号。
        function: String,
        /// 源 block 标签。
        block: String,
    },
    /// `Jump` 传出的参数数量与目标块参数数量不一致。
    JumpArgumentCountMismatch {
        /// 函数符号。
        function: String,
        /// 源 block 标签。
        block: String,
        /// 目标块声明的参数数量。
        expected: usize,
        /// `Jump` 实际传出的参数数量。
        actual: usize,
    },
    /// `Jump` 第 N 个参数类型与目标块参数类型不一致。
    JumpArgumentTypeMismatch {
        /// 函数符号。
        function: String,
        /// 源 block 标签。
        block: String,
        /// 目标 block 标签。
        target_block: String,
        /// 参数下标（从 0 开始）。
        index: usize,
        /// `Jump` 实际参数类型。
        argument_type: ValkyrieType,
        /// 目标块参数类型。
        parameter_type: ValkyrieType,
    },
    /// 终结符指向不存在的目标块。
    TargetBlockMissing {
        /// 函数符号。
        function: String,
        /// 源 block 标签。
        block: String,
    },
    /// effect 恢复点参数个数不合法。
    EffectResumeParameterCountMismatch {
        /// 函数符号。
        function: String,
        /// 源 block 标签。
        block: String,
        /// effect 要求的恢复点参数个数。
        expected: usize,
        /// 恢复点实际参数个数。
        actual: usize,
    },
    /// effect 终结符缺少 payload。
    EffectPayloadMissing {
        /// 函数符号。
        function: String,
        /// 源 block 标签。
        block: String,
    },
    /// `Future<T>` / `Promise<T>` effect payload 类型不满足形状约束。
    EffectPayloadFutureTypeMismatch {
        /// 函数符号。
        function: String,
        /// 源 block 标签。
        block: String,
        /// 产生该约束的 effect 类别。
        effect: MirEffectKind,
        /// payload 的静态类型。
        payload_type: ValkyrieType,
    },
    /// `yield from` effect payload 类型不满足 `Generator<T>` / `Iterator<T>` 形状约束。
    EffectPayloadGeneratorTypeMismatch {
        /// 函数符号。
        function: String,
        /// 源 block 标签。
        block: String,
        /// payload 的静态类型。
        payload_type: ValkyrieType,
    },
    /// effect 恢复点参数类型与期望类型不一致。
    ResumeParameterTypeMismatch {
        /// 函数符号。
        function: String,
        /// 源 block 标签。
        block: String,
        /// 恢复点参数实际类型。
        actual_type: ValkyrieType,
        /// effect 期望的类型。
        expected_type: ValkyrieType,
    },
}

impl MirValidationError {
    /// 本变体对应的稳定诊断 code。
    pub fn code(&self) -> &'static str {
        match self {
            Self::DiagnosticRejected { .. } => "valkyrie.mir.diagnostic_rejected",
            Self::ContractFailed(_) => "valkyrie.mir.contract_failed",
            Self::EntryBlockMissing { .. } => "valkyrie.mir.entry_block_missing",
            Self::UnknownStateValue { .. } => "valkyrie.mir.unknown_state_value",
            Self::JumpTargetMissing { .. }
            | Self::JumpArgumentCountMismatch { .. }
            | Self::JumpArgumentTypeMismatch { .. }
            | Self::TargetBlockMissing { .. }
            | Self::EffectResumeParameterCountMismatch { .. }
            | Self::EffectPayloadMissing { .. }
            | Self::EffectPayloadFutureTypeMismatch { .. }
            | Self::EffectPayloadGeneratorTypeMismatch { .. }
            | Self::ResumeParameterTypeMismatch { .. } => "valkyrie.mir.control_flow_rejected",
        }
    }

    /// 失败事实的 source 身份；MIR 结构本身不携带源码身份。
    pub fn source(&self) -> DiagnosticSource {
        DiagnosticSource { id: "<mir>".into(), revision: None }
    }

    /// 失败事实的定位 span；只有 MIR lowering 诊断能提供源码定位。
    pub fn span(&self) -> DiagnosticSpan {
        match self {
            Self::DiagnosticRejected { diagnostic } => mir_diagnostic_span(diagnostic),
            _ => DiagnosticSpan { start: 0, end: 0 },
        }
    }
}

impl From<MirValidationError> for DiagnosticRow {
    fn from(error: MirValidationError) -> Self {
        let mut row = DiagnosticRow::new(
            error.code(),
            DiagnosticSeverity::Error,
            DiagnosticPhase::Mir,
            DiagnosticOwner::ValkyrieFrontend,
            error.source(),
            error.span(),
        );

        match &error {
            MirValidationError::DiagnosticRejected { diagnostic } => {
                row = mir_diagnostic_args(row, diagnostic);
            }
            MirValidationError::ContractFailed(error) => {
                row = row
                    .with_arg("code", DiagnosticArg::String(error.code.to_string()))
                    .with_arg("function", DiagnosticArg::CallableShort(error.function.clone()))
                    .with_arg("location", DiagnosticArg::String(error.location.clone()))
                    .with_cause(error.detail.clone());
            }
            MirValidationError::EntryBlockMissing { function } => {
                row = row.with_arg("function", DiagnosticArg::CallableShort(function.clone()));
            }
            MirValidationError::UnknownStateValue { function, block } => {
                row = row
                    .with_arg("function", DiagnosticArg::CallableShort(function.clone()))
                    .with_arg("block", DiagnosticArg::String(block.clone()));
            }
            MirValidationError::JumpTargetMissing { function, block } => {
                row = row
                    .with_arg("function", DiagnosticArg::CallableShort(function.clone()))
                    .with_arg("block", DiagnosticArg::String(block.clone()));
            }
            MirValidationError::JumpArgumentCountMismatch { function, block, expected, actual } => {
                row = row
                    .with_arg("function", DiagnosticArg::CallableShort(function.clone()))
                    .with_arg("block", DiagnosticArg::String(block.clone()))
                    .with_arg("expected", DiagnosticArg::Int(*expected as i64))
                    .with_arg("actual", DiagnosticArg::Int(*actual as i64));
            }
            MirValidationError::JumpArgumentTypeMismatch { function, block, target_block, index, argument_type, parameter_type } => {
                row = row
                    .with_arg("function", DiagnosticArg::CallableShort(function.clone()))
                    .with_arg("block", DiagnosticArg::String(block.clone()))
                    .with_arg("target_block", DiagnosticArg::String(target_block.clone()))
                    .with_arg("index", DiagnosticArg::Int(*index as i64))
                    .with_arg("argument_type", DiagnosticArg::TypeName(display_type(argument_type)))
                    .with_arg("parameter_type", DiagnosticArg::TypeName(display_type(parameter_type)));
            }
            MirValidationError::TargetBlockMissing { function, block } => {
                row = row
                    .with_arg("function", DiagnosticArg::CallableShort(function.clone()))
                    .with_arg("block", DiagnosticArg::String(block.clone()));
            }
            MirValidationError::EffectResumeParameterCountMismatch { function, block, expected, actual } => {
                row = row
                    .with_arg("function", DiagnosticArg::CallableShort(function.clone()))
                    .with_arg("block", DiagnosticArg::String(block.clone()))
                    .with_arg("expected", DiagnosticArg::Int(*expected as i64))
                    .with_arg("actual", DiagnosticArg::Int(*actual as i64));
            }
            MirValidationError::EffectPayloadMissing { function, block } => {
                row = row
                    .with_arg("function", DiagnosticArg::CallableShort(function.clone()))
                    .with_arg("block", DiagnosticArg::String(block.clone()));
            }
            MirValidationError::EffectPayloadFutureTypeMismatch { function, block, effect, payload_type } => {
                row = row
                    .with_arg("function", DiagnosticArg::CallableShort(function.clone()))
                    .with_arg("block", DiagnosticArg::String(block.clone()))
                    .with_arg("effect", DiagnosticArg::String(await_effect_name(*effect).to_string()))
                    .with_arg("payload_type", DiagnosticArg::TypeName(display_type(payload_type)));
            }
            MirValidationError::EffectPayloadGeneratorTypeMismatch { function, block, payload_type } => {
                row = row
                    .with_arg("function", DiagnosticArg::CallableShort(function.clone()))
                    .with_arg("block", DiagnosticArg::String(block.clone()))
                    .with_arg("payload_type", DiagnosticArg::TypeName(display_type(payload_type)));
            }
            MirValidationError::ResumeParameterTypeMismatch { function, block, actual_type, expected_type } => {
                row = row
                    .with_arg("function", DiagnosticArg::CallableShort(function.clone()))
                    .with_arg("block", DiagnosticArg::String(block.clone()))
                    .with_arg("actual_type", DiagnosticArg::TypeName(display_type(actual_type)))
                    .with_arg("expected_type", DiagnosticArg::TypeName(display_type(expected_type)));
            }
        }

        row.with_internal_message(error.code().to_string())
    }
}

impl std::error::Error for MirValidationError {}

impl miette::Diagnostic for MirValidationError {}

impl std::fmt::Display for MirValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DiagnosticRejected { diagnostic } => write!(f, "{}", mir_diagnostic_message(diagnostic)),
            Self::ContractFailed(error) => {
                write!(f, "{} {} at {}: {}", error.code, error.function, error.location, error.detail)
            }
            Self::EntryBlockMissing { function } => {
                write!(f, "control-flow schedule validation failed: function `{function}` has no entry block")
            }
            Self::UnknownStateValue { function, block } => write!(
                f,
                "control-flow schedule validation failed: dispatch block `{block}` of function `{function}` references an unknown state value"
            ),
            Self::JumpTargetMissing { function, block } => {
                write!(f, "控制流调度校验失败：函数 `{function}` 的 block `{block}` 跳转到了不存在的目标块")
            }
            Self::JumpArgumentCountMismatch { function, block, .. } => {
                write!(f, "控制流调度校验失败：函数 `{function}` 的 block `{block}` 传出的参数数量与目标块参数数量不一致")
            }
            Self::JumpArgumentTypeMismatch { function, block, target_block, index, argument_type, parameter_type } => write!(
                f,
                "控制流调度校验失败：`MIR` 函数 `{function}` 的 block `{block}` 跳向 `{target_block}` 时，第 {} 个 Jump 参数类型为 `{}`，目标参数类型为 `{}`",
                index + 1,
                display_type(argument_type),
                display_type(parameter_type)
            ),
            Self::TargetBlockMissing { function, block } => {
                write!(f, "控制流调度校验失败：函数 `{function}` 的 block `{block}` 指向了不存在的目标块")
            }
            Self::EffectResumeParameterCountMismatch { function, block, expected, actual } => write!(
                f,
                "控制流调度校验失败：`MIR` 函数 `{function}` 的 block `{block}` 所指向的 effect 恢复点参数个数不合法，期望 {expected} 个，当前为 {actual}"
            ),
            Self::EffectPayloadMissing { function, block } => {
                write!(f, "控制流调度校验失败：`MIR` 函数 `{function}` 的 block `{block}` 缺少 effect payload")
            }
            Self::EffectPayloadFutureTypeMismatch { function, block, effect, payload_type } => write!(
                f,
                "控制流调度校验失败：`MIR` 函数 `{function}` 的 block `{block}` 的 {} effect payload 类型 `{}` 不满足 `Future<T>` / `Promise<T>` 形状",
                await_effect_name(*effect),
                display_type(payload_type)
            ),
            Self::EffectPayloadGeneratorTypeMismatch { function, block, payload_type } => write!(
                f,
                "控制流调度校验失败：`MIR` 函数 `{function}` 的 block `{block}` 的 `yield from` effect payload 类型 `{}` 不满足 `Generator<T>` / `Iterator<T>` 形状",
                display_type(payload_type)
            ),
            Self::ResumeParameterTypeMismatch { function, block, actual_type, expected_type } => write!(
                f,
                "控制流调度校验失败：`MIR` 函数 `{function}` 的 block `{block}` 所指向恢复点参数类型为 `{}`，期望 `{}`",
                display_type(actual_type),
                display_type(expected_type)
            ),
        }
    }
}

/// 需要 `Future<T>` / `Promise<T>` 形状约束的 effect 短名。
fn await_effect_name(effect: MirEffectKind) -> &'static str {
    match effect {
        MirEffectKind::Await => "`await`",
        MirEffectKind::AsyncSpawn => "`awake`",
        MirEffectKind::AsyncBlock => "`block`",
        _ => unreachable!(),
    }
}

/// 把 MIR 诊断的结构化字段映射为诊断参数。
fn mir_diagnostic_args(row: DiagnosticRow, diagnostic: &MirDiagnostic) -> DiagnosticRow {
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

/// 从 MIR 诊断中提取可定位的源码 span。
fn mir_diagnostic_span(diagnostic: &MirDiagnostic) -> DiagnosticSpan {
    match diagnostic {
        MirDiagnostic::UnsupportedExpression { span, .. } => DiagnosticSpan { start: span.span.start as usize, end: span.span.end as usize },
        _ => DiagnosticSpan { start: 0, end: 0 },
    }
}

/// 把 MIR 生产的结构化诊断渲染为人类可读文本（仅用于 `Display`）。
fn mir_diagnostic_message(diagnostic: &MirDiagnostic) -> String {
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
