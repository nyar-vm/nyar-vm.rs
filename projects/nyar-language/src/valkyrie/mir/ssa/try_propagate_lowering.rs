use crate::types::{
    Identifier, NamePath,
    hir::{HirExpr, HirPattern, ValkyrieType},
};

use super::{
    MirBuilder, MirConstant, MirInstruction, MirOperand, MirOperation, MirStorageKind, MirTerminator, MirValueOrigin,
    infer_builder_operand_type, value_semantics::storage_kind_for_named_type,
};
use crate::hir::{is_nullable_type, nullable_payload_type};

impl MirBuilder {
    pub(super) fn lower_try_propagate_expr(&mut self, expr: &HirExpr) -> MirOperand {
        let value = self.lower_expr_to_operand(expr);
        let operand_type = infer_builder_operand_type(&value, &self.value_types);
        if matches!(
            operand_type.as_ref(),
            Some(ValkyrieType::Apply(base, _))
                if matches!(base.as_ref(), ValkyrieType::Named(name) if name.as_str() == "Result")
        ) {
            return self.lower_result_try_propagate(value);
        }
        if matches!(
            operand_type.as_ref(),
            Some(ValkyrieType::Apply(base, _))
                if matches!(base.as_ref(), ValkyrieType::Named(name) if name.as_str() == "Option")
        ) {
            return self.lower_option_try_propagate(value);
        }
        self.lower_nullable_try_propagate(value)
    }

    fn lower_nullable_try_propagate(&mut self, value: MirOperand) -> MirOperand {
        let is_null = self.lower_static_call("is_null", vec![value.clone()], MirValueOrigin::Temporary);
        self.value_types.insert(is_null, ValkyrieType::Boolean);
        let branch_block = self.current_block;
        let branch_label = self.current_label.clone();
        let early_exit_block = self.new_block("try_propagate_early_exit");
        let continue_block = self.new_block("try_propagate_ok");
        self.current_block = branch_block;
        self.current_label = branch_label.clone();
        self.terminate(MirTerminator::Branch {
            condition: MirOperand::Value(is_null),
            then_target: early_exit_block,
            else_target: continue_block,
        });
        self.flush_block(&branch_label);

        self.current_block = early_exit_block;
        self.current_label = "try_propagate_early_exit".to_string();
        self.instructions.clear();
        self.terminator = None;
        self.terminate_nullable_try_early_exit(value.clone());
        self.flush_block("try_propagate_early_exit");

        self.current_block = continue_block;
        self.current_label = "try_propagate_ok".to_string();
        self.instructions.clear();
        self.terminator = None;
        let payload_type =
            infer_builder_operand_type(&value, &self.value_types).and_then(|ty| nullable_payload_type(&ty)).unwrap_or(ValkyrieType::Unit);
        self.lower_nullable_payload_operand(value, &payload_type)
    }

    fn lower_option_try_propagate(&mut self, value: MirOperand) -> MirOperand {
        let some_pattern = HirPattern::Name(NamePath::new(vec![Identifier::new("Some")]));
        let matched = self.lower_pattern_match_operand(&some_pattern, value.clone());
        let branch_block = self.current_block;
        let branch_label = self.current_label.clone();
        let return_block = self.new_block("try_propagate_none");
        let continue_block = self.new_block("try_propagate_some");
        self.current_block = branch_block;
        self.current_label = branch_label.clone();
        self.terminate(MirTerminator::Branch { condition: matched, then_target: continue_block, else_target: return_block });
        self.flush_block(&branch_label);

        self.current_block = return_block;
        self.current_label = "try_propagate_none".to_string();
        self.instructions.clear();
        self.terminator = None;
        self.terminate_function_exit(super::exit_lowering::FunctionExitKind::Return(value.clone()));
        self.flush_block("try_propagate_none");

        self.current_block = continue_block;
        self.current_label = "try_propagate_some".to_string();
        self.instructions.clear();
        self.terminator = None;
        self.lower_option_some_payload(value)
    }

    fn lower_result_try_propagate(&mut self, value: MirOperand) -> MirOperand {
        let fine_pattern = HirPattern::Name(NamePath::new(vec![Identifier::new("Fine")]));
        let matched = self.lower_pattern_match_operand(&fine_pattern, value.clone());
        let branch_block = self.current_block;
        let branch_label = self.current_label.clone();
        let return_block = self.new_block("try_propagate_fail");
        let continue_block = self.new_block("try_propagate_fine");
        self.current_block = branch_block;
        self.current_label = branch_label.clone();
        self.terminate(MirTerminator::Branch { condition: matched, then_target: continue_block, else_target: return_block });
        self.flush_block(&branch_label);

        self.current_block = return_block;
        self.current_label = "try_propagate_fail".to_string();
        self.instructions.clear();
        self.terminator = None;
        self.terminate_function_exit(super::exit_lowering::FunctionExitKind::Return(value.clone()));
        self.flush_block("try_propagate_fail");

        self.current_block = continue_block;
        self.current_label = "try_propagate_fine".to_string();
        self.instructions.clear();
        self.terminator = None;
        self.lower_result_fine_payload(value)
    }

    fn lower_nullable_payload_operand(&mut self, nullable: MirOperand, payload_type: &ValkyrieType) -> MirOperand {
        let output = self.lower_static_call("unwrap_null", vec![nullable.clone()], MirValueOrigin::Temporary);
        self.value_types.insert(output, payload_type.clone());
        MirOperand::Value(output)
    }

    fn lower_option_some_payload(&mut self, value: MirOperand) -> MirOperand {
        self.lower_sum_payload(value, "Option", "Some")
    }

    fn lower_result_fine_payload(&mut self, value: MirOperand) -> MirOperand {
        self.lower_sum_payload(value, "Result", "Fine")
    }

    /// Emit an explicit nominal-sum payload extraction. The source-language
    /// producer supplies the resolved sum/variant identity; backends must never
    /// reconstruct it from `value`, `error`, `Fine`, `Some`, or a physical
    /// object carrier.
    fn lower_sum_payload(&mut self, value: MirOperand, sum_type: &str, variant: &str) -> MirOperand {
        let object_ty = infer_builder_operand_type(&value, &self.value_types);
        let payload_type = object_ty.as_ref().and_then(|ty| match ty {
            ValkyrieType::Apply(base, args) if matches!(base.as_ref(), ValkyrieType::Named(name) if name.as_str() == sum_type) => {
                args.first().cloned()
            }
            _ => None,
        });
        let Some(payload_type) = payload_type
        else {
            // The HIR try-propagate validator guarantees this shape. A direct
            // internal caller that violates it is not representable in
            // Semantic MIR and must fail closed, not synthesize a Unit value.
            panic!("try-propagate sum payload has no resolved payload type");
        };
        let type_args = object_ty.as_ref().map(super::expr_lowering::type_args_from_sum_shaped).unwrap_or_default();
        let output = self.next_value(MirValueOrigin::Temporary);
        self.instructions.push(MirInstruction::from_operation(MirOperation::SumPayloadGet {
            sum_type: sum_type.to_string(),
            type_args,
            variant: variant.to_string(),
            payload_type: payload_type.clone(),
            object: value,
        }));
        self.value_types.insert(output, payload_type);
        MirOperand::Value(output)
    }

    pub(super) fn lower_anonymous_class_expr(
        &mut self,
        class_name: &Identifier,
        fields: &[(Identifier, Box<HirExpr>)],
        captures: &[crate::types::hir::HirCapture],
    ) -> MirOperand {
        let mut struct_fields = Vec::with_capacity(captures.len() + fields.len());
        for capture in captures {
            let field_name = format!("__cap_{}", capture.identifier.name.as_str());
            let capture_operand =
                self.bindings.get(capture.identifier.name.as_str()).cloned().unwrap_or(MirOperand::Constant(MirConstant::Unit));
            struct_fields.push((field_name, capture_operand));
        }
        for (name, init) in fields {
            struct_fields.push((name.to_string(), self.lower_expr_to_operand(init)));
        }
        let value = self.next_value(MirValueOrigin::Temporary);
        let storage = storage_kind_for_named_type(&class_name.to_string(), &self.struct_is_value_type);
        let layout_id = self.aggregate_layouts.type_name_to_layout.get(class_name.as_str()).copied();
        self.instructions
            .push(MirInstruction::from_operation(MirOperation::StructNew { type_name: class_name.to_string(), fields: struct_fields }));
        self.value_types.insert(value, ValkyrieType::Named(class_name.clone()));
        MirOperand::Value(value)
    }
}
