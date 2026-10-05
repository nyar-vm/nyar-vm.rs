use super::{MirBuilder, MirDiagnostic, MirLowerer, MirOperand, MirOperation};
use crate::{
    Identifier, NamePath, ValkyrieCompiler,
    types::hir::{HirExprKind, ValkyrieType},
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
    assert!(crate::valkyrie::mir::validation::validate_semantic_module(&mir).is_err());
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
    for (owner, expected) in [("BooleanOwner.read", ValkyrieType::Boolean), ("IntegerOwner.read", ValkyrieType::Integer64 { signed: true })] {
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
    let HirExprKind::Call { call_kind, resolved: Some(resolved), .. } = &apply.body.expr.as_ref().expect("调用表达式").kind
    else {
        panic!("运算符调用必须绑定声明");
    };
    assert_eq!(*call_kind, crate::types::hir::HirCallKind::Operator(nyar_types::builtin_operator::infix_add()));
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
            MirDiagnostic::UnresolvedOperatorCallable { operator } => Some(*operator),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(unresolved, vec![nyar_types::builtin_operator::infix_lt(); 2]);
    assert!(crate::valkyrie::mir::validation::validate_module(&mir).is_err());
}

#[test]
fn integer_literal_equality_does_not_fabricate_a_false_result() {
    let mut builder = MirBuilder::new(
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        None,
    );
    let result = builder.lower_eq_constant_operand(
        MirOperand::Constant(crate::valkyrie::mir::MirConstant::Int(1)),
        crate::valkyrie::mir::MirConstant::Int(1),
        &ValkyrieType::Integer64 { signed: true },
    );
    assert_eq!(result, MirOperand::Constant(crate::valkyrie::mir::MirConstant::Unit));
    assert!(matches!(
        builder.diagnostics.as_slice(),
        [MirDiagnostic::UnresolvedOperatorCallable { operator }]
            if *operator == nyar_types::builtin_operator::infix_eq()
    ));
}
