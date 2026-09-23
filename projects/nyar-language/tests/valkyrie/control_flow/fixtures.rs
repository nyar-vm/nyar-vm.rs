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

pub const YIELD_GENERATOR: &str = r#"
micro gen() {
    yield 1
    return
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

pub const YIELD_FROM_ITERATOR: &str = r#"
micro gen() {
    let values: Iterator<i32> = ()
    yield from values
    return
}
"#;

pub const AWAIT_FUTURE: &str = r#"
micro async_fn() {
    let future: Future<i32> = ()
    future.await
    return
}
"#;

use nyar::{Identifier, QualifiedName};
use nyar_language::{MirLowerer, MirTerminator, SourceID, ValkyrieCompiler, build_state_machine_suspend_payload};

/// Compile a fixture source into a `HirModule`, panicking on failure.
pub fn compile_fixture(source: &str) -> nyar_language::types::hir::HirModule {
    ValkyrieCompiler::new(SourceID { version_id: 9600 }).compile_source(source).expect("compile fixture")
}

/// Build the `QualifiedName` operation needed by `build_state_machine_suspend_payload` from a
/// MIR function symbol string (e.g. `main::gen`). The payload builder filters `ProgramFacts`
/// by `function.symbol == operation`, where `function_symbol` prefixes the function name with
/// the module name, so callers must pass the fully-qualified symbol rather than the bare name.
fn qualified_symbol_from_mir(hir: &nyar_language::types::hir::HirModule, function_suffix: &str) -> QualifiedName {
    let mir = MirLowerer::lower_module(hir);
    let function = mir
        .functions
        .iter()
        .find(|function| function.symbol.ends_with(function_suffix))
        .unwrap_or_else(|| panic!("expected mir function ending with `{function_suffix}`"));
    let parts: Vec<Identifier> = function.symbol.split("::").map(Identifier::new).collect();
    QualifiedName::new(parts)
}

/// Assert `return 42` lowers to a `Return` terminator.
pub fn assert_return_mir_shape(hir: &nyar_language::types::hir::HirModule) {
    let mir = MirLowerer::lower_module_semantic(hir);
    let function = &mir.functions[0];
    assert!(function.blocks.iter().any(|block| matches!(block.terminator, MirTerminator::Return { .. })), "expected Return terminator");
}

/// Assert `fetch()?` lowers to `try_propagate_ok` and `try_propagate_early_exit` blocks.
pub fn assert_nullable_try_mir_shape(hir: &nyar_language::types::hir::HirModule) {
    let mir = MirLowerer::lower_module(hir);
    let function = mir.functions.iter().find(|f| f.symbol.ends_with("::main")).expect("main function");
    assert!(function.blocks.iter().any(|block| block.label.contains("try_propagate_ok")), "expected try_propagate_ok block");
    assert!(function.blocks.iter().any(|block| block.label.contains("try_propagate_early_exit")), "expected try_propagate_early_exit block");
}

/// Assert `yield 1` lowers to a single `Yield` suspend state and the backend payload mirrors it.
pub fn assert_yield_state_machine_shape(hir: &nyar_language::types::hir::HirModule) {
    let mir = MirLowerer::lower_module(hir);
    let function = mir.functions.iter().find(|f| f.symbol.contains("gen")).expect("gen");
    let plan = function.suspend_plan.as_ref().expect("suspend plan");
    assert_eq!(plan.states.len(), 1);

    let symbol = qualified_symbol_from_mir(hir, "gen");
    let payload = build_state_machine_suspend_payload(hir, &[symbol]);
    let artifact = payload.functions.first().expect("state machine artifact");
    assert_eq!(artifact.states.len(), 1);
    assert_eq!(artifact.states[0].effect, "Yield");
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

/// Assert `yield from values` lowers to a DelegateYield state with Iterator.next witness binding.
pub fn assert_yield_from_state_machine_shape(hir: &nyar_language::types::hir::HirModule) {
    use nyar_language::valkyrie::mir::MirEffectKind;

    let mir = MirLowerer::lower_module(hir);
    let function = mir.functions.iter().find(|f| f.symbol.contains("gen")).expect("gen function");
    let descriptor = function.suspend_plan.as_ref().expect("suspend plan");
    assert_eq!(descriptor.states.len(), 1);
    assert_eq!(descriptor.states[0].effect, MirEffectKind::DelegateYield);

    let symbol = qualified_symbol_from_mir(hir, "gen");
    let payload = build_state_machine_suspend_payload(hir, &[symbol]);
    let artifact = payload.functions.first().expect("state machine artifact");
    assert_eq!(artifact.states.len(), 1);
    assert_eq!(artifact.states[0].effect, "DelegateYield");
    assert!(
        artifact.states[0].witness_bindings.iter().any(|binding| binding.trait_name == "Iterator" && binding.method_name == "next"),
        "expected Iterator.next witness binding"
    );
}

/// Assert `future.await` lowers to an Await state with Future.poll witness binding and Integer32 resume type.
pub fn assert_await_state_machine_shape(hir: &nyar_language::types::hir::HirModule) {
    use nyar_language::{types::hir::ValkyrieType, valkyrie::mir::MirEffectKind};

    let mir = MirLowerer::lower_module(hir);
    let function = mir.functions.iter().find(|f| f.symbol.contains("async_fn")).expect("async_fn function");
    let descriptor = function.suspend_plan.as_ref().expect("suspend plan");
    assert_eq!(descriptor.states.len(), 1);
    assert_eq!(descriptor.states[0].effect, MirEffectKind::Await);
    assert_eq!(descriptor.states[0].resume_parameter_type, Some(ValkyrieType::Integer32 { signed: true }));

    let symbol = qualified_symbol_from_mir(hir, "async_fn");
    let payload = build_state_machine_suspend_payload(hir, &[symbol]);
    let artifact = payload.functions.first().expect("state machine artifact");
    assert_eq!(artifact.states.len(), 1);
    assert_eq!(artifact.states[0].effect, "Await");
    assert!(
        artifact.states[0].witness_bindings.iter().any(|binding| binding.trait_name == "Future" && binding.method_name == "poll"),
        "expected Future.poll witness binding"
    );
}
