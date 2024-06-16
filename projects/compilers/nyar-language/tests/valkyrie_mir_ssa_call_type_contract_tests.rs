use nyar_language::{
    Identifier, NamePath, ValkyrieCompiler,
    types::hir::{HirExprKind, ValkyrieType},
    valkyrie::mir::ssa::{MirDiagnostic, MirLowerer, MirOperand, MirOperation, test_support::TestMirBuilder},
};

#[test]
fn missing_hir_static_contract_cannot_be_reconstructed_from_spelling() {
    let mut hir = ValkyrieCompiler::default()
        .compile_source("micro answer() -> i64 { 7 } micro caller() -> i64 { answer() }")
        .expect("源码必须先完成解析");
    let caller = hir.functions.iter_mut().find(|function| function.name.as_str() == "caller").unwrap();
    let HirExprKind::Call { resolved, .. } = &mut caller.body.expr.as_mut().unwrap().kind
    else {
        panic!("预期调用表达式");
    };
    assert!(resolved.take().is_some());
    let mir = MirLowerer::lower_module_semantic(&hir);
    assert!(mir.diagnostics.iter().any(|diagnostic| matches!(diagnostic, MirDiagnostic::UnresolvedCallableIdentity { .. })));
    let caller = mir.functions.iter().find(|function| function.symbol.ends_with("::caller")).unwrap();
    assert!(
        !caller.blocks.iter().flat_map(|block| &block.instructions).any(|instruction| matches!(instruction.kind, MirOperation::Call { .. }))
    );
    assert!(nyar_language::valkyrie::mir::validation::validate_semantic_module(&mir).is_err());
}

#[test]
fn resolved_callable_identity_survives_diagnostic_name_changes() {
    let mut hir = ValkyrieCompiler::default()
        .compile_source("micro answer() -> i64 { 7 } micro caller() -> i64 { answer() }")
        .expect("源码必须先完成解析");
    let caller = hir.functions.iter_mut().find(|function| function.name.as_str() == "caller").unwrap();
    let HirExprKind::Call { resolved: Some(resolved), .. } = &mut caller.body.expr.as_mut().unwrap().kind
    else {
        panic!("预期已解析调用");
    };
    let path = NamePath::new(vec![Identifier::new("Owner"), Identifier::new("infix +")]);
    let instance = resolved.instance.expect("源码解析必须持有实例身份");
    resolved.symbol = path.clone();
    let mir = MirLowerer::lower_module_semantic(&hir);
    let caller = mir.functions.iter().find(|function| function.symbol.ends_with("::caller")).unwrap();
    let callees = caller
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match &instruction.kind {
            MirOperation::Call { callee, .. } => Some(callee.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(callees, vec![MirOperand::Callable(instance)]);
}

#[test]
fn same_method_names_keep_declared_result_types() {
    let hir = ValkyrieCompiler::default()
        .compile_source(
            r#"
structure BooleanOwner {
    micro read() -> bool { return true }
}

structure IntegerOwner {
    micro read() -> i64 { return 7 }
}

micro read_boolean() -> bool { return BooleanOwner.read() }
micro read_integer() -> i64 { return IntegerOwner.read() }
"#,
        )
        .expect("同名不同 owner 的静态方法必须完成 HIR 解析");
    let mir = MirLowerer::lower_module_semantic(&hir);
    for (owner, expected) in [("BooleanOwner.read", ValkyrieType::Boolean), ("IntegerOwner.read", ValkyrieType::Named(Identifier::new("i64")))]
    {
        let instance =
            mir.functions.iter().find(|function| function.symbol == owner).and_then(|function| function.instance).expect("被调用方法实例");
        let mut matches = 0;
        for function in &mir.functions {
            for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
                if let MirOperation::Call { callee: MirOperand::Callable(callee), arguments } = &instruction.kind {
                    if *callee == instance {
                        matches += 1;
                        assert!(arguments.is_empty(), "无 self 声明不得增加接收者");
                        assert_eq!(instruction.results.len(), 1, "非 unit 返回值必须绑定结果");
                        assert_eq!(function.value_types.get(&instruction.results[0]), Some(&expected));
                    }
                }
            }
        }
        assert_eq!(matches, 1, "调用必须保持完整 owner 身份");
    }
}

#[test]
fn declared_operator_uses_its_callable_identity_and_signature() {
    let hir = ValkyrieCompiler::default()
        .compile_source(
            r#"
structure Number { }
imply Number {
    infix `+`(self, rhs: Number) -> bool { true }
}
micro apply(left: Number, right: Number) -> bool { left + right }
"#,
        )
        .expect("用户运算符必须经声明 overload 解析");
    let operator = &hir.impls[0].methods[0];
    let instance = operator.instance.expect("operator 声明实例身份");
    let apply = hir.functions.iter().find(|function| function.name.as_str() == "apply").expect("apply");
    let HirExprKind::Call { resolved: Some(resolved), .. } = &apply.body.expr.as_ref().expect("调用表达式").kind
    else {
        panic!("运算符调用必须绑定声明");
    };
    assert_eq!(resolved.symbol.to_string(), "Number.infix +", "运算符语法糖只保留 owner 限定的普通成员名");
    assert_eq!(resolved.instance, Some(instance));
    assert!(resolved.has_receiver, "receiver 由声明中的 self 参数决定");
    assert_eq!(resolved.return_type, ValkyrieType::Boolean);
    assert_eq!(resolved.parameter_types.len(), 2);

    let mir = MirLowerer::lower_module_semantic(&hir);
    let apply = mir.functions.iter().find(|function| function.symbol.ends_with("::apply")).expect("Semantic MIR apply");
    let calls = apply
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match &instruction.kind {
            MirOperation::Call { callee: MirOperand::Callable(callee), arguments } if *callee == instance => Some((instruction, arguments)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].1.len(), 2, "显式参数包含签名声明的 self，不由语法补造");
    assert_eq!(apply.value_types.get(&calls[0].0.results[0]), Some(&ValkyrieType::Boolean));
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn integer_pattern_comparisons_fail_without_operator_callable_contracts() {
    let hir = ValkyrieCompiler::default()
        .compile_source(
            r#"
micro literal_match(value: i64) -> i64 {
    return match value {
        case 7: 1
        else: 0
    };
}
micro range_match(value: i64) -> i64 {
    return match value {
        case 2..=8: 1
        else: 0
    };
}
"#,
        )
        .expect("pattern source must parse and resolve types");
    let mir = MirLowerer::lower_module_semantic(&hir);
    let unresolved = mir
        .diagnostics
        .iter()
        .filter_map(|diagnostic| match diagnostic {
            MirDiagnostic::UnresolvedCallableIdentity { symbol } => Some(symbol.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(unresolved, vec!["infix <".to_string(); 2]);
    assert!(nyar_language::valkyrie::mir::validation::validate_module(&mir).is_err());
}

#[test]
fn u32_infix_subtraction_call_matches_declared_return_type() {
    let hir = ValkyrieCompiler::default()
        .compile_source(
            r#"
namespace core::primitive;
structure u32 { }
imply u32 {
    infix `-`(self, rhs: u32): u32 {
        return rhs
    }
}
micro demo(value: u32): u32 {
    return value - 1
}
"#,
        )
        .expect("u32 subtraction must parse");
    let mir = MirLowerer::lower_module_semantic(&hir);
    nyar_language::valkyrie::mir::validation::validate_semantic_module(&mir).expect("u32 subtraction call must match declared return type");
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn some_short_name_without_sum_identity_fails_closed() {
    let mut hir = ValkyrieCompiler::default()
        .compile_source(
            "unite Option<T> { Some { value: T }, None } \
             micro wrap(value: utf8) -> Option<utf8> { return Some(value) }",
        )
        .expect("Option/Some 源码必须完成 HIR");
    let wrap = hir.functions.iter_mut().find(|function| function.name.as_str() == "wrap").expect("wrap");
    let body = wrap.body.expr.as_mut().expect("wrap body");
    let call = match &mut body.kind {
        HirExprKind::Return(Some(expr)) => expr.as_mut(),
        HirExprKind::Call { .. } => body,
        other => panic!("预期 Some 调用或 return Some，得到 {other:?}"),
    };
    let HirExprKind::Call { resolved: Some(resolved), .. } = &mut call.kind
    else {
        panic!("预期 Some 调用表达式");
    };
    resolved.sum_owner = None;
    resolved.sum_variant = None;
    let mir = MirLowerer::lower_module_semantic(&hir);
    assert!(
        mir.diagnostics.iter().any(|diagnostic| matches!(diagnostic, MirDiagnostic::UnresolvedVariantIdentity { .. })),
        "缺 sum 身份的 Some 不得成功降低: {:?}",
        mir.diagnostics
    );
    assert!(
        !mir.functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.instructions)
            .any(|instruction| { matches!(instruction.kind, MirOperation::SumNew { .. }) }),
        "缺身份时不得发出 SumNew"
    );
}

#[test]
fn integer_literal_equality_does_not_fabricate_a_false_result() {
    let mut builder = TestMirBuilder::new();
    let result = builder.lower_eq_constant_operand(
        MirOperand::Constant(nyar_language::valkyrie::mir::MirConstant::Int(1)),
        nyar_language::valkyrie::mir::MirConstant::Int(1),
        &ValkyrieType::Integer64 { signed: true },
    );
    assert_eq!(result, MirOperand::Constant(nyar_language::valkyrie::mir::MirConstant::Unit));
    assert!(matches!(
        builder.diagnostics(),
        [MirDiagnostic::UnresolvedCallableIdentity { symbol }] if symbol == "infix =="
    ));
}
