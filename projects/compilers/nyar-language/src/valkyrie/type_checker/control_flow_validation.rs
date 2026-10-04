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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InferenceTypeVar(pub usize);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeError {
    UnboundVariable { name: Identifier },
    Mismatch { expected: HirType, found: HirType },
    UnsupportedExpression,
}

#[derive(Debug, Default)]
pub struct TypeInference {
    next_var: usize,
    variables: BTreeMap<Identifier, HirType>,
}

impl TypeInference {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fresh_var(&mut self) -> InferenceTypeVar {
        let current = self.next_var;
        self.next_var += 1;
        InferenceTypeVar(current)
    }

    pub fn bind_variable(&mut self, name: Identifier, ty: HirType) {
        self.variables.insert(name, ty);
    }

    pub fn get_variable_type(&self, name: &Identifier) -> Option<&HirType> {
        self.variables.get(name)
    }

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
            HirExprKind::TryPropagate(inner) => {
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

    pub fn apply_subst(&self, ty: &HirType) -> HirType {
        ty.clone()
    }

    pub fn is_numeric(&self, ty: &HirType) -> bool {
        matches!(ty, HirType::Integer32 { signed: _ } | HirType::Integer64 { signed: _ } | HirType::Float32 | HirType::Float64)
    }

    pub fn is_integer(&self, ty: &HirType) -> bool {
        matches!(ty, HirType::Integer32 { signed: _ } | HirType::Integer64 { signed: _ })
    }

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

#[cfg(test)]
mod tests {
    use super::{TypeError, TypeInference};
    use crate::{types::{Identifier, NamePath, SourceID, SourceSpan, hir::{HirExpr, HirExprKind}}, valkyrie::hir::HirResolvedCall};

    fn span() -> SourceSpan {
        SourceSpan::new(SourceID::default(), 0, 0)
    }

    #[test]
    fn unresolved_operator_name_cannot_supply_a_result_type() {
        let expression = HirExpr {
            kind: HirExprKind::Call {
                call_kind: crate::types::hir::HirCallKind::Operator(nyar_types::builtin_operator::infix_eq()),
                callee: Box::new(HirExpr { kind: HirExprKind::Path(NamePath::new(vec![Identifier::new("infix ==")])), span: span() }),
                args: Vec::new(),
                resolved: None,
            },
            span: span(),
        };
        assert_eq!(TypeInference::new().infer(&expression), Err(TypeError::UnsupportedExpression));
    }

    #[test]
    fn resolved_call_result_comes_from_its_signature() {
        let expression = HirExpr {
            kind: HirExprKind::Call {
                call_kind: crate::types::hir::HirCallKind::Operator(nyar_types::builtin_operator::infix_add()),
                callee: Box::new(HirExpr { kind: HirExprKind::Path(NamePath::new(vec![Identifier::new("infix +")])), span: span() }),
                args: Vec::new(),
                resolved: Some(HirResolvedCall {
                    declaration: None,
                    instance: None,
                    symbol: NamePath::new(vec![Identifier::new("declared_callable")]),
                    domain: crate::types::hir::HirCallableDomain::Operator,
                    return_type: crate::types::hir::ValkyrieType::Boolean,
                    parameter_types: Vec::new(),
                    has_receiver: false,
                    extractor_payload_type: None,
                }),
            },
            span: span(),
        };
        assert_eq!(TypeInference::new().infer(&expression), Ok(crate::types::hir::ValkyrieType::Boolean));
    }
}
