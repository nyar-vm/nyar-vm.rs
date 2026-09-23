//! 所有权 `mut` / `ref` / `own` 语义检查骨架。

use crate::types::{Identifier, SourceSpan};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnershipMode {
    Ref,
    Mut,
    Own,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnershipError {
    InvalidOwnershipTransition { name: Identifier, span: Option<SourceSpan> },
    EscapingBorrow { name: Identifier, span: Option<SourceSpan> },
}

#[derive(Debug, Default)]
pub struct OwnershipChecker;

impl OwnershipChecker {
    pub fn check_binding(&self, _name: &Identifier, _mode: OwnershipMode) -> Result<(), OwnershipError> {
        Ok(())
    }
}
