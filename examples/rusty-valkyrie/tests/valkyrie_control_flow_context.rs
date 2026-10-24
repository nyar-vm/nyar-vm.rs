use rusty_valkyrie::{
    control_flow::{ControlFlowContext, ScopeKind, TryScopeData},
    *,
};

#[test]
fn label_registry_resolves_outer_loop_for_break() {
    let mut ctx = ControlFlowContext::default();
    ctx.push_loop(Some("outer".to_string()), false);
    ctx.push_loop(Some("inner".to_string()), false);
    assert!(ctx.resolve_loop_mut(Some("outer")).is_some());
    assert!(ctx.resolve_loop_mut(Some("inner")).is_some());
    assert!(ctx.resolve_loop_mut(Some("missing")).is_none());
}

#[test]
fn resolve_continue_requires_active_loop_scope() {
    let mut ctx = ControlFlowContext::default();
    assert!(!ctx.resolve_continue(None));
    ctx.push_loop(Some("outer".to_string()), false);
    assert!(ctx.resolve_continue(None));
    assert!(ctx.resolve_continue(Some("outer")));
    assert!(!ctx.resolve_continue(Some("inner")));
}

#[test]
fn try_scope_stack_tracks_innermost_scope() {
    let mut ctx = ControlFlowContext::default();
    assert!(!ctx.in_try_scope());
    ctx.push_try(TryScopeData { is_optional: true, is_forced: false, result_type: None });
    assert!(ctx.in_try_scope());
    assert!(ctx.current_try_scope().is_some_and(|scope| scope.is_optional));
    ctx.pop_try();
    assert!(!ctx.in_try_scope());
}

#[test]
fn case_chain_scope_tracks_fallthrough_target() {
    let mut ctx = ControlFlowContext::default();
    ctx.push_case_chain(Some("case_chain".to_string()), Some(3), true);
    let scope = ctx.current_case_chain_scope().expect("case chain scope");
    assert_eq!(scope.next_arm_target, Some(3));
    assert!(scope.fallthrough_allowed);
    ctx.pop_case_chain();
    assert!(ctx.current_case_chain_scope().is_none());
}

#[test]
fn generator_scope_tracks_yield_points() {
    let mut ctx = ControlFlowContext::default();
    ctx.push_generator(Some("gen".to_string()), true);
    {
        let scope = ctx.current_generator_scope_mut().expect("generator scope");
        scope.yield_points.push(1);
        scope.yield_points.push(2);
    }
    let scope = ctx.current_generator_scope().expect("generator scope");
    assert!(scope.is_async_generator);
    assert_eq!(scope.yield_points, vec![1, 2]);
    ctx.pop_generator();
    assert!(ctx.current_generator_scope().is_none());
}

#[test]
fn async_scope_tracks_await_points() {
    let mut ctx = ControlFlowContext::default();
    ctx.push_async(Some("async".to_string()), true);
    {
        let scope = ctx.current_async_scope_mut().expect("async scope");
        scope.await_points.push(5);
    }
    let scope = ctx.current_async_scope().expect("async scope");
    assert!(scope.is_async_fn);
    assert_eq!(scope.await_points, vec![5]);
    ctx.pop_async();
    assert!(ctx.current_async_scope().is_none());
}

#[test]
fn catch_scope_tracks_raise_and_resume() {
    let mut ctx = ControlFlowContext::default();
    ctx.push_catch(Some("catch".to_string()), true, true);
    let scope = ctx.current_catch_scope().expect("catch scope");
    assert!(scope.raise_allowed);
    assert!(scope.resume_allowed);
    ctx.pop_catch();
    assert!(ctx.current_catch_scope().is_none());
}

#[test]
fn label_registry_supports_all_scope_kinds() {
    let mut ctx = ControlFlowContext::default();
    ctx.push_loop(Some("loop".to_string()), false);
    ctx.push_try(TryScopeData { is_optional: false, is_forced: false, result_type: None });
    ctx.push_case_chain(Some("case_chain".to_string()), None, false);
    ctx.push_generator(Some("gen".to_string()), false);
    ctx.push_async(Some("async".to_string()), false);
    ctx.push_catch(Some("catch".to_string()), false, false);

    assert_eq!(ctx.resolve_scope("loop"), Some((ScopeKind::Loop, 0)));
    assert_eq!(ctx.resolve_scope("case_chain"), Some((ScopeKind::CaseChain, 2)));
    assert_eq!(ctx.resolve_scope("gen"), Some((ScopeKind::Generator, 3)));
    assert_eq!(ctx.resolve_scope("async"), Some((ScopeKind::Async, 4)));
    assert_eq!(ctx.resolve_scope("catch"), Some((ScopeKind::Catch, 5)));
    assert!(ctx.resolve_scope("missing").is_none());
}

#[test]
fn label_registry_removes_label_on_pop() {
    let mut ctx = ControlFlowContext::default();
    ctx.push_loop(Some("outer".to_string()), false);
    ctx.push_loop(Some("inner".to_string()), false);
    assert!(ctx.resolve_scope("inner").is_some());
    ctx.pop_loop();
    assert!(ctx.resolve_scope("inner").is_none());
    assert!(ctx.resolve_scope("outer").is_some());
    ctx.pop_loop();
    assert!(ctx.resolve_scope("outer").is_none());
}

#[test]
fn nested_scopes_track_independent_kinds() {
    let mut ctx = ControlFlowContext::default();
    ctx.push_loop(Some("loop".to_string()), false);
    ctx.push_try(TryScopeData { is_optional: false, is_forced: false, result_type: None });
    ctx.push_generator(None, false);

    assert!(ctx.in_try_scope());
    assert!(ctx.current_generator_scope().is_some());
    assert!(ctx.resolve_continue(None));
    assert!(ctx.resolve_continue(Some("loop")));

    ctx.pop_generator();
    ctx.pop_try();
    ctx.pop_loop();

    assert!(!ctx.in_try_scope());
    assert!(ctx.current_generator_scope().is_none());
    assert!(!ctx.resolve_continue(None));
}

/// Regression: nested loops with the same label name must not lose the
/// outer binding when the inner loop is popped. The label should resolve
/// to the inner loop while it is active, and fall back to the outer loop
/// after the inner one is popped.
#[test]
fn label_shadowing_restores_outer_binding_on_pop() {
    let mut ctx = ControlFlowContext::default();
    ctx.push_loop(Some("x".to_string()), false);
    // Inner loop with the same label shadows the outer binding.
    ctx.push_loop(Some("x".to_string()), false);
    // While both are active, 'x' resolves to the inner loop (index 1).
    assert_eq!(ctx.resolve_scope("x"), Some((ScopeKind::Loop, 1)));
    // Popping the inner loop should restore the outer binding, not remove
    // the label entirely.
    ctx.pop_loop();
    assert_eq!(ctx.resolve_scope("x"), Some((ScopeKind::Loop, 0)));
    // Popping the outer loop removes the last binding.
    ctx.pop_loop();
    assert!(ctx.resolve_scope("x").is_none());
}

/// Regression: triple-nested same-name labels restore correctly across
/// multiple pops.
#[test]
fn label_shadowing_handles_three_levels() {
    let mut ctx = ControlFlowContext::default();
    ctx.push_loop(Some("L".to_string()), false);
    ctx.push_loop(Some("L".to_string()), false);
    ctx.push_loop(Some("L".to_string()), false);
    assert_eq!(ctx.resolve_scope("L"), Some((ScopeKind::Loop, 2)));
    ctx.pop_loop();
    assert_eq!(ctx.resolve_scope("L"), Some((ScopeKind::Loop, 1)));
    ctx.pop_loop();
    assert_eq!(ctx.resolve_scope("L"), Some((ScopeKind::Loop, 0)));
    ctx.pop_loop();
    assert!(ctx.resolve_scope("L").is_none());
}

/// Regression: `set_validating_break_value` with a label must set the flag
/// on the loop bound to that label, not on an unrelated innermost loop.
#[test]
fn set_validating_break_value_targets_labelled_loop() {
    let mut ctx = ControlFlowContext::default();
    ctx.push_loop(Some("outer".to_string()), true);
    ctx.push_loop(Some("inner".to_string()), true);
    // break 'outer expr should flag the outer loop, not the inner one.
    ctx.set_validating_break_value(true, Some("outer"));
    assert!(!ctx.current_validating_break_value(), "inner loop should not be flagged");
    // Pop the inner loop; the outer loop's flag should still be set.
    ctx.pop_loop();
    assert!(ctx.current_validating_break_value(), "outer loop should be flagged");
    ctx.set_validating_break_value(false, Some("outer"));
    assert!(!ctx.current_validating_break_value());
    ctx.pop_loop();
}

/// `pop_*` should panic when the innermost scope is not of the expected
/// kind. This catches push/pop pairing bugs in callers.
#[test]
#[should_panic(expected = "pop_loop called when innermost scope is not a Loop")]
fn pop_loop_panics_on_kind_mismatch() {
    let mut ctx = ControlFlowContext::default();
    ctx.push_try(TryScopeData { is_optional: false, is_forced: false, result_type: None });
    ctx.pop_loop();
}
