//! MIR 模块级控制流调度校验入口。

use super::{function_validation::validate_function, semantic_contract::validate_semantic_module};
use crate::{
    mir::MirModule,
    valkyrie::HirError,
};

/// 校验 MIR 模块并在失败时返回前端解析错误。
pub fn validate_module(module: &MirModule) -> Result<(), HirError> {
    // MIR lowering 已经把不可继续 lowering 的构造冻结成结构化诊断；
    // 校验层只转发第一条本人生产的事实，不在 HIR 侧重拼场景字符串。
    if let Some(diagnostic) = module.diagnostics.first() {
        return Err(HirError::MirDiagnosticRejected { diagnostic: diagnostic.clone() });
    }
    validate_semantic_module(module).map_err(HirError::MirContractFailed)?;
    for function in &module.functions {
        validate_function(function)?;
    }
    Ok(())
}
