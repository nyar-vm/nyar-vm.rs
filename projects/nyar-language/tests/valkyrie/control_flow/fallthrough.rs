use nyar_language::{
    MirTerminator, ValkyrieCompiler,
    lir::LirTerminator,
    types::{SourceID, hir::HirExprKind},
};

fn compile_source_to_legacy_lir(
    compiler: &ValkyrieCompiler,
    source: &str,
) -> Result<nyar_language::lir::LirModule, std_data::text::valkyrie::ParseError> {
    let hir = compiler.compile_source(source)?;
    let mir = nyar_language::MirLowerer::lower_module(&hir);
    let lir = nyar_language::lir::LirLowerer::lower_mir_module(&hir, &mir);
    nyar_language::lir::validation::validate_module(&lir)?;
    Ok(lir)
}

#[test]
fn rejects_fallthrough_in_value_match_expression() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 1971 });
    let error = compiler
        .compile_source_to_mir(
            r#"micro main() -> bool {
    return match value {
        case Flag():
            fallthrough
        else:
            true
    };
}
"#,
        )
        .unwrap_err();
    assert!(error.to_string().contains("fallthrough"));
}

#[test]
fn lowers_statement_match_fallthrough_into_mir_and_lir_jump_chain() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 1972 });
    let source = r#"micro main() {
    match value {
        case Flag():
            fallthrough
        else:
            return
    };
    return
}
"#;

    let hir = compiler.compile_source(source).unwrap();
    let statement = &hir.functions[0].body.statements[0];
    match &statement.kind {
        nyar_language::types::hir::HirStatementKind::Expr(expr) => {
            assert!(matches!(expr.kind, HirExprKind::Case { .. }));
        }
        _ => panic!("expected expression statement"),
    }

    let mir = compiler.compile_source_to_mir(source).unwrap();
    let first_fallthrough_block = mir.functions[0]
        .blocks
        .iter()
        .find(|block| matches!(block.terminator, MirTerminator::Jump { .. }) && block.label.starts_with("case_arm_0"))
        .expect("expected first case arm block");
    let target = match first_fallthrough_block.terminator {
        MirTerminator::Jump { target, .. } => target,
        _ => unreachable!(),
    };
    let target_block = mir.functions[0].blocks.iter().find(|block| block.id == target).expect("expected mir jump target block");
    assert!(target_block.label.starts_with("case_arm_1"));

    let lir = compile_source_to_legacy_lir(&compiler, source).unwrap();
    let first_lir_fallthrough_block = lir.functions[0]
        .blocks
        .iter()
        .find(|block| matches!(block.terminator, LirTerminator::Jump { .. }) && block.label.starts_with("case_arm_0"))
        .expect("expected first lir case arm block");
    let target = match first_lir_fallthrough_block.terminator {
        LirTerminator::Jump { target, .. } => target,
        _ => unreachable!(),
    };
    let target_block = lir.functions[0].blocks.iter().find(|block| block.id == target).expect("expected lir jump target block");
    assert!(target_block.label.starts_with("case_arm_1"));
}

#[test]
fn lowers_explicit_case_statement_source_entry_into_case_chain() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 1974 });
    let source = r#"micro main() {
    case value {
        case Flag():
            fallthrough
        else:
            return
    };
    return
}
"#;

    let hir = compiler.compile_source(source).unwrap();
    let statement = &hir.functions[0].body.statements[0];
    let nyar_language::types::hir::HirStatementKind::Expr(case_expr) = &statement.kind
    else {
        panic!("expected case expression statement");
    };
    assert!(matches!(case_expr.kind, HirExprKind::Case { .. }));

    let mir = compiler.compile_source_to_mir(source).unwrap();
    let first_case_block = mir.functions[0]
        .blocks
        .iter()
        .find(|block| matches!(block.terminator, MirTerminator::Jump { .. }) && block.label.starts_with("case_arm_0"))
        .expect("expected first case arm block");
    let target = match first_case_block.terminator {
        MirTerminator::Jump { target, .. } => target,
        _ => unreachable!(),
    };
    let target_block = mir.functions[0].blocks.iter().find(|block| block.id == target).expect("expected mir jump target block");
    assert!(target_block.label.starts_with("case_arm_1"));

    let lir = compile_source_to_legacy_lir(&compiler, source).unwrap();
    let first_lir_case_block = lir.functions[0]
        .blocks
        .iter()
        .find(|block| matches!(block.terminator, LirTerminator::Jump { .. }) && block.label.starts_with("case_arm_0"))
        .expect("expected first lir case arm block");
    let target = match first_lir_case_block.terminator {
        LirTerminator::Jump { target, .. } => target,
        _ => unreachable!(),
    };
    let target_block = lir.functions[0].blocks.iter().find(|block| block.id == target).expect("expected lir jump target block");
    assert!(target_block.label.starts_with("case_arm_1"));
}

#[test]
fn rejects_fallthrough_in_last_case_arm() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 1973 });
    let error = compiler
        .compile_source_to_mir(
            r#"micro main() {
    match value {
        case Flag():
            false
        else:
            fallthrough
    };
}
"#,
        )
        .unwrap_err();
    assert!(error.to_string().contains("最后一个 `case` arm"), "{error}");
}

#[test]
fn fallthrough_reenters_next_case_arm_check_and_guard_chain() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 1975 });
    let source = r#"micro main() {
    case value {
        case 0:
            fallthrough
        case n if n > 0:
            return
        else:
            return
    };
    return
}
"#;

    let mir = compiler.compile_source_to_mir(source).unwrap();
    let first_case_block = mir.functions[0]
        .blocks
        .iter()
        .find(|block| matches!(block.terminator, MirTerminator::Jump { .. }) && block.label.starts_with("case_arm_0"))
        .expect("expected first case arm block");
    let target = match first_case_block.terminator {
        MirTerminator::Jump { target, .. } => target,
        _ => unreachable!(),
    };
    let target_block = mir.functions[0].blocks.iter().find(|block| block.id == target).expect("expected mir jump target block");
    assert_eq!(target_block.label, "case_arm_1");
    assert!(mir.functions[0].blocks.iter().any(|block| block.label == "case_arm_1_body"), "expected guarded next arm body block");

    let lir = compile_source_to_legacy_lir(&compiler, source).unwrap();
    let first_lir_case_block = lir.functions[0]
        .blocks
        .iter()
        .find(|block| matches!(block.terminator, LirTerminator::Jump { .. }) && block.label.starts_with("case_arm_0"))
        .expect("expected first lir case arm block");
    let target = match first_lir_case_block.terminator {
        LirTerminator::Jump { target, .. } => target,
        _ => unreachable!(),
    };
    let target_block = lir.functions[0].blocks.iter().find(|block| block.id == target).expect("expected lir jump target block");
    assert_eq!(target_block.label, "case_arm_1");
    assert!(lir.functions[0].blocks.iter().any(|block| block.label == "case_arm_1_body"), "expected guarded next arm body block");
}

#[test]
fn records_case_chain_metadata_for_case_fallthrough_across_mir_and_lir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 1976 });
    let source = r#"micro main() {
    case value {
        case 0:
            fallthrough
        case 1 if value > 0:
            return
        else:
            return
    };
    return
}
"#;

    let hir = compiler.compile_source(source).unwrap();
    let statement = &hir.functions[0].body.statements[0];
    let nyar_language::types::hir::HirStatementKind::Expr(case_expr) = &statement.kind
    else {
        panic!("expected case expression statement");
    };
    assert!(matches!(case_expr.kind, HirExprKind::Case { .. }));

    let mir = compiler.compile_source_to_mir(source).unwrap();
    let mir_chain = &mir.functions[0].case_chains[0];
    assert!(!mir_chain.produce_value);
    assert_eq!(mir_chain.arms.len(), 3);
    assert_eq!(mir_chain.first_arm, mir_chain.arms[0].entry_block);
    assert_eq!(mir_chain.arms[0].fallthrough_target, Some(mir_chain.arms[1].entry_block));
    assert!(mir_chain.arms[1].guard_block.is_some());
    assert_eq!(mir_chain.arms[2].fallthrough_target, None);

    let lir = compile_source_to_legacy_lir(&compiler, source).unwrap();
    let lir_chain = &lir.functions[0].case_chains[0];
    assert_eq!(lir_chain.dispatch_block, mir_chain.dispatch_block);
    assert_eq!(lir_chain.first_arm, mir_chain.first_arm);
    assert_eq!(lir_chain.no_match_block, mir_chain.no_match_block);
    assert_eq!(lir_chain.exit_block, mir_chain.exit_block);
    assert_eq!(lir_chain.produce_value, mir_chain.produce_value);
    assert_eq!(lir_chain.arms.len(), mir_chain.arms.len());
    assert_eq!(lir_chain.arms[0].fallthrough_target, mir_chain.arms[0].fallthrough_target);
    assert_eq!(lir_chain.arms[1].guard_block, mir_chain.arms[1].guard_block);
    assert_eq!(lir_chain.arms[2].fallthrough_target, mir_chain.arms[2].fallthrough_target);
}
