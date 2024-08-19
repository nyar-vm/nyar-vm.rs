//! MIR 模块级控制流调度校验入口。

use super::{function_validation::validate_function, semantic_contract::validate_semantic_module};
use crate::{
    mir::{MirDiagnostic, MirModule},
    valkyrie::HirError,
};

/// 校验 MIR 模块并在失败时返回前端解析错误。
pub fn validate_module(module: &MirModule) -> Result<(), HirError> {
    for diagnostic in &module.diagnostics {
        match diagnostic {
            MirDiagnostic::PatternLoweringFailed { reason, .. } => {
                return Err(HirError::invalid(format!("MIR lowering 失败：{reason}")));
            }
            MirDiagnostic::UnsupportedExpression { span, kind } => {
                return Err(HirError::invalid(format!("MIR lowering rejected unsupported HIR expression `{kind}` at source span {span:?}")));
            }
            MirDiagnostic::UnresolvedValueType { context } => {
                return Err(HirError::invalid(format!("MIR lowering missing value type fact: {context}")));
            }
            MirDiagnostic::UnresolvedVariantIdentity { sum_type, variant } => {
                return Err(HirError::invalid(format!("MIR lowering unresolved variant identity `{sum_type}::{variant}`")));
            }
            MirDiagnostic::UnresolvedNominalIdentity { type_name } => {
                return Err(HirError::invalid(format!("MIR lowering unresolved nominal identity `{type_name}`")));
            }
            MirDiagnostic::UnresolvedFieldIdentity { field } => {
                return Err(HirError::invalid(format!("MIR lowering unresolved field identity `{field}`")));
            }
            MirDiagnostic::UnresolvedCallableIdentity { symbol } => {
                return Err(HirError::invalid(format!("MIR lowering unresolved callable identity `{symbol}`")));
            }
        }
    }
    validate_semantic_module(module)
        .map_err(|error| HirError::invalid(format!("{} {} at {}: {}", error.code, error.function, error.location, error.detail)))?;
    for function in &module.functions {
        validate_function(function)?;
    }
    Ok(())
}
