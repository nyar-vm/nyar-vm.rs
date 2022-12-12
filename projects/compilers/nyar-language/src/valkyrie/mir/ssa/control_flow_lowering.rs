use crate::types::{
    Identifier, NamePath, SourceID, SourceSpan,
    hir::{
        HirBlock, HirCallArgument, HirExpr, HirExprKind, HirIdentifier, HirLiteral, HirPattern, HirStatement, HirStatementKind, ValkyrieType,
    },
};

use crate::valkyrie::control_flow::TryScopeData;

use super::{
    MirBuilder, MirConstant, MirOperand, MirOperation, MirTerminator, MirValueOrigin, control_flow_context::MirLoopContext,
    infer_builder_operand_type,
};

impl MirBuilder {
    /// `loop pat in coll` → indexed while using language `ArrayLength` / `ArrayGet`.
    ///
    /// Comparison / increment go through operator `Call` (`infix <` / `infix +`), never an
    /// IntrinsicOpcode table (ADR 0010 / ADR 0011).
    pub(super) fn lower_for_in_as_indexed_while(
        &mut self,
        label: &Option<crate::types::Identifier>,
        loop_pattern: &HirPattern,
        iterator_expr: &HirExpr,
        condition: &Option<Box<HirExpr>>,
        body: &HirBlock,
    ) -> MirOperand {
        let pre_loop_bindings = self.bindings.clone();
        let outer_block_id = self.current_block;
        let outer_label = self.current_label.clone();

        let collection = self.lower_expr_to_operand(iterator_expr);
        let element_ty = Self::loop_bind_type_hint(&collection, &self.value_types);

        let len_value = self.next_value(MirValueOrigin::Temporary);
        self.push_instruction(MirOperation::ArrayLength { array: collection.clone() }, vec![len_value]);
        self.value_types.insert(len_value, ValkyrieType::Integer32 { signed: true });
        let len_operand = MirOperand::Value(len_value);

        let zero = MirOperand::Constant(MirConstant::Int(0));
        let one = MirOperand::Constant(MirConstant::Int(1));

        let loop_header_id = super::MirBlockRef(self.blocks.len() as u32);
        self.terminate(MirTerminator::Jump { target: loop_header_id, arguments: vec![zero.clone()] });
        self.flush_block(&outer_label);
        self.terminator = None;

        self.new_block("for_in_header");
        debug_assert_eq!(self.current_block, loop_header_id);
        let index_param = self.next_value(MirValueOrigin::BlockParameter { block: loop_header_id, name: "__for_in_i".to_string() });
        self.value_types.insert(index_param, ValkyrieType::Integer32 { signed: true });
        self.blocks[loop_header_id.0 as usize].parameters.push(index_param);
        let index_operand = MirOperand::Value(index_param);

        let loop_body_id = self.new_block("for_in_body");
        let loop_exit_id = self.new_block("for_in_exit");

        // Header: while i < len [&& user condition]
        self.current_block = loop_header_id;
        self.current_label = "for_in_header".to_string();
        self.instructions.clear();
        self.terminator = None;
        self.bindings = pre_loop_bindings.clone();

        let cmp_value = self.next_value(MirValueOrigin::CallResult);
        self.push_instruction(
            MirOperation::Call {
                callee: MirOperand::Symbol(NamePath::new(vec![Identifier::new("infix <")])),
                arguments: vec![index_operand.clone(), len_operand.clone()],
            },
            vec![cmp_value],
        );
        self.value_types.insert(cmp_value, ValkyrieType::Boolean);
        let bounds_ok = MirOperand::Value(cmp_value);

        // Optional `while` filter on for-in: nest as a second branch (no `infix &&` Call —
        // `&&` is short-circuit control flow in HIR, not an operator method).
        let then_target = if condition.is_some() {
            let filter_id = self.new_block("for_in_filter");
            self.current_block = loop_header_id;
            self.current_label = "for_in_header".to_string();
            self.terminate(MirTerminator::Branch { condition: bounds_ok, then_target: filter_id, else_target: loop_exit_id });
            self.flush_block("for_in_header");
            self.terminator = None;

            self.current_block = filter_id;
            self.current_label = "for_in_filter".to_string();
            self.instructions.clear();
            self.bindings = pre_loop_bindings.clone();
            let extra_val = self.lower_expr_to_operand(condition.as_ref().unwrap());
            self.terminate(MirTerminator::Branch { condition: extra_val, then_target: loop_body_id, else_target: loop_exit_id });
            self.flush_block("for_in_filter");
            self.terminator = None;
            loop_body_id
        }
        else {
            self.terminate(MirTerminator::Branch { condition: bounds_ok, then_target: loop_body_id, else_target: loop_exit_id });
            self.flush_block("for_in_header");
            self.terminator = None;
            loop_body_id
        };
        let _ = then_target;

        self.control_flow.push_loop(
            label.as_ref().map(|value| value.to_string()),
            MirLoopContext {
                header: loop_header_id,
                exit: loop_exit_id,
                exit_value: None,
                exit_reached_by_break: false,
                carried_values: vec!["__for_in_i".to_string()],
                carried_value_refs: std::collections::BTreeMap::from([("__for_in_i".to_string(), index_param)]),
            },
        );

        // Body: elem = coll[i]; bind; statements; i = i + 1; continue
        self.current_block = loop_body_id;
        self.current_label = "for_in_body".to_string();
        if let Some(loop_body) = self.blocks.get_mut(loop_body_id.0 as usize) {
            loop_body.instructions.clear();
            loop_body.terminator = MirTerminator::Unreachable;
        }
        self.instructions.clear();
        self.terminator = None;
        self.bindings = pre_loop_bindings.clone();

        let elem_value = self.next_value(MirValueOrigin::Temporary);
        self.push_instruction(MirOperation::ArrayGet { array: collection, index: index_operand.clone() }, vec![elem_value]);
        if let Some(ty) = element_ty.clone() {
            self.value_types.insert(elem_value, ty);
        }
        self.bind_pattern_from_operand_with_payload(loop_pattern, MirOperand::Value(elem_value), element_ty, None);

        for statement in &body.statements {
            self.lower_statement(statement);
            if self.terminator.is_some() {
                break;
            }
        }
        if self.terminator.is_none() {
            if let Some(tail) = &body.expr {
                let _ = self.lower_expr_to_operand(tail);
            }
        }

        let _loop_context = self.control_flow.pop_loop();

        if self.terminator.is_none() {
            let next_index = self.next_value(MirValueOrigin::CallResult);
            self.push_instruction(
                MirOperation::Call {
                    callee: MirOperand::Symbol(NamePath::new(vec![Identifier::new("infix +")])),
                    arguments: vec![index_operand, one],
                },
                vec![next_index],
            );
            self.value_types.insert(next_index, ValkyrieType::Integer32 { signed: true });
            self.terminate(MirTerminator::Jump { target: loop_header_id, arguments: vec![MirOperand::Value(next_index)] });
        }
        let body_label = self.current_label.clone();
        self.flush_block(&body_label);

        self.current_block = loop_exit_id;
        self.current_label = "for_in_exit".to_string();
        self.instructions.clear();
        self.terminator = None;
        self.bindings = pre_loop_bindings;
        // Keep CFG well-formed; caller may terminate later.
        let _ = outer_block_id;
        MirOperand::Constant(MirConstant::Unit)
    }

    pub(super) fn lower_if_expr(
        &mut self,
        condition: &HirExpr,
        then_branch: &crate::hir::HirBlock,
        else_branch: &Option<Box<crate::hir::HirBlock>>,
        expected_type: Option<&ValkyrieType>,
    ) -> MirOperand {
        let condition_type = ValkyrieType::Boolean;
        let cond_val = self.lower_expr_to_operand_with_hint(condition, Some(&condition_type));
        let pre_if_bindings = self.bindings.clone();
        let cond_block_id = self.current_block;

        let then_block = self.new_block("then");
        let else_block = self.new_block("else");
        let merge_block = self.new_block("merge");
        let mut exit_value = None;

        self.current_block = cond_block_id;
        self.terminate(MirTerminator::Branch { condition: cond_val, then_target: then_block, else_target: else_block });
        self.flush_block("cond");

        self.current_block = then_block;
        self.bindings = pre_if_bindings.clone();
        let then_result = self.lower_branch_block_value_with_hint(then_branch, expected_type);
        let then_returns = self.terminator.is_some();
        if self.terminator.is_none() {
            let ty = expected_type.cloned().or_else(|| infer_builder_operand_type(&then_result, &self.value_types));
            self.ensure_branch_exit_parameter(merge_block, &mut exit_value, ty);
            self.terminate(MirTerminator::Jump { target: merge_block, arguments: vec![then_result] });
        }
        self.flush_block("then");

        self.current_block = else_block;
        self.bindings = pre_if_bindings.clone();
        let else_returns = if let Some(else_body) = else_branch {
            let result = self.lower_branch_block_value_with_hint(else_body, expected_type);
            let returns = self.terminator.is_some();
            if self.terminator.is_none() {
                let ty = expected_type.cloned().or_else(|| infer_builder_operand_type(&result, &self.value_types));
                self.ensure_branch_exit_parameter(merge_block, &mut exit_value, ty);
                self.terminate(MirTerminator::Jump { target: merge_block, arguments: vec![result] });
            }
            returns
        }
        else {
            if self.terminator.is_none() {
                let arguments = if exit_value.is_some() { vec![MirOperand::Constant(MirConstant::Unit)] } else { Vec::new() };
                self.terminate(MirTerminator::Jump { target: merge_block, arguments });
            }
            false
        };
        self.flush_block("else");

        self.current_block = merge_block;
        self.bindings = pre_if_bindings;
        if then_returns && else_returns {
            self.terminate(MirTerminator::Unreachable);
        }

        exit_value.map(MirOperand::Value).unwrap_or(MirOperand::Constant(MirConstant::Unit))
    }

    pub(super) fn lower_if_let_expr(
        &mut self,
        pattern: &crate::hir::HirPattern,
        scrutinee: &HirExpr,
        then_branch: &crate::hir::HirBlock,
        else_branch: &Option<Box<crate::hir::HirBlock>>,
    ) -> MirOperand {
        let scrutinee_operand = self.lower_expr_to_operand(scrutinee);
        let (match_cond, payload) = self.lower_pattern_match_probe(pattern, scrutinee_operand.clone());
        let pre_bindings = self.bindings.clone();
        let cond_block_id = self.current_block;

        let then_block = self.new_block("if_let_then");
        let else_block = self.new_block("if_let_else");
        let merge_block = self.new_block("if_let_merge");
        let mut exit_value = None;

        self.current_block = cond_block_id;
        self.terminate(MirTerminator::Branch { condition: match_cond, then_target: then_block, else_target: else_block });
        self.flush_block("if_let_cond");

        self.current_block = then_block;
        self.bindings = pre_bindings.clone();
        self.bind_pattern_from_operand_with_payload(pattern, scrutinee_operand, None, payload);
        let then_result = self.lower_branch_block_value(then_branch);
        if self.terminator.is_none() {
            let ty = infer_builder_operand_type(&then_result, &self.value_types);
            self.ensure_branch_exit_parameter(merge_block, &mut exit_value, ty);
            self.terminate(MirTerminator::Jump { target: merge_block, arguments: vec![then_result] });
        }
        self.flush_block("if_let_then");

        self.current_block = else_block;
        self.bindings = pre_bindings.clone();
        if let Some(else_body) = else_branch {
            let else_result = self.lower_branch_block_value(else_body);
            if self.terminator.is_none() {
                let ty = infer_builder_operand_type(&else_result, &self.value_types);
                self.ensure_branch_exit_parameter(merge_block, &mut exit_value, ty);
                self.terminate(MirTerminator::Jump { target: merge_block, arguments: vec![else_result] });
            }
        }
        else if self.terminator.is_none() {
            let arguments = if exit_value.is_some() { vec![MirOperand::Constant(MirConstant::Unit)] } else { Vec::new() };
            self.terminate(MirTerminator::Jump { target: merge_block, arguments });
        }
        self.flush_block("if_let_else");

        self.current_block = merge_block;
        self.bindings = pre_bindings;
        exit_value.map(MirOperand::Value).unwrap_or(MirOperand::Constant(MirConstant::Unit))
    }

    pub(super) fn lower_block_expr(&mut self, body: &crate::hir::HirBlock) -> MirOperand {
        let saved_bindings = self.bindings.clone();
        let saved_static_bindings = self.static_bindings.clone();

        for statement in &body.statements {
            self.lower_statement(statement);
            if self.terminator.is_some() {
                break;
            }
        }

        let result = if self.terminator.is_none() {
            body.expr.as_ref().map(|expr| self.lower_expr_to_operand(expr)).unwrap_or(MirOperand::Constant(MirConstant::Unit))
        }
        else {
            MirOperand::Constant(MirConstant::Unit)
        };

        self.bindings = saved_bindings;
        self.static_bindings = saved_static_bindings;
        result
    }

    pub(super) fn lower_loop_expr(
        &mut self,
        label: &Option<crate::types::Identifier>,
        pattern: &Option<crate::hir::HirPattern>,
        iterator: &Option<Box<HirExpr>>,
        condition: &Option<Box<HirExpr>>,
        body: &crate::hir::HirBlock,
    ) -> MirOperand {
        if let (Some(loop_pattern), Some(iterator_expr)) = (pattern.as_ref(), iterator.as_deref()) {
            if let Some(iteration_items) = self.resolve_static_iterable_items(iterator_expr) {
                for item in iteration_items {
                    self.bind_pattern_from_expr(loop_pattern, &item, None);
                    for statement in &body.statements {
                        self.lower_statement(statement);
                        if self.terminator.is_some() {
                            break;
                        }
                    }
                    if self.terminator.is_some() {
                        break;
                    }
                    if let Some(tail) = &body.expr {
                        let _ = self.lower_expr_to_operand(tail);
                    }
                    if self.terminator.is_some() {
                        break;
                    }
                }
                return MirOperand::Constant(MirConstant::Unit);
            }
            // Irrefutable `loop pat in coll` (for-in) vs refutable `while let pat = scrutinee`.
            // For-in must index the collection; matching the whole coll once + `while true` livelocks.
            if !loop_pattern.refutability().is_refutable() {
                return self.lower_for_in_as_indexed_while(label, loop_pattern, iterator_expr, condition, body);
            }
        }

        let pre_loop_bindings = self.bindings.clone();
        let outer_block_id = self.current_block;
        let loop_header_id = super::MirBlockRef(self.blocks.len() as u32);

        let outer_label = self.current_label.clone();
        self.terminate(MirTerminator::Jump { target: loop_header_id, arguments: Vec::new() });
        self.flush_block(&outer_label);
        self.terminator = None;

        self.new_block("loop_header");
        let loop_body_id = self.new_block("loop_body");
        let loop_exit_id = self.new_block("loop_exit");

        self.control_flow.push_temp_loop(MirLoopContext {
            header: loop_header_id,
            exit: loop_exit_id,
            exit_value: None,
            exit_reached_by_break: false,
            carried_values: Vec::new(),
            carried_value_refs: std::collections::BTreeMap::new(),
        });
        self.current_block = loop_body_id;
        self.current_label = "loop_body".to_string();
        self.instructions.clear();
        self.terminator = None;
        self.bindings = pre_loop_bindings.clone();

        for statement in &body.statements {
            self.lower_statement(statement);
            if self.terminator.is_some() {
                break;
            }
        }
        if let Some(tail) = &body.expr {
            let _ = self.lower_expr_to_operand(tail);
        }

        if self.terminator.is_none() {
            if self.current_label.starts_with("after_") {
                self.terminate(MirTerminator::Unreachable);
            }
            else {
                self.terminate(MirTerminator::Jump { target: loop_header_id, arguments: Vec::new() });
            }
        }
        let first_pass_body_label = self.current_label.clone();
        self.flush_block(&first_pass_body_label);
        self.terminator = None;

        let carried_bindings_after = self.bindings.clone();
        let mut carried_names: Vec<String> =
            pre_loop_bindings.keys().filter(|name| carried_bindings_after.get(*name) != pre_loop_bindings.get(*name)).cloned().collect();
        // `if` / `match` arms restore `bindings` at merge, so mutations that only happen
        // inside arms look unchanged after the first pass. Still treat those StoreVar
        // targets as loop-carried, or continue/back-edges pass empty args and the header
        // keeps reading the pre-loop SSA locals (CLR: index stuck at 0 → NRE / livelock).
        {
            let mut seen: std::collections::BTreeSet<String> = carried_names.iter().cloned().collect();
            for block in &self.blocks {
                if block.id.0 < loop_body_id.0 {
                    continue;
                }
                if block.id == loop_exit_id {
                    continue;
                }
                for instruction in &block.instructions {
                    if let super::MirOperation::StoreVar { name, .. } = &instruction.kind {
                        if pre_loop_bindings.contains_key(name) && seen.insert(name.clone()) {
                            carried_names.push(name.clone());
                        }
                    }
                }
            }
        }

        let _temp_context = self.control_flow.pop_temp_loop();

        // First-pass body may have created `after_*` / merge blocks that jump back to the
        // header with empty args. Those orphans survive the second pass and fail SMIR007
        // (`jump arity differs`). Keep only header/body/exit shells before rebuilding.
        let keep_through = loop_exit_id.0 as usize + 1;
        if self.blocks.len() > keep_through {
            self.blocks.truncate(keep_through);
        }
        if let Some(loop_body) = self.blocks.get_mut(loop_body_id.0 as usize) {
            loop_body.instructions.clear();
            loop_body.terminator = MirTerminator::Unreachable;
        }

        let mut carried_value_refs: std::collections::BTreeMap<String, super::MirValueRef> = std::collections::BTreeMap::new();
        for name in &carried_names {
            let ty =
                pre_loop_bindings.get(name).and_then(|op| if let MirOperand::Value(v) = op { self.value_types.get(v).cloned() } else { None });
            let value = self.next_value(super::MirValueOrigin::BlockParameter { block: loop_header_id, name: name.to_string() });
            if let Some(ty) = ty {
                self.value_types.insert(value, ty);
            }
            self.blocks[loop_header_id.0 as usize].parameters.push(value);
            carried_value_refs.insert(name.to_string(), value);
        }

        let carried_args: Vec<MirOperand> = carried_names
            .iter()
            .map(|name| pre_loop_bindings.get(name).cloned().expect("loop-carried name must exist before the loop"))
            .collect();
        if let Some(outer_block) = self.blocks.get(outer_block_id.0 as usize) {
            if matches!(outer_block.terminator, MirTerminator::Jump { target, .. } if target == loop_header_id) {
                self.blocks[outer_block_id.0 as usize].terminator =
                    MirTerminator::Jump { target: loop_header_id, arguments: carried_args.clone() };
            }
        }

        self.current_block = loop_header_id;
        self.current_label = "loop_header".to_string();
        self.instructions.clear();
        self.terminator = None;
        self.bindings = pre_loop_bindings.clone();
        for name in &carried_names {
            if let Some(&param) = carried_value_refs.get(name) {
                self.bindings.insert(name.clone(), MirOperand::Value(param));
            }
        }

        let mut loop_pattern_bindings = std::collections::BTreeMap::new();
        if let (Some(loop_pattern), Some(iterator_expr)) = (pattern.as_ref(), iterator.as_deref()) {
            let scrutinee_operand = self.lower_expr_to_operand(iterator_expr);
            let (match_cond, payload) = self.lower_pattern_match_probe(loop_pattern, scrutinee_operand.clone());
            let loop_pat_bind_id = super::MirBlockRef(self.blocks.len() as u32);
            self.terminate(MirTerminator::Branch { condition: match_cond, then_target: loop_pat_bind_id, else_target: loop_exit_id });
            self.flush_block("loop_header");
            self.terminator = None;
            self.new_block("loop_pat_bind");

            self.instructions.clear();
            self.bindings = pre_loop_bindings.clone();
            for name in &carried_names {
                if let Some(&param) = carried_value_refs.get(name) {
                    self.bindings.insert(name.clone(), MirOperand::Value(param));
                }
            }
            let element_ty = Self::loop_bind_type_hint(&scrutinee_operand, &self.value_types);
            self.bind_pattern_from_operand_with_payload(loop_pattern, scrutinee_operand, element_ty, payload);
            loop_pattern_bindings = self
                .bindings
                .iter()
                .filter(|(name, operand)| pre_loop_bindings.get(name.as_str()) != Some(operand))
                .map(|(name, operand)| (name.clone(), operand.clone()))
                .collect();

            let cond_val = if let Some(condition) = condition {
                self.lower_expr_to_operand(condition)
            }
            else {
                MirOperand::Constant(MirConstant::Bool(true))
            };
            self.terminate(MirTerminator::Branch { condition: cond_val, then_target: loop_body_id, else_target: loop_exit_id });
            self.flush_block("loop_pat_bind");
            self.terminator = None;
        }
        else {
            let cond_val = self.lower_expr_to_operand(condition.as_ref().unwrap_or(&Box::new(HirExpr {
                kind: HirExprKind::Literal(HirLiteral::Bool(true)),
                span: SourceSpan::new(SourceID::default(), 0, 0),
            })));
            self.terminate(MirTerminator::Branch { condition: cond_val, then_target: loop_body_id, else_target: loop_exit_id });
            self.flush_block("loop_header");
            self.terminator = None;
        }

        self.control_flow.push_loop(
            label.as_ref().map(|value| value.to_string()),
            MirLoopContext {
                header: loop_header_id,
                exit: loop_exit_id,
                exit_value: None,
                exit_reached_by_break: false,
                carried_values: carried_names.clone(),
                carried_value_refs: carried_value_refs.clone(),
            },
        );
        self.current_block = loop_body_id;
        self.current_label = "loop_body".to_string();
        if let Some(loop_body) = self.blocks.get_mut(loop_body_id.0 as usize) {
            loop_body.instructions.clear();
            loop_body.terminator = super::MirTerminator::Unreachable;
        }
        self.instructions.clear();
        self.terminator = None;
        self.bindings = pre_loop_bindings.clone();
        for name in &carried_names {
            if let Some(&param) = carried_value_refs.get(name) {
                self.bindings.insert(name.clone(), MirOperand::Value(param));
            }
        }
        for (name, operand) in &loop_pattern_bindings {
            self.bindings.insert(name.clone(), operand.clone());
        }

        for statement in &body.statements {
            self.lower_statement(statement);
        }
        if let Some(tail) = &body.expr {
            let _ = self.lower_expr_to_operand(tail);
        }

        let loop_context = self.control_flow.pop_loop();

        if self.terminator.is_none() {
            if self.current_label.starts_with("after_") {
                self.terminate(MirTerminator::Unreachable);
            }
            else {
                let continue_args: Vec<MirOperand> = carried_names
                    .iter()
                    .map(|name| {
                        self.bindings
                            .get(name)
                            .cloned()
                            .or_else(|| pre_loop_bindings.get(name).cloned())
                            .expect("loop-carried binding must remain available at continue")
                    })
                    .collect();
                self.terminate(MirTerminator::Jump { target: loop_header_id, arguments: continue_args });
            }
        }
        let second_pass_body_label = self.current_label.clone();
        self.flush_block(&second_pass_body_label);
        self.terminator = None;

        // Final safety: every jump into the header must match parameter arity.
        let header_arity = self.blocks[loop_header_id.0 as usize].parameters.len();
        for block in &mut self.blocks {
            if let MirTerminator::Jump { target, arguments } = &mut block.terminator {
                if *target == loop_header_id && arguments.len() != header_arity {
                    *arguments = carried_names
                        .iter()
                        .map(|name| pre_loop_bindings.get(name).cloned().expect("loop-carried name must exist before the loop"))
                        .collect();
                }
            }
        }

        self.current_block = loop_exit_id;
        self.current_label = "loop_exit".to_string();
        self.instructions.clear();
        self.terminator = None;
        // After exit, rebind carried names to **header parameters** (not the body's latest
        // StoreVar SSA temps). Header params share the same CLR local as StoreVar once slots
        // unify them, so post-loop loads see the last write. Pointing at StoreVar outputs
        // breaks emit when `loop_exit` is lowered before the body (silent empty `ret` →
        // InvalidProgram). Nested stale reads are fixed by that local unify, not by swapping
        // the MIR binding to a temp that may lack a planned local yet.
        self.bindings = pre_loop_bindings;
        for name in &carried_names {
            if let Some(&param) = carried_value_refs.get(name) {
                self.bindings.insert(name.clone(), MirOperand::Value(param));
            }
        }

        // `while true` / bare `loop` with no `break` never reaches loop_exit; only
        // in-body `return` leaves. Leave Unreachable so the function epilogue does not
        // wrap the loop's Unit fallthrough as Return(Unit) (SMIR007 on non-Unit fns,
        // e.g. Utf8Text.split).
        let condition_is_constant_true = match condition {
            None => true,
            Some(cond) => matches!(&cond.kind, HirExprKind::Literal(HirLiteral::Bool(true))),
        };
        let infinite_no_break = pattern.is_none() && condition_is_constant_true && !loop_context.exit_reached_by_break;
        if infinite_no_break {
            self.terminate(MirTerminator::Unreachable);
        }

        loop_context.exit_value.map(MirOperand::Value).unwrap_or(MirOperand::Constant(MirConstant::Unit))
    }

    pub(super) fn lower_break_expr(&mut self, label: &Option<crate::types::Identifier>, expr: &Option<Box<HirExpr>>) -> MirOperand {
        if let Some(loop_index) = self.resolve_loop_index(label.as_ref()) {
            self.control_flow.loop_at_mut(loop_index).exit_reached_by_break = true;
            let exit = self.control_flow.loop_at(loop_index).exit;
            let arguments = if let Some(expr) = expr.as_deref() {
                let operand = self.lower_expr_to_operand(expr);
                let ty = infer_builder_operand_type(&operand, &self.value_types);
                let _ = self.ensure_loop_exit_parameter(loop_index, ty);
                vec![operand]
            }
            else if self.control_flow.loop_at(loop_index).exit_value.is_some() {
                vec![MirOperand::Constant(MirConstant::Unit)]
            }
            else {
                Vec::new()
            };
            self.terminate(MirTerminator::Jump { target: exit, arguments });
        }
        else {
            self.terminate(MirTerminator::Unreachable);
        }
        MirOperand::Constant(MirConstant::Unit)
    }

    pub(super) fn lower_continue_expr(&mut self, label: &Option<crate::types::Identifier>) -> MirOperand {
        if let Some(loop_index) = self.resolve_loop_index(label.as_ref()) {
            let loop_context = self.control_flow.loop_at(loop_index).clone();
            let MirLoopContext { header, carried_values, .. } = loop_context;
            let arguments: Vec<MirOperand> = carried_values.iter().filter_map(|name| self.bindings.get(name).cloned()).collect();
            self.terminate(MirTerminator::Jump { target: header, arguments });
        }
        else {
            self.terminate(MirTerminator::Unreachable);
        }
        MirOperand::Constant(MirConstant::Unit)
    }

    fn lower_branch_block_value(&mut self, body: &crate::hir::HirBlock) -> MirOperand {
        self.lower_branch_block_value_with_hint(body, None)
    }

    fn lower_branch_block_value_with_hint(&mut self, body: &crate::hir::HirBlock, expected_type: Option<&ValkyrieType>) -> MirOperand {
        for statement in &body.statements {
            self.lower_statement(statement);
            if self.terminator.is_some() {
                break;
            }
        }
        if self.terminator.is_some() {
            return MirOperand::Constant(MirConstant::Unit);
        }
        body.expr
            .as_ref()
            .map(|expr| self.lower_expr_to_operand_with_hint(expr, expected_type))
            .unwrap_or(MirOperand::Constant(MirConstant::Unit))
    }

    pub(in crate::valkyrie::mir::ssa) fn ensure_branch_exit_parameter(
        &mut self,
        exit_block: super::MirBlockRef,
        exit_value: &mut Option<super::MirValueRef>,
        ty: Option<crate::hir::ValkyrieType>,
    ) {
        if exit_value.is_some() {
            if let (Some(value), Some(ty)) = (*exit_value, ty) {
                self.value_types.entry(value).or_insert(ty);
            }
            return;
        }
        let value = self.ensure_block_parameter(exit_block, "branch_result", ty);
        *exit_value = Some(value);
    }

    /// `loop item in iterable` 目前仍绑定整个 iterable 操作数；为字段访问与后端 owner 解析补上元素类型提示。
    fn loop_bind_type_hint(
        operand: &MirOperand,
        value_types: &std::collections::BTreeMap<super::MirValueRef, ValkyrieType>,
    ) -> Option<ValkyrieType> {
        infer_builder_operand_type(operand, value_types).and_then(|ty| match ty {
            ValkyrieType::Array(inner) => Some(*inner),
            ValkyrieType::FixedArray { element, .. } => Some(*element),
            _ => None,
        })
    }
}
