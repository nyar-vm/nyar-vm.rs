use crate::types::hir::HirMatchArm;

use super::{
    MirBlockRef, MirBuilder, MirCaseArm, MirCaseChain, MirConstant, MirOperand, MirTerminator, control_flow_context::MirFallthroughContext,
    infer_builder_operand_type,
};

impl MirBuilder {
    pub(super) fn lower_match_expr(&mut self, scrutinee: &crate::hir::HirExpr, arms: &[HirMatchArm]) -> MirOperand {
        self.lower_case_like_expr(scrutinee, arms, true)
    }

    pub(super) fn lower_case_expr(&mut self, scrutinee: &crate::hir::HirExpr, arms: &[HirMatchArm]) -> MirOperand {
        self.lower_case_like_expr(scrutinee, arms, false)
    }

    fn lower_case_like_expr(&mut self, scrutinee: &crate::hir::HirExpr, arms: &[HirMatchArm], produce_value: bool) -> MirOperand {
        let scrutinee_operand = self.lower_expr_to_operand(scrutinee);
        if arms.is_empty() {
            return MirOperand::Constant(MirConstant::Unit);
        }

        let entry_block = self.current_block;
        let entry_label = self.current_label.clone();
        let entry_instructions = self.instructions.clone();
        let entry_terminator = self.terminator.clone();

        let arm_blocks: Vec<MirBlockRef> = arms.iter().enumerate().map(|(index, _)| self.new_block(&format!("match_arm_{index}"))).collect();
        let no_match_block = self.new_block("match_no_match");
        let exit_block = self.new_block("match_exit");

        self.current_block = entry_block;
        self.current_label = entry_label.clone();
        self.instructions = entry_instructions;
        self.terminator = entry_terminator;
        self.terminate(MirTerminator::Jump { target: arm_blocks[0], arguments: Vec::new() });
        self.flush_block(&entry_label);

        let saved_bindings = self.bindings.clone();
        let saved_static_bindings = self.static_bindings.clone();
        let mut exit_value = None;
        let exit_label = if produce_value { "match_exit" } else { "case_exit" };
        let no_match_label = if produce_value { "match_no_match" } else { "case_no_match" };
        let mut chain_arms = Vec::with_capacity(arms.len());

        for (index, arm) in arms.iter().enumerate() {
            let arm_block = arm_blocks[index];
            let next_target = arm_blocks.get(index + 1).copied().unwrap_or(no_match_block);
            let arm_label = if produce_value { format!("match_arm_{index}") } else { format!("case_arm_{index}") };
            let needs_pattern_check =
                !matches!(arm.pattern, crate::hir::HirPattern::Wildcard | crate::hir::HirPattern::Variable(_) | crate::hir::HirPattern::Else);
            let mut check_block = None;
            let mut guard_block = None;
            let mut extractor_payload = None;

            self.current_block = arm_block;
            self.current_label = arm_label.clone();
            self.instructions.clear();
            self.terminator = None;
            self.bindings = saved_bindings.clone();
            self.static_bindings = saved_static_bindings.clone();

            if needs_pattern_check {
                let body_or_guard_block = self.new_block(&format!("{arm_label}_check"));
                check_block = Some(body_or_guard_block);
                self.current_block = arm_block;
                self.current_label = arm_label.clone();
                self.instructions.clear();
                self.terminator = None;
                self.bindings = saved_bindings.clone();
                self.static_bindings = saved_static_bindings.clone();
                let (matched, payload) = self.lower_pattern_match_probe(&arm.pattern, scrutinee_operand.clone());
                extractor_payload = payload;
                self.terminate(MirTerminator::Branch { condition: matched, then_target: body_or_guard_block, else_target: next_target });
                self.flush_block(&arm_label);
                self.current_block = body_or_guard_block;
                self.current_label = format!("{arm_label}_check");
                self.instructions.clear();
                self.terminator = None;
                self.bindings = saved_bindings.clone();
                self.static_bindings = saved_static_bindings.clone();
            }

            if let Some(guard) = &arm.guard {
                let current_guard_block = self.current_block;
                guard_block = Some(current_guard_block);
                let guard_label = self.current_label.clone();
                let body_block = self.new_block(&format!("{arm_label}_body"));
                self.current_block = current_guard_block;
                self.current_label = guard_label;
                self.instructions.clear();
                self.terminator = None;
                self.bindings = saved_bindings.clone();
                self.static_bindings = saved_static_bindings.clone();
                self.bind_pattern_from_operand_with_payload(&arm.pattern, scrutinee_operand.clone(), None, extractor_payload.clone());
                let guard_value = self.lower_expr_to_operand(guard);
                self.terminate(MirTerminator::Branch { condition: guard_value, then_target: body_block, else_target: next_target });
                let flushed_guard_label = self.current_label.clone();
                self.flush_block(&flushed_guard_label);
                self.current_block = body_block;
                self.current_label = format!("{arm_label}_body");
                self.instructions.clear();
                self.terminator = None;
            }

            let body_block = self.current_block;
            self.bind_pattern_from_operand_with_payload(&arm.pattern, scrutinee_operand.clone(), None, extractor_payload.clone());
            self.control_flow.push_fallthrough(MirFallthroughContext { target: next_target });
            let arm_result = self.lower_expr_to_operand(&arm.body);
            let _ = self.control_flow.pop_fallthrough();
            if self.terminator.is_none() {
                if produce_value {
                    let ty = infer_builder_operand_type(&arm_result, &self.value_types);
                    let exit_parameter = self.ensure_match_exit_parameter(exit_block, &mut exit_value, ty);
                    debug_assert_eq!(self.blocks[exit_block.0 as usize].parameters.first().copied(), Some(exit_parameter));
                    self.terminate(MirTerminator::Jump { target: exit_block, arguments: vec![arm_result] });
                }
                else {
                    self.terminate(MirTerminator::Jump { target: exit_block, arguments: Vec::new() });
                }
            }
            let current_arm_label = self.current_label.clone();
            self.flush_block(&current_arm_label);
            chain_arms.push(MirCaseArm {
                entry_block: arm_block,
                check_block,
                guard_block,
                body_block,
                next_arm_target: next_target,
                exit_target: exit_block,
                fallthrough_target: arm_blocks.get(index + 1).copied(),
            });
        }

        self.current_block = no_match_block;
        self.current_label = no_match_label.to_string();
        self.instructions.clear();
        self.terminator = None;
        if produce_value {
            // Arms that all `return` never call `ensure_match_exit_parameter`, leaving
            // match_exit with 0 params. The no_match edge still used to jump with Unit
            // (arity 1) → SMIR007. Ensure the exit slot exists first; exhaustive matches
            // never take this edge at runtime.
            if exit_value.is_none() {
                self.ensure_match_exit_parameter(exit_block, &mut exit_value, Some(crate::hir::ValkyrieType::Unit));
            }
            self.terminate(MirTerminator::Jump { target: exit_block, arguments: vec![MirOperand::Constant(MirConstant::Unit)] });
        }
        else {
            self.terminate(MirTerminator::Jump { target: exit_block, arguments: Vec::new() });
        }
        self.flush_block(no_match_label);

        self.current_block = exit_block;
        self.current_label = exit_label.to_string();
        self.instructions.clear();
        self.terminator = None;
        self.case_chains.push(MirCaseChain {
            dispatch_block: entry_block,
            first_arm: arm_blocks[0],
            no_match_block,
            exit_block,
            produce_value,
            arms: chain_arms,
        });
        if produce_value {
            exit_value.map(MirOperand::Value).unwrap_or(MirOperand::Constant(MirConstant::Unit))
        }
        else {
            MirOperand::Constant(MirConstant::Unit)
        }
    }

    pub(super) fn lower_fallthrough_expr(&mut self) -> MirOperand {
        if let Some(context) = self.control_flow.current_fallthrough().cloned() {
            self.terminate(MirTerminator::Jump { target: context.target, arguments: Vec::new() });
            let label = self.current_label.clone();
            self.flush_block(&label);
            self.new_block("after_fallthrough");
            self.instructions.clear();
            self.terminator = Some(MirTerminator::Unreachable);
        }
        else {
            self.terminate(MirTerminator::Unreachable);
            let label = self.current_label.clone();
            self.flush_block(&label);
            self.new_block("after_invalid_fallthrough");
            self.instructions.clear();
            self.terminator = Some(MirTerminator::Unreachable);
        }
        MirOperand::Constant(MirConstant::Unit)
    }

    fn ensure_match_exit_parameter(
        &mut self,
        exit_block: MirBlockRef,
        exit_value: &mut Option<super::MirValueRef>,
        ty: Option<crate::hir::ValkyrieType>,
    ) -> super::MirValueRef {
        if let Some(value) = *exit_value {
            if let Some(ty) = ty {
                self.value_types.entry(value).or_insert(ty);
            }
            return value;
        }
        let value = self.ensure_block_parameter(exit_block, "match_result", ty);
        *exit_value = Some(value);
        value
    }
}
