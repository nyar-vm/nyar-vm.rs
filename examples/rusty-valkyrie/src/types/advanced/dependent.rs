//! 依赖类型检查骨架。

use crate::types::hir::ValkyrieType;

/// 依赖类型检查错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependentTypeError {
    /// 索引越界。
    IndexOutOfBounds,
    /// 证明义务未满足。
    ProofObligationFailed(String),
}

/// 依赖类型检查器。
#[derive(Debug, Default)]
pub struct DependentTypeChecker;

impl DependentTypeChecker {
    /// 检查类型是否满足依赖类型约束。
    pub fn check_type(&self, _ty: &ValkyrieType) -> Result<(), DependentTypeError> {
        Ok(())
    }
}
