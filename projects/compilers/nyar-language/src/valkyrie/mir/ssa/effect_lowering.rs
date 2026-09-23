use crate::types::hir::{HirBlock, HirExprKind};

use super::*;

/// 判断 HIR 表达式树中是否包含 `raise` 表达式。
///
/// 不下钻 `Lambda` / `AnonymousClass` 内部——这些是独立函数作用域，
/// 其内部的 `raise` 属于该闭包/方法自身，不应算作外层函数的效应。
fn hir_expr_contains_raise(expr: &HirExpr) -> bool {
    match &expr.kind {
        HirExprKind::Raise(_) => true,
        HirExprKind::Lambda { .. } | HirExprKind::AnonymousClass { .. } => false,
        HirExprKind::Call { callee, args, .. } => hir_expr_contains_raise(callee) || args.iter().any(|arg| hir_expr_contains_raise(&arg.value)),
        HirExprKind::Construct { args, .. } => args.iter().any(hir_expr_contains_raise),
        HirExprKind::FieldInit { value, .. } => hir_expr_contains_raise(value),
        HirExprKind::ArrayNew { length, .. } => hir_expr_contains_raise(length),
        HirExprKind::ArrayLiteral { items } => items.iter().any(hir_expr_contains_raise),
        HirExprKind::FieldAccess { object, .. } => hir_expr_contains_raise(object),
        HirExprKind::StoreField { object, value, .. } => hir_expr_contains_raise(object) || hir_expr_contains_raise(value),
        HirExprKind::GenericApply { callee, .. } => hir_expr_contains_raise(callee),
        HirExprKind::Block(block) => hir_block_contains_raise(block),
        HirExprKind::If { condition, then_branch, else_branch } => {
            hir_expr_contains_raise(condition)
                || hir_block_contains_raise(then_branch)
                || else_branch.as_ref().is_some_and(|block| hir_block_contains_raise(block))
        }
        HirExprKind::IfLet { scrutinee, then_branch, else_branch, .. } => {
            hir_expr_contains_raise(scrutinee)
                || hir_block_contains_raise(then_branch)
                || else_branch.as_ref().is_some_and(|block| hir_block_contains_raise(block))
        }
        HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
            hir_expr_contains_raise(scrutinee) || arms.iter().any(|arm| hir_expr_contains_raise(&arm.body))
        }
        HirExprKind::Loop { iterator, condition, body, .. } => {
            iterator.as_ref().is_some_and(|expr| hir_expr_contains_raise(expr))
                || condition.as_ref().is_some_and(|expr| hir_expr_contains_raise(expr))
                || hir_block_contains_raise(body)
        }
        HirExprKind::Assign { value, .. } => hir_expr_contains_raise(value),
        HirExprKind::Break { expr, .. } => expr.as_ref().is_some_and(|expr| hir_expr_contains_raise(expr)),
        HirExprKind::Yield(inner) | HirExprKind::Return(inner) => inner.as_ref().is_some_and(|expr| hir_expr_contains_raise(expr)),
        HirExprKind::YieldFrom(inner) | HirExprKind::Await(inner) | HirExprKind::Awake(inner) | HirExprKind::BlockOn(inner) => {
            hir_expr_contains_raise(inner)
        }
        HirExprKind::Resume(inner) | HirExprKind::TryPropagate(inner) => hir_expr_contains_raise(inner),
        HirExprKind::Catch { expr, arms } => hir_expr_contains_raise(expr) || arms.iter().any(|arm| hir_expr_contains_raise(&arm.body)),
        HirExprKind::TryScope { body, .. } => hir_block_contains_raise(body),
        HirExprKind::With { base, updates } => hir_expr_contains_raise(base) || updates.iter().any(|(_, expr)| hir_expr_contains_raise(expr)),
        HirExprKind::SuperCall { args, .. } => args.iter().any(hir_expr_contains_raise),
        HirExprKind::Literal(_) | HirExprKind::Variable(_) | HirExprKind::Path(_) | HirExprKind::Continue { .. } | HirExprKind::Fallthrough => {
            false
        }
    }
}

/// 判断 HIR 块中是否包含 `raise` 表达式。
fn hir_block_contains_raise(block: &HirBlock) -> bool {
    block.statements.iter().any(|statement| match &statement.kind {
        HirStatementKind::Let { initializer, .. } => initializer.as_ref().is_some_and(|expr| hir_expr_contains_raise(expr)),
        HirStatementKind::Expr(expr) => hir_expr_contains_raise(expr),
    }) || block.expr.as_ref().is_some_and(|expr| hir_expr_contains_raise(expr))
}

/// 判断 HIR 表达式树中是否包含 `return` 表达式。
///
/// 不下钻 `Lambda` / `AnonymousClass` 内部。若被内联函数含 `return`，
/// 直接内联会改变 `return` 的归属函数，因此这类函数不可内联。
fn hir_expr_contains_return(expr: &HirExpr) -> bool {
    match &expr.kind {
        HirExprKind::Return(_) => true,
        HirExprKind::Lambda { .. } | HirExprKind::AnonymousClass { .. } => false,
        HirExprKind::Call { callee, args, .. } => {
            hir_expr_contains_return(callee) || args.iter().any(|arg| hir_expr_contains_return(&arg.value))
        }
        HirExprKind::Construct { args, .. } => args.iter().any(hir_expr_contains_return),
        HirExprKind::FieldInit { value, .. } => hir_expr_contains_return(value),
        HirExprKind::ArrayNew { length, .. } => hir_expr_contains_return(length),
        HirExprKind::ArrayLiteral { items } => items.iter().any(hir_expr_contains_return),
        HirExprKind::FieldAccess { object, .. } => hir_expr_contains_return(object),
        HirExprKind::StoreField { object, value, .. } => hir_expr_contains_return(object) || hir_expr_contains_return(value),
        HirExprKind::GenericApply { callee, .. } => hir_expr_contains_return(callee),
        HirExprKind::Block(block) => hir_block_contains_return(block),
        HirExprKind::If { condition, then_branch, else_branch } => {
            hir_expr_contains_return(condition)
                || hir_block_contains_return(then_branch)
                || else_branch.as_ref().is_some_and(|block| hir_block_contains_return(block))
        }
        HirExprKind::IfLet { scrutinee, then_branch, else_branch, .. } => {
            hir_expr_contains_return(scrutinee)
                || hir_block_contains_return(then_branch)
                || else_branch.as_ref().is_some_and(|block| hir_block_contains_return(block))
        }
        HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
            hir_expr_contains_return(scrutinee) || arms.iter().any(|arm| hir_expr_contains_return(&arm.body))
        }
        HirExprKind::Loop { iterator, condition, body, .. } => {
            iterator.as_ref().is_some_and(|expr| hir_expr_contains_return(expr))
                || condition.as_ref().is_some_and(|expr| hir_expr_contains_return(expr))
                || hir_block_contains_return(body)
        }
        HirExprKind::Assign { value, .. } => hir_expr_contains_return(value),
        HirExprKind::Break { expr, .. } => expr.as_ref().is_some_and(|expr| hir_expr_contains_return(expr)),
        HirExprKind::Yield(inner) => inner.as_ref().is_some_and(|expr| hir_expr_contains_return(expr)),
        HirExprKind::YieldFrom(inner) | HirExprKind::Await(inner) | HirExprKind::Awake(inner) | HirExprKind::BlockOn(inner) => {
            hir_expr_contains_return(inner)
        }
        HirExprKind::Raise(inner) | HirExprKind::Resume(inner) | HirExprKind::TryPropagate(inner) => hir_expr_contains_return(inner),
        HirExprKind::Catch { expr, arms } => hir_expr_contains_return(expr) || arms.iter().any(|arm| hir_expr_contains_return(&arm.body)),
        HirExprKind::TryScope { body, .. } => hir_block_contains_return(body),
        HirExprKind::With { base, updates } => hir_expr_contains_return(base) || updates.iter().any(|(_, expr)| hir_expr_contains_return(expr)),
        HirExprKind::SuperCall { args, .. } => args.iter().any(hir_expr_contains_return),
        HirExprKind::Literal(_) | HirExprKind::Variable(_) | HirExprKind::Path(_) | HirExprKind::Continue { .. } | HirExprKind::Fallthrough => {
            false
        }
    }
}

/// 判断 HIR 块中是否包含 `return` 表达式。
fn hir_block_contains_return(block: &HirBlock) -> bool {
    block.statements.iter().any(|statement| match &statement.kind {
        HirStatementKind::Let { initializer, .. } => initializer.as_ref().is_some_and(|expr| hir_expr_contains_return(expr)),
        HirStatementKind::Expr(expr) => hir_expr_contains_return(expr),
    }) || block.expr.as_ref().is_some_and(|expr| hir_expr_contains_return(expr))
}

/// 判断 HIR 表达式树中是否调用了指定名称的函数（用于递归检测）。
///
/// 不下钻 `Lambda` / `AnonymousClass` 内部。
fn hir_expr_calls_name(expr: &HirExpr, name: &str) -> bool {
    match &expr.kind {
        HirExprKind::Call { callee, args, resolved } => {
            let callee_matches = resolved
                .as_ref()
                .map(|resolved| resolved.symbol.to_string() == name)
                .unwrap_or_else(|| callee_name_matches(&callee.kind, name))
                || callee_name_matches(&callee.kind, name);
            callee_matches || hir_expr_calls_name(callee, name) || args.iter().any(|arg| hir_expr_calls_name(&arg.value, name))
        }
        HirExprKind::Lambda { .. } | HirExprKind::AnonymousClass { .. } => false,
        HirExprKind::Construct { args, .. } => args.iter().any(|expr| hir_expr_calls_name(expr, name)),
        HirExprKind::FieldInit { value, .. } => hir_expr_calls_name(value, name),
        HirExprKind::ArrayNew { length, .. } => hir_expr_calls_name(length, name),
        HirExprKind::ArrayLiteral { items } => items.iter().any(|expr| hir_expr_calls_name(expr, name)),
        HirExprKind::FieldAccess { object, .. } => hir_expr_calls_name(object, name),
        HirExprKind::StoreField { object, value, .. } => hir_expr_calls_name(object, name) || hir_expr_calls_name(value, name),
        HirExprKind::GenericApply { callee, .. } => hir_expr_calls_name(callee, name),
        HirExprKind::Block(block) => hir_block_calls_name(block, name),
        HirExprKind::If { condition, then_branch, else_branch } => {
            hir_expr_calls_name(condition, name)
                || hir_block_calls_name(then_branch, name)
                || else_branch.as_ref().is_some_and(|block| hir_block_calls_name(block, name))
        }
        HirExprKind::IfLet { scrutinee, then_branch, else_branch, .. } => {
            hir_expr_calls_name(scrutinee, name)
                || hir_block_calls_name(then_branch, name)
                || else_branch.as_ref().is_some_and(|block| hir_block_calls_name(block, name))
        }
        HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
            hir_expr_calls_name(scrutinee, name) || arms.iter().any(|arm| hir_expr_calls_name(&arm.body, name))
        }
        HirExprKind::Loop { iterator, condition, body, .. } => {
            iterator.as_ref().is_some_and(|expr| hir_expr_calls_name(expr, name))
                || condition.as_ref().is_some_and(|expr| hir_expr_calls_name(expr, name))
                || hir_block_calls_name(body, name)
        }
        HirExprKind::Return(inner) => inner.as_ref().is_some_and(|expr| hir_expr_calls_name(expr, name)),
        HirExprKind::Assign { value, .. } => hir_expr_calls_name(value, name),
        HirExprKind::Break { expr, .. } => expr.as_ref().is_some_and(|expr| hir_expr_calls_name(expr, name)),
        HirExprKind::Yield(inner) => inner.as_ref().is_some_and(|expr| hir_expr_calls_name(expr, name)),
        HirExprKind::YieldFrom(inner) | HirExprKind::Await(inner) | HirExprKind::Awake(inner) | HirExprKind::BlockOn(inner) => {
            hir_expr_calls_name(inner, name)
        }
        HirExprKind::Raise(inner) | HirExprKind::Resume(inner) | HirExprKind::TryPropagate(inner) => hir_expr_calls_name(inner, name),
        HirExprKind::Catch { expr, arms } => hir_expr_calls_name(expr, name) || arms.iter().any(|arm| hir_expr_calls_name(&arm.body, name)),
        HirExprKind::TryScope { body, .. } => hir_block_calls_name(body, name),
        HirExprKind::With { base, updates } => {
            hir_expr_calls_name(base, name) || updates.iter().any(|(_, expr)| hir_expr_calls_name(expr, name))
        }
        HirExprKind::SuperCall { args, .. } => args.iter().any(|expr| hir_expr_calls_name(expr, name)),
        HirExprKind::Literal(_) | HirExprKind::Variable(_) | HirExprKind::Path(_) | HirExprKind::Continue { .. } | HirExprKind::Fallthrough => {
            false
        }
    }
}

/// 判断 HIR 块中是否调用了指定名称的函数。
fn hir_block_calls_name(block: &HirBlock, name: &str) -> bool {
    block.statements.iter().any(|statement| match &statement.kind {
        HirStatementKind::Let { initializer, .. } => initializer.as_ref().is_some_and(|expr| hir_expr_calls_name(expr, name)),
        HirStatementKind::Expr(expr) => hir_expr_calls_name(expr, name),
    }) || block.expr.as_ref().is_some_and(|expr| hir_expr_calls_name(expr, name))
}

/// 从调用表达式中提取被调用函数的简单名（用于匹配内联候选）。
fn extract_call_callee_name(expr: &HirExpr) -> Option<String> {
    let HirExprKind::Call { callee, resolved, .. } = &expr.kind
    else {
        return None;
    };
    if let Some(resolved) = resolved {
        return Some(resolved.symbol.to_string());
    }
    match &callee.kind {
        HirExprKind::Path(path) => Some(path.to_string()),
        HirExprKind::Variable(identifier) => Some(identifier.name.to_string()),
        _ => None,
    }
}

/// 收集模块中可内联到 `catch` 调用点的效应函数。
///
/// 一个函数可内联当且仅当：
/// - 函数体包含 `raise`（否则无需内联，普通调用即可）；
/// - 函数体不含 `return`（`return` 在内联后会错误地返回外层函数）；
/// - 函数不直接递归（不含对自身的调用，避免无限展开）；
/// - 函数无泛型参数；
/// - 函数非抽象。
pub(super) fn collect_effectful_inline_targets(module: &HirModule) -> BTreeMap<String, HirFunction> {
    let mut targets = BTreeMap::new();
    for function in &module.functions {
        if function.is_abstract || !function.generics.is_empty() {
            continue;
        }
        let name = function.name.to_string();
        if hir_block_contains_return(&function.body) {
            continue;
        }
        if !hir_block_contains_raise(&function.body) {
            continue;
        }
        let body_calls_self = hir_block_calls_name(&function.body, &name);
        if body_calls_self {
            continue;
        }
        targets.insert(name, function.clone());
    }
    targets
}

impl MirBuilder {
    fn record_suspend_point(
        &mut self,
        effect: MirEffectKind,
        suspend_block: MirBlockRef,
        resume_target: MirBlockRef,
        payload: Option<&MirOperand>,
        resume_parameter_count: usize,
        continuation_index: Option<usize>,
        carrier_type: Option<ValkyrieType>,
    ) {
        let payload_type = payload.and_then(|operand| infer_builder_operand_type(operand, &self.value_types));
        let spill_candidates: Vec<MirValueRef> = self
            .bindings
            .values()
            .filter_map(|operand| match operand {
                MirOperand::Value(value) => Some(*value),
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let state_id = self.next_state_id();
        self.suspend_points.push(MirSuspendPoint {
            state_id,
            effect,
            suspend_block,
            resume_target,
            resume_parameter_count,
            payload_type,
            spill_candidates,
            continuation_index,
            carrier_type,
        });
    }

    fn lower_perform_effect(
        &mut self,
        label: &str,
        effect: MirEffectKind,
        payload: Option<MirOperand>,
        resume_parameter_type: Option<ValkyrieType>,
    ) -> MirOperand {
        let current_block = self.current_block;
        let current_label = self.current_label.clone();
        let resume_block = self.new_block(label);
        let resume_value = self.ensure_block_parameter(resume_block, label, resume_parameter_type);
        self.current_block = current_block;
        self.current_label = current_label.clone();
        let continuation_index = self.control_flow.current_resume().map(|ctx| ctx.continuation);
        self.record_suspend_point(effect, current_block, resume_block, payload.as_ref(), 1, continuation_index, None);
        self.terminate(MirTerminator::PerformEffect { effect, payload, resume_target: resume_block });
        self.flush_block(&current_label);
        self.current_block = resume_block;
        self.current_label = label.to_string();
        self.instructions.clear();
        self.terminator = None;
        MirOperand::Value(resume_value)
    }

    fn lower_perform_effect_without_resume_parameter(&mut self, label: &str, effect: MirEffectKind, payload: Option<MirOperand>) {
        let current_block = self.current_block;
        let current_label = self.current_label.clone();
        let resume_block = self.new_block(label);
        self.current_block = current_block;
        self.current_label = current_label.clone();
        let continuation_index = self.control_flow.current_resume().map(|ctx| ctx.continuation);
        self.record_suspend_point(effect, current_block, resume_block, payload.as_ref(), 0, continuation_index, None);
        self.terminate(MirTerminator::PerformEffect { effect, payload, resume_target: resume_block });
        self.flush_block(&current_label);
        self.current_block = resume_block;
        self.current_label = label.to_string();
        self.instructions.clear();
        self.terminator = None;
    }

    pub(super) fn lower_uncaught_raise(&mut self, payload: Option<MirOperand>) -> MirOperand {
        let carrier_type = payload.as_ref().and_then(|operand| infer_builder_operand_type(operand, &self.value_types));
        let resume_parameter_type = self.lookup_effectful_resume_type(&carrier_type);
        self.lower_perform_effect_with_carrier("raise_resume", MirEffectKind::Raise, payload, resume_parameter_type, carrier_type)
    }

    /// 查询当前模块的 `Effectful::Resume` 关联类型映射。
    ///
    /// - 若 carrier_type 为 `Named(n)` 或 `Apply(Named(n), _)` 且 map 中存在 `n`，返回对应的 Resume 类型；
    /// - 若 carrier_type 可识别但未在 map 中找到，说明未声明 `Effectful`，按 Valkyrie 约定 resume 类型为 `Never`；
    /// - carrier_type 无法识别时返回 `None`，保留旧异常路径语义（无可恢复值）。
    fn lookup_effectful_resume_type(&self, carrier_type: &Option<ValkyrieType>) -> Option<ValkyrieType> {
        let carrier = carrier_type.as_ref()?;
        let name = match carrier {
            ValkyrieType::Named(name) => Some(name.as_str()),
            ValkyrieType::Apply(base, _) => match base.as_ref() {
                ValkyrieType::Named(name) => Some(name.as_str()),
                _ => None,
            },
            _ => None,
        }?;
        if let Some(resume_type) = self.effectful_resume_map.get(name) {
            return Some(resume_type.clone());
        }
        // 载体未声明 Effectful：按约定 Resume = !（Never）。
        // 使用 Named("Never") 表示，与文档 `throw(): Never` 与 spec/nominal.rs 的约定一致。
        Some(ValkyrieType::Named(crate::types::Identifier::new("Never")))
    }

    /// 与 [`lower_perform_effect`] 同构，但允许携带用户 effect 载体类型。
    ///
    /// 仅 `Raise` 路径使用：被 raise 的值的类型即为载体类型，用于后续查询
    /// `Effectful::Resume` 关联类型以推断 resume 参数类型。
    fn lower_perform_effect_with_carrier(
        &mut self,
        label: &str,
        effect: MirEffectKind,
        payload: Option<MirOperand>,
        resume_parameter_type: Option<ValkyrieType>,
        carrier_type: Option<ValkyrieType>,
    ) -> MirOperand {
        let current_block = self.current_block;
        let current_label = self.current_label.clone();
        let resume_block = self.new_block(label);
        let resume_value = self.ensure_block_parameter(resume_block, label, resume_parameter_type);
        self.current_block = current_block;
        self.current_label = current_label.clone();
        let continuation_index = self.control_flow.current_resume().map(|ctx| ctx.continuation);
        self.record_suspend_point(effect, current_block, resume_block, payload.as_ref(), 1, continuation_index, carrier_type.clone());
        self.terminate(MirTerminator::PerformEffect { effect, payload, resume_target: resume_block });
        self.flush_block(&current_label);
        self.current_block = resume_block;
        self.current_label = label.to_string();
        self.instructions.clear();
        self.terminator = None;
        MirOperand::Value(resume_value)
    }

    pub(super) fn lower_handler_raise(&mut self, handler_index: usize, payload: Option<MirOperand>) -> MirOperand {
        let payload = payload.unwrap_or(MirOperand::Constant(MirConstant::Unit));
        let payload_type = infer_builder_operand_type(&payload, &self.value_types);
        let carrier_type = payload_type.clone();
        // 用户 effect 载体：catch arm 内 resume 的类型应来自 `Effectful::Resume`，
        // 而非沿用 payload 类型。这保证 `raise Read{} → resume("data")` 中
        // `let data = raise Read{}` 推断为 `utf8` 而非 `Read`。
        let resume_parameter_type = self.lookup_effectful_resume_type(&carrier_type).or_else(|| payload_type.clone());
        let current_block = self.current_block;
        let current_label = self.current_label.clone();
        let dispatch_block = self.new_block("catch_dispatch");
        let resume_block = self.new_block("catch_resume");
        let resume_value = self.ensure_block_parameter(resume_block, "catch_resume", resume_parameter_type.clone());
        self.current_block = current_block;
        self.current_label = current_label.clone();
        self.terminate(MirTerminator::Jump { target: dispatch_block, arguments: vec![payload] });
        self.flush_block(&current_label);

        self.current_block = dispatch_block;
        self.current_label = "catch_dispatch".to_string();
        self.instructions.clear();
        self.terminator = None;
        let raised_value = self.ensure_block_parameter(dispatch_block, "raised_effect", payload_type.clone());

        let arms = self.control_flow.handler_at(handler_index).arms.clone();
        let handler_exit = self.control_flow.handler_at(handler_index).exit;
        let continuation_index = self.continuations.len();
        self.continuations.push(MirContinuation {
            dispatch_block,
            resume_target: resume_block,
            resume_parameter: resume_value,
            resume_parameter_type: resume_parameter_type.clone(),
            handler_exit,
            carrier_type,
        });
        if arms.is_empty() {
            self.terminate(MirTerminator::Jump { target: handler_exit, arguments: Vec::new() });
            self.flush_block("catch_dispatch");
        }
        else {
            let arm_blocks: Vec<MirBlockRef> =
                arms.iter().enumerate().map(|(index, _)| self.new_block(&format!("catch_arm_{index}"))).collect();
            let no_match_block = self.new_block("catch_no_match");

            self.current_block = dispatch_block;
            self.current_label = "catch_dispatch".to_string();
            self.instructions.clear();
            self.terminator = None;
            self.terminate(MirTerminator::Jump { target: arm_blocks[0], arguments: Vec::new() });
            self.flush_block("catch_dispatch");

            let saved_bindings = self.bindings.clone();
            let saved_static_bindings = self.static_bindings.clone();

            for (index, arm) in arms.iter().enumerate() {
                let arm_block = arm_blocks[index];
                let next_target = arm_blocks.get(index + 1).copied().unwrap_or(no_match_block);
                let arm_label = format!("catch_arm_{index}");
                let needs_pattern_check = !matches!(arm.pattern, HirPattern::Wildcard | HirPattern::Variable(_) | HirPattern::Else);

                self.current_block = arm_block;
                self.current_label = arm_label.clone();
                self.instructions.clear();
                self.terminator = None;
                self.bindings = saved_bindings.clone();
                self.static_bindings = saved_static_bindings.clone();

                if needs_pattern_check {
                    let saved_arm_block = self.current_block;
                    let saved_arm_label = self.current_label.clone();
                    let body_or_guard_block = self.new_block(&format!("catch_arm_{index}_match"));
                    self.current_block = saved_arm_block;
                    self.current_label = saved_arm_label;
                    self.instructions.clear();
                    self.terminator = None;
                    self.bindings = saved_bindings.clone();
                    self.static_bindings = saved_static_bindings.clone();
                    let raised_operand = MirOperand::Value(raised_value);
                    let (matched, payload) = self.lower_pattern_match_probe(&arm.pattern, raised_operand.clone());
                    self.terminate(MirTerminator::Branch { condition: matched, then_target: body_or_guard_block, else_target: next_target });
                    self.flush_block(&arm_label);
                    self.current_block = body_or_guard_block;
                    self.current_label = format!("catch_arm_{index}_match");
                    self.instructions.clear();
                    self.terminator = None;
                    self.bindings = saved_bindings.clone();
                    self.static_bindings = saved_static_bindings.clone();
                    self.bind_catch_arm_pattern(&arm.pattern, raised_operand, payload);
                }

                if let Some(guard) = &arm.guard {
                    let saved_guard_block = self.current_block;
                    let saved_guard_label = self.current_label.clone();
                    let body_block = self.new_block(&format!("catch_arm_{index}_body"));
                    self.current_block = saved_guard_block;
                    self.current_label = saved_guard_label;
                    self.instructions.clear();
                    self.terminator = None;
                    self.bindings = saved_bindings.clone();
                    self.static_bindings = saved_static_bindings.clone();
                    self.bind_catch_arm_pattern(&arm.pattern, MirOperand::Value(raised_value), None);

                    self.suspended_handler_depth += 1;
                    let guard_value = self.lower_expr_to_operand(guard);
                    self.suspended_handler_depth = self.suspended_handler_depth.saturating_sub(1);
                    self.terminate(MirTerminator::Branch { condition: guard_value, then_target: body_block, else_target: next_target });
                    let guard_label = self.current_label.clone();
                    self.flush_block(&guard_label);

                    self.current_block = body_block;
                    self.current_label = format!("catch_arm_{index}_body");
                    self.instructions.clear();
                    self.terminator = None;
                }

                self.bind_catch_arm_pattern(&arm.pattern, MirOperand::Value(raised_value), None);
                self.control_flow.push_resume(MirResumeContinuationContext {
                    continuation: continuation_index,
                    target: resume_block,
                    parameter: resume_value,
                    parameter_name: "catch_resume",
                    parameter_type: resume_parameter_type.clone(),
                });
                self.suspended_handler_depth += 1;
                let arm_result = self.lower_expr_to_operand(&arm.body);
                self.suspended_handler_depth = self.suspended_handler_depth.saturating_sub(1);
                self.control_flow.pop_resume();
                if self.terminator.is_none() {
                    let ty = infer_builder_operand_type(&arm_result, &self.value_types);
                    let _ = self.ensure_handler_exit_parameter(handler_index, ty);
                    self.terminate(MirTerminator::Jump { target: handler_exit, arguments: vec![arm_result] });
                }
                let current_arm_label = self.current_label.clone();
                self.flush_block(&current_arm_label);
            }

            self.current_block = no_match_block;
            self.current_label = "catch_no_match".to_string();
            self.instructions.clear();
            self.terminator = None;
            let propagated = if handler_index > 0 {
                self.lower_handler_raise(handler_index - 1, Some(MirOperand::Value(raised_value)))
            }
            else {
                self.lower_uncaught_raise(Some(MirOperand::Value(raised_value)))
            };
            if self.terminator.is_none() {
                let ty = infer_builder_operand_type(&propagated, &self.value_types);
                let _ = self.ensure_handler_exit_parameter(handler_index, ty);
                self.terminate(MirTerminator::Jump { target: handler_exit, arguments: vec![propagated] });
            }
            let no_match_label = self.current_label.clone();
            self.flush_block(&no_match_label);
        }

        self.current_block = resume_block;
        self.current_label = "catch_resume".to_string();
        self.instructions.clear();
        self.terminator = None;
        MirOperand::Value(resume_value)
    }

    pub(super) fn lower_yield_expr(&mut self, value: Option<&HirExpr>) -> MirOperand {
        let payload = Some(value.map(|expr| self.lower_expr_to_operand(expr)).unwrap_or(MirOperand::Constant(MirConstant::Unit)));
        self.lower_perform_effect("yield_resume", MirEffectKind::Yield, payload, Some(ValkyrieType::Unit))
    }

    pub(super) fn lower_yield_from_expr(&mut self, value: &HirExpr) -> MirOperand {
        let payload = Some(self.lower_expr_to_operand(value));
        let resume_type = payload.as_ref().and_then(|operand| infer_builder_operand_type(operand, &self.value_types));
        self.lower_perform_effect("yield_from_resume", MirEffectKind::DelegateYield, payload, resume_type)
    }

    pub(super) fn lower_await_expr(&mut self, value: &HirExpr) -> MirOperand {
        let payload = Some(self.lower_expr_to_operand(value));
        let resume_type =
            payload.as_ref().and_then(|operand| infer_builder_operand_type(operand, &self.value_types)).and_then(|ty| future_resume_type(&ty));
        self.lower_perform_effect("await_resume", MirEffectKind::Await, payload, resume_type)
    }

    pub(super) fn lower_awake_expr(&mut self, value: &HirExpr) -> MirOperand {
        let payload = Some(self.lower_expr_to_operand(value));
        self.lower_perform_effect_without_resume_parameter("awake_resume", MirEffectKind::AsyncSpawn, payload);
        MirOperand::Constant(MirConstant::Unit)
    }

    pub(super) fn lower_block_on_expr(&mut self, value: &HirExpr) -> MirOperand {
        let payload = Some(self.lower_expr_to_operand(value));
        let resume_type =
            payload.as_ref().and_then(|operand| infer_builder_operand_type(operand, &self.value_types)).and_then(|ty| future_resume_type(&ty));
        self.lower_perform_effect("block_resume", MirEffectKind::AsyncBlock, payload, resume_type)
    }

    pub(super) fn lower_raise_expr(&mut self, value: &HirExpr) -> MirOperand {
        let payload = Some(self.lower_expr_to_operand(value));
        if let Some(handler_index) = self.control_flow.handler_count().checked_sub(self.suspended_handler_depth.saturating_add(1)) {
            self.lower_handler_raise(handler_index, payload)
        }
        else {
            self.lower_uncaught_raise(payload)
        }
    }

    pub(super) fn lower_resume_expr(&mut self, value: &HirExpr) -> MirOperand {
        let resume_value = self.lower_expr_to_operand(value);
        if let Some(resume_context) = self.control_flow.current_resume().cloned() {
            let resume_type = infer_builder_operand_type(&resume_value, &self.value_types);
            if let Some(resume_type) = resume_type.clone() {
                self.continuations[resume_context.continuation].resume_parameter_type.get_or_insert(resume_type);
            }
            let parameter = self.ensure_block_parameter(
                resume_context.target,
                resume_context.parameter_name,
                resume_context.parameter_type.clone().or(resume_type.clone()),
            );
            debug_assert_eq!(parameter, resume_context.parameter);
            if let Some(resume_type) = resume_type {
                self.value_types.entry(resume_context.parameter).or_insert(resume_type);
            }
            self.terminate(MirTerminator::Jump { target: resume_context.target, arguments: vec![resume_value] });
            let label = self.current_label.clone();
            self.flush_block(&label);
            self.new_block("after_resume");
            self.instructions.clear();
            self.terminator = Some(MirTerminator::Unreachable);
        }
        else {
            self.terminate(MirTerminator::Unreachable);
            let label = self.current_label.clone();
            self.flush_block(&label);
            self.new_block("after_invalid_resume");
            self.instructions.clear();
            self.terminator = Some(MirTerminator::Unreachable);
        }
        MirOperand::Constant(MirConstant::Unit)
    }

    pub(super) fn lower_catch_expr(&mut self, expr: &HirExpr, arms: &[HirMatchArm]) -> MirOperand {
        let current_block = self.current_block;
        let current_label = self.current_label.clone();
        let exit_block = self.new_block("catch_exit");
        self.current_block = current_block;
        self.current_label = current_label.clone();
        self.instructions.clear();
        self.terminator = None;

        self.control_flow.push_handler(MirHandlerDispatchContext { arms: arms.to_vec(), exit: exit_block, exit_value: None });

        let expr_result = self.try_inline_effectful_callee(expr).unwrap_or_else(|| self.lower_expr_to_operand(expr));
        let handler_index = self.control_flow.handler_count() - 1;
        if self.terminator.is_none() {
            let ty = infer_builder_operand_type(&expr_result, &self.value_types);
            let _ = self.ensure_handler_exit_parameter(handler_index, ty);
            self.terminate(MirTerminator::Jump { target: exit_block, arguments: vec![expr_result] });
            let label = self.current_label.clone();
            self.flush_block(&label);
        }
        let handler_context = self.control_flow.pop_handler();

        self.current_block = exit_block;
        self.current_label = "catch_exit".to_string();
        self.instructions.clear();
        self.terminator = None;
        handler_context.exit_value.map(MirOperand::Value).unwrap_or(MirOperand::Constant(MirConstant::Unit))
    }

    /// 尝试在 `catch` 调用点内联被调用的效应函数。
    ///
    /// 当 `expr` 是对模块级效应函数（含 `raise`、无 `return`、非递归）的直接调用时，
    /// 将其函数体内联到当前 `catch` 上下文中：先把实参 lower 为 operand 并绑定到形参名，
    /// 再依次 lower 函数体的语句与尾表达式。这样函数体内部的 `raise` 会直接被当前
    /// `catch` handler 捕获，从而实现跨函数 `raise` 传播。
    ///
    /// 返回 `Some(operand)` 表示成功内联并产生结果值；返回 `None` 表示 `expr` 不是
    /// 可内联的效应函数调用，调用方应回退到普通 `lower_expr_to_operand`。
    fn try_inline_effectful_callee(&mut self, expr: &HirExpr) -> Option<MirOperand> {
        let callee_name = extract_call_callee_name(expr)?;
        let callee = self.effectful_inline_targets.get(&callee_name)?.clone();
        let HirExprKind::Call { args, .. } = &expr.kind
        else {
            return None;
        };
        if args.len() != callee.params.len() {
            return None;
        }

        let saved_return_type = self.current_return_type.clone();
        self.current_return_type = callee.return_type.clone();

        let mut saved_bindings: Vec<(String, Option<MirOperand>)> = Vec::new();
        for (param, arg) in callee.params.iter().zip(args.iter()) {
            let arg_operand = self.lower_expr_to_operand(&arg.value);
            let param_name = param.name.name.to_string();
            saved_bindings.push((param_name.clone(), self.bindings.get(&param_name).cloned()));
            self.bindings.insert(param_name, arg_operand);
        }

        for statement in &callee.body.statements {
            self.lower_statement(statement);
            if self.terminator.is_some() {
                break;
            }
        }

        let result = if self.terminator.is_none() {
            if let Some(tail_expr) = &callee.body.expr {
                self.lower_expr_to_operand(tail_expr)
            }
            else {
                MirOperand::Constant(MirConstant::Unit)
            }
        }
        else {
            MirOperand::Constant(MirConstant::Unit)
        };

        for (name, saved) in saved_bindings {
            match saved {
                Some(operand) => {
                    self.bindings.insert(name, operand);
                }
                None => {
                    self.bindings.remove(&name);
                }
            }
        }

        self.current_return_type = saved_return_type;
        Some(result)
    }
}
