//! 所有权 `mut` / `ref` / `own` 语义检查骨架。

use crate::types::{Identifier, SourceSpan};

/// 所有权模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnershipMode {
    /// 不可变引用。
    Ref,
    /// 可变引用。
    Mut,
    /// 拥有所有权。
    Own,
}

/// 所有权检查错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnershipError {
    /// 非法所有权转移。
    InvalidOwnershipTransition {
        /// 绑定名。
        name: Identifier,
        /// 相关源码跨度。
        span: Option<SourceSpan>,
    },
    /// 借用逃逸。
    EscapingBorrow {
        /// 绑定名。
        name: Identifier,
        /// 相关源码跨度。
        span: Option<SourceSpan>,
    },
}

/// 所有权检查器。
#[derive(Debug, Default)]
pub struct OwnershipChecker;

impl OwnershipChecker {
    /// 检查绑定是否满足给定所有权模式。
    pub fn check_binding(&self, _name: &Identifier, _mode: OwnershipMode) -> Result<(), OwnershipError> {
        Ok(())
    }
}
