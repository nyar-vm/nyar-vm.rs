use rusty_valkyrie::{
    types::{SourceSpan, hir::HirIdentifier},
    {
        hir::call_binding::{CallBindingError, bind_call_arguments},
        types::hir::{
            HirCallArgument, HirExpr,
            types::{HirParam, HirParameterBindingKind, HirVariadicKind},
        },
    },
    *,
};

fn param(name: &str, binding_kind: HirParameterBindingKind) -> HirParam {
    HirParam {
        name: HirIdentifier { name: Identifier::new(name), shadow_index: 0, span: SourceSpan::new(SourceID::default(), 0, 0) },
        ty: rusty_valkyrie::types::hir::ValkyrieType::Unit,
        is_mutable: false,
        binding_kind,
        default: None,
        variadic: HirVariadicKind::None,
    }
}

fn param_with_default(name: &str, binding_kind: HirParameterBindingKind, default: i64) -> HirParam {
    HirParam {
        default: Some(HirExpr {
            kind: rusty_valkyrie::types::hir::HirExprKind::Literal(rusty_valkyrie::types::hir::HirLiteral::Integer64(default)),
            span: SourceSpan::new(SourceID::default(), 0, 0),
        }),
        ..param(name, binding_kind)
    }
}

fn positional(value: i64) -> HirCallArgument {
    HirCallArgument::positional(HirExpr {
        kind: rusty_valkyrie::types::hir::HirExprKind::Literal(rusty_valkyrie::types::hir::HirLiteral::Integer64(value)),
        span: SourceSpan::new(SourceID::default(), 0, 0),
    })
}

fn keyword(name: &str, value: i64) -> HirCallArgument {
    HirCallArgument {
        name: Some(Identifier::new(name)),
        value: HirExpr {
            kind: rusty_valkyrie::types::hir::HirExprKind::Literal(rusty_valkyrie::types::hir::HirLiteral::Integer64(value)),
            span: SourceSpan::new(SourceID::default(), 0, 0),
        },
    }
}

#[test]
fn binds_mixed_positional_and_keyword() {
    let params = vec![
        param("a", HirParameterBindingKind::PositionalOnly),
        param("b", HirParameterBindingKind::PositionalOrKeyword),
        param("c", HirParameterBindingKind::KeywordOnly),
    ];
    let args = vec![positional(1), positional(2), keyword("c", 3)];
    let bound = bind_call_arguments(&params, &args).expect("bind");
    assert_eq!(bound.len(), 3);
}

#[test]
fn fills_default_arguments() {
    let params = vec![
        param("a", HirParameterBindingKind::PositionalOnly),
        param_with_default("b", HirParameterBindingKind::PositionalOrKeyword, 1),
        param_with_default("c", HirParameterBindingKind::KeywordOnly, 0),
    ];
    let bound = bind_call_arguments(&params, &[positional(9)]).expect("bind");
    assert_eq!(bound.len(), 3);
}

#[test]
fn rejects_positional_only_by_name() {
    let params = vec![param("a", HirParameterBindingKind::PositionalOnly)];
    let err = bind_call_arguments(&params, &[keyword("a", 1)]).unwrap_err();
    assert!(matches!(err, CallBindingError::PositionalOnlyPassedByName { .. }));
}

#[test]
fn rejects_keyword_only_positionally() {
    let params = vec![param("c", HirParameterBindingKind::KeywordOnly)];
    let err = bind_call_arguments(&params, &[positional(1)]).unwrap_err();
    assert!(matches!(err, CallBindingError::KeywordOnlyPassedPositionally { .. }));
}
