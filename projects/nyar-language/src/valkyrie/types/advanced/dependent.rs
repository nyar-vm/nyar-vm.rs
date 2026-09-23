//! 依赖类型检查骨架。

use crate::types::hir::ValkyrieType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependentTypeError {
    IndexOutOfBounds,
    ProofObligationFailed(String),
}

#[derive(Debug, Default)]
pub struct DependentTypeChecker;

impl DependentTypeChecker {
    pub fn check_type(&self, _ty: &ValkyrieType) -> Result<(), DependentTypeError> {
        Ok(())
    }
}
