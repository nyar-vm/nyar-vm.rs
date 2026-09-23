//! 线性类型与使用权检查骨架。

use crate::types::hir::ValkyrieType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageKind {
    Copy,
    Move,
    Borrow,
    MutBorrow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinearTypeError {
    UseAfterMove(String),
    DoubleMutableBorrow(String),
}

#[derive(Debug, Default)]
pub struct LinearTypeChecker;

impl LinearTypeChecker {
    pub fn check_type(&self, _ty: &ValkyrieType) -> Result<(), LinearTypeError> {
        Ok(())
    }
}
