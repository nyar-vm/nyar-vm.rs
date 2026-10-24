//! Field-name binding for brace struct/constructor literals.

use crate::types::{
    Identifier,
    hir::{HirExpr, HirExprKind, HirField},
};

/// 结构体/构造字面量字段绑定错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstructBindingError {
    /// 实参不是字段初始化表达式。
    NotAFieldInit,
    /// 未知字段名。
    UnknownField {
        /// 字段名。
        name: Identifier,
    },
    /// 重复字段名。
    DuplicateField {
        /// 字段名。
        name: Identifier,
    },
    /// 缺少必需字段。
    MissingField {
        /// 字段名。
        name: Identifier,
    },
}

/// Bind `Point { y: 2, x: 1 }` field inits to declaration order.
pub fn bind_construct_fields(fields: &[HirField], args: &[HirExpr]) -> Result<Vec<HirExpr>, ConstructBindingError> {
    let mut bound: Vec<Option<HirExpr>> = vec![None; fields.len()];
    for arg in args {
        let HirExprKind::FieldInit { name, value, .. } = &arg.kind
        else {
            return Err(ConstructBindingError::NotAFieldInit);
        };
        let Some(index) = fields.iter().position(|field| field.name == *name)
        else {
            return Err(ConstructBindingError::UnknownField { name: name.clone() });
        };
        if bound[index].is_some() {
            return Err(ConstructBindingError::DuplicateField { name: name.clone() });
        }
        bound[index] = Some(*value.clone());
    }
    let mut ordered = Vec::with_capacity(fields.len());
    for (field, value) in fields.iter().zip(bound) {
        let Some(value) = value
        else {
            return Err(ConstructBindingError::MissingField { name: field.name.clone() });
        };
        ordered.push(value);
    }
    Ok(ordered)
}
