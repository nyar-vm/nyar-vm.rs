//! Field-name binding for brace struct/constructor literals.

use crate::types::{
    Identifier,
    hir::{HirExpr, HirExprKind, HirField},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstructBindingError {
    NotAFieldInit,
    UnknownField { name: Identifier },
    DuplicateField { name: Identifier },
    MissingField { name: Identifier },
}

/// Bind `Point { y: 2, x: 1 }` field inits to declaration order.
pub fn bind_construct_fields(fields: &[HirField], args: &[HirExpr]) -> Result<Vec<HirExpr>, ConstructBindingError> {
    let mut bound: Vec<Option<HirExpr>> = vec![None; fields.len()];
    for arg in args {
        let HirExprKind::FieldInit { name, value } = &arg.kind
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::hir::{HirDocumentation, HirVisibility, ValkyrieType};

    fn field(name: &str) -> HirField {
        HirField {
            name: Identifier::new(name),
            doc: HirDocumentation::default(),
            ty: ValkyrieType::Integer64 { signed: true },
            visibility: HirVisibility::public(),
            is_mutable: false,
        }
    }

    fn field_init(name: &str, value: i64) -> HirExpr {
        HirExpr {
            kind: HirExprKind::FieldInit {
                name: Identifier::new(name),
                value: Box::new(HirExpr {
                    kind: HirExprKind::Literal(crate::types::hir::HirLiteral::Integer64(value)),
                    span: crate::types::SourceSpan::new(crate::types::SourceID::default(), 0, 0),
                }),
            },
            span: crate::types::SourceSpan::new(crate::types::SourceID::default(), 0, 0),
        }
    }

    #[test]
    fn binds_out_of_order_fields() {
        let ordered = bind_construct_fields(&[field("x"), field("y")], &[field_init("y", 2), field_init("x", 1)]).expect("bind");
        assert_eq!(ordered.len(), 2);
    }
}
