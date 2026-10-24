use rusty_valkyrie::{
    hir::bind_construct_fields,
    types::hir::{HirDocumentation, HirExpr, HirExprKind, HirField, HirVisibility, ValkyrieType},
    *,
};

fn field(name: &str) -> HirField {
    HirField {
        id: None,
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
                kind: HirExprKind::Literal(rusty_valkyrie::types::hir::HirLiteral::Integer64(value)),
                span: rusty_valkyrie::types::SourceSpan::new(rusty_valkyrie::types::SourceID::default(), 0, 0),
            }),
            field_id: None,
        },
        span: rusty_valkyrie::types::SourceSpan::new(rusty_valkyrie::types::SourceID::default(), 0, 0),
    }
}

#[test]
fn binds_out_of_order_fields() {
    let ordered = bind_construct_fields(&[field("x"), field("y")], &[field_init("y", 2), field_init("x", 1)]).expect("bind");
    assert_eq!(ordered.len(), 2);
}
