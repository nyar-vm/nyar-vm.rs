//! Higher-Kinded Types (HKT) 检查骨架。

use crate::types::hir::ValkyrieType;

/// 类型构造器标识（如 `Option`、`Future`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeConstructor {
    pub name: String,
    pub arity: usize,
}

/// HKT 相关错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HktError {
    ArityMismatch { expected: usize, found: usize },
    UnknownConstructor(String),
}

/// HKT 检查器。
#[derive(Debug, Default)]
pub struct HktChecker {
    constructors: Vec<TypeConstructor>,
}

impl HktChecker {
    pub fn register_constructor(&mut self, constructor: TypeConstructor) {
        self.constructors.push(constructor);
    }

    pub fn check_type(&self, _ty: &ValkyrieType) -> Result<(), HktError> {
        Ok(())
    }
}
