use std::collections::BTreeMap;

use crate::types::{
    Identifier,
    hir::{HirExpr, HirExprKind, HirLiteral, HirResolvedCall, HirStatementKind, ValkyrieType as HirType},
};

use crate::hir::nullable_payload_type;

fn signed_int64_type() -> HirType {
    HirType::Integer64 { signed: true }
}

fn bool_type() -> HirType {
    HirType::Boolean
}

/// 类型推断中的新鲜类型变量标识（按创建顺序编号）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InferenceTypeVar(pub usize);

/// 类型推断失败的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeError {
    /// 引用了未绑定的变量。
    UnboundVariable {
        /// 未绑定的变量名。
        name: Identifier,
    },
    /// 期望类型与实际类型不一致。
    Mismatch {
        /// 期望的类型。
        expected: HirType,
        /// 实际得到的类型。
        found: HirType,
    },
    /// 该表达式形态不支持类型推断。
    UnsupportedExpression,
}

/// 轻量局部类型推断：维护变量名到类型的绑定并提供 unify。
#[derive(Debug, Default)]
pub struct TypeInference {
    next_var: usize,
    variables: BTreeMap<Identifier, HirType>,
}

impl TypeInference {
    /// 创建空的类型推断实例。
    pub fn new() -> Self {
        Self::default()
    }

    /// 分配一个新的新鲜类型变量并递增计数器。
    pub fn fresh_var(&mut self) -> InferenceTypeVar {
        let current = self.next_var;
        self.next_var += 1;
        InferenceTypeVar(current)
    }

    /// 将变量名绑定到给定类型。
    pub fn bind_variable(&mut self, name: Identifier, ty: HirType) {
        self.variables.insert(name, ty);
    }

    /// 返回变量名当前绑定的类型。
    pub fn get_variable_type(&self, name: &Identifier) -> Option<&HirType> {
        self.variables.get(name)
    }

    /// 推断表达式类型；无法处理的形态返回 `UnsupportedExpression`。
    pub fn infer(&mut self, expr: &HirExpr) -> Result<HirType, TypeError> {
        match &expr.kind {
            HirExprKind::Literal(HirLiteral::Integer64(_)) => Ok(signed_int64_type()),
            HirExprKind::Literal(HirLiteral::Float64(_)) => Ok(HirType::Float64),
            HirExprKind::Literal(HirLiteral::String(_)) => Ok(HirType::Utf8),
            HirExprKind::Literal(HirLiteral::Bool(_)) => Ok(bool_type()),
            HirExprKind::Literal(HirLiteral::Unit) => Ok(HirType::Unit),
            HirExprKind::Variable(identifier) => {
                self.variables.get(&identifier.name).cloned().ok_or_else(|| TypeError::UnboundVariable { name: identifier.name.clone() })
            }
            HirExprKind::Call { resolved, .. } => self.infer_call(resolved.as_ref()),
            HirExprKind::If { condition, then_branch, else_branch }
            | HirExprKind::IfLet { scrutinee: condition, then_branch, else_branch, .. } => {
                let condition_ty = self.infer(condition)?;
                self.unify(&condition_ty, &bool_type())?;
                let then_ty = infer_block_type(self, then_branch)?;
                let else_ty = else_branch.as_deref().map(|branch| infer_block_type(self, branch)).transpose()?.unwrap_or(HirType::Unit);
                self.unify(&then_ty, &else_ty)?;
                Ok(then_ty)
            }
            HirExprKind::TryPropagate { inner, .. } => {
                let inner_ty = self.infer(inner)?;
                Ok(nullable_payload_type(&inner_ty).unwrap_or(inner_ty))
            }
            HirExprKind::TryScope { is_optional, result_type, body, .. } => {
                if let Some(ty) = result_type {
                    return Ok(ty.clone());
                }
                let body_ty = infer_block_type(self, body)?;
                if *is_optional { Ok(HirType::Union(vec![body_ty, HirType::Named(Identifier::new("null"))])) } else { Ok(body_ty) }
            }
            _ => Err(TypeError::UnsupportedExpression),
        }
    }

    fn infer_call(&mut self, resolved: Option<&HirResolvedCall>) -> Result<HirType, TypeError> {
        resolved.map(|call| call.return_type.clone()).ok_or(TypeError::UnsupportedExpression)
    }

    /// 统一左右两侧类型：数组、函数、元组逐项递归，其余按相等判定。
    pub fn unify(&mut self, left: &HirType, right: &HirType) -> Result<(), TypeError> {
        if left == &HirType::AutoType || right == &HirType::AutoType {
            return Ok(());
        }
        match (left, right) {
            (HirType::Array(lhs), HirType::Array(rhs)) => self.unify(lhs, rhs),
            (HirType::Function(lhs_fn), HirType::Function(rhs_fn)) => {
                let lhs_params = &lhs_fn.params;
                let rhs_params = &rhs_fn.params;
                if lhs_params.len() != rhs_params.len() {
                    return Err(TypeError::Mismatch { expected: left.clone(), found: right.clone() });
                }
                for (lhs, rhs) in lhs_params.iter().zip(rhs_params) {
                    self.unify(lhs, rhs)?;
                }
                self.unify(&lhs_fn.return_type, &rhs_fn.return_type)
            }
            (HirType::Tuple(lhs), HirType::Tuple(rhs)) => {
                if lhs.len() != rhs.len() {
                    return Err(TypeError::Mismatch { expected: left.clone(), found: right.clone() });
                }
                for (lhs, rhs) in lhs.iter().zip(rhs) {
                    self.unify(lhs, rhs)?;
                }
                Ok(())
            }
            _ if left == right => Ok(()),
            _ => Err(TypeError::Mismatch { expected: left.clone(), found: right.clone() }),
        }
    }

    /// 应用当前替换表到给定类型并返回结果。
    pub fn apply_subst(&self, ty: &HirType) -> HirType {
        ty.clone()
    }

    /// 判断类型是否为数值类型（整型或浮点）。
    pub fn is_numeric(&self, ty: &HirType) -> bool {
        matches!(ty, HirType::Integer32 { signed: _ } | HirType::Integer64 { signed: _ } | HirType::Float32 | HirType::Float64)
    }

    /// 判断类型是否为整型。
    pub fn is_integer(&self, ty: &HirType) -> bool {
        matches!(ty, HirType::Integer32 { signed: _ } | HirType::Integer64 { signed: _ })
    }

    /// 清空类型变量计数与全部变量绑定。
    pub fn clear(&mut self) {
        self.next_var = 0;
        self.variables.clear();
    }
}

fn infer_block_type(inference: &mut TypeInference, block: &crate::types::hir::HirBlock) -> Result<HirType, TypeError> {
    for statement in &block.statements {
        if let HirStatementKind::Expr(expr) = &statement.kind {
            let _ = inference.infer(expr)?;
        }
    }

    match &block.expr {
        Some(expr) => inference.infer(expr),
        None => Ok(HirType::Unit),
    }
}
