//! Shared control-flow fixture sources and cross-layer assertions.

pub const EXPLICIT_RETURN: &str = r#"
micro main() -> i64 {
    return 42
}
"#;

pub const NULLABLE_TRY_PROPAGATE: &str = r#"
micro fetch(input: i64) -> i64? {
    input
}
micro main() -> i64? {
    let value = fetch(1)?;
    value
}
"#;

pub const BREAK_IN_LOOP: &str = r#"
micro loop_break() {
    loop {
        break
    }
}
"#;

pub const CONTINUE_IN_LOOP: &str = r#"
micro loop_continue() -> i64 {
    let i: i64 = 0
    while i < 10 {
        i = i + 1
        if i == 5 {
            continue
        }
    }
    return i
}
"#;

pub const FALLTHROUGH_IN_CASE: &str = r#"
micro case_fallthrough(value: i64) {
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

use nyar_language::{MirLowerer, MirTerminator, SourceID, ValkyrieCompiler};

/// Compile a fixture source into a `HirModule`, panicking on failure.
pub fn compile_fixture(source: &str) -> nyar_language::types::hir::HirModule {
    ValkyrieCompiler::new(SourceID { version_id: 9600 }).compile_source(source).expect("compile fixture")
}

/// Assert `return 42` lowers to a `Return` terminator.
pub fn assert_return_mir_shape(hir: &nyar_language::types::hir::HirModule) {
    let mir = MirLowerer::lower_module_semantic(hir);
    let function = &mir.functions[0];
    assert!(function.blocks.iter().any(|block| matches!(block.terminator, MirTerminator::Return { .. })), "expected Return terminator");
}

/// Assert `fetch()?` lowers to `try_propagate_ok` and `try_propagate_early_exit` blocks.
pub fn assert_nullable_try_mir_shape(hir: &nyar_language::types::hir::HirModule) {
    let mir = MirLowerer::lower_module_semantic(hir);
    let function = mir.functions.iter().find(|f| f.symbol.ends_with("::main")).expect("main function");
    assert!(function.blocks.iter().any(|block| block.label.contains("try_propagate_ok")), "expected try_propagate_ok block");
    assert!(function.blocks.iter().any(|block| block.label.contains("try_propagate_early_exit")), "expected try_propagate_early_exit block");
}

/// Assert `loop { break }` lowers to a Jump terminator that reaches the `loop_exit` block.
pub fn assert_break_mir_shape(hir: &nyar_language::types::hir::HirModule) {
    let mir = MirLowerer::lower_module_semantic(hir);
    let function = mir.functions.iter().find(|f| f.symbol.contains("loop_break")).expect("loop_break function");
    let loop_exit = function.blocks.iter().find(|block| block.label == "loop_exit").expect("expected loop_exit block");
    let reaches_exit = function.blocks.iter().any(|block| match &block.terminator {
        MirTerminator::Jump { target, .. } => *target == loop_exit.id,
        _ => false,
    });
    assert!(reaches_exit, "break must jump to loop_exit");
    assert!(
        function.blocks.iter().any(|block| matches!(block.terminator, MirTerminator::Return { .. })),
        "expected Return terminator after loop exit"
    );
}

/// Assert `continue` inside a `while` loop lowers to a Jump terminator that reaches the loop header.
pub fn assert_continue_mir_shape(hir: &nyar_language::types::hir::HirModule) {
    let mir = MirLowerer::lower_module_semantic(hir);
    let function = mir.functions.iter().find(|f| f.symbol.contains("loop_continue")).expect("loop_continue function");
    let loop_header = function.blocks.iter().find(|block| block.label.contains("loop_header")).expect("expected loop_header block");
    let reaches_header = function.blocks.iter().any(|block| match &block.terminator {
        MirTerminator::Jump { target, .. } => *target == loop_header.id,
        _ => false,
    });
    assert!(reaches_header, "continue must jump to loop_header");
}

/// Assert `fallthrough` in a `case` arm lowers to a Jump terminator from `case_arm_0` to `case_arm_1`.
pub fn assert_fallthrough_mir_shape(hir: &nyar_language::types::hir::HirModule) {
    let mir = MirLowerer::lower_module_semantic(hir);
    let function = mir.functions.iter().find(|f| f.symbol.contains("case_fallthrough")).expect("case_fallthrough function");
    let fallthrough_block = function
        .blocks
        .iter()
        .find(|block| matches!(block.terminator, MirTerminator::Jump { .. }) && block.label.starts_with("case_arm_0"))
        .expect("expected case_arm_0 block with Jump terminator");
    let target = match fallthrough_block.terminator {
        MirTerminator::Jump { target, .. } => target,
        _ => unreachable!(),
    };
    let target_block = function.blocks.iter().find(|block| block.id == target).expect("expected mir jump target block");
    assert!(target_block.label.starts_with("case_arm_1"), "fallthrough must jump from case_arm_0 to case_arm_1, got {}", target_block.label);
}
