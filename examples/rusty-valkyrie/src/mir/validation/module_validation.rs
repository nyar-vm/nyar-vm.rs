//! MIR 模块级控制流调度校验入口。

use super::{errors::MirValidationError, function_validation::validate_function, semantic_contract::validate_semantic_module};
use crate::mir::MirModule;

/// 校验 MIR 模块并返回本阶段自己生产的结构化错误。
pub fn validate_module(module: &MirModule) -> Result<(), MirValidationError> {
    // MIR lowering 已经把不可继续 lowering 的构造冻结成结构化诊断；
    // 校验层只转发第一条本人生产的事实，不在校验侧重拼场景字符串。
    if let Some(diagnostic) = module.diagnostics.first() {
        return Err(MirValidationError::DiagnosticRejected { diagnostic: diagnostic.clone() });
    }
    validate_semantic_module(module).map_err(MirValidationError::ContractFailed)?;
    for function in &module.functions {
        validate_function(function)?;
    }
    Ok(())
}
