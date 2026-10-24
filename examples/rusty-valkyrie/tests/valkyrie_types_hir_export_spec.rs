use rusty_valkyrie::{
    types::hir::{
        HirArgument, HirAttribute, HirExpr, HirExprKind, parse_export_spec_from_annotations, parse_workload_phase_from_annotations,
        resolve_attribute_id, snake_case_to_camel_case,
        types::{HirLiteral, HirStringLiteral, HirStringSegment},
    },
    *,
};
use nyar_types::builtin_attribute;

#[test]
fn snake_case_to_camel_case_examples() {
    assert_eq!(snake_case_to_camel_case("two_sum"), "twoSum");
    assert_eq!(snake_case_to_camel_case("max_sub_array"), "maxSubArray");
    assert_eq!(snake_case_to_camel_case("api_ping"), "apiPing");
}

#[test]
fn attribute_id_maps_export_and_main() {
    let export = HirAttribute::new(NamePath::new(vec![Identifier::new("export")]));
    let main = HirAttribute::new(NamePath::new(vec![Identifier::new("main")]));
    assert_eq!(resolve_attribute_id(&export), Some(builtin_attribute::export()));
    assert_eq!(resolve_attribute_id(&main), Some(builtin_attribute::main()));
    let custom = HirAttribute::new(NamePath::new(vec![Identifier::new("my_attr")]));
    let custom_id = resolve_attribute_id(&custom).expect("user attribute interned");
    assert_ne!(custom_id, builtin_attribute::export());
    assert_eq!(resolve_attribute_id(&custom), Some(custom_id));
}

#[test]
fn workload_phase_parses_positional_and_named() {
    let positional = HirArgument {
        key: None,
        value: Box::new(HirExpr {
            kind: HirExprKind::Literal(HirLiteral::String(HirStringLiteral {
                prefix: None,
                quote_count: 1,
                segments: vec![HirStringSegment::Text("request".into())],
            })),
            span: SourceSpan { source: rusty_valkyrie::types::SourceID { version_id: 0 }, span: (0..0).into() },
        }),
    };
    let annotations = vec![HirAttribute::with_arguments(NamePath::new(vec![Identifier::new("workload_phase")]), vec![positional])];
    assert_eq!(parse_workload_phase_from_annotations(&annotations).as_deref(), Some("request"));

    let named = HirArgument {
        key: Some(Identifier::new("name")),
        value: Box::new(HirExpr {
            kind: HirExprKind::Literal(HirLiteral::String(HirStringLiteral {
                prefix: None,
                quote_count: 1,
                segments: vec![HirStringSegment::Text("batch".into())],
            })),
            span: SourceSpan { source: rusty_valkyrie::types::SourceID { version_id: 0 }, span: (0..0).into() },
        }),
    };
    let annotations = vec![HirAttribute::with_arguments(NamePath::new(vec![Identifier::new("workload_phase")]), vec![named])];
    assert_eq!(parse_workload_phase_from_annotations(&annotations).as_deref(), Some("batch"));
}

#[test]
fn workload_phase_rejects_empty_name() {
    let empty = HirArgument {
        key: None,
        value: Box::new(HirExpr {
            kind: HirExprKind::Literal(HirLiteral::String(HirStringLiteral {
                prefix: None,
                quote_count: 1,
                segments: vec![HirStringSegment::Text(String::new())],
            })),
            span: SourceSpan { source: rusty_valkyrie::types::SourceID { version_id: 0 }, span: (0..0).into() },
        }),
    };
    let annotations = vec![HirAttribute::with_arguments(NamePath::new(vec![Identifier::new("workload_phase")]), vec![empty])];
    assert!(parse_workload_phase_from_annotations(&annotations).is_none());
}

#[test]
fn illegal_export_case_fails_closed() {
    let case_arg = HirArgument {
        key: Some(Identifier::new("case")),
        value: Box::new(HirExpr {
            kind: HirExprKind::Literal(HirLiteral::String(HirStringLiteral {
                prefix: None,
                quote_count: 1,
                segments: vec![HirStringSegment::Text("PascalCase".into())],
            })),
            span: SourceSpan { source: rusty_valkyrie::types::SourceID { version_id: 0 }, span: (0..0).into() },
        }),
    };
    let annotations = vec![HirAttribute::with_arguments(NamePath::new(vec![Identifier::new("export")]), vec![case_arg])];
    assert!(parse_export_spec_from_annotations(&annotations).is_none());
}
