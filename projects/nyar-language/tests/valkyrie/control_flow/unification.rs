//! Task 3 回归测试：验证 HIR 控制流校验迁入统一 `ControlFlowContext` 后，
//! 六类作用域跟踪、标签解析、lambda 继承与非法跳转拒绝行为保持一致。

use nyar_language::{
    SourceID, ValkyrieCompiler,
    valkyrie::control_flow::{ControlFlowContext, ScopeKind, TryScopeData},
};

fn compile(source: &str) -> nyar_language::types::hir::HirModule {
    ValkyrieCompiler::new(SourceID { version_id: 9600 }).compile_source(source).expect("compile")
}

fn compile_err(source: &str) -> String {
    ValkyrieCompiler::new(SourceID { version_id: 9601 }).compile_source(source).expect_err("expected compile error").to_string()
}

/// 嵌套 try / loop / case chain / generator / async / catch 作用域时，
/// `current_*` 查询方法应正确反映各层状态，且内层不影响外层可见性。
#[test]
fn nested_try_loop_case_yield_scopes_track_correctly() {
    let mut ctx = ControlFlowContext::default();

    // async 层：is_async_fn=true，默认 allow_blocking=false
    ctx.push_async(None, true);
    assert!(ctx.in_async_scope());
    assert!(!ctx.current_allow_blocking());

    // generator 层：默认 allow_yield=false，显式开启后 current_allow_yield 应为 true
    ctx.push_generator(None, false);
    assert!(ctx.in_generator_scope());
    assert!(!ctx.current_allow_yield());
    if let Some(scope) = ctx.current_generator_scope_mut() {
        scope.allow_yield = true;
    }
    assert!(ctx.current_allow_yield());

    // loop 层：带 label，accepts_break_value=true
    ctx.push_loop(Some("outer".to_string()), true);
    assert!(ctx.resolve_continue(Some("outer")));
    assert!(ctx.resolve_loop_mut(Some("outer")).is_some());

    // try 层：in_try_scope 应为 true
    ctx.push_try(TryScopeData { is_optional: false, is_forced: false, result_type: None });
    assert!(ctx.in_try_scope());

    // case chain 层：进入 arm body 后 current_case_chain_arm_body 应为 true
    ctx.push_case_chain(None, Some(1), true);
    assert!(!ctx.current_case_chain_arm_body());
    if let Some(scope) = ctx.current_case_chain_scope_mut() {
        scope.in_arm_body = true;
    }
    assert!(ctx.current_case_chain_arm_body());

    // catch 层：depth=1，进入 arm body 后 current_catch_arm_body 应为 true
    ctx.push_catch(None, true, true);
    assert_eq!(ctx.current_catch_depth(), 1);
    assert!(!ctx.current_catch_arm_body());
    if let Some(scope) = ctx.current_catch_scope_mut() {
        scope.in_arm_body = true;
        scope.depth = 1;
    }
    assert!(ctx.current_catch_arm_body());

    // 跨作用域查询：外层状态在内层仍可见
    assert!(ctx.in_async_scope());
    assert!(ctx.in_generator_scope());
    assert!(ctx.in_try_scope());
    assert!(ctx.current_allow_yield());

    // 逐层弹出，状态应正确回退
    ctx.pop_catch();
    assert_eq!(ctx.current_catch_depth(), 0);
    ctx.pop_case_chain();
    assert!(!ctx.current_case_chain_arm_body());
    ctx.pop_try();
    assert!(!ctx.in_try_scope());
    ctx.pop_loop();
    assert!(!ctx.resolve_continue(Some("outer")));
    ctx.pop_generator();
    assert!(!ctx.in_generator_scope());
    assert!(!ctx.current_allow_yield());
    ctx.pop_async();
    assert!(!ctx.in_async_scope());

    assert!(ctx.is_empty());
}

/// Lambda 通过 `clone_scopes` 继承外层 generator 作用域，
/// 继承后的上下文应保留 allow_yield 标志与 label registry。
#[test]
fn lambda_inherits_outer_generator_context() {
    let mut parent = ControlFlowContext::default();
    parent.push_generator(Some("gen".to_string()), true);
    if let Some(scope) = parent.current_generator_scope_mut() {
        scope.allow_yield = true;
    }

    // lambda 继承外层控制流
    let lambda = parent.clone_scopes();

    // 继承了 generator scope 与 allow_yield 标志
    assert!(lambda.in_generator_scope());
    assert!(lambda.current_allow_yield());
    assert!(lambda.current_generator_scope().is_some_and(|scope| scope.is_async_generator));

    // label registry 也被继承
    assert_eq!(lambda.resolve_scope("gen"), Some((ScopeKind::Generator, 0)));

    // 外层修改不应影响已 clone 的 lambda 上下文
    parent.pop_generator();
    assert!(parent.in_generator_scope() == false);
    assert!(lambda.in_generator_scope());
}

/// Lambda 通过 `clone_scopes` 继承外层 async 作用域，
/// 继承后的上下文应保留 allow_blocking 标志与 is_async_fn 状态。
#[test]
fn lambda_inherits_outer_async_context() {
    let mut parent = ControlFlowContext::default();
    parent.push_async(Some("async".to_string()), true);
    if let Some(scope) = parent.current_async_scope_mut() {
        scope.allow_blocking = true;
    }

    // lambda 继承外层控制流
    let lambda = parent.clone_scopes();

    // 继承了 async scope 与 allow_blocking 标志
    assert!(lambda.in_async_scope());
    assert!(lambda.current_allow_blocking());
    assert!(lambda.current_async_scope().is_some_and(|scope| scope.is_async_fn));

    // label registry 也被继承
    assert_eq!(lambda.resolve_scope("async"), Some((ScopeKind::Async, 0)));

    // 外层修改不应影响已 clone 的 lambda 上下文
    parent.pop_async();
    assert!(!parent.in_async_scope());
    assert!(lambda.in_async_scope());
}

/// `fallthrough` 出现在非 case chain 作用域（如 loop 内）应被 HIR 校验拒绝。
#[test]
fn fallthrough_in_non_case_scope_rejected() {
    let error = compile_err(
        r#"
micro main() {
    'outer: loop {
        fallthrough
        break 'outer
    }
}
"#,
    );
    assert!(error.contains("fallthrough"), "expected fallthrough rejection, got: {error}");
    assert!(error.contains("case"), "expected case scope hint, got: {error}");
}

/// `break 'label` 应能跨越中间的 try / 内层 loop 作用域解析到外层带 label 的 loop。
#[test]
fn break_label_resolves_across_try_scope() {
    let mut ctx = ControlFlowContext::default();

    // 模拟：'outer: loop { try? { loop { break 'outer } } }
    ctx.push_loop(Some("outer".to_string()), false);
    ctx.push_try(TryScopeData { is_optional: true, is_forced: false, result_type: None });
    ctx.push_loop(None, false);

    // break 'outer 应跨过 try 和内层 loop 解析到外层 loop
    let outer = ctx.resolve_loop_mut(Some("outer")).expect("break label should resolve across try scope");
    assert!(!outer.accepts_break_value);

    // 无 label 的 break 应解析到内层 loop
    assert!(ctx.resolve_loop_mut(None).is_some());

    // 未注册的 label 应解析失败
    assert!(ctx.resolve_loop_mut(Some("missing")).is_none());

    ctx.pop_loop();
    ctx.pop_try();
    ctx.pop_loop();
    assert!(ctx.is_empty());
}

/// `continue 'label` 应只解析到带该 label 的 loop，
/// 无 label 的 continue 解析到内层 loop，未注册 label 解析失败。
#[test]
fn continue_label_resolves_to_outer_loop_only() {
    let mut ctx = ControlFlowContext::default();

    // 模拟：'outer: loop { loop { continue 'outer } }
    ctx.push_loop(Some("outer".to_string()), false);
    ctx.push_loop(None, false);

    // continue 'outer 解析到外层 loop
    assert!(ctx.resolve_continue(Some("outer")));
    // 无 label continue 解析到内层 loop
    assert!(ctx.resolve_continue(None));
    // 未注册 label 解析失败
    assert!(!ctx.resolve_continue(Some("inner")));

    ctx.pop_loop();
    // 弹出内层后，无 label continue 仍可解析到外层
    assert!(ctx.resolve_continue(None));
    ctx.pop_loop();
    assert!(!ctx.resolve_continue(None));
}

/// `resume` 出现在 catch arm body 外（如顶层函数体）应被 HIR 校验拒绝。
#[test]
fn resume_outside_catch_arm_rejected() {
    let error = compile_err(
        r#"
micro main() {
    resume value
}
"#,
    );
    assert!(error.contains("resume"), "expected resume rejection, got: {error}");
    assert!(error.contains("catch"), "expected catch scope hint, got: {error}");
}
