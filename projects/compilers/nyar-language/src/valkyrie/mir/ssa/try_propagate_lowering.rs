use crate::types::{
    Identifier,
    hir::{HirExpr, HirResolvedCall, ValkyrieType},
};

use super::{
    MirBuilder, MirConstant, MirDiagnostic, MirInstruction, MirOperand, MirOperation, MirTerminator, MirValueOrigin, infer_builder_operand_type,
};
use crate::hir::nullable_payload_type;

impl MirBuilder {
    pub(super) fn lower_try_propagate_expr(&mut self, inner: &HirExpr, resolved_ok_variant: Option<&HirResolvedCall>) -> MirOperand {
        let value = self.lower_expr_to_operand(inner);
        if let Some(resolved) = resolved_ok_variant {
            if let Some((owner, variant)) = resolved.sum_owner.zip(resolved.sum_variant) {
                // 标签仅供块命名；身份与 payload 只认 sum_owner / sum_variant。
                let is_result = self.result_declaration == Some(owner);
                return self.lower_sum_try_propagate(value, owner, variant, is_result);
            }
        }
        let operand_type = infer_builder_operand_type(&value, &self.value_types);
        if matches!(operand_type.as_ref(), Some(ValkyrieType::Nullable(_))) {
            return self.lower_nullable_try_propagate(value);
        }
        self.diagnostics
            .push(MirDiagnostic::UnresolvedValueType { context: "try-propagate operand missing HIR-resolved ok variant facts".to_string() });
        MirOperand::Constant(MirConstant::Unit)
    }

    fn lower_nullable_try_propagate(&mut self, value: MirOperand) -> MirOperand {
        let is_null_operand = self.emit_null_is_null_operand(value.clone());
        let branch_block = self.current_block;
        let branch_label = self.current_label.clone();
        let early_exit_block = self.new_block("try_propagate_early_exit");
        let continue_block = self.new_block("try_propagate_ok");
        self.current_block = branch_block;
        self.current_label = branch_label.clone();
        self.terminate(MirTerminator::Branch { condition: is_null_operand, then_target: early_exit_block, else_target: continue_block });
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
        let Some(payload_type) = infer_builder_operand_type(&value, &self.value_types).and_then(|ty| nullable_payload_type(&ty))
        else {
            self.diagnostics.push(super::MirDiagnostic::UnresolvedValueType { context: "nullable try-propagate 载荷".to_string() });
            return MirOperand::Constant(MirConstant::Unit);
        };
        self.lower_nullable_payload_operand(value, &payload_type)
    }

    fn lower_sum_try_propagate(
        &mut self,
        value: MirOperand,
        owner: nyar_types::ItemId,
        variant: nyar_types::ItemId,
        is_result: bool,
    ) -> MirOperand {
        let Some(actual_type) = infer_builder_operand_type(&value, &self.value_types)
        else {
            return MirOperand::Constant(MirConstant::Unit);
        };
        let Some(matched) = self.emit_sum_variant_is_operand(value.clone(), owner, variant, &actual_type)
        else {
            return MirOperand::Constant(MirConstant::Unit);
        };
        let (early_exit_label, continue_label) =
            if is_result { ("try_propagate_fail", "try_propagate_fine") } else { ("try_propagate_none", "try_propagate_some") };
        let branch_block = self.current_block;
        let branch_label = self.current_label.clone();
        let return_block = self.new_block(early_exit_label);
        let continue_block = self.new_block(continue_label);
        self.current_block = branch_block;
        self.current_label = branch_label.clone();
        self.terminate(MirTerminator::Branch { condition: matched, then_target: continue_block, else_target: return_block });
        self.flush_block(&branch_label);

        self.current_block = return_block;
        self.current_label = early_exit_label.to_string();
        self.instructions.clear();
        self.terminator = None;
        self.terminate_function_exit(super::exit_lowering::FunctionExitKind::Return(value.clone()));
        self.flush_block(early_exit_label);

        self.current_block = continue_block;
        self.current_label = continue_label.to_string();
        self.instructions.clear();
        self.terminator = None;
        self.lower_sum_payload_by_declarations(value, owner, variant)
    }

    fn lower_nullable_payload_operand(&mut self, nullable: MirOperand, payload_type: &ValkyrieType) -> MirOperand {
        self.emit_null_unwrap_operand(nullable, payload_type.clone())
    }

    fn emit_sum_variant_is_operand(
        &mut self,
        object: MirOperand,
        owner: nyar_types::ItemId,
        variant: nyar_types::ItemId,
        actual_type: &ValkyrieType,
    ) -> Option<MirOperand> {
        let (nominal, variant_id) = self.sum_identity_by_declarations(owner, variant)?;
        let value = self.next_value(MirValueOrigin::Temporary);
        self.push_instruction(
            MirOperation::SumVariantIs {
                nominal,
                type_args: super::expr_lowering::type_args_from_sum_shaped(actual_type),
                variant: variant_id,
                object,
            },
            vec![value],
        );
        self.value_types.insert(value, ValkyrieType::Boolean);
        Some(MirOperand::Value(value))
    }

    /// Emit nominal-sum payload extraction from declaration identities attached to the
    /// lowered `sum_types` table. MIR must not call spelling-based `sum_identity`.
    fn lower_sum_payload_by_declarations(&mut self, value: MirOperand, owner: nyar_types::ItemId, variant: nyar_types::ItemId) -> MirOperand {
        let object_ty = infer_builder_operand_type(&value, &self.value_types);
        let sum = super::MirSumDeclaration::find_by_declaration(&self.sum_types, owner);
        let variant_decl = sum.and_then(|sum| sum.variants.iter().find(|item| item.declaration == Some(variant)));
        let payload_type = match (sum, variant_decl, object_ty.as_ref()) {
            (Some(sum), Some(variant_decl), Some(ty)) => {
                let args = super::expr_lowering::type_args_from_sum_shaped(ty);
                sum.instantiate_payload(variant_decl, &args).flatten()
            }
            _ => None,
        };
        let Some(payload_type) = payload_type
        else {
            panic!("try-propagate sum payload 缺少 sum_owner/sum_variant 代入结果");
        };
        let type_args = object_ty.as_ref().map(super::expr_lowering::type_args_from_sum_shaped).unwrap_or_default();
        let Some((nominal, variant_id)) = self.sum_identity_by_declarations(owner, variant)
        else {
            return MirOperand::Constant(MirConstant::Unit);
        };
        let output = self.next_value(MirValueOrigin::Temporary);
        self.instructions.push(MirInstruction::from_operation(MirOperation::SumPayloadGet {
            nominal,
            type_args,
            variant: variant_id,
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
        // 匿名类提升时按 captures → 声明字段的顺序生成 `HirStruct`，字段身份由 HIR 注册；
        // 此处只按同一顺序消费已冻结的 `FieldId`，不按名称重新解析。
        let mut matches = self
            .field_declarations
            .iter()
            .enumerate()
            .filter(|(_, item)| item.name == class_name.as_str() && item.declaration.is_some())
            .map(|(index, _)| index);
        let Some(index) = matches.next()
        else {
            self.diagnostics.push(super::MirDiagnostic::UnresolvedNominalIdentity { type_name: class_name.to_string() });
            return MirOperand::Constant(MirConstant::Unit);
        };
        if matches.next().is_some() {
            self.diagnostics.push(super::MirDiagnostic::UnresolvedNominalIdentity { type_name: class_name.to_string() });
            return MirOperand::Constant(MirConstant::Unit);
        }
        let structure = &self.field_declarations[index];
        let (Some(declaration), nominal) = (structure.declaration, structure.nominal)
        else {
            self.diagnostics.push(super::MirDiagnostic::UnresolvedNominalIdentity { type_name: class_name.to_string() });
            return MirOperand::Constant(MirConstant::Unit);
        };
        if structure.fields.len() != struct_fields.len() {
            self.diagnostics.push(super::MirDiagnostic::UnresolvedFieldIdentity { field: "try propagation aggregate".to_string() });
            return MirOperand::Constant(MirConstant::Unit);
        }
        let fields = structure.fields.iter().map(|field| field.id).zip(struct_fields.into_iter().map(|(_, value)| value)).collect::<Vec<_>>();
        self.instructions.push(MirInstruction::from_operation(MirOperation::StructNew { nominal, fields }));
        self.value_types.insert(value, ValkyrieType::Nominal { declaration, arguments: Vec::new() });
        MirOperand::Value(value)
    }
}
