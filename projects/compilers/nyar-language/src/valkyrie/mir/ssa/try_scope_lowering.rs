use crate::types::hir::{HirBlock, ValkyrieType};

use crate::valkyrie::control_flow::TryScopeData;

use super::{MirBuilder, MirOperand, MirTerminator, control_flow_context::MirTryScopeContext, infer_builder_operand_type};

impl MirBuilder {
    pub(super) fn lower_try_scope_expr(
        &mut self,
        is_optional: bool,
        is_forced: bool,
        result_type: &Option<ValkyrieType>,
        body: &HirBlock,
    ) -> MirOperand {
        let _ = is_forced;
        let entry_block = self.current_block;
        let entry_label = self.current_label.clone();

        let try_exit = self.new_block("try_exit");
        let mut exit_value = None;
        if is_optional || result_type.is_some() {
            let exit_type = result_type.clone().unwrap_or_else(|| ValkyrieType::Nullable(Box::new(ValkyrieType::Unit)));
            self.ensure_branch_exit_parameter(try_exit, &mut exit_value, Some(exit_type));
        }

        self.current_block = entry_block;
        self.current_label = entry_label.clone();
        self.instructions.clear();
        self.terminator = None;

        let pre_bindings = self.bindings.clone();
        self.control_flow.push_try(
            TryScopeData { is_optional, is_forced, result_type: result_type.clone() },
            MirTryScopeContext { exit: try_exit, exit_value },
        );

        let body_result = self.lower_block_expr(body);

        self.control_flow.pop_try();

        // 始终 flush 当前块：若 body 内的 `break`/`continue`/`return` 已经设置了 terminator，
        // 必须保留该 terminator 写入 entry 块，否则跳转会被静默丢弃（参见 lower_if_expr 的同款模式）。
        if self.terminator.is_none() {
            let arguments = if exit_value.is_some() { vec![body_result.clone()] } else { Vec::new() };
            self.terminate(MirTerminator::Jump { target: try_exit, arguments });
        }
        self.flush_block(&entry_label);

        self.current_block = try_exit;
        self.current_label = "try_exit".to_string();
        self.instructions.clear();
        self.terminator = None;
        self.bindings = pre_bindings;

        if let Some(value) = exit_value {
            if let Some(ty) = self.value_types.get(&value).cloned().or_else(|| infer_builder_operand_type(&body_result, &self.value_types)) {
                self.value_types.insert(value, ty);
            }
            MirOperand::Value(value)
        }
        else {
            body_result
        }
    }
}
