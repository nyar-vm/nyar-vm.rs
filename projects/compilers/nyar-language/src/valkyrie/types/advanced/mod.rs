//! 高级类型系统扩展：HKT、依赖类型、线性类型与所有权语义。

pub mod dependent;
pub mod hkt;
pub mod linear;
pub mod ownership;

pub use dependent::{DependentTypeChecker, DependentTypeError};
pub use hkt::{HktChecker, HktError, TypeConstructor};
pub use linear::{LinearTypeChecker, LinearTypeError, UsageKind};
pub use ownership::{OwnershipChecker, OwnershipError, OwnershipMode};

use crate::types::hir::{HirModule, ValkyrieType};

/// 高级类型检查会话，组合 HKT / 依赖 / 线性 / 所有权四类检查器。
#[derive(Debug, Default)]
pub struct AdvancedTypeSession {
    hkt: HktChecker,
    dependent: DependentTypeChecker,
    linear: LinearTypeChecker,
    ownership: OwnershipChecker,
}

impl AdvancedTypeSession {
    /// 创建空会话。
    pub fn new() -> Self {
        Self::default()
    }

    /// 对模块执行高级类型检查，返回首个错误（若有）。
    pub fn check_module(&mut self, module: &HirModule) -> Result<(), AdvancedTypeError> {
        for function in &module.functions {
            self.check_function_signature(&function.return_type)?;
            for param in &function.params {
                self.check_type(&param.ty)?;
            }
        }
        Ok(())
    }

    fn check_type(&mut self, ty: &ValkyrieType) -> Result<(), AdvancedTypeError> {
        self.hkt.check_type(ty).map_err(AdvancedTypeError::Hkt)?;
        self.dependent.check_type(ty).map_err(AdvancedTypeError::Dependent)?;
        self.linear.check_type(ty).map_err(AdvancedTypeError::Linear)?;
        Ok(())
    }

    fn check_function_signature(&mut self, ty: &ValkyrieType) -> Result<(), AdvancedTypeError> {
        self.check_type(ty)
    }
}

/// 高级类型检查错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvancedTypeError {
    Hkt(HktError),
    Dependent(DependentTypeError),
    Linear(LinearTypeError),
    Ownership(OwnershipError),
}
