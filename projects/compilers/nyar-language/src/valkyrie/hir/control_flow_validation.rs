use std::collections::{BTreeMap, BTreeSet};

use crate::{
    types::{
        Identifier, NamePath,
        hir::{
            HirBlock, HirExpr, HirExprKind, HirFunction, HirLiteral, HirMatchArm, HirModule, HirPattern, HirStatement, HirStatementKind,
            ValkyrieType,
        },
    },
    valkyrie::{
        control_flow::{ControlFlowContext, TryScopeData},
        hir::{is_nullable_type, is_option_apply_type, is_result_apply_type, nullable_payload_type},
    },
};
use std_data::text::valkyrie::ParseError;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct HirValidationState {
    current_function: Option<String>,
    in_guard: bool,
    self_type_stack: Vec<ValkyrieType>,
    return_type_stack: Vec<ValkyrieType>,
    local_scopes: Vec<BTreeMap<String, ValkyrieType>>,
    /// Unified loop / try / case chain / catch / generator / async scope tracking.
    control_flow: ControlFlowContext,
}

type LoopValidationContext = crate::valkyrie::control_flow::LoopScopeData;

pub fn validate_control_flow_module(module: &HirModule) -> Result<(), ParseError> {
    for function in &module.functions {
        let mut state = HirValidationState::default();
        validate_hir_function(function, &mut state)?;
    }
    for struct_def in &module.structs {
        for method in &struct_def.methods {
            let mut state = HirValidationState::default();
            push_self_type(&mut state, ValkyrieType::Named(struct_def.name.clone()));
            validate_hir_function(method, &mut state)?;
            pop_self_type(&mut state);
        }
        for property in &struct_def.properties {
            if let Some(getter) = &property.getter {
                let mut state = HirValidationState::default();
                push_self_type(&mut state, ValkyrieType::Named(struct_def.name.clone()));
                validate_hir_function(getter, &mut state)?;
                pop_self_type(&mut state);
            }
            if let Some(setter) = &property.setter {
                let mut state = HirValidationState::default();
                push_self_type(&mut state, ValkyrieType::Named(struct_def.name.clone()));
                validate_hir_function(setter, &mut state)?;
                pop_self_type(&mut state);
            }
        }
    }
    for trait_def in &module.traits {
        for method in &trait_def.methods {
            let mut state = HirValidationState::default();
            validate_hir_function(method, &mut state)?;
        }
        for method in &trait_def.default_methods {
            let mut state = HirValidationState::default();
            validate_hir_function(method, &mut state)?;
        }
    }
    for impl_block in &module.impls {
        for method in &impl_block.methods {
            let mut state = HirValidationState::default();
            push_self_type(&mut state, impl_block.target.clone());
            validate_hir_function(method, &mut state)?;
            pop_self_type(&mut state);
        }
    }
    Ok(())
}

fn validate_hir_function(function: &HirFunction, state: &mut HirValidationState) -> Result<(), ParseError> {
    let saved_function = state.current_function.clone();
    state.current_function = Some(function.name.to_string());
    let is_async = function_is_async(&function.body);
    let allow_blocking_value = !is_async;
    let is_generator = function_body_contains_yield(&function.body);
    state.control_flow.push_async(None, is_async);
    if let Some(scope) = state.control_flow.current_async_scope_mut() {
        scope.allow_blocking = allow_blocking_value;
    }
    if is_generator {
        state.control_flow.push_generator(None, is_async);
        if let Some(scope) = state.control_flow.current_generator_scope_mut() {
            scope.allow_yield = true;
        }
    }
    push_return_type(state, resolve_contextual_type(&function.return_type, state));
    push_local_scope(state);
    for param in &function.params {
        bind_local_type(state, param.name.name.as_str(), resolve_contextual_type(&param.ty, state));
    }
    for statement in &function.body.statements {
        validate_hir_statement(statement, state)?;
    }
    if let Some(expr) = &function.body.expr {
        validate_hir_expr(expr, state, true)?;
    }
    pop_local_scope(state);
    pop_return_type(state);
    if is_generator {
        state.control_flow.pop_generator();
    }
    state.control_flow.pop_async();
    state.current_function = saved_function;
    Ok(())
}

fn validate_hir_statement(statement: &HirStatement, state: &mut HirValidationState) -> Result<(), ParseError> {
    match &statement.kind {
        HirStatementKind::Let { pattern, initializer, ty, .. } => {
            if let Some(initializer) = initializer {
                validate_hir_expr(initializer, state, true)?;
            }
            validate_pattern_semantics(pattern)?;
            let inferred_type = ty.clone().or_else(|| initializer.as_ref().and_then(|value| infer_static_expr_type(value, state)));
            if let Some(inferred_type) = inferred_type {
                bind_pattern_type(state, pattern, &inferred_type);
            }
        }
        HirStatementKind::Expr(expr) => validate_hir_expr(expr, state, false)?,
    }
    Ok(())
}

fn validate_hir_block(block: &HirBlock, state: &mut HirValidationState, value_context: bool) -> Result<(), ParseError> {
    push_local_scope(state);
    for statement in &block.statements {
        validate_hir_statement(statement, state)?;
    }
    if let Some(expr) = &block.expr {
        validate_hir_expr(expr, state, value_context)?;
    }
    pop_local_scope(state);
    Ok(())
}

fn validate_hir_expr(expr: &HirExpr, state: &mut HirValidationState, value_context: bool) -> Result<(), ParseError> {
    match &expr.kind {
        HirExprKind::Call { callee, args, .. } => {
            validate_hir_expr(callee, state, true)?;
            for arg in args {
                validate_hir_expr(&arg.value, state, true)?;
            }
        }
        HirExprKind::FieldInit { value, .. }
        | HirExprKind::Await(value)
        | HirExprKind::Awake(value)
        | HirExprKind::BlockOn(value)
        | HirExprKind::YieldFrom(value)
        | HirExprKind::TryPropagate(value)
        | HirExprKind::Raise(value)
        | HirExprKind::Resume(value) => {
            if matches!(expr.kind, HirExprKind::Resume(_))
                && (state.control_flow.current_catch_depth() == 0 || !state.control_flow.current_catch_arm_body())
            {
                return Err(ParseError::invalid("控制流调度校验失败：检测到未位于 `catch arm body` 的 `resume`"));
            }
            if state.in_guard {
                let control_flow_name = match &expr.kind {
                    HirExprKind::Await(_) => "`await`",
                    HirExprKind::Awake(_) => "`awake`",
                    HirExprKind::BlockOn(_) => "`block`",
                    HirExprKind::YieldFrom(_) => "`yield from`",
                    HirExprKind::Resume(_) => "`resume`",
                    HirExprKind::TryPropagate(_) => "`?`",
                    _ => "",
                };
                if !control_flow_name.is_empty() {
                    return Err(ParseError::invalid(format!("控制流调度校验失败：guard 中不允许出现会打断控制流连续性的 {control_flow_name}")));
                }
            }
            if matches!(expr.kind, HirExprKind::BlockOn(_)) {
                validate_blocking_context(expr, state)?;
            }
            if matches!(expr.kind, HirExprKind::Await(_)) {
                validate_await_context(expr, state)?;
            }
            if matches!(expr.kind, HirExprKind::YieldFrom(_)) {
                validate_yield_context(expr, state)?;
            }
            if matches!(expr.kind, HirExprKind::TryPropagate(_)) {
                validate_try_propagate_context(expr, state)?;
            }
            validate_hir_expr(value, state, true)?;
            if matches!(expr.kind, HirExprKind::YieldFrom(_)) {
                validate_yield_from_value(expr, value, state)?;
            }
            if matches!(expr.kind, HirExprKind::Await(_) | HirExprKind::Awake(_) | HirExprKind::BlockOn(_)) {
                validate_future_control_operand(expr, value, state)?;
            }
        }
        HirExprKind::Construct { args, .. } | HirExprKind::ArrayLiteral { items: args } => {
            for arg in args {
                validate_hir_expr(arg, state, true)?;
            }
        }
        HirExprKind::Fallthrough => {
            let in_case_arm = state.control_flow.current_case_chain_arm_body();
            let in_catch_arm = state.control_flow.current_catch_arm_body();
            let has_fallthrough_target = state.control_flow.current_case_chain_scope().is_some_and(|scope| scope.next_arm_target.is_some());
            if in_case_arm && has_fallthrough_target && !in_catch_arm {
                return Ok(());
            }
            if in_case_arm && !has_fallthrough_target {
                return Err(ParseError::invalid("控制流调度校验失败：最后一个 `case` arm 不允许使用 `fallthrough`"));
            }
            return Err(ParseError::invalid("控制流调度校验失败：`fallthrough` 仅允许出现在 `case` statement 体系中"));
        }
        HirExprKind::With { base, updates } => {
            validate_hir_expr(base, state, true)?;
            for (_, value) in updates {
                validate_hir_expr(value, state, true)?;
            }
        }
        HirExprKind::TryScope { is_optional, is_forced, result_type, body } => {
            state.control_flow.push_try(TryScopeData { is_optional: *is_optional, is_forced: *is_forced, result_type: result_type.clone() });
            validate_hir_block(body, state, true)?;
            state.control_flow.pop_try();
        }
        HirExprKind::SuperCall { args, .. } => {
            for arg in args {
                validate_hir_expr(arg, state, true)?;
            }
        }
        HirExprKind::ArrayNew { length, .. } => validate_hir_expr(length, state, true)?,
        HirExprKind::FieldAccess { object, .. } => validate_hir_expr(object, state, true)?,
        HirExprKind::StoreField { object, value, .. } => {
            validate_hir_expr(object, state, true)?;
            validate_hir_expr(value, state, true)?;
        }
        HirExprKind::GenericApply { callee, .. } => validate_hir_expr(callee, state, true)?,
        HirExprKind::Block(block) => validate_hir_block(block, state, value_context)?,
        HirExprKind::Lambda { params, body, .. } => {
            let mut lambda_state = lambda_validation_state(state, body);
            if let HirExprKind::Lambda { return_type, .. } = &expr.kind {
                push_return_type(&mut lambda_state, return_type.clone());
            }
            let lambda_is_async = function_body_contains_await(body);
            let lambda_contains_yield = block_contains_yield(body);
            lambda_state.control_flow.push_async(None, lambda_is_async);
            if lambda_contains_yield {
                lambda_state.control_flow.push_generator(None, lambda_is_async);
                if let Some(scope) = lambda_state.control_flow.current_generator_scope_mut() {
                    scope.allow_yield = true;
                }
            }
            push_local_scope(&mut lambda_state);
            for param in params {
                bind_local_type(&mut lambda_state, param.name.name.as_str(), param.ty.clone());
            }
            validate_hir_block(body, &mut lambda_state, true)?;
            pop_local_scope(&mut lambda_state);
            if lambda_contains_yield {
                lambda_state.control_flow.pop_generator();
            }
            lambda_state.control_flow.pop_async();
            pop_return_type(&mut lambda_state);
        }
        HirExprKind::AnonymousClass { fields, methods, .. } => {
            for (_, value) in fields {
                validate_hir_expr(value, state, true)?;
            }
            for method in methods {
                validate_hir_function(method, &mut HirValidationState::default())?;
            }
        }
        HirExprKind::If { condition, then_branch, else_branch } => {
            validate_hir_expr(condition, state, true)?;
            validate_hir_block(then_branch, state, value_context)?;
            if let Some(else_branch) = else_branch {
                validate_hir_block(else_branch, state, value_context)?;
            }
        }
        HirExprKind::IfLet { pattern, scrutinee, then_branch, else_branch } => {
            validate_hir_expr(scrutinee, state, true)?;
            push_local_scope(state);
            validate_pattern_semantics(pattern)?;
            if let Some(scrutinee_type) = infer_static_expr_type(scrutinee, state) {
                bind_pattern_type(state, pattern, &scrutinee_type);
            }
            validate_hir_block(then_branch, state, value_context)?;
            pop_local_scope(state);
            if let Some(else_branch) = else_branch {
                validate_hir_block(else_branch, state, value_context)?;
            }
        }
        HirExprKind::Match { scrutinee, arms } => {
            validate_hir_expr(scrutinee, state, true)?;
            let scrutinee_type = infer_static_expr_type(scrutinee, state);
            for arm in arms {
                push_local_scope(state);
                validate_pattern_semantics(&arm.pattern)?;
                if let Some(scrutinee_type) = &scrutinee_type {
                    bind_pattern_type(state, &arm.pattern, scrutinee_type);
                }
                let saved_in_guard = state.in_guard;
                state.in_guard = true;
                if let Some(guard) = &arm.guard {
                    validate_hir_expr(guard, state, true)?;
                }
                state.in_guard = saved_in_guard;
                validate_hir_expr(&arm.body, state, value_context)?;
                pop_local_scope(state);
            }
        }
        HirExprKind::Case { scrutinee, arms } => {
            validate_hir_expr(scrutinee, state, true)?;
            let scrutinee_type = infer_static_expr_type(scrutinee, state);
            let mut previous_fallthrough_bindings = BTreeSet::new();
            state.control_flow.push_case_chain(None, None, arms.len() > 1);
            for (index, arm) in arms.iter().enumerate() {
                if let Some(leaked_name) = detect_case_fallthrough_binding_leak(arm, &previous_fallthrough_bindings, state) {
                    state.control_flow.pop_case_chain();
                    return Err(ParseError::invalid(format!(
                        "控制流调度校验失败：`fallthrough` 不继承上一 `case` arm �?pattern 绑定，当�?arm 非法引用�?`{leaked_name}`"
                    )));
                }
                push_local_scope(state);
                validate_pattern_semantics(&arm.pattern)?;
                if let Some(scrutinee_type) = &scrutinee_type {
                    bind_pattern_type(state, &arm.pattern, scrutinee_type);
                }
                let saved_in_guard = state.in_guard;
                state.in_guard = true;
                if let Some(guard) = &arm.guard {
                    validate_hir_expr(guard, state, true)?;
                }
                state.in_guard = saved_in_guard;
                let has_next_arm = index + 1 < arms.len();
                if let Some(scope) = state.control_flow.current_case_chain_scope_mut() {
                    scope.in_arm_body = true;
                    scope.next_arm_target = if has_next_arm { Some(index + 1) } else { None };
                }
                validate_hir_expr(&arm.body, state, false)?;
                if let Some(scope) = state.control_flow.current_case_chain_scope_mut() {
                    scope.in_arm_body = false;
                }
                pop_local_scope(state);
                previous_fallthrough_bindings =
                    if expr_contains_fallthrough(&arm.body) { collect_pattern_bound_names(&arm.pattern) } else { BTreeSet::new() };
            }
            state.control_flow.pop_case_chain();
        }
        HirExprKind::Loop { iterator, condition, body, label, pattern, .. } => {
            if let Some(iterator) = iterator {
                validate_hir_expr(iterator, state, true)?;
            }
            if let Some(condition) = condition {
                validate_hir_expr(condition, state, true)?;
            }
            if let (Some(pattern), Some(iterator)) = (pattern, iterator) {
                validate_pattern_semantics(pattern)?;
                if let Some(iterator_type) = infer_static_expr_type(iterator, state) {
                    bind_pattern_type(state, pattern, &iterator_type);
                }
            }
            push_loop_context(state, label.as_ref().map(|it| it.as_str()), value_context);
            validate_hir_block(body, state, false)?;
            pop_loop_context(state);
        }
        HirExprKind::Return(value) | HirExprKind::Yield(value) => {
            if matches!(expr.kind, HirExprKind::Yield(_)) && state.in_guard {
                return Err(ParseError::invalid("控制流调度校验失败：guard 中不允许出现会打断控制流连续性的 `yield`"));
            }
            if matches!(expr.kind, HirExprKind::Yield(_)) {
                validate_yield_context(expr, state)?;
            }
            if let Some(value) = value {
                validate_hir_expr(value, state, true)?;
            }
            if matches!(expr.kind, HirExprKind::Return(_)) {
                validate_return_value(expr, value.as_deref(), state)?;
            }
            else {
                validate_yield_value(expr, value.as_deref(), state)?;
            }
        }
        HirExprKind::Assign { value, .. } => validate_hir_expr(value, state, true)?,
        HirExprKind::Break { label, expr } => {
            if let Some(expr) = expr {
                let label_str = label.as_ref().map(|it| it.as_str());
                state.control_flow.set_validating_break_value(true, label_str);
                validate_hir_expr(expr, state, true)?;
                state.control_flow.set_validating_break_value(false, label_str);
                let break_type = infer_static_expr_type(expr, state);
                let target = resolve_break_target_mut(label.as_ref().map(|it| it.as_str()), state);
                if let Some(loop_context) = target {
                    if !loop_context.accepts_break_value {
                        return Err(ParseError::invalid("控制流调度校验失败：`break expr` 的目标 loop 当前不接受值"));
                    }
                    if let Some(break_type) = break_type {
                        if let Some(expected_type) = &loop_context.break_value_type {
                            if !control_flow_value_type_compatible(expected_type, &break_type) {
                                return Err(ParseError::invalid(format!(
                                    "控制流调度校验失败：`break expr` 的值类型 `{}` 与目标 loop 已收敛的结果类型 `{}` 不兼容",
                                    display_type(&break_type),
                                    display_type(expected_type)
                                )));
                            }
                        }
                        else {
                            loop_context.break_value_type = Some(break_type);
                        }
                    }
                }
                else {
                    return Err(ParseError::invalid("控制流调度校验失败：`break expr` 未找到可接受值的目标 loop"));
                }
            }
            else if resolve_break_target_mut(label.as_ref().map(|it| it.as_str()), state).is_none() {
                return Err(ParseError::invalid("控制流调度校验失败：`break` 未找到目标 loop"));
            }
        }
        HirExprKind::Continue { label } => {
            if !resolve_continue_target(label.as_ref(), state) {
                return Err(ParseError::invalid("控制流调度校验失败：`continue` 未找到目标 loop"));
            }
        }
        HirExprKind::Catch { expr, arms } => {
            validate_hir_expr(expr, state, true)?;
            let new_depth = state.control_flow.current_catch_depth() + 1;
            state.control_flow.push_catch(None, true, true);
            if let Some(scope) = state.control_flow.current_catch_scope_mut() {
                scope.depth = new_depth;
            }
            for arm in arms {
                let saved_in_guard = state.in_guard;
                state.in_guard = true;
                if let Some(scope) = state.control_flow.current_catch_scope_mut() {
                    scope.in_arm_body = false;
                }
                if let Some(guard) = &arm.guard {
                    validate_hir_expr(guard, state, true)?;
                }
                state.in_guard = false;
                if let Some(scope) = state.control_flow.current_catch_scope_mut() {
                    scope.in_arm_body = true;
                }
                validate_hir_expr(&arm.body, state, value_context)?;
                if let Some(scope) = state.control_flow.current_catch_scope_mut() {
                    scope.in_arm_body = false;
                }
                state.in_guard = saved_in_guard;
            }
            state.control_flow.pop_catch();
        }
        HirExprKind::Literal(_) | HirExprKind::Variable(_) | HirExprKind::Path(_) => {}
    }
    Ok(())
}

fn push_loop_context(state: &mut HirValidationState, label: Option<&str>, value_context: bool) {
    state.control_flow.push_loop(label.map(str::to_string), value_context);
}

fn pop_loop_context(state: &mut HirValidationState) {
    state.control_flow.pop_loop();
}

/// Lambda 继承外层 async / generator 能力，但 **不继承 label registry**：
/// `break 'outer` / `continue 'outer` 不能跨越函数边界（lambda 是独立函数帧）。
/// 见 `ControlFlowContext::clone_for_function_boundary`。
fn lambda_validation_state(parent: &HirValidationState, _body: &HirBlock) -> HirValidationState {
    HirValidationState {
        in_guard: false,
        self_type_stack: parent.self_type_stack.clone(),
        return_type_stack: Vec::new(),
        local_scopes: Vec::new(),
        control_flow: parent.control_flow.clone_for_function_boundary(),
        current_function: parent.current_function.clone(),
    }
}

fn resolve_break_target_mut<'a>(label: Option<&str>, state: &'a mut HirValidationState) -> Option<&'a mut LoopValidationContext> {
    state.control_flow.resolve_loop_mut(label)
}

fn resolve_continue_target(label: Option<&crate::types::Identifier>, state: &HirValidationState) -> bool {
    state.control_flow.resolve_continue(label.map(|it| it.as_str()))
}

fn infer_static_expr_type(expr: &HirExpr, state: &HirValidationState) -> Option<ValkyrieType> {
    match &expr.kind {
        HirExprKind::Literal(literal) => infer_literal_type(literal),
        HirExprKind::Variable(identifier) => lookup_local_type(state, identifier.name.as_str()),
        HirExprKind::Block(block) => infer_static_block_type(block, state),
        HirExprKind::If { then_branch, else_branch, .. } => {
            let then_type = infer_static_block_type(then_branch, state)?;
            let else_type = else_branch.as_ref().and_then(|branch| infer_static_block_type(branch, state))?;
            if control_flow_value_type_compatible(&then_type, &else_type) { Some(then_type) } else { None }
        }
        HirExprKind::Match { scrutinee, arms } => infer_static_match_type(scrutinee, arms, state),
        HirExprKind::Case { .. } => None,
        HirExprKind::Awake(_) | HirExprKind::Yield(_) | HirExprKind::YieldFrom(_) => Some(ValkyrieType::Unit),
        HirExprKind::Await(value) | HirExprKind::BlockOn(value) => {
            let value_type = infer_static_expr_type(value, state)?;
            future_resume_type(&value_type)
        }
        HirExprKind::Call { callee, resolved, .. } => {
            if is_boolean_operator_call(callee) {
                return Some(ValkyrieType::Boolean);
            }
            resolved.as_ref().map(|call| call.return_type.clone())
        }
        HirExprKind::Construct { name, resolved, .. } => {
            resolved.as_ref().map(|call| call.return_type.clone()).or_else(|| Some(ValkyrieType::Named(name.clone())))
        }
        HirExprKind::TryPropagate(inner) => infer_static_expr_type(inner, state).and_then(|ty| {
            nullable_payload_type(&ty).or_else(|| {
                if is_option_apply_type(&ty) {
                    match ty {
                        ValkyrieType::Apply(_, args) => args.first().cloned(),
                        _ => None,
                    }
                }
                else if is_result_apply_type(&ty) {
                    match ty {
                        ValkyrieType::Apply(_, args) => args.first().cloned(),
                        _ => None,
                    }
                }
                else {
                    None
                }
            })
        }),
        HirExprKind::TryScope { is_optional, is_forced, result_type, body } => {
            infer_try_scope_type(*is_optional, *is_forced, result_type.as_ref(), body, state)
        }
        _ => None,
    }
}

fn is_boolean_operator_call(callee: &HirExpr) -> bool {
    let name = callable_name_from_expr(callee);
    matches!(name, Some("infix ==" | "infix !=" | "infix <" | "infix <=" | "infix >" | "infix >=" | "infix &&" | "infix ||"))
}

fn callable_name_from_expr(callee: &HirExpr) -> Option<&str> {
    match &callee.kind {
        HirExprKind::Variable(identifier) => Some(identifier.name.as_str()),
        HirExprKind::Path(path) => path.parts().last().map(|part| part.as_str()),
        HirExprKind::GenericApply { callee, .. } => callable_name_from_expr(callee),
        _ => None,
    }
}

fn infer_try_scope_type(
    is_optional: bool,
    is_forced: bool,
    result_type: Option<&ValkyrieType>,
    body: &HirBlock,
    state: &HirValidationState,
) -> Option<ValkyrieType> {
    if let Some(explicit) = result_type {
        return Some(explicit.clone());
    }
    let body_type = infer_static_block_type(body, state)?;
    if is_optional {
        return Some(ValkyrieType::Nullable(Box::new(body_type)));
    }
    if is_forced {
        return Some(body_type);
    }
    Some(body_type)
}

fn validate_try_propagate_context(expr: &HirExpr, state: &HirValidationState) -> Result<(), ParseError> {
    let HirExprKind::TryPropagate(inner) = &expr.kind
    else {
        return Ok(());
    };
    let Some(operand_type) = infer_static_expr_type(inner, state)
    else {
        return Ok(());
    };
    let nullable_operand = nullable_payload_type(&operand_type).is_some();
    let option_operand = is_option_apply_type(&operand_type);
    let result_operand = is_result_apply_type(&operand_type);
    if !nullable_operand && !option_operand && !result_operand {
        return Err(ParseError::invalid(format!("`?` cannot be applied to `{}`", display_type(&operand_type))));
    }
    if !state.control_flow.in_try_scope() {
        let return_type = current_return_type(state).cloned().unwrap_or(ValkyrieType::Unit);
        if nullable_operand && !is_nullable_type(&return_type) {
            return Err(ParseError::invalid(format!(
                "`?` requires a nullable enclosing return or `try` scope; got `{}`",
                display_type(&return_type)
            )));
        }
        if option_operand && !is_option_apply_type(&return_type) {
            return Err(ParseError::invalid(format!(
                "`?` on `Option` requires enclosing `Option` return or `try` scope; got `{}`",
                display_type(&return_type)
            )));
        }
        if result_operand && !is_result_apply_type(&return_type) {
            return Err(ParseError::invalid(format!(
                "`?` on `Result` requires enclosing `Result` return or `try` scope; got `{}`",
                display_type(&return_type)
            )));
        }
    }
    Ok(())
}

fn resolve_contextual_type(ty: &ValkyrieType, state: &HirValidationState) -> ValkyrieType {
    match ty {
        ValkyrieType::SelfType => current_self_type(state).cloned().unwrap_or(ValkyrieType::SelfType),
        // HIR 中 `Self` 有时以命名类型形式保留，优先回填当前实现者类型。
        ValkyrieType::Named(name) if name.as_str() == "Self" => {
            current_self_type(state).cloned().unwrap_or_else(|| ValkyrieType::Named(name.clone()))
        }
        ValkyrieType::Apply(base, arguments) => ValkyrieType::Apply(
            Box::new(resolve_contextual_type(base, state)),
            arguments.iter().map(|arg| resolve_contextual_type(arg, state)).collect(),
        ),
        ValkyrieType::Function(function) => ValkyrieType::Function(Box::new(crate::types::hir::FunctionType {
            params: function.params.iter().map(|param| resolve_contextual_type(param, state)).collect(),
            return_type: resolve_contextual_type(&function.return_type, state),
        })),
        ValkyrieType::Tuple(items) => ValkyrieType::Tuple(items.iter().map(|item| resolve_contextual_type(item, state)).collect()),
        ValkyrieType::Row(row) => ValkyrieType::Row(crate::types::hir::RowType {
            methods: row
                .methods
                .iter()
                .map(|method| crate::types::hir::RowMethodType {
                    name: method.name.clone(),
                    params: method.params.iter().map(|param| resolve_contextual_type(param, state)).collect(),
                    return_type: resolve_contextual_type(&method.return_type, state),
                })
                .collect(),
        }),
        ValkyrieType::Array(item) => ValkyrieType::Array(Box::new(resolve_contextual_type(item, state))),
        ValkyrieType::FixedArray { element, length } => {
            ValkyrieType::FixedArray { element: Box::new(resolve_contextual_type(element, state)), length: *length }
        }
        ValkyrieType::TypeLambda(lambda) => ValkyrieType::TypeLambda(Box::new(crate::types::hir::TypeLambda {
            params: lambda.params.clone(),
            body: resolve_contextual_type(&lambda.body, state),
        })),
        ValkyrieType::TraitObject(object) => ValkyrieType::TraitObject(crate::types::hir::TraitObject {
            trait_path: object.trait_path.clone(),
            type_arguments: object.type_arguments.iter().map(|arg| resolve_contextual_type(arg, state)).collect(),
        }),
        ValkyrieType::Associated(associated) => ValkyrieType::Associated(Box::new(crate::types::hir::AssociatedType {
            base: resolve_contextual_type(&associated.base, state),
            name: associated.name.clone(),
            type_arguments: associated.type_arguments.iter().map(|arg| resolve_contextual_type(arg, state)).collect(),
        })),
        other => other.clone(),
    }
}

fn infer_static_block_type(block: &HirBlock, state: &HirValidationState) -> Option<ValkyrieType> {
    if let Some(expr) = &block.expr { infer_static_expr_type(expr, state) } else { Some(ValkyrieType::Unit) }
}

fn infer_static_match_type(scrutinee: &HirExpr, arms: &[HirMatchArm], state: &HirValidationState) -> Option<ValkyrieType> {
    let mut inferred_type: Option<ValkyrieType> = None;
    let scrutinee_type = infer_static_expr_type(scrutinee, state);
    for arm in arms {
        let mut arm_state = state.clone();
        push_local_scope(&mut arm_state);
        if let Some(scrutinee_type) = &scrutinee_type {
            bind_pattern_type(&mut arm_state, &arm.pattern, scrutinee_type);
        }
        let arm_type = infer_static_expr_type(&arm.body, &arm_state)?;
        if let Some(current_type) = &inferred_type {
            if !control_flow_value_type_compatible(current_type, &arm_type) {
                return None;
            }
        }
        else {
            inferred_type = Some(arm_type);
        }
    }
    inferred_type
}

fn infer_literal_type(literal: &HirLiteral) -> Option<ValkyrieType> {
    match literal {
        HirLiteral::Integer64(_) => Some(ValkyrieType::Integer64 { signed: true }),
        HirLiteral::Float64(_) => Some(ValkyrieType::Float64),
        HirLiteral::Bool(_) => Some(ValkyrieType::Boolean),
        HirLiteral::Unit => Some(ValkyrieType::Unit),
        HirLiteral::String(_) => None,
    }
}

fn validate_future_control_operand(expr: &HirExpr, value: &HirExpr, state: &HirValidationState) -> Result<(), ParseError> {
    let Some(value_type) = infer_static_expr_type(value, state)
    else {
        return Ok(());
    };
    if future_resume_type(&value_type).is_some() {
        return Ok(());
    }
    let control_flow_name = match expr.kind {
        HirExprKind::Await(_) => "`await`",
        HirExprKind::Awake(_) => "`awake`",
        HirExprKind::BlockOn(_) => "`block`",
        _ => return Ok(()),
    };
    Err(ParseError::invalid(format!(
        "控制流调度校验失败：{control_flow_name} 需�?`Future<T>` �?`Promise<T>` 类型的操作数，当前得�?`{}`",
        display_type(&value_type)
    )))
}

fn validate_return_value(expr: &HirExpr, value: Option<&HirExpr>, state: &HirValidationState) -> Result<(), ParseError> {
    let Some(expected_raw) = current_return_type(state)
    else {
        return Ok(());
    };
    let expected_type = resolve_contextual_type(expected_raw, state);

    let Some(value) = value
    else {
        if return_without_value_compatible(&expected_type) {
            return Ok(());
        }
        return Err(ParseError::invalid(format!("控制流调度校验失败：函数返回类型 `{}` 不接受无值 `return`", display_type(&expected_type))));
    };

    let Some(actual_raw) = infer_static_expr_type(value, state)
    else {
        return Ok(());
    };
    let actual_type = resolve_contextual_type(&actual_raw, state);
    if control_flow_value_type_compatible(&expected_type, &actual_type) {
        return Ok(());
    }
    // 自举阶段：比较/相等调用在 overload 回填前可能暂时推成文本类型。
    if matches!(&expected_type, ValkyrieType::Boolean) && is_text_like_type(&actual_type) && matches!(value.kind, HirExprKind::Call { .. }) {
        return Ok(());
    }

    let _ = expr;
    let function_hint = state.current_function.as_ref().map(|name| format!("（函数 `{name}`）")).unwrap_or_default();
    Err(ParseError::invalid(format!(
        "控制流调度校验失败：`return expr` 的值类型 `{}` 与函数返回类型 `{}` 不兼容{function_hint}",
        display_type(&actual_type),
        display_type(&expected_type)
    )))
}

/// 校验 `yield expr` 的值类型与函数返回类型 `Generator<T>` 的 `T` 一致。
///
/// 当函数返回类型为 `Generator<T>` / `Iterator<T>` / `Coroutine<T>` 时，
/// 函数体内所有 `yield expr` 的 `expr` 类型需与 `T` 兼容。
/// 若返回类型不是生成器协议，则跳过校验（由 `validate_yield_context` 负责 scope 校验）。
fn validate_yield_value(expr: &HirExpr, value: Option<&HirExpr>, state: &HirValidationState) -> Result<(), ParseError> {
    let Some(expected_raw) = current_return_type(state)
    else {
        return Ok(());
    };
    let expected_return = resolve_contextual_type(expected_raw, state);
    let Some(yield_item_type) = generator_yield_type(&expected_return)
    else {
        return Ok(());
    };
    let Some(value) = value
    else {
        if matches!(yield_item_type, ValkyrieType::Unit) {
            return Ok(());
        }
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：函数返回类型 `{}` 要求 `yield` 携带值，但遇到无值 `yield`",
            display_type(&expected_return)
        )));
    };
    let Some(actual_raw) = infer_static_expr_type(value, state)
    else {
        return Ok(());
    };
    let actual_type = resolve_contextual_type(&actual_raw, state);
    if control_flow_value_type_compatible(&yield_item_type, &actual_type) {
        return Ok(());
    }
    let _ = expr;
    let function_hint = state.current_function.as_ref().map(|name| format!("（函数 `{name}`）")).unwrap_or_default();
    Err(ParseError::invalid(format!(
        "控制流调度校验失败：`yield expr` 的值类型 `{}` 与函数返回类型 `{}` 的 yield 元素类型 `{}` 不兼容{function_hint}",
        display_type(&actual_type),
        display_type(&expected_return),
        display_type(&yield_item_type)
    )))
}

/// 校验 `yield from generator_expr` 的元素类型与函数返回类型 `Generator<T>` 的 `T` 一致。
///
/// `yield from` 委托另一个生成器，要求被委托生成器的 yield 元素类型与当前函数返回类型的元素类型兼容。
fn validate_yield_from_value(expr: &HirExpr, value: &HirExpr, state: &HirValidationState) -> Result<(), ParseError> {
    let Some(expected_raw) = current_return_type(state)
    else {
        return Ok(());
    };
    let expected_return = resolve_contextual_type(expected_raw, state);
    let Some(expected_item_type) = generator_yield_type(&expected_return)
    else {
        return Ok(());
    };
    let Some(source_type_raw) = infer_static_expr_type(value, state)
    else {
        return Ok(());
    };
    let source_type = resolve_contextual_type(&source_type_raw, state);
    let Some(source_item_type) = generator_yield_type(&source_type)
    else {
        return Err(ParseError::invalid(format!(
            "控制流调度校验失败：`yield from` 的操作数类型 `{}` 不是生成器协议（`Generator<T>` / `Iterator<T>` / `Coroutine<T>`）",
            display_type(&source_type)
        )));
    };
    if control_flow_value_type_compatible(&expected_item_type, &source_item_type) {
        return Ok(());
    }
    let _ = expr;
    Err(ParseError::invalid(format!(
        "控制流调度校验失败：`yield from` 的元素类型 `{}` 与函数返回类型 `{}` 的 yield 元素类型 `{}` 不兼容",
        display_type(&source_item_type),
        display_type(&expected_return),
        display_type(&expected_item_type)
    )))
}

fn validate_blocking_context(expr: &HirExpr, state: &HirValidationState) -> Result<(), ParseError> {
    if !matches!(expr.kind, HirExprKind::BlockOn(_)) {
        return Ok(());
    }
    if state.control_flow.current_validating_break_value() {
        return Ok(());
    }
    if state.control_flow.in_async_scope() {
        return Err(ParseError::invalid("控制流调度校验失败：`block` 不允许出现在 async 函数上下文"));
    }
    if state.control_flow.current_allow_blocking() {
        return Ok(());
    }
    Err(ParseError::invalid("控制流调度校验失败：`block` 当前不位于允许阻塞的上下文"))
}

fn validate_await_context(expr: &HirExpr, state: &HirValidationState) -> Result<(), ParseError> {
    if !matches!(expr.kind, HirExprKind::Await(_)) {
        return Ok(());
    }
    if state.control_flow.current_validating_break_value() {
        return Ok(());
    }
    if state.control_flow.in_async_scope() {
        return Ok(());
    }
    Err(ParseError::invalid("控制流调度校验失败：`await` 只允许出现在 async 函数上下文"))
}

fn validate_yield_context(expr: &HirExpr, state: &HirValidationState) -> Result<(), ParseError> {
    let control_flow_name = match expr.kind {
        HirExprKind::Yield(_) => "`yield`",
        HirExprKind::YieldFrom(_) => "`yield from`",
        _ => {
            return Ok(());
        }
    };
    if state.control_flow.current_validating_break_value() {
        return Ok(());
    }
    if state.control_flow.current_allow_yield() {
        return Ok(());
    }
    Err(ParseError::invalid(format!("控制流调度校验失败：{control_flow_name} 只允许出现在生成器函数或等价 handler 上下文")))
}

/// 检测函数体是否包含 `yield` / `yield from`（不进入 lambda 体）。
pub fn function_body_contains_yield(block: &HirBlock) -> bool {
    shallow_block_contains_yield(block)
}

/// 检测函数体是否包含显式 `.await`（不进入 lambda 体）。
pub fn function_body_contains_await(block: &HirBlock) -> bool {
    shallow_block_contains_await(block)
}

/// 检测函数体是否包含显式 `.block`（不进入 lambda 体）。
pub fn function_body_contains_block_on(block: &HirBlock) -> bool {
    shallow_block_contains_block_on(block)
}

/// 由显式 `.await` 推导函数是否为 async（无 `async` 关键字）。
pub fn function_is_async(block: &HirBlock) -> bool {
    function_body_contains_await(block)
}

/// 函数是否可能包含 yield / await / block（不含 fire-and-forget 的 `.awake`）。
/// 遗留名称，保留给需要区分 "may yield/await/block" 与 "has suspend effects" 的场景。
pub fn function_may_yield_or_await(block: &HirBlock) -> bool {
    block_contains_yield(block) || block_contains_await(block) || block_contains_block_on(block)
}

/// 函数是否可能挂起（yield / await / block / awake）。
/// 含 lambda 体，供 `ProgramFacts.can_suspend` 与 resume frame 规划使用。
pub fn function_can_suspend(block: &HirBlock) -> bool {
    function_has_suspend_effects(block)
}

/// 函数是否包含需要后端 suspend/spawn 载荷的 async control effect（含 `.awake`）。
/// 含 lambda 体，供 frontend 分区与 driver payload 规划使用。
pub fn function_has_suspend_effects(block: &HirBlock) -> bool {
    function_may_yield_or_await(block) || block_contains_awake(block)
}

/// 函数是否包含 effect handler 控制流（catch / raise / resume）。
pub fn function_can_effect_handle(block: &HirBlock) -> bool {
    block_contains_raise(block) || block_contains_catch(block) || block_contains_resume(block)
}

/// 函数是否需要进入 suspend fragment（yield/await/block/awake 或 effect handler）。
pub fn function_needs_suspend_fragment(block: &HirBlock) -> bool {
    function_has_suspend_effects(block) || function_can_effect_handle(block)
}

fn shallow_block_contains_yield(block: &HirBlock) -> bool {
    block.statements.iter().any(shallow_statement_contains_yield) || block.expr.as_ref().is_some_and(|expr| shallow_expr_contains_yield(expr))
}

fn shallow_block_contains_await(block: &HirBlock) -> bool {
    block.statements.iter().any(shallow_statement_contains_await) || block.expr.as_ref().is_some_and(|expr| shallow_expr_contains_await(expr))
}

fn shallow_block_contains_block_on(block: &HirBlock) -> bool {
    block.statements.iter().any(shallow_statement_contains_block_on)
        || block.expr.as_ref().is_some_and(|expr| shallow_expr_contains_block_on(expr))
}

fn shallow_statement_contains_yield(statement: &HirStatement) -> bool {
    match &statement.kind {
        HirStatementKind::Let { initializer, .. } => initializer.as_ref().is_some_and(|expr| shallow_expr_contains_yield(expr)),
        HirStatementKind::Expr(expr) => shallow_expr_contains_yield(expr),
    }
}

fn shallow_statement_contains_await(statement: &HirStatement) -> bool {
    match &statement.kind {
        HirStatementKind::Let { initializer, .. } => initializer.as_ref().is_some_and(|expr| shallow_expr_contains_await(expr)),
        HirStatementKind::Expr(expr) => shallow_expr_contains_await(expr),
    }
}

fn shallow_statement_contains_block_on(statement: &HirStatement) -> bool {
    match &statement.kind {
        HirStatementKind::Let { initializer, .. } => initializer.as_ref().is_some_and(|expr| shallow_expr_contains_block_on(expr)),
        HirStatementKind::Expr(expr) => shallow_expr_contains_block_on(expr),
    }
}

fn shallow_expr_contains_yield(expr: &HirExpr) -> bool {
    match &expr.kind {
        HirExprKind::Yield(_) | HirExprKind::YieldFrom(_) => true,
        HirExprKind::Lambda { .. } | HirExprKind::Break { .. } => false,
        HirExprKind::Call { callee, args, .. } => {
            shallow_expr_contains_yield(callee) || args.iter().any(|arg| shallow_expr_contains_yield(&arg.value))
        }
        HirExprKind::Construct { args, .. } | HirExprKind::ArrayLiteral { items: args } => args.iter().any(shallow_expr_contains_yield),
        HirExprKind::ArrayNew { length, .. }
        | HirExprKind::FieldInit { value: length, .. }
        | HirExprKind::FieldAccess { object: length, .. }
        | HirExprKind::Await(length)
        | HirExprKind::Awake(length)
        | HirExprKind::BlockOn(length)
        | HirExprKind::Raise(length)
        | HirExprKind::Resume(length)
        | HirExprKind::TryPropagate(length)
        | HirExprKind::Assign { value: length, .. }
        | HirExprKind::GenericApply { callee: length, .. } => shallow_expr_contains_yield(length),
        HirExprKind::Fallthrough | HirExprKind::Literal(_) | HirExprKind::Variable(_) | HirExprKind::Path(_) | HirExprKind::Continue { .. } => {
            false
        }
        HirExprKind::With { base, updates } => {
            shallow_expr_contains_yield(base) || updates.iter().any(|(_, value)| shallow_expr_contains_yield(value))
        }
        HirExprKind::TryScope { body, .. } => shallow_block_contains_yield(body),
        HirExprKind::SuperCall { args, .. } => args.iter().any(shallow_expr_contains_yield),
        HirExprKind::StoreField { object, value, .. } => shallow_expr_contains_yield(object) || shallow_expr_contains_yield(value),
        HirExprKind::Block(block) => shallow_block_contains_yield(block),
        HirExprKind::AnonymousClass { fields, methods, .. } => {
            fields.iter().any(|(_, value)| shallow_expr_contains_yield(value))
                || methods.iter().any(|method| shallow_block_contains_yield(&method.body))
        }
        HirExprKind::If { condition, then_branch, else_branch } | HirExprKind::IfLet { scrutinee: condition, then_branch, else_branch, .. } => {
            shallow_expr_contains_yield(condition)
                || shallow_block_contains_yield(then_branch)
                || else_branch.as_ref().is_some_and(|branch| shallow_block_contains_yield(branch))
        }
        HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
            shallow_expr_contains_yield(scrutinee)
                || arms.iter().any(|arm| {
                    arm.guard.as_ref().is_some_and(|guard| shallow_expr_contains_yield(guard)) || shallow_expr_contains_yield(&arm.body)
                })
        }
        HirExprKind::Loop { iterator, condition, body, .. } => {
            iterator.as_ref().is_some_and(|expr| shallow_expr_contains_yield(expr))
                || condition.as_ref().is_some_and(|expr| shallow_expr_contains_yield(expr))
                || shallow_block_contains_yield(body)
        }
        HirExprKind::Catch { expr, arms } => {
            shallow_expr_contains_yield(expr)
                || arms.iter().any(|arm| {
                    arm.guard.as_ref().is_some_and(|guard| shallow_expr_contains_yield(guard)) || shallow_expr_contains_yield(&arm.body)
                })
        }
        HirExprKind::Return(value) => value.as_ref().is_some_and(|expr| shallow_expr_contains_yield(expr)),
    }
}

fn shallow_expr_contains_await(expr: &HirExpr) -> bool {
    match &expr.kind {
        HirExprKind::Await(_) => true,
        HirExprKind::Lambda { .. } | HirExprKind::Break { .. } => false,
        HirExprKind::Call { callee, args, .. } => {
            shallow_expr_contains_await(callee) || args.iter().any(|arg| shallow_expr_contains_await(&arg.value))
        }
        HirExprKind::Construct { args, .. } | HirExprKind::ArrayLiteral { items: args } => args.iter().any(shallow_expr_contains_await),
        HirExprKind::ArrayNew { length, .. }
        | HirExprKind::FieldInit { value: length, .. }
        | HirExprKind::FieldAccess { object: length, .. }
        | HirExprKind::Awake(length)
        | HirExprKind::BlockOn(length)
        | HirExprKind::Raise(length)
        | HirExprKind::Resume(length)
        | HirExprKind::TryPropagate(length)
        | HirExprKind::Assign { value: length, .. }
        | HirExprKind::GenericApply { callee: length, .. } => shallow_expr_contains_await(length),
        HirExprKind::Yield(_) | HirExprKind::YieldFrom(_) => false,
        HirExprKind::Fallthrough | HirExprKind::Literal(_) | HirExprKind::Variable(_) | HirExprKind::Path(_) | HirExprKind::Continue { .. } => {
            false
        }
        HirExprKind::With { base, updates } => {
            shallow_expr_contains_await(base) || updates.iter().any(|(_, value)| shallow_expr_contains_await(value))
        }
        HirExprKind::TryScope { body, .. } => shallow_block_contains_await(body),
        HirExprKind::SuperCall { args, .. } => args.iter().any(shallow_expr_contains_await),
        HirExprKind::StoreField { object, value, .. } => shallow_expr_contains_await(object) || shallow_expr_contains_await(value),
        HirExprKind::Block(block) => shallow_block_contains_await(block),
        HirExprKind::AnonymousClass { fields, methods, .. } => {
            fields.iter().any(|(_, value)| shallow_expr_contains_await(value))
                || methods.iter().any(|method| shallow_block_contains_await(&method.body))
        }
        HirExprKind::If { condition, then_branch, else_branch } | HirExprKind::IfLet { scrutinee: condition, then_branch, else_branch, .. } => {
            shallow_expr_contains_await(condition)
                || shallow_block_contains_await(then_branch)
                || else_branch.as_ref().is_some_and(|branch| shallow_block_contains_await(branch))
        }
        HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
            shallow_expr_contains_await(scrutinee)
                || arms.iter().any(|arm| {
                    arm.guard.as_ref().is_some_and(|guard| shallow_expr_contains_await(guard)) || shallow_expr_contains_await(&arm.body)
                })
        }
        HirExprKind::Loop { iterator, condition, body, .. } => {
            iterator.as_ref().is_some_and(|expr| shallow_expr_contains_await(expr))
                || condition.as_ref().is_some_and(|expr| shallow_expr_contains_await(expr))
                || shallow_block_contains_await(body)
        }
        HirExprKind::Catch { expr, arms } => {
            shallow_expr_contains_await(expr)
                || arms.iter().any(|arm| {
                    arm.guard.as_ref().is_some_and(|guard| shallow_expr_contains_await(guard)) || shallow_expr_contains_await(&arm.body)
                })
        }
        HirExprKind::Return(value) => value.as_ref().is_some_and(|expr| shallow_expr_contains_await(expr)),
    }
}

fn shallow_expr_contains_block_on(expr: &HirExpr) -> bool {
    match &expr.kind {
        HirExprKind::BlockOn(_) => true,
        HirExprKind::Lambda { .. } | HirExprKind::Break { .. } => false,
        HirExprKind::Call { callee, args, .. } => {
            shallow_expr_contains_block_on(callee) || args.iter().any(|arg| shallow_expr_contains_block_on(&arg.value))
        }
        HirExprKind::Construct { args, .. } | HirExprKind::ArrayLiteral { items: args } => args.iter().any(shallow_expr_contains_block_on),
        HirExprKind::ArrayNew { length, .. }
        | HirExprKind::FieldInit { value: length, .. }
        | HirExprKind::FieldAccess { object: length, .. }
        | HirExprKind::Await(length)
        | HirExprKind::Awake(length)
        | HirExprKind::Raise(length)
        | HirExprKind::Resume(length)
        | HirExprKind::TryPropagate(length)
        | HirExprKind::Assign { value: length, .. }
        | HirExprKind::GenericApply { callee: length, .. } => shallow_expr_contains_block_on(length),
        HirExprKind::Yield(_) | HirExprKind::YieldFrom(_) => false,
        HirExprKind::Fallthrough | HirExprKind::Literal(_) | HirExprKind::Variable(_) | HirExprKind::Path(_) | HirExprKind::Continue { .. } => {
            false
        }
        HirExprKind::With { base, updates } => {
            shallow_expr_contains_block_on(base) || updates.iter().any(|(_, value)| shallow_expr_contains_block_on(value))
        }
        HirExprKind::TryScope { body, .. } => shallow_block_contains_block_on(body),
        HirExprKind::SuperCall { args, .. } => args.iter().any(shallow_expr_contains_block_on),
        HirExprKind::StoreField { object, value, .. } => shallow_expr_contains_block_on(object) || shallow_expr_contains_block_on(value),
        HirExprKind::Block(block) => shallow_block_contains_block_on(block),
        HirExprKind::AnonymousClass { fields, methods, .. } => {
            fields.iter().any(|(_, value)| shallow_expr_contains_block_on(value))
                || methods.iter().any(|method| shallow_block_contains_block_on(&method.body))
        }
        HirExprKind::If { condition, then_branch, else_branch } | HirExprKind::IfLet { scrutinee: condition, then_branch, else_branch, .. } => {
            shallow_expr_contains_block_on(condition)
                || shallow_block_contains_block_on(then_branch)
                || else_branch.as_ref().is_some_and(|branch| shallow_block_contains_block_on(branch))
        }
        HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
            shallow_expr_contains_block_on(scrutinee)
                || arms.iter().any(|arm| {
                    arm.guard.as_ref().is_some_and(|guard| shallow_expr_contains_block_on(guard)) || shallow_expr_contains_block_on(&arm.body)
                })
        }
        HirExprKind::Loop { iterator, condition, body, .. } => {
            iterator.as_ref().is_some_and(|expr| shallow_expr_contains_block_on(expr))
                || condition.as_ref().is_some_and(|expr| shallow_expr_contains_block_on(expr))
                || shallow_block_contains_block_on(body)
        }
        HirExprKind::Catch { expr, arms } => {
            shallow_expr_contains_block_on(expr)
                || arms.iter().any(|arm| {
                    arm.guard.as_ref().is_some_and(|guard| shallow_expr_contains_block_on(guard)) || shallow_expr_contains_block_on(&arm.body)
                })
        }
        HirExprKind::Return(value) => value.as_ref().is_some_and(|expr| shallow_expr_contains_block_on(expr)),
    }
}

fn block_contains_yield(block: &HirBlock) -> bool {
    block.statements.iter().any(statement_contains_yield) || block.expr.as_ref().is_some_and(|expr| expr_contains_yield(expr))
}

fn statement_contains_yield(statement: &HirStatement) -> bool {
    match &statement.kind {
        HirStatementKind::Let { initializer, .. } => initializer.as_ref().is_some_and(|expr| expr_contains_yield(expr)),
        HirStatementKind::Expr(expr) => expr_contains_yield(expr),
    }
}

fn expr_contains_yield(expr: &HirExpr) -> bool {
    match &expr.kind {
        HirExprKind::Yield(_) | HirExprKind::YieldFrom(_) => true,
        HirExprKind::Lambda { body, .. } => block_contains_yield(body),
        HirExprKind::Call { callee, args, .. } => expr_contains_yield(callee) || args.iter().any(|arg| expr_contains_yield(&arg.value)),
        HirExprKind::Construct { args, .. } | HirExprKind::ArrayLiteral { items: args } => args.iter().any(expr_contains_yield),
        HirExprKind::ArrayNew { length, .. }
        | HirExprKind::FieldInit { value: length, .. }
        | HirExprKind::FieldAccess { object: length, .. }
        | HirExprKind::Await(length)
        | HirExprKind::Awake(length)
        | HirExprKind::BlockOn(length)
        | HirExprKind::Raise(length)
        | HirExprKind::Resume(length)
        | HirExprKind::TryPropagate(length)
        | HirExprKind::Assign { value: length, .. }
        | HirExprKind::GenericApply { callee: length, .. } => expr_contains_yield(length),
        HirExprKind::Fallthrough | HirExprKind::Literal(_) | HirExprKind::Variable(_) | HirExprKind::Path(_) | HirExprKind::Continue { .. } => {
            false
        }
        HirExprKind::With { base, updates } => expr_contains_yield(base) || updates.iter().any(|(_, value)| expr_contains_yield(value)),
        HirExprKind::TryScope { body, .. } => block_contains_yield(body),
        HirExprKind::SuperCall { args, .. } => args.iter().any(expr_contains_yield),
        HirExprKind::StoreField { object, value, .. } => expr_contains_yield(object) || expr_contains_yield(value),
        HirExprKind::Block(block) => block_contains_yield(block),
        HirExprKind::AnonymousClass { fields, methods, .. } => {
            fields.iter().any(|(_, value)| expr_contains_yield(value))
                || methods.iter().any(|method| function_body_contains_yield(&method.body))
        }
        HirExprKind::If { condition, then_branch, else_branch } | HirExprKind::IfLet { scrutinee: condition, then_branch, else_branch, .. } => {
            expr_contains_yield(condition)
                || block_contains_yield(then_branch)
                || else_branch.as_ref().is_some_and(|branch| block_contains_yield(branch))
        }
        HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
            expr_contains_yield(scrutinee)
                || arms.iter().any(|arm| arm.guard.as_ref().is_some_and(|guard| expr_contains_yield(guard)) || expr_contains_yield(&arm.body))
        }
        HirExprKind::Loop { iterator, condition, body, .. } => {
            iterator.as_ref().is_some_and(|expr| expr_contains_yield(expr))
                || condition.as_ref().is_some_and(|expr| expr_contains_yield(expr))
                || block_contains_yield(body)
        }
        HirExprKind::Catch { expr, arms } => {
            expr_contains_yield(expr)
                || arms.iter().any(|arm| arm.guard.as_ref().is_some_and(|guard| expr_contains_yield(guard)) || expr_contains_yield(&arm.body))
        }
        HirExprKind::Return(value) | HirExprKind::Break { expr: value, .. } => value.as_ref().is_some_and(|expr| expr_contains_yield(expr)),
    }
}

fn block_contains_await(block: &HirBlock) -> bool {
    block.statements.iter().any(statement_contains_await) || block.expr.as_ref().is_some_and(|expr| expr_contains_await(expr))
}

fn block_contains_block_on(block: &HirBlock) -> bool {
    block.statements.iter().any(statement_contains_block_on) || block.expr.as_ref().is_some_and(|expr| expr_contains_block_on(expr))
}

fn block_contains_awake(block: &HirBlock) -> bool {
    block.statements.iter().any(statement_contains_awake) || block.expr.as_ref().is_some_and(|expr| expr_contains_awake(expr))
}

fn block_contains_raise(block: &HirBlock) -> bool {
    block.statements.iter().any(statement_contains_raise) || block.expr.as_ref().is_some_and(|expr| expr_contains_raise(expr))
}

fn block_contains_catch(block: &HirBlock) -> bool {
    block.statements.iter().any(statement_contains_catch) || block.expr.as_ref().is_some_and(|expr| expr_contains_catch(expr))
}

fn block_contains_resume(block: &HirBlock) -> bool {
    block.statements.iter().any(statement_contains_resume) || block.expr.as_ref().is_some_and(|expr| expr_contains_resume(expr))
}

fn statement_contains_raise(statement: &HirStatement) -> bool {
    match &statement.kind {
        HirStatementKind::Let { initializer, .. } => initializer.as_ref().is_some_and(|expr| expr_contains_raise(expr)),
        HirStatementKind::Expr(expr) => expr_contains_raise(expr),
    }
}

fn statement_contains_catch(statement: &HirStatement) -> bool {
    match &statement.kind {
        HirStatementKind::Let { initializer, .. } => initializer.as_ref().is_some_and(|expr| expr_contains_catch(expr)),
        HirStatementKind::Expr(expr) => expr_contains_catch(expr),
    }
}

fn statement_contains_resume(statement: &HirStatement) -> bool {
    match &statement.kind {
        HirStatementKind::Let { initializer, .. } => initializer.as_ref().is_some_and(|expr| expr_contains_resume(expr)),
        HirStatementKind::Expr(expr) => expr_contains_resume(expr),
    }
}

fn expr_contains_raise(expr: &HirExpr) -> bool {
    match &expr.kind {
        HirExprKind::Raise(_) => true,
        HirExprKind::Call { callee, args, .. } => expr_contains_raise(callee) || args.iter().any(|arg| expr_contains_raise(&arg.value)),
        HirExprKind::Construct { args, .. } | HirExprKind::ArrayLiteral { items: args } => args.iter().any(expr_contains_raise),
        HirExprKind::ArrayNew { length, .. }
        | HirExprKind::FieldInit { value: length, .. }
        | HirExprKind::FieldAccess { object: length, .. }
        | HirExprKind::Await(length)
        | HirExprKind::Awake(length)
        | HirExprKind::BlockOn(length)
        | HirExprKind::Resume(length)
        | HirExprKind::TryPropagate(length)
        | HirExprKind::Assign { value: length, .. }
        | HirExprKind::GenericApply { callee: length, .. } => expr_contains_raise(length),
        HirExprKind::With { base, updates } => expr_contains_raise(base) || updates.iter().any(|(_, value)| expr_contains_raise(value)),
        HirExprKind::TryScope { body, .. } => block_contains_raise(body),
        HirExprKind::SuperCall { args, .. } => args.iter().any(expr_contains_raise),
        HirExprKind::StoreField { object, value, .. } => expr_contains_raise(object) || expr_contains_raise(value),
        HirExprKind::Block(block) | HirExprKind::Lambda { body: block, .. } => block_contains_raise(block),
        HirExprKind::AnonymousClass { fields, methods, .. } => {
            fields.iter().any(|(_, value)| expr_contains_raise(value)) || methods.iter().any(|method| block_contains_raise(&method.body))
        }
        HirExprKind::If { condition, then_branch, else_branch } | HirExprKind::IfLet { scrutinee: condition, then_branch, else_branch, .. } => {
            expr_contains_raise(condition)
                || block_contains_raise(then_branch)
                || else_branch.as_ref().is_some_and(|branch| block_contains_raise(branch))
        }
        HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
            expr_contains_raise(scrutinee)
                || arms.iter().any(|arm| arm.guard.as_ref().is_some_and(|guard| expr_contains_raise(guard)) || expr_contains_raise(&arm.body))
        }
        HirExprKind::Loop { iterator, condition, body, .. } => {
            iterator.as_ref().is_some_and(|expr| expr_contains_raise(expr))
                || condition.as_ref().is_some_and(|expr| expr_contains_raise(expr))
                || block_contains_raise(body)
        }
        HirExprKind::Catch { expr, arms } => {
            expr_contains_catch(expr)
                || arms.iter().any(|arm| arm.guard.as_ref().is_some_and(|guard| expr_contains_raise(guard)) || expr_contains_raise(&arm.body))
        }
        HirExprKind::Return(value) | HirExprKind::Break { expr: value, .. } | HirExprKind::Yield(value) => {
            value.as_ref().is_some_and(|expr| expr_contains_raise(expr))
        }
        _ => false,
    }
}

fn expr_contains_catch(expr: &HirExpr) -> bool {
    match &expr.kind {
        HirExprKind::Catch { .. } => true,
        HirExprKind::Call { callee, args, .. } => expr_contains_catch(callee) || args.iter().any(|arg| expr_contains_catch(&arg.value)),
        HirExprKind::Construct { args, .. } | HirExprKind::ArrayLiteral { items: args } => args.iter().any(expr_contains_catch),
        HirExprKind::ArrayNew { length, .. }
        | HirExprKind::FieldInit { value: length, .. }
        | HirExprKind::FieldAccess { object: length, .. }
        | HirExprKind::Await(length)
        | HirExprKind::Awake(length)
        | HirExprKind::BlockOn(length)
        | HirExprKind::Raise(length)
        | HirExprKind::Resume(length)
        | HirExprKind::TryPropagate(length)
        | HirExprKind::Assign { value: length, .. }
        | HirExprKind::GenericApply { callee: length, .. } => expr_contains_catch(length),
        HirExprKind::With { base, updates } => expr_contains_catch(base) || updates.iter().any(|(_, value)| expr_contains_catch(value)),
        HirExprKind::TryScope { body, .. } => block_contains_catch(body),
        HirExprKind::SuperCall { args, .. } => args.iter().any(expr_contains_catch),
        HirExprKind::StoreField { object, value, .. } => expr_contains_catch(object) || expr_contains_catch(value),
        HirExprKind::Block(block) | HirExprKind::Lambda { body: block, .. } => block_contains_catch(block),
        HirExprKind::AnonymousClass { fields, methods, .. } => {
            fields.iter().any(|(_, value)| expr_contains_catch(value)) || methods.iter().any(|method| block_contains_catch(&method.body))
        }
        HirExprKind::If { condition, then_branch, else_branch } | HirExprKind::IfLet { scrutinee: condition, then_branch, else_branch, .. } => {
            expr_contains_catch(condition)
                || block_contains_catch(then_branch)
                || else_branch.as_ref().is_some_and(|branch| block_contains_catch(branch))
        }
        HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
            expr_contains_catch(scrutinee)
                || arms.iter().any(|arm| arm.guard.as_ref().is_some_and(|guard| expr_contains_catch(guard)) || expr_contains_catch(&arm.body))
        }
        HirExprKind::Loop { iterator, condition, body, .. } => {
            iterator.as_ref().is_some_and(|expr| expr_contains_catch(expr))
                || condition.as_ref().is_some_and(|expr| expr_contains_catch(expr))
                || block_contains_catch(body)
        }
        HirExprKind::Catch { .. } => true,
        HirExprKind::Return(value) | HirExprKind::Break { expr: value, .. } | HirExprKind::Yield(value) => {
            value.as_ref().is_some_and(|expr| expr_contains_catch(expr))
        }
        _ => false,
    }
}

fn expr_contains_resume(expr: &HirExpr) -> bool {
    match &expr.kind {
        HirExprKind::Resume(_) => true,
        HirExprKind::Call { callee, args, .. } => expr_contains_resume(callee) || args.iter().any(|arg| expr_contains_resume(&arg.value)),
        HirExprKind::Construct { args, .. } | HirExprKind::ArrayLiteral { items: args } => args.iter().any(expr_contains_resume),
        HirExprKind::ArrayNew { length, .. }
        | HirExprKind::FieldInit { value: length, .. }
        | HirExprKind::FieldAccess { object: length, .. }
        | HirExprKind::Await(length)
        | HirExprKind::Awake(length)
        | HirExprKind::BlockOn(length)
        | HirExprKind::Raise(length)
        | HirExprKind::TryPropagate(length)
        | HirExprKind::Assign { value: length, .. }
        | HirExprKind::GenericApply { callee: length, .. } => expr_contains_resume(length),
        HirExprKind::With { base, updates } => expr_contains_resume(base) || updates.iter().any(|(_, value)| expr_contains_resume(value)),
        HirExprKind::TryScope { body, .. } => block_contains_resume(body),
        HirExprKind::SuperCall { args, .. } => args.iter().any(expr_contains_resume),
        HirExprKind::StoreField { object, value, .. } => expr_contains_resume(object) || expr_contains_resume(value),
        HirExprKind::Block(block) | HirExprKind::Lambda { body: block, .. } => block_contains_resume(block),
        HirExprKind::AnonymousClass { fields, methods, .. } => {
            fields.iter().any(|(_, value)| expr_contains_resume(value)) || methods.iter().any(|method| block_contains_resume(&method.body))
        }
        HirExprKind::If { condition, then_branch, else_branch } | HirExprKind::IfLet { scrutinee: condition, then_branch, else_branch, .. } => {
            expr_contains_resume(condition)
                || block_contains_resume(then_branch)
                || else_branch.as_ref().is_some_and(|branch| block_contains_resume(branch))
        }
        HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
            expr_contains_resume(scrutinee)
                || arms.iter().any(|arm| arm.guard.as_ref().is_some_and(|guard| expr_contains_resume(guard)) || expr_contains_resume(&arm.body))
        }
        HirExprKind::Loop { iterator, condition, body, .. } => {
            iterator.as_ref().is_some_and(|expr| expr_contains_resume(expr))
                || condition.as_ref().is_some_and(|expr| expr_contains_resume(expr))
                || block_contains_resume(body)
        }
        HirExprKind::Catch { expr, arms } => {
            expr_contains_resume(expr)
                || arms.iter().any(|arm| arm.guard.as_ref().is_some_and(|guard| expr_contains_resume(guard)) || expr_contains_resume(&arm.body))
        }
        HirExprKind::Return(value) | HirExprKind::Break { expr: value, .. } | HirExprKind::Yield(value) => {
            value.as_ref().is_some_and(|expr| expr_contains_resume(expr))
        }
        _ => false,
    }
}

fn statement_contains_await(statement: &HirStatement) -> bool {
    match &statement.kind {
        HirStatementKind::Let { initializer, .. } => initializer.as_ref().is_some_and(|expr| expr_contains_await(expr)),
        HirStatementKind::Expr(expr) => expr_contains_await(expr),
    }
}

fn statement_contains_block_on(statement: &HirStatement) -> bool {
    match &statement.kind {
        HirStatementKind::Let { initializer, .. } => initializer.as_ref().is_some_and(|expr| expr_contains_block_on(expr)),
        HirStatementKind::Expr(expr) => expr_contains_block_on(expr),
    }
}

fn statement_contains_awake(statement: &HirStatement) -> bool {
    match &statement.kind {
        HirStatementKind::Let { initializer, .. } => initializer.as_ref().is_some_and(|expr| expr_contains_awake(expr)),
        HirStatementKind::Expr(expr) => expr_contains_awake(expr),
    }
}

fn expr_contains_await(expr: &HirExpr) -> bool {
    match &expr.kind {
        HirExprKind::Await(_) => true,
        HirExprKind::Lambda { body, .. } => block_contains_await(body),
        HirExprKind::Call { callee, args, .. } => expr_contains_await(callee) || args.iter().any(|arg| expr_contains_await(&arg.value)),
        HirExprKind::Construct { args, .. } | HirExprKind::ArrayLiteral { items: args } => args.iter().any(expr_contains_await),
        HirExprKind::ArrayNew { length, .. }
        | HirExprKind::FieldInit { value: length, .. }
        | HirExprKind::FieldAccess { object: length, .. }
        | HirExprKind::Awake(length)
        | HirExprKind::BlockOn(length)
        | HirExprKind::Yield(Some(length))
        | HirExprKind::YieldFrom(length)
        | HirExprKind::Raise(length)
        | HirExprKind::Resume(length)
        | HirExprKind::TryPropagate(length)
        | HirExprKind::Assign { value: length, .. }
        | HirExprKind::GenericApply { callee: length, .. } => expr_contains_await(length),
        HirExprKind::Yield(None)
        | HirExprKind::Fallthrough
        | HirExprKind::Literal(_)
        | HirExprKind::Variable(_)
        | HirExprKind::Path(_)
        | HirExprKind::Continue { .. } => false,
        HirExprKind::With { base, updates } => expr_contains_await(base) || updates.iter().any(|(_, value)| expr_contains_await(value)),
        HirExprKind::TryScope { body, .. } => block_contains_await(body),
        HirExprKind::SuperCall { args, .. } => args.iter().any(expr_contains_await),
        HirExprKind::StoreField { object, value, .. } => expr_contains_await(object) || expr_contains_await(value),
        HirExprKind::Block(block) => block_contains_await(block),
        HirExprKind::AnonymousClass { fields, methods, .. } => {
            fields.iter().any(|(_, value)| expr_contains_await(value))
                || methods.iter().any(|method| function_body_contains_await(&method.body))
        }
        HirExprKind::If { condition, then_branch, else_branch } | HirExprKind::IfLet { scrutinee: condition, then_branch, else_branch, .. } => {
            expr_contains_await(condition)
                || block_contains_await(then_branch)
                || else_branch.as_ref().is_some_and(|branch| block_contains_await(branch))
        }
        HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
            expr_contains_await(scrutinee)
                || arms.iter().any(|arm| arm.guard.as_ref().is_some_and(|guard| expr_contains_await(guard)) || expr_contains_await(&arm.body))
        }
        HirExprKind::Loop { iterator, condition, body, .. } => {
            iterator.as_ref().is_some_and(|expr| expr_contains_await(expr))
                || condition.as_ref().is_some_and(|expr| expr_contains_await(expr))
                || block_contains_await(body)
        }
        HirExprKind::Catch { expr, arms } => {
            expr_contains_await(expr)
                || arms.iter().any(|arm| arm.guard.as_ref().is_some_and(|guard| expr_contains_await(guard)) || expr_contains_await(&arm.body))
        }
        HirExprKind::Return(value) | HirExprKind::Break { expr: value, .. } => value.as_ref().is_some_and(|expr| expr_contains_await(expr)),
    }
}

fn expr_contains_block_on(expr: &HirExpr) -> bool {
    match &expr.kind {
        HirExprKind::BlockOn(_) => true,
        HirExprKind::Lambda { body, .. } => block_contains_block_on(body),
        HirExprKind::Call { callee, args, .. } => expr_contains_block_on(callee) || args.iter().any(|arg| expr_contains_block_on(&arg.value)),
        HirExprKind::Construct { args, .. } | HirExprKind::ArrayLiteral { items: args } => args.iter().any(expr_contains_block_on),
        HirExprKind::ArrayNew { length, .. }
        | HirExprKind::FieldInit { value: length, .. }
        | HirExprKind::FieldAccess { object: length, .. }
        | HirExprKind::Await(length)
        | HirExprKind::Awake(length)
        | HirExprKind::Yield(Some(length))
        | HirExprKind::YieldFrom(length)
        | HirExprKind::Raise(length)
        | HirExprKind::Resume(length)
        | HirExprKind::TryPropagate(length)
        | HirExprKind::Assign { value: length, .. }
        | HirExprKind::GenericApply { callee: length, .. } => expr_contains_block_on(length),
        HirExprKind::Yield(None)
        | HirExprKind::Fallthrough
        | HirExprKind::Literal(_)
        | HirExprKind::Variable(_)
        | HirExprKind::Path(_)
        | HirExprKind::Continue { .. } => false,
        HirExprKind::With { base, updates } => expr_contains_block_on(base) || updates.iter().any(|(_, value)| expr_contains_block_on(value)),
        HirExprKind::TryScope { body, .. } => block_contains_block_on(body),
        HirExprKind::SuperCall { args, .. } => args.iter().any(expr_contains_block_on),
        HirExprKind::StoreField { object, value, .. } => expr_contains_block_on(object) || expr_contains_block_on(value),
        HirExprKind::Block(block) => block_contains_block_on(block),
        HirExprKind::AnonymousClass { fields, methods, .. } => {
            fields.iter().any(|(_, value)| expr_contains_block_on(value))
                || methods.iter().any(|method| function_body_contains_block_on(&method.body))
        }
        HirExprKind::If { condition, then_branch, else_branch } | HirExprKind::IfLet { scrutinee: condition, then_branch, else_branch, .. } => {
            expr_contains_block_on(condition)
                || block_contains_block_on(then_branch)
                || else_branch.as_ref().is_some_and(|branch| block_contains_block_on(branch))
        }
        HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
            expr_contains_block_on(scrutinee)
                || arms
                    .iter()
                    .any(|arm| arm.guard.as_ref().is_some_and(|guard| expr_contains_block_on(guard)) || expr_contains_block_on(&arm.body))
        }
        HirExprKind::Loop { iterator, condition, body, .. } => {
            iterator.as_ref().is_some_and(|expr| expr_contains_block_on(expr))
                || condition.as_ref().is_some_and(|expr| expr_contains_block_on(expr))
                || block_contains_block_on(body)
        }
        HirExprKind::Catch { expr, arms } => {
            expr_contains_block_on(expr)
                || arms
                    .iter()
                    .any(|arm| arm.guard.as_ref().is_some_and(|guard| expr_contains_block_on(guard)) || expr_contains_block_on(&arm.body))
        }
        HirExprKind::Return(value) | HirExprKind::Break { expr: value, .. } => value.as_ref().is_some_and(|expr| expr_contains_block_on(expr)),
    }
}

fn expr_contains_awake(expr: &HirExpr) -> bool {
    match &expr.kind {
        HirExprKind::Awake(_) => true,
        HirExprKind::Lambda { body, .. } => block_contains_awake(body),
        HirExprKind::Call { callee, args, .. } => expr_contains_awake(callee) || args.iter().any(|arg| expr_contains_awake(&arg.value)),
        HirExprKind::Construct { args, .. } | HirExprKind::ArrayLiteral { items: args } => args.iter().any(expr_contains_awake),
        HirExprKind::ArrayNew { length, .. }
        | HirExprKind::FieldInit { value: length, .. }
        | HirExprKind::FieldAccess { object: length, .. }
        | HirExprKind::Await(length)
        | HirExprKind::BlockOn(length)
        | HirExprKind::Yield(Some(length))
        | HirExprKind::YieldFrom(length)
        | HirExprKind::Raise(length)
        | HirExprKind::Resume(length)
        | HirExprKind::TryPropagate(length)
        | HirExprKind::Assign { value: length, .. }
        | HirExprKind::GenericApply { callee: length, .. } => expr_contains_awake(length),
        HirExprKind::Yield(None)
        | HirExprKind::Fallthrough
        | HirExprKind::Literal(_)
        | HirExprKind::Variable(_)
        | HirExprKind::Path(_)
        | HirExprKind::Continue { .. } => false,
        HirExprKind::With { base, updates } => expr_contains_awake(base) || updates.iter().any(|(_, value)| expr_contains_awake(value)),
        HirExprKind::TryScope { body, .. } => block_contains_awake(body),
        HirExprKind::SuperCall { args, .. } => args.iter().any(expr_contains_awake),
        HirExprKind::StoreField { object, value, .. } => expr_contains_awake(object) || expr_contains_awake(value),
        HirExprKind::Block(block) => block_contains_awake(block),
        HirExprKind::AnonymousClass { fields, methods, .. } => {
            fields.iter().any(|(_, value)| expr_contains_awake(value)) || methods.iter().any(|method| block_contains_awake(&method.body))
        }
        HirExprKind::If { condition, then_branch, else_branch } | HirExprKind::IfLet { scrutinee: condition, then_branch, else_branch, .. } => {
            expr_contains_awake(condition)
                || block_contains_awake(then_branch)
                || else_branch.as_ref().is_some_and(|branch| block_contains_awake(branch))
        }
        HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
            expr_contains_awake(scrutinee)
                || arms.iter().any(|arm| arm.guard.as_ref().is_some_and(|guard| expr_contains_awake(guard)) || expr_contains_awake(&arm.body))
        }
        HirExprKind::Loop { iterator, condition, body, .. } => {
            iterator.as_ref().is_some_and(|expr| expr_contains_awake(expr))
                || condition.as_ref().is_some_and(|expr| expr_contains_awake(expr))
                || block_contains_awake(body)
        }
        HirExprKind::Catch { expr, arms } => {
            expr_contains_awake(expr)
                || arms.iter().any(|arm| arm.guard.as_ref().is_some_and(|guard| expr_contains_awake(guard)) || expr_contains_awake(&arm.body))
        }
        HirExprKind::Return(value) | HirExprKind::Break { expr: value, .. } => value.as_ref().is_some_and(|expr| expr_contains_awake(expr)),
    }
}

fn push_local_scope(state: &mut HirValidationState) {
    state.local_scopes.push(BTreeMap::new());
}

fn pop_local_scope(state: &mut HirValidationState) {
    let _ = state.local_scopes.pop();
}

fn push_return_type(state: &mut HirValidationState, ty: ValkyrieType) {
    state.return_type_stack.push(ty);
}

fn pop_return_type(state: &mut HirValidationState) {
    let _ = state.return_type_stack.pop();
}

fn current_return_type(state: &HirValidationState) -> Option<&ValkyrieType> {
    state.return_type_stack.last()
}

fn push_self_type(state: &mut HirValidationState, ty: ValkyrieType) {
    state.self_type_stack.push(ty);
}

fn pop_self_type(state: &mut HirValidationState) {
    let _ = state.self_type_stack.pop();
}

fn current_self_type(state: &HirValidationState) -> Option<&ValkyrieType> {
    state.self_type_stack.last()
}

fn bind_local_type(state: &mut HirValidationState, name: &str, ty: ValkyrieType) {
    if let Some(scope) = state.local_scopes.last_mut() {
        scope.insert(name.to_string(), ty);
    }
}

fn bind_pattern_type(state: &mut HirValidationState, pattern: &HirPattern, ty: &ValkyrieType) {
    bind_pattern_type_with_hint(state, pattern, Some(ty));
}

fn validate_pattern_semantics(pattern: &HirPattern) -> Result<(), ParseError> {
    match pattern {
        HirPattern::Name(name) => {
            if is_ambiguous_bare_name_pattern(name) {
                return Err(ParseError::invalid(format!(
                    "控制流调度校验失败：当前禁止会与变量绑定歧义的单段小写裸名字模式 `{}`；`Variant` 或 `foo::bar::value` 这类不歧义的 variant 形式仍然允许",
                    render_pattern_name(name)
                )));
            }
        }
        HirPattern::Tuple(items) | HirPattern::Or(items) => {
            for item in items {
                validate_pattern_semantics(item)?;
            }
        }
        HirPattern::Extractor(extractor) => match extractor {
            crate::types::hir::HirExtractorPattern::Array { prefix, suffix, .. } => {
                for item in prefix {
                    validate_pattern_semantics(item)?;
                }
                for item in suffix {
                    validate_pattern_semantics(item)?;
                }
            }
            crate::types::hir::HirExtractorPattern::Constructor { fields, .. } => {
                for field in fields {
                    validate_pattern_semantics(field)?;
                }
            }
        },
        HirPattern::Object { fields, .. } => {
            for (_, field_pattern) in fields {
                validate_pattern_semantics(field_pattern)?;
            }
        }
        HirPattern::Bind { pattern, .. } | HirPattern::Mut(pattern) | HirPattern::Pin { pattern, .. } => {
            validate_pattern_semantics(pattern)?;
        }
        _ => {}
    }
    Ok(())
}

fn is_ambiguous_bare_name_pattern(name: &NamePath) -> bool {
    name.parts().len() == 1
        && name.parts().first().and_then(|identifier| identifier.as_str().chars().next()).is_some_and(|ch| ch.is_lowercase())
}

fn render_pattern_name(name: &NamePath) -> String {
    name.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>().join("::")
}

fn bind_pattern_type_with_hint(state: &mut HirValidationState, pattern: &HirPattern, ty: Option<&ValkyrieType>) {
    match pattern {
        HirPattern::Variable(identifier) => {
            bind_local_type(state, identifier.name.as_str(), ty.cloned().unwrap_or(ValkyrieType::AutoType));
        }
        HirPattern::TypedBind { identifier, ty: pattern_type } => {
            let explicit = pattern_type.parts().last().cloned().map(ValkyrieType::Named);
            bind_local_type(state, identifier.name.as_str(), explicit.or_else(|| ty.cloned()).unwrap_or(ValkyrieType::AutoType));
        }
        HirPattern::Tuple(items) => {
            if let Some(ValkyrieType::Tuple(types)) = ty {
                for (item, item_type) in items.iter().zip(types.iter()) {
                    bind_pattern_type_with_hint(state, item, Some(item_type));
                }
            }
            for item in items.iter().skip(match ty {
                Some(ValkyrieType::Tuple(types)) => types.len(),
                _ => 0,
            }) {
                bind_pattern_type_with_hint(state, item, None);
            }
        }
        HirPattern::Extractor(extractor) => match extractor {
            crate::types::hir::HirExtractorPattern::Array { prefix, rest, suffix, .. } => {
                let item_hint = match ty {
                    Some(ValkyrieType::Array(item)) => Some(item.as_ref()),
                    _ => None,
                };
                for item in prefix {
                    bind_pattern_type_with_hint(state, item, item_hint);
                }
                if let Some(rest) = rest {
                    bind_local_type(
                        state,
                        rest.name.as_str(),
                        item_hint.cloned().map(|item| ValkyrieType::Array(Box::new(item))).unwrap_or(ValkyrieType::AutoType),
                    );
                }
                for item in suffix {
                    bind_pattern_type_with_hint(state, item, item_hint);
                }
            }
            crate::types::hir::HirExtractorPattern::Constructor { fields, .. } => {
                for field in fields {
                    bind_pattern_type_with_hint(state, field, None);
                }
            }
        },
        HirPattern::Object { fields, rest, .. } => {
            for (_, field_pattern) in fields {
                bind_pattern_type_with_hint(state, field_pattern, None);
            }
            if let Some(rest) = rest {
                bind_local_type(state, rest.name.as_str(), ty.cloned().unwrap_or(ValkyrieType::AutoType));
            }
        }
        HirPattern::Or(patterns) => {
            for pattern in patterns {
                bind_pattern_type_with_hint(state, pattern, ty);
            }
        }
        HirPattern::Bind { identifier, pattern } => {
            bind_pattern_type_with_hint(state, pattern, ty);
            bind_local_type(state, identifier.name.as_str(), ty.cloned().unwrap_or(ValkyrieType::AutoType));
        }
        HirPattern::Mut(pattern) | HirPattern::Pin { pattern, .. } => {
            bind_pattern_type_with_hint(state, pattern, ty);
        }
        _ => {}
    }
}

fn collect_pattern_bound_names(pattern: &HirPattern) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    collect_pattern_bound_names_into(pattern, &mut names);
    names
}

fn collect_pattern_bound_names_into(pattern: &HirPattern, names: &mut BTreeSet<String>) {
    match pattern {
        HirPattern::Variable(identifier) => {
            names.insert(identifier.name.to_string());
        }
        HirPattern::TypedBind { identifier, .. } => {
            names.insert(identifier.name.to_string());
        }
        HirPattern::Tuple(items) => {
            for item in items {
                collect_pattern_bound_names_into(item, names);
            }
        }
        HirPattern::Extractor(extractor) => match extractor {
            crate::types::hir::HirExtractorPattern::Array { prefix, rest, suffix, .. } => {
                for item in prefix {
                    collect_pattern_bound_names_into(item, names);
                }
                if let Some(rest) = rest {
                    names.insert(rest.name.to_string());
                }
                for item in suffix {
                    collect_pattern_bound_names_into(item, names);
                }
            }
            crate::types::hir::HirExtractorPattern::Constructor { fields, .. } => {
                for field in fields {
                    collect_pattern_bound_names_into(field, names);
                }
            }
        },
        HirPattern::Object { fields, rest, .. } => {
            for (_, field_pattern) in fields {
                collect_pattern_bound_names_into(field_pattern, names);
            }
            if let Some(rest) = rest {
                names.insert(rest.name.to_string());
            }
        }
        HirPattern::Or(patterns) => {
            for pattern in patterns {
                collect_pattern_bound_names_into(pattern, names);
            }
        }
        HirPattern::Bind { identifier, pattern } => {
            names.insert(identifier.name.to_string());
            collect_pattern_bound_names_into(pattern, names);
        }
        HirPattern::Mut(pattern) | HirPattern::Pin { pattern, .. } => {
            collect_pattern_bound_names_into(pattern, names);
        }
        _ => {}
    }
}

fn detect_case_fallthrough_binding_leak(
    arm: &HirMatchArm,
    previous_fallthrough_bindings: &BTreeSet<String>,
    state: &HirValidationState,
) -> Option<String> {
    if previous_fallthrough_bindings.is_empty() {
        return None;
    }

    let current_bindings = collect_pattern_bound_names(&arm.pattern);
    let mut referenced = BTreeSet::new();
    if let Some(guard) = &arm.guard {
        collect_expr_variable_names(guard, &mut referenced);
    }
    collect_expr_variable_names(&arm.body, &mut referenced);

    referenced.into_iter().find(|name| {
        previous_fallthrough_bindings.contains(name) && !current_bindings.contains(name) && lookup_local_type(state, name).is_none()
    })
}

fn collect_expr_variable_names(expr: &HirExpr, names: &mut BTreeSet<String>) {
    match &expr.kind {
        HirExprKind::Variable(identifier) => {
            names.insert(identifier.name.to_string());
        }
        HirExprKind::Call { callee, args, .. } => {
            collect_expr_variable_names(callee, names);
            for arg in args {
                collect_expr_variable_names(&arg.value, names);
            }
        }
        HirExprKind::Construct { args, .. } | HirExprKind::ArrayLiteral { items: args } => {
            for arg in args {
                collect_expr_variable_names(arg, names);
            }
        }
        HirExprKind::ArrayNew { length, .. }
        | HirExprKind::FieldInit { value: length, .. }
        | HirExprKind::FieldAccess { object: length, .. }
        | HirExprKind::Await(length)
        | HirExprKind::Awake(length)
        | HirExprKind::BlockOn(length)
        | HirExprKind::Yield(Some(length))
        | HirExprKind::YieldFrom(length)
        | HirExprKind::Raise(length)
        | HirExprKind::Resume(length)
        | HirExprKind::TryPropagate(length)
        | HirExprKind::Assign { value: length, .. }
        | HirExprKind::GenericApply { callee: length, .. } => collect_expr_variable_names(length, names),
        HirExprKind::Yield(None) | HirExprKind::Fallthrough | HirExprKind::Literal(_) | HirExprKind::Path(_) | HirExprKind::Continue { .. } => {
        }
        HirExprKind::With { base, updates } => {
            collect_expr_variable_names(base, names);
            for (_, value) in updates {
                collect_expr_variable_names(value, names);
            }
        }
        HirExprKind::TryScope { body, .. } => {
            for statement in &body.statements {
                collect_statement_variable_names(statement, names);
            }
            if let Some(expr) = &body.expr {
                collect_expr_variable_names(expr, names);
            }
        }
        HirExprKind::SuperCall { args, .. } => {
            for arg in args {
                collect_expr_variable_names(arg, names);
            }
        }
        HirExprKind::StoreField { object, value, .. } => {
            collect_expr_variable_names(object, names);
            collect_expr_variable_names(value, names);
        }
        HirExprKind::Block(block) => {
            for statement in &block.statements {
                collect_statement_variable_names(statement, names);
            }
            if let Some(expr) = &block.expr {
                collect_expr_variable_names(expr, names);
            }
        }
        HirExprKind::Lambda { body, .. } => {
            for statement in &body.statements {
                collect_statement_variable_names(statement, names);
            }
            if let Some(expr) = &body.expr {
                collect_expr_variable_names(expr, names);
            }
        }
        HirExprKind::AnonymousClass { fields, methods, .. } => {
            for (_, value) in fields {
                collect_expr_variable_names(value, names);
            }
            for method in methods {
                for statement in &method.body.statements {
                    collect_statement_variable_names(statement, names);
                }
                if let Some(expr) = &method.body.expr {
                    collect_expr_variable_names(expr, names);
                }
            }
        }
        HirExprKind::If { condition, then_branch, else_branch } | HirExprKind::IfLet { scrutinee: condition, then_branch, else_branch, .. } => {
            collect_expr_variable_names(condition, names);
            for statement in &then_branch.statements {
                collect_statement_variable_names(statement, names);
            }
            if let Some(expr) = &then_branch.expr {
                collect_expr_variable_names(expr, names);
            }
            if let Some(else_branch) = else_branch {
                for statement in &else_branch.statements {
                    collect_statement_variable_names(statement, names);
                }
                if let Some(expr) = &else_branch.expr {
                    collect_expr_variable_names(expr, names);
                }
            }
        }
        HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
            collect_expr_variable_names(scrutinee, names);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    collect_expr_variable_names(guard, names);
                }
                collect_expr_variable_names(&arm.body, names);
            }
        }
        HirExprKind::Loop { iterator, condition, body, .. } => {
            if let Some(iterator) = iterator {
                collect_expr_variable_names(iterator, names);
            }
            if let Some(condition) = condition {
                collect_expr_variable_names(condition, names);
            }
            for statement in &body.statements {
                collect_statement_variable_names(statement, names);
            }
            if let Some(expr) = &body.expr {
                collect_expr_variable_names(expr, names);
            }
        }
        HirExprKind::Return(Some(value)) | HirExprKind::Break { expr: Some(value), .. } => collect_expr_variable_names(value, names),
        HirExprKind::Return(None) | HirExprKind::Break { expr: None, .. } | HirExprKind::Catch { .. } => {}
    }
}

fn collect_statement_variable_names(statement: &HirStatement, names: &mut BTreeSet<String>) {
    match &statement.kind {
        HirStatementKind::Let { initializer, .. } => {
            if let Some(initializer) = initializer {
                collect_expr_variable_names(initializer, names);
            }
        }
        HirStatementKind::Expr(expr) => collect_expr_variable_names(expr, names),
    }
}

fn expr_contains_fallthrough(expr: &HirExpr) -> bool {
    match &expr.kind {
        HirExprKind::Fallthrough => true,
        HirExprKind::Call { callee, args, .. } => {
            expr_contains_fallthrough(callee) || args.iter().any(|arg| expr_contains_fallthrough(&arg.value))
        }
        HirExprKind::Construct { args, .. } | HirExprKind::ArrayLiteral { items: args } => args.iter().any(expr_contains_fallthrough),
        HirExprKind::ArrayNew { length, .. }
        | HirExprKind::FieldInit { value: length, .. }
        | HirExprKind::FieldAccess { object: length, .. }
        | HirExprKind::Await(length)
        | HirExprKind::Awake(length)
        | HirExprKind::BlockOn(length)
        | HirExprKind::Yield(Some(length))
        | HirExprKind::YieldFrom(length)
        | HirExprKind::Raise(length)
        | HirExprKind::Resume(length)
        | HirExprKind::TryPropagate(length)
        | HirExprKind::Assign { value: length, .. }
        | HirExprKind::GenericApply { callee: length, .. } => expr_contains_fallthrough(length),
        HirExprKind::Yield(None) | HirExprKind::Literal(_) | HirExprKind::Variable(_) | HirExprKind::Path(_) | HirExprKind::Continue { .. } => {
            false
        }
        HirExprKind::With { base, updates } => {
            expr_contains_fallthrough(base) || updates.iter().any(|(_, value)| expr_contains_fallthrough(value))
        }
        HirExprKind::TryScope { body, .. } => {
            body.statements.iter().any(statement_contains_fallthrough) || body.expr.as_ref().is_some_and(|expr| expr_contains_fallthrough(expr))
        }
        HirExprKind::SuperCall { args, .. } => args.iter().any(expr_contains_fallthrough),
        HirExprKind::StoreField { object, value, .. } => expr_contains_fallthrough(object) || expr_contains_fallthrough(value),
        HirExprKind::Block(block) => {
            block.statements.iter().any(statement_contains_fallthrough)
                || block.expr.as_ref().is_some_and(|expr| expr_contains_fallthrough(expr))
        }
        HirExprKind::Lambda { body, .. } => {
            body.statements.iter().any(statement_contains_fallthrough) || body.expr.as_ref().is_some_and(|expr| expr_contains_fallthrough(expr))
        }
        HirExprKind::AnonymousClass { fields, methods, .. } => {
            fields.iter().any(|(_, value)| expr_contains_fallthrough(value))
                || methods.iter().any(|method| {
                    method.body.statements.iter().any(statement_contains_fallthrough)
                        || method.body.expr.as_ref().is_some_and(|expr| expr_contains_fallthrough(expr))
                })
        }
        HirExprKind::If { condition, then_branch, else_branch } | HirExprKind::IfLet { scrutinee: condition, then_branch, else_branch, .. } => {
            expr_contains_fallthrough(condition)
                || then_branch.statements.iter().any(statement_contains_fallthrough)
                || then_branch.expr.as_ref().is_some_and(|expr| expr_contains_fallthrough(expr))
                || else_branch.as_ref().is_some_and(|branch| {
                    branch.statements.iter().any(statement_contains_fallthrough)
                        || branch.expr.as_ref().is_some_and(|expr| expr_contains_fallthrough(expr))
                })
        }
        HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
            expr_contains_fallthrough(scrutinee)
                || arms
                    .iter()
                    .any(|arm| arm.guard.as_ref().is_some_and(|guard| expr_contains_fallthrough(guard)) || expr_contains_fallthrough(&arm.body))
        }
        HirExprKind::Loop { iterator, condition, body, .. } => {
            iterator.as_ref().is_some_and(|expr| expr_contains_fallthrough(expr))
                || condition.as_ref().is_some_and(|expr| expr_contains_fallthrough(expr))
                || body.statements.iter().any(statement_contains_fallthrough)
                || body.expr.as_ref().is_some_and(|expr| expr_contains_fallthrough(expr))
        }
        HirExprKind::Return(Some(value)) | HirExprKind::Break { expr: Some(value), .. } => expr_contains_fallthrough(value),
        HirExprKind::Return(None) | HirExprKind::Break { expr: None, .. } | HirExprKind::Catch { .. } => false,
    }
}

fn statement_contains_fallthrough(statement: &HirStatement) -> bool {
    match &statement.kind {
        HirStatementKind::Let { initializer, .. } => initializer.as_ref().is_some_and(|expr| expr_contains_fallthrough(expr)),
        HirStatementKind::Expr(expr) => expr_contains_fallthrough(expr),
    }
}

fn lookup_local_type(state: &HirValidationState, name: &str) -> Option<ValkyrieType> {
    state.local_scopes.iter().rev().find_map(|scope| scope.get(name).cloned())
}

fn control_flow_value_type_compatible(expected: &ValkyrieType, actual: &ValkyrieType) -> bool {
    let expected = normalize_control_flow_type(expected);
    let actual = normalize_control_flow_type(actual);
    expected == actual
        || matches!(expected, ValkyrieType::AutoType)
        || matches!(actual, ValkyrieType::AutoType)
        // 整数字面量默认同为 i64；在 return / break 处允许与目标整数宽度互转（自举阶段宽化检测）。
        || (is_integer_type(&expected) && is_integer_type(&actual))
        // 自举阶段：未回填的 `Self` 与具体返回值暂不硬拦（trait 默认方法等无 self 上下文）。
        || is_unresolved_self_type(&expected)
        || is_unresolved_self_type(&actual)
        || array_types_compatible(&expected, &actual)
        || type_param_names_equal(&expected, &actual)
        || nullable_return_compatible(&expected, &actual)
        || generic_apply_types_compatible(&expected, &actual)
        || bare_named_matches_generic_apply(&expected, &actual)
        || collection_facade_compatible(&expected, &actual)
        || (is_uninstantiated_type_param(&actual) && !is_uninstantiated_type_param(&expected) && !matches!(expected, ValkyrieType::AutoType))
}

fn collection_facade_compatible(expected: &ValkyrieType, actual: &ValkyrieType) -> bool {
    let (ValkyrieType::Apply(exp_base, exp_args), ValkyrieType::Apply(act_base, act_args)) = (expected, actual)
    else {
        return false;
    };
    if exp_args.len() != act_args.len() {
        return false;
    }
    if !exp_args.iter().zip(act_args.iter()).all(|(expected_arg, actual_arg)| {
        expected_arg == actual_arg
            || (is_uninstantiated_type_param(expected_arg) && is_uninstantiated_type_param(actual_arg))
            || control_flow_value_type_compatible(expected_arg, actual_arg)
    }) {
        return false;
    }
    let Some(exp_name) = named_type_name(exp_base)
    else {
        return false;
    };
    let Some(act_name) = named_type_name(act_base)
    else {
        return false;
    };
    exp_name == "List" && matches!(act_name, "ArrayList" | "LinkedList")
}

fn bare_named_matches_generic_apply(expected: &ValkyrieType, actual: &ValkyrieType) -> bool {
    match (expected, actual) {
        (ValkyrieType::Apply(base, _), ValkyrieType::Named(name)) => named_type_name(base).is_some_and(|base_name| base_name == name.as_str()),
        _ => false,
    }
}

fn generic_apply_types_compatible(left: &ValkyrieType, right: &ValkyrieType) -> bool {
    generic_apply_type_matches(left, right) || generic_apply_type_matches(right, left)
}

fn generic_apply_type_matches(actual: &ValkyrieType, receiver: &ValkyrieType) -> bool {
    let (ValkyrieType::Apply(actual_base, actual_args), ValkyrieType::Apply(receiver_base, receiver_args)) = (actual, receiver)
    else {
        return false;
    };
    if actual_args.len() != receiver_args.len() {
        return false;
    }
    if actual_base != receiver_base {
        return false;
    }
    receiver_args.iter().zip(actual_args.iter()).all(|(expected, actual_arg)| match expected {
        ValkyrieType::Named(_) => true,
        other => *other == *actual_arg,
    })
}

fn nullable_return_compatible(expected: &ValkyrieType, actual: &ValkyrieType) -> bool {
    match (nullable_payload_type(expected), nullable_payload_type(actual)) {
        (Some(lhs), Some(rhs)) => lhs == rhs,
        (Some(lhs), None) => lhs == *actual,
        (None, Some(rhs)) => *expected == rhs,
        _ => false,
    }
}

fn normalize_control_flow_type(ty: &ValkyrieType) -> ValkyrieType {
    match ty {
        ValkyrieType::Named(name) if name.as_str() == "bool" => ValkyrieType::Boolean,
        ValkyrieType::Named(name) if name.as_str() == "Self" => ValkyrieType::SelfType,
        // `Array<T>` 与语法糖 `[T]` 统一为 Array 形态，便于 return 校验。
        ValkyrieType::Apply(base, arguments) if arguments.len() == 1 && matches!(named_type_name(base), Some("Array")) => {
            ValkyrieType::Array(Box::new(normalize_control_flow_type(&arguments[0])))
        }
        ValkyrieType::Array(item) => ValkyrieType::Array(Box::new(normalize_control_flow_type(item))),
        ValkyrieType::FixedArray { element, length } => {
            ValkyrieType::FixedArray { element: Box::new(normalize_control_flow_type(element)), length: *length }
        }
        other => other.clone(),
    }
}

fn is_uninstantiated_type_param(ty: &ValkyrieType) -> bool {
    matches!(ty, ValkyrieType::Named(name) if {
        let text = name.as_str();
        text.len() == 1 && text.chars().next().is_some_and(|ch| ch.is_ascii_uppercase())
    })
}

fn text_like_types_compatible(left: &ValkyrieType, right: &ValkyrieType) -> bool {
    is_text_like_type(left) && is_text_like_type(right)
}

fn is_text_like_type(ty: &ValkyrieType) -> bool {
    matches!(ty, ValkyrieType::Utf8 | ValkyrieType::Utf16)
        || matches!(ty, ValkyrieType::Named(name) if matches!(name.as_str(), "utf8" | "Utf8Text" | "utf16" | "Utf16Text"))
}

fn array_types_compatible(expected: &ValkyrieType, actual: &ValkyrieType) -> bool {
    match (array_element_type(expected), array_element_type(actual)) {
        (Some(a), Some(b)) => {
            let a = normalize_control_flow_type(a);
            let b = normalize_control_flow_type(b);
            a == b
                || matches!(a, ValkyrieType::AutoType)
                || matches!(b, ValkyrieType::AutoType)
                || is_unresolved_self_type(&a)
                || is_unresolved_self_type(&b)
                || type_param_names_equal(&a, &b)
                || generic_apply_types_compatible(&a, &b)
                || text_like_types_compatible(&a, &b)
                || is_uninstantiated_type_param(&a)
                || is_uninstantiated_type_param(&b)
        }
        _ => false,
    }
}

fn array_element_type(ty: &ValkyrieType) -> Option<&ValkyrieType> {
    match ty {
        ValkyrieType::Array(item) => Some(item.as_ref()),
        ValkyrieType::Apply(base, arguments) if arguments.len() == 1 && matches!(named_type_name(base), Some("Array")) => arguments.first(),
        _ => None,
    }
}

fn type_param_name(ty: &ValkyrieType) -> Option<&str> {
    match ty {
        ValkyrieType::Generic(generic) => Some(generic.name.as_str()),
        ValkyrieType::Named(name) => Some(name.as_str()),
        _ => None,
    }
}

fn type_param_names_equal(left: &ValkyrieType, right: &ValkyrieType) -> bool {
    let left_generic = matches!(left, ValkyrieType::Generic(_));
    let right_generic = matches!(right, ValkyrieType::Generic(_));
    if !left_generic && !right_generic {
        return false;
    }
    match (type_param_name(left), type_param_name(right)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

fn is_unresolved_self_type(ty: &ValkyrieType) -> bool {
    matches!(ty, ValkyrieType::SelfType)
}

fn is_integer_type(ty: &ValkyrieType) -> bool {
    match ty {
        ValkyrieType::Integer8 { .. }
        | ValkyrieType::Integer16 { .. }
        | ValkyrieType::Integer32 { .. }
        | ValkyrieType::Integer64 { .. }
        | ValkyrieType::Integer128 { .. } => true,
        ValkyrieType::Named(name) => matches!(name.as_str(), "usize" | "isize" | "byte" | "sbyte"),
        _ => false,
    }
}

fn return_without_value_compatible(expected: &ValkyrieType) -> bool {
    matches!(expected, ValkyrieType::Unit | ValkyrieType::Void | ValkyrieType::AutoType)
}

fn future_resume_type(ty: &ValkyrieType) -> Option<ValkyrieType> {
    match ty {
        ValkyrieType::Apply(base, arguments) if arguments.len() == 1 && matches!(named_type_name(base), Some("Future" | "Promise")) => {
            arguments.first().cloned()
        }
        _ => None,
    }
}

/// 从 `Generator<T>` / `Iterator<T>` / `Coroutine<T>` 提取 yield 元素类型 `T`。
///
/// witness table 机制不区分 dyn trait 与 impl trait，因此函数返回值可直接写
/// `Iterator<T>` 或 `Generator<T>`，本函数统一提取其类型参数作为 yield 值的期望类型。
fn generator_yield_type(ty: &ValkyrieType) -> Option<ValkyrieType> {
    match ty {
        ValkyrieType::Apply(base, arguments)
            if !arguments.is_empty() && matches!(named_type_name(base), Some("Generator" | "Iterator" | "Coroutine")) =>
        {
            arguments.first().cloned()
        }
        ValkyrieType::Named(name) if matches!(name.as_str(), "Generator" | "Iterator" | "Coroutine") => Some(ValkyrieType::Unit),
        _ => None,
    }
}

fn named_type_name(ty: &ValkyrieType) -> Option<&str> {
    match ty {
        ValkyrieType::Named(name) => Some(name.as_str()),
        ValkyrieType::Apply(base, _) => named_type_name(base),
        _ => None,
    }
}

fn display_type(ty: &ValkyrieType) -> String {
    match ty {
        ValkyrieType::Void => "void".to_string(),
        ValkyrieType::Unit => "unit".to_string(),
        ValkyrieType::Boolean => "bool".to_string(),
        ValkyrieType::Integer8 { signed } => integer_type_name(*signed, 8),
        ValkyrieType::Integer16 { signed } => integer_type_name(*signed, 16),
        ValkyrieType::Integer32 { signed } => integer_type_name(*signed, 32),
        ValkyrieType::Integer64 { signed } => integer_type_name(*signed, 64),
        ValkyrieType::Integer128 { signed } => integer_type_name(*signed, 128),
        ValkyrieType::Float32 => "f32".to_string(),
        ValkyrieType::Float64 => "f64".to_string(),
        ValkyrieType::Character => "char".to_string(),
        ValkyrieType::Utf8 => "utf8".to_string(),
        ValkyrieType::Utf16 => "utf16".to_string(),
        ValkyrieType::Named(name) => name.to_string(),
        ValkyrieType::Apply(base, arguments) => {
            format!("{}<{}>", display_type(base), arguments.iter().map(display_type).collect::<Vec<_>>().join(", "))
        }
        ValkyrieType::Generic(generic) => generic.name.to_string(),
        ValkyrieType::Function(function) => format!(
            "micro({}) -> {}",
            function.params.iter().map(display_type).collect::<Vec<_>>().join(", "),
            display_type(&function.return_type)
        ),
        ValkyrieType::Tuple(items) => format!("({})", items.iter().map(display_type).collect::<Vec<_>>().join(", ")),
        ValkyrieType::Row(row) => format!(
            "{{ {} }}",
            row.methods
                .iter()
                .map(|method| {
                    format!(
                        "{}({}) -> {}",
                        method.name,
                        method.params.iter().map(display_type).collect::<Vec<_>>().join(", "),
                        display_type(&method.return_type)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ValkyrieType::Array(item) => format!("[{}]", display_type(item)),
        ValkyrieType::FixedArray { element, length } => format!("[{}; {}]", display_type(element), length),
        ValkyrieType::TypeLambda(lambda) => format!(
            "type lambda({}) -> {}",
            lambda.params.iter().map(|item| item.name.to_string()).collect::<Vec<_>>().join(", "),
            display_type(&lambda.body)
        ),
        ValkyrieType::TraitObject(object) => {
            format!("{}<{}>", object.trait_path, object.type_arguments.iter().map(display_type).collect::<Vec<_>>().join(", "))
        }
        ValkyrieType::Associated(associated) => format!("{}::{}", display_type(&associated.base), associated.name),
        ValkyrieType::AutoType => "auto".to_string(),
        ValkyrieType::SelfType => "Self".to_string(),
        ValkyrieType::Nullable(payload) => format!("{}?", display_type(payload)),
        ValkyrieType::Union(members) => members.iter().map(display_type).collect::<Vec<_>>().join(" | "),
        ValkyrieType::Intersection(members) => members.iter().map(display_type).collect::<Vec<_>>().join(" & "),
    }
}

fn integer_type_name(signed: bool, bits: u16) -> String {
    if signed { format!("i{bits}") } else { format!("u{bits}") }
}
