//! 线性类型与使用权检查骨架。

use crate::types::hir::ValkyrieType;

/// 值的使用方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageKind {
    /// 按值复制。
    Copy,
    /// 按值移动。
    Move,
    /// 不可变借用。
    Borrow,
    /// 可变借用。
    MutBorrow,
}

/// 线性类型检查错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinearTypeError {
    /// 移动后再次使用。
    UseAfterMove(String),
    /// 重复可变借用。
    DoubleMutableBorrow(String),
}

/// 线性类型检查器。
#[derive(Debug, Default)]
pub struct LinearTypeChecker;

impl LinearTypeChecker {
    /// 检查类型是否满足线性/仿射约束。
    pub fn check_type(&self, _ty: &ValkyrieType) -> Result<(), LinearTypeError> {
        Ok(())
    }
}
