//! MIR tests for the unified control-flow context (Task 4 of unify-high-order-control-flow spec).
//!
//! These tests verify that the unified `MirBuilderControlFlow` handler/resume
//! stacks produce correct MIR structure for `catch`/`raise`/`resume`,
//! `break`, `fallthrough`, and `TryPropagate(?)` exits.

use nyar_language::{
    MirLowerer, MirTerminator, SourceID, ValkyrieCompiler,
    mir::ssa::test_support::{block, expr, lower_test_function, lower_test_module, span},
    types::{
        Identifier,
        hir::{HirDocumentation, HirExprKind, HirFunction, HirLiteral, HirMatchArm, HirPattern, HirVisibility, ValkyrieType},
    },
};

fn compile_mir(source: &str) -> nyar_language::valkyrie::mir::MirModule {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9400 }).compile_source(source).expect("compile");
    MirLowerer::lower_module_semantic(&hir)
}

fn find_function<'a>(mir: &'a nyar_language::valkyrie::mir::MirModule, name: &str) -> &'a nyar_language::valkyrie::mir::ssa::MirFunction {
    mir.functions
        .iter()
        .find(|f| f.symbol.ends_with(&format!("::{name}")) || f.symbol == name)
        .unwrap_or_else(|| panic!("expected mir function {name}"))
}

fn count_blocks_with_label(function: &nyar_language::valkyrie::mir::ssa::MirFunction, label: &str) -> usize {
    function.blocks.iter().filter(|block| block.label == label).count()
}

#[test]
fn catch_dispatch_uses_unified_handler_context() {
    let mir = lower_test_function(expr(HirExprKind::Catch {
        expr: Box::new(expr(HirExprKind::Raise(Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true))))))),
        arms: vec![HirMatchArm { pattern: HirPattern::Else, guard: None, body: Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(false)))) }],
    }));

    assert!(
        mir.blocks.iter().any(|block| block.label == "catch_dispatch"),
        "expected catch_dispatch block produced via unified handler context"
    );
    assert!(mir.blocks.iter().any(|block| block.label == "catch_exit"), "expected catch_exit block produced via unified handler context");
}

#[test]
fn raise_in_catch_arm_uses_unified_resume_stack() {
    let mir = lower_test_function(expr(HirExprKind::Catch {
        expr: Box::new(expr(HirExprKind::Raise(Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true))))))),
        arms: vec![HirMatchArm {
            pattern: HirPattern::Else,
            guard: None,
            body: Box::new(expr(HirExprKind::Resume(Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(false))))))),
        }],
    }));

    let continuation = mir.continuations.first().expect("expected continuation metadata from unified resume stack");
    let resume_block =
        mir.blocks.iter().find(|block| block.id == continuation.resume_target).expect("expected catch_resume block referenced by continuation");
    assert_eq!(resume_block.label, "catch_resume");
    assert!(
        resume_block.parameters.contains(&continuation.resume_parameter),
        "expected catch_resume block to carry the continuation resume parameter"
    );
}

#[test]
fn nested_catch_dispatch_preserves_handler_stack_order() {
    let inner_catch = expr(HirExprKind::Catch {
        expr: Box::new(expr(HirExprKind::Raise(Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true))))))),
        arms: vec![HirMatchArm {
            pattern: HirPattern::Else,
            guard: None,
            body: Box::new(expr(HirExprKind::Resume(Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(false))))))),
        }],
    });
    let mir = lower_test_function(expr(HirExprKind::Catch {
        expr: Box::new(inner_catch),
        arms: vec![HirMatchArm {
            pattern: HirPattern::Else,
            guard: None,
            body: Box::new(expr(HirExprKind::Resume(Box::new(expr(HirExprKind::Literal(HirLiteral::Bool(true))))))),
        }],
    }));

    assert_eq!(
        count_blocks_with_label(&mir, "catch_dispatch"),
        2,
        "expected two catch_dispatch blocks for nested catch (handler stack push/pop order preserved)"
    );
    assert_eq!(
        count_blocks_with_label(&mir, "catch_resume"),
        2,
        "expected two catch_resume blocks for nested catch (resume stack push/pop order preserved)"
    );
    assert_eq!(
        count_blocks_with_label(&mir, "catch_exit"),
        2,
        "expected two catch_exit blocks for nested catch (handler exit blocks preserved)"
    );
    assert_eq!(mir.continuations.len(), 2, "expected two continuation entries for nested catch (unified resume stack preserves order)");
}

#[test]
fn break_expr_uses_loop_exit_parameter() {
    let function = HirFunction {
        name: Identifier::new("main"),
        declaring_namespace: nyar_language::types::NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::new(),
        generics: Vec::new(),
        params: Vec::new(),
        return_type: ValkyrieType::Integer64 { signed: true },
        body: block(
            Vec::new(),
            Some(expr(HirExprKind::Loop {
                label: None,
                pattern: None,
                iterator: None,
                condition: None,
                body: Box::new(block(
                    Vec::new(),
                    Some(expr(HirExprKind::Break { label: None, expr: Some(Box::new(expr(HirExprKind::Literal(HirLiteral::Integer64(42))))) })),
                )),
            })),
        ),
        span: span(),
        visibility: HirVisibility::default(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    };
    let mir = lower_test_module(vec![function], Vec::new());
    let main = &mir.functions[0];

    let loop_exit = main.blocks.iter().find(|block| block.label == "loop_exit").expect("expected loop_exit block");
    assert!(!loop_exit.parameters.is_empty(), "expected loop_exit block to carry exit value parameter for break expr");

    let has_break_jump = main.blocks.iter().any(|block| {
        matches!(
            &block.terminator,
            MirTerminator::Jump { target, arguments } if *target == loop_exit.id && !arguments.is_empty()
        )
    });
    assert!(has_break_jump, "expected Jump terminator targeting loop_exit with break value argument");
}

#[test]
fn fallthrough_jumps_to_next_arm_entry() {
    let function = HirFunction {
        name: Identifier::new("main"),
        declaring_namespace: nyar_language::types::NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::new(),
        generics: Vec::new(),
        params: vec![nyar_language::types::hir::HirParam {
            name: nyar_language::types::hir::HirIdentifier { name: Identifier::new("input"), shadow_index: 0, span: span() },
            ty: ValkyrieType::Integer64 { signed: true },
            ..Default::default()
        }],
        return_type: ValkyrieType::Unit,
        body: block(
            Vec::new(),
            Some(expr(HirExprKind::Case {
                scrutinee: Box::new(expr(HirExprKind::Variable(nyar_language::types::hir::HirIdentifier {
                    name: Identifier::new("input"),
                    shadow_index: 0,
                    span: span(),
                }))),
                arms: vec![
                    HirMatchArm {
                        pattern: HirPattern::Literal(HirLiteral::Integer64(1)),
                        guard: None,
                        body: Box::new(expr(HirExprKind::Fallthrough)),
                    },
                    HirMatchArm { pattern: HirPattern::Else, guard: None, body: Box::new(expr(HirExprKind::Literal(HirLiteral::Unit))) },
                ],
            })),
        ),
        span: span(),
        visibility: HirVisibility::default(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    };
    let mir = lower_test_module(vec![function], Vec::new());
    let main = &mir.functions[0];

    let next_arm = main.blocks.iter().find(|block| block.label == "case_arm_1").expect("expected case_arm_1 block as fallthrough target");

    let has_fallthrough_jump = main.blocks.iter().any(|block| {
        matches!(
            &block.terminator,
            MirTerminator::Jump { target, arguments } if *target == next_arm.id && arguments.is_empty()
        )
    });
    assert!(has_fallthrough_jump, "expected fallthrough to produce Jump to next case arm entry");
}

#[test]
fn try_propagate_generates_explicit_branch_return() {
    let mir = compile_mir(
        r#"
micro fetch() -> i64? {
    null
}
micro main() -> i64? {
    let value = fetch()?;
    value
}
"#,
    );
    let function = find_function(&mir, "main");

    assert!(
        function.blocks.iter().any(|block| matches!(block.terminator, MirTerminator::Branch { .. })),
        "expected Branch terminator from try_propagate in non-try scope"
    );

    let early_exit =
        function.blocks.iter().find(|block| block.label == "try_propagate_early_exit").expect("expected try_propagate_early_exit block");
    assert!(
        matches!(early_exit.terminator, MirTerminator::Return { .. }),
        "expected Return terminator in try_propagate early exit (non-try scope generates explicit Branch + Return)"
    );
}
