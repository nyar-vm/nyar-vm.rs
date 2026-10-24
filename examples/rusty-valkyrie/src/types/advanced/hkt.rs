//! Higher-Kinded Types (HKT) 检查骨架。

use crate::types::hir::ValkyrieType;

/// 类型构造器标识（如 `Option`、`Future`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeConstructor {
    /// 构造器名称。
    pub name: String,
    /// 类型参数个数（元数）。
    pub arity: usize,
}

/// HKT 相关错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HktError {
    /// 元数不匹配。
    ArityMismatch {
        /// 期望元数。
        expected: usize,
        /// 实际元数。
        found: usize,
    },
    /// 未知类型构造器。
    UnknownConstructor(String),
}

/// HKT 检查器。
#[derive(Debug, Default)]
pub struct HktChecker {
    constructors: Vec<TypeConstructor>,
}

impl HktChecker {
    /// 注册类型构造器。
    pub fn register_constructor(&mut self, constructor: TypeConstructor) {
        self.constructors.push(constructor);
    }

    /// 检查类型是否满足 HKT 约束。
    pub fn check_type(&self, _ty: &ValkyrieType) -> Result<(), HktError> {
        Ok(())
    }
}
