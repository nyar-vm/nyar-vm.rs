use super::{MirDiagnostic, MirLowerer, MirOperand, MirOperation};
use crate::{Identifier, NamePath, ValkyrieCompiler, types::hir::{HirExprKind, ValkyrieType}};

#[test]
fn missing_hir_static_contract_cannot_be_reconstructed_from_spelling() {
    let mut hir = ValkyrieCompiler::default().compile_source(
        "micro answer() -> i64 { 7 } micro caller() -> i64 { answer() }",
    ).expect("源码必须先完成解析");
    let caller = hir.functions.iter_mut().find(|function| function.name.as_str() == "caller").unwrap();
    let HirExprKind::Call { resolved, .. } = &mut caller.body.expr.as_mut().unwrap().kind else {
        panic!("预期调用表达式");
    };
    assert!(resolved.take().is_some());
    let mir = MirLowerer::lower_module_semantic(&hir);
    assert!(mir.diagnostics.iter().any(|diagnostic| matches!(diagnostic, MirDiagnostic::UnresolvedCallableIdentity { .. })));
    let caller = mir.functions.iter().find(|function| function.symbol.ends_with("::caller")).unwrap();
    assert!(!caller.blocks.iter().flat_map(|block| &block.instructions).any(|instruction| matches!(instruction.kind, MirOperation::Call { .. })));
    assert!(crate::valkyrie::mir::validation::validate_semantic_module(&mir).is_err());
}

#[test]
fn resolved_callable_identity_survives_diagnostic_name_changes() {
    let mut hir = ValkyrieCompiler::default().compile_source(
        "micro answer() -> i64 { 7 } micro caller() -> i64 { answer() }",
    ).expect("源码必须先完成解析");
    let caller = hir.functions.iter_mut().find(|function| function.name.as_str() == "caller").unwrap();
    let HirExprKind::Call { resolved: Some(resolved), .. } = &mut caller.body.expr.as_mut().unwrap().kind else {
        panic!("预期已解析调用");
    };
    let path = NamePath::new(vec![Identifier::new("Owner"), Identifier::new("infix +")]);
    let instance = resolved.instance.expect("源码解析必须持有实例身份");
    resolved.symbol = path.clone();
    let mir = MirLowerer::lower_module_semantic(&hir);
    let caller = mir.functions.iter().find(|function| function.symbol.ends_with("::caller")).unwrap();
    let callees = caller.blocks.iter().flat_map(|block| &block.instructions).filter_map(|instruction| match &instruction.kind {
        MirOperation::Call { callee, .. } => Some(callee.clone()),
        _ => None,
    }).collect::<Vec<_>>();
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
    for (owner, expected) in [
        ("BooleanOwner.read", ValkyrieType::Boolean),
        ("IntegerOwner.read", ValkyrieType::Integer64 { signed: true }),
    ] {
        let instance = mir.functions.iter().find(|function| function.symbol == owner)
            .and_then(|function| function.instance).expect("被调用方法实例");
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
