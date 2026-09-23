use nyar_language::{
    type_checker::*,
    types::{
        Identifier, NamePath, SourceID, SourceSpan,
        hir::{FunctionType, HirBlock, HirCallArgument, HirExpr, HirExprKind, HirLiteral, ValkyrieType},
    },
};

fn test_span() -> SourceSpan {
    SourceSpan::new(SourceID::default(), 0, 0)
}

fn operator_call(name: &str, args: Vec<HirExpr>) -> HirExpr {
    HirExpr {
        kind: HirExprKind::Call {
            callee: Box::new(HirExpr { kind: HirExprKind::Path(NamePath::new(vec![Identifier::new(name)])), span: test_span() }),
            args: args.into_iter().map(HirCallArgument::positional).collect(),
            resolved: None,
        },
        span: test_span(),
    }
}

#[test]
fn test_int_literal() {
    let mut inf = TypeInference::new();
    let expr = HirExpr { kind: HirExprKind::Literal(HirLiteral::Integer64(42)), span: test_span() };
    let t = inf.infer(&expr).unwrap();
    assert_eq!(t, ValkyrieType::Integer64 { signed: true });
}

#[test]
fn test_float_literal() {
    let mut inf = TypeInference::new();
    let expr = HirExpr { kind: HirExprKind::Literal(HirLiteral::Float64(ordered_float::OrderedFloat(3.14))), span: test_span() };
    let t = inf.infer(&expr).unwrap();
    assert_eq!(t, ValkyrieType::Float64);
}

#[test]
fn test_bool_literal() {
    let mut inf = TypeInference::new();
    let expr = HirExpr { kind: HirExprKind::Literal(HirLiteral::Bool(true)), span: test_span() };
    let t = inf.infer(&expr).unwrap();
    assert_eq!(t, ValkyrieType::Boolean);
}

#[test]
fn test_string_literal_defaults_to_utf8() {
    let mut inf = TypeInference::new();
    let expr = HirExpr {
        kind: HirExprKind::Literal(HirLiteral::String(nyar_language::types::hir::HirStringLiteral {
            prefix: None,
            quote_count: 1,
            segments: vec![nyar_language::types::hir::HirStringSegment::Text("hello".to_string())],
        })),
        span: test_span(),
    };
    let t = inf.infer(&expr).unwrap();
    assert_eq!(t, ValkyrieType::Utf8);
}

#[test]
fn test_unit_literal() {
    let mut inf = TypeInference::new();
    let expr = HirExpr { kind: HirExprKind::Literal(HirLiteral::Unit), span: test_span() };
    let t = inf.infer(&expr).unwrap();
    assert_eq!(t, ValkyrieType::Unit);
}

#[test]
fn test_variable_propagation() {
    let mut inf = TypeInference::new();
    inf.bind_variable(Identifier::new("x"), ValkyrieType::Integer64 { signed: true });

    let expr = HirExpr {
        kind: HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
            name: Identifier::new("x"),
            shadow_index: 0,
            span: test_span(),
        }),
        span: test_span(),
    };
    let t = inf.infer(&expr).unwrap();
    assert_eq!(t, ValkyrieType::Integer64 { signed: true });
}

#[test]
fn test_unbound_variable() {
    let mut inf = TypeInference::new();
    let expr = HirExpr {
        kind: HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
            name: Identifier::new("unknown"),
            shadow_index: 0,
            span: test_span(),
        }),
        span: test_span(),
    };
    let result = inf.infer(&expr);
    assert!(matches!(result, Err(TypeError::UnboundVariable { .. })));
}

#[test]
fn test_binary_add() {
    let mut inf = TypeInference::new();
    let left = HirExpr { kind: HirExprKind::Literal(HirLiteral::Integer64(1)), span: test_span() };
    let right = HirExpr { kind: HirExprKind::Literal(HirLiteral::Integer64(2)), span: test_span() };
    let expr = operator_call("infix +", vec![left, right]);
    let t = inf.infer(&expr).unwrap();
    assert_eq!(t, ValkyrieType::Integer64 { signed: true });
}

#[test]
fn test_binary_comparison() {
    let mut inf = TypeInference::new();
    let left = HirExpr { kind: HirExprKind::Literal(HirLiteral::Integer64(1)), span: test_span() };
    let right = HirExpr { kind: HirExprKind::Literal(HirLiteral::Integer64(2)), span: test_span() };
    let expr = operator_call("infix <", vec![left, right]);
    let t = inf.infer(&expr).unwrap();
    assert_eq!(t, ValkyrieType::Boolean);
}

#[test]
fn test_bool_equality_call() {
    let mut inf = TypeInference::new();
    let left = HirExpr { kind: HirExprKind::Literal(HirLiteral::Bool(true)), span: test_span() };
    let right = HirExpr { kind: HirExprKind::Literal(HirLiteral::Bool(false)), span: test_span() };
    let expr = operator_call("infix ==", vec![left, right]);
    let t = inf.infer(&expr).unwrap();
    assert_eq!(t, ValkyrieType::Boolean);
}

#[test]
fn test_unary_neg() {
    let mut inf = TypeInference::new();
    let inner = HirExpr { kind: HirExprKind::Literal(HirLiteral::Integer64(42)), span: test_span() };
    let expr = operator_call("prefix -", vec![inner]);
    let t = inf.infer(&expr).unwrap();
    assert_eq!(t, ValkyrieType::Integer64 { signed: true });
}

#[test]
fn test_unary_not() {
    let mut inf = TypeInference::new();
    let inner = HirExpr { kind: HirExprKind::Literal(HirLiteral::Bool(true)), span: test_span() };
    let expr = operator_call("prefix !", vec![inner]);
    let t = inf.infer(&expr).unwrap();
    assert_eq!(t, ValkyrieType::Boolean);
}

#[test]
fn test_if_expression_bool_result() {
    let mut inf = TypeInference::new();
    let expr = HirExpr {
        kind: HirExprKind::If {
            condition: Box::new(HirExpr { kind: HirExprKind::Literal(HirLiteral::Bool(true)), span: test_span() }),
            then_branch: Box::new(HirBlock {
                statements: Vec::new(),
                expr: Some(Box::new(HirExpr { kind: HirExprKind::Literal(HirLiteral::Bool(false)), span: test_span() })),
                span: test_span(),
            }),
            else_branch: Some(Box::new(HirBlock {
                statements: Vec::new(),
                expr: Some(Box::new(HirExpr { kind: HirExprKind::Literal(HirLiteral::Bool(true)), span: test_span() })),
                span: test_span(),
            })),
        },
        span: test_span(),
    };
    let t = inf.infer(&expr).unwrap();
    assert_eq!(t, ValkyrieType::Boolean);
}

#[test]
fn test_fresh_var() {
    let mut inf = TypeInference::new();
    let v1 = inf.fresh_var();
    let v2 = inf.fresh_var();
    let v3 = inf.fresh_var();

    assert_eq!(v1, InferenceTypeVar(0));
    assert_eq!(v2, InferenceTypeVar(1));
    assert_eq!(v3, InferenceTypeVar(2));
}

#[test]
fn test_unify_same_types() {
    let mut inf = TypeInference::new();
    let result = inf.unify(&ValkyrieType::Integer64 { signed: true }, &ValkyrieType::Integer64 { signed: true });
    assert!(result.is_ok());
}

#[test]
fn test_unify_different_types() {
    let mut inf = TypeInference::new();
    let result = inf.unify(&ValkyrieType::Integer64 { signed: true }, &ValkyrieType::Boolean);
    assert!(matches!(result, Err(TypeError::Mismatch { .. })));
}

#[test]
fn test_unify_with_infer() {
    let mut inf = TypeInference::new();
    let result = inf.unify(&ValkyrieType::AutoType, &ValkyrieType::Integer64 { signed: true });
    assert!(result.is_ok());

    let result = inf.unify(&ValkyrieType::Integer64 { signed: true }, &ValkyrieType::AutoType);
    assert!(result.is_ok());
}

#[test]
fn test_unify_array_types() {
    let mut inf = TypeInference::new();
    let t1 = ValkyrieType::Array(Box::new(ValkyrieType::Integer64 { signed: true }));
    let t2 = ValkyrieType::Array(Box::new(ValkyrieType::Integer64 { signed: true }));
    let result = inf.unify(&t1, &t2);
    assert!(result.is_ok());
}

#[test]
fn test_unify_function_types() {
    let mut inf = TypeInference::new();
    let t1 = ValkyrieType::Function(Box::new(FunctionType {
        params: vec![ValkyrieType::Integer64 { signed: true }],
        return_type: ValkyrieType::Boolean,
    }));
    let t2 = ValkyrieType::Function(Box::new(FunctionType {
        params: vec![ValkyrieType::Integer64 { signed: true }],
        return_type: ValkyrieType::Boolean,
    }));
    let result = inf.unify(&t1, &t2);
    assert!(result.is_ok());
}

#[test]
fn test_apply_subst() {
    let inf = TypeInference::new();
    let t = ValkyrieType::Array(Box::new(ValkyrieType::Integer64 { signed: true }));
    let result = inf.apply_subst(&t);
    assert_eq!(result, t);
}

#[test]
fn test_is_numeric() {
    let inf = TypeInference::new();

    assert!(inf.is_numeric(&ValkyrieType::Integer32 { signed: true }));
    assert!(inf.is_numeric(&ValkyrieType::Integer64 { signed: true }));
    assert!(inf.is_numeric(&ValkyrieType::Float32));
    assert!(inf.is_numeric(&ValkyrieType::Float64));

    assert!(!inf.is_numeric(&ValkyrieType::Boolean));
    assert!(!inf.is_numeric(&ValkyrieType::Utf8));
    assert!(!inf.is_numeric(&ValkyrieType::Unit));
}

#[test]
fn test_is_integer() {
    let inf = TypeInference::new();

    assert!(inf.is_integer(&ValkyrieType::Integer32 { signed: true }));
    assert!(inf.is_integer(&ValkyrieType::Integer64 { signed: true }));

    assert!(!inf.is_integer(&ValkyrieType::Float32));
    assert!(!inf.is_integer(&ValkyrieType::Float64));
    assert!(!inf.is_integer(&ValkyrieType::Boolean));
}

#[test]
fn test_clear() {
    let mut inf = TypeInference::new();
    inf.fresh_var();
    inf.fresh_var();
    inf.bind_variable(Identifier::new("x"), ValkyrieType::Integer64 { signed: true });

    inf.clear();

    let v = inf.fresh_var();
    assert_eq!(v, InferenceTypeVar(0));
    assert!(inf.get_variable_type(&Identifier::new("x")).is_none());
}

#[test]
fn infers_try_propagate_as_payload_type() {
    let mut inf = TypeInference::new();
    let inner = HirExpr { kind: HirExprKind::Literal(HirLiteral::Integer64(1)), span: test_span() };
    let expr = HirExpr {
        kind: HirExprKind::TryPropagate(Box::new(HirExpr { kind: HirExprKind::Literal(HirLiteral::Integer64(1)), span: test_span() })),
        span: test_span(),
    };
    let _ = inner;
    let ty = inf.infer(&expr).unwrap();
    assert_eq!(ty, ValkyrieType::Integer64 { signed: true });
}
