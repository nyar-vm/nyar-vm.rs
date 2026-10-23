use nyar_language::{
    types::{
        Identifier, NamePath, SourceID, SourceSpan,
        hir::{HirExpr, HirExprKind},
    },
    valkyrie::{
        hir::HirResolvedCall,
        type_checker::{TypeError, TypeInference},
    },
};

fn span() -> SourceSpan {
    SourceSpan::new(SourceID::default(), 0, 0)
}

#[test]
fn unresolved_member_call_cannot_supply_a_result_type() {
    let expression = HirExpr {
        kind: HirExprKind::Call {
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
            callee: Box::new(HirExpr { kind: HirExprKind::Path(NamePath::new(vec![Identifier::new("infix +")])), span: span() }),
            args: Vec::new(),
            resolved: Some(HirResolvedCall {
                declaration: None,
                instance: None,
                intrinsic: None,
                sum_owner: None,
                sum_variant: None,
                symbol: NamePath::new(vec![Identifier::new("declared_callable")]),
                domain: nyar_language::types::hir::HirCallableDomain::Function,
                return_type: nyar_language::types::hir::ValkyrieType::Boolean,
                parameter_types: Vec::new(),
                has_receiver: false,
                extractor_payload_type: None,
            }),
        },
        span: span(),
    };
    assert_eq!(TypeInference::new().infer(&expression), Ok(nyar_language::types::hir::ValkyrieType::Boolean));
}
