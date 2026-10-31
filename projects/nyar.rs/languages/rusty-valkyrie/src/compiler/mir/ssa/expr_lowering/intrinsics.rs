use crate::compiler::{
    hir::is_nullable_type,
    types::{
        Identifier, NamePath,
        hir::{FunctionType, HirCallableDomain, HirExpr, HirExprKind, HirLiteral, HirResolvedCall, ValkyrieType},
    },
};

use crate::compiler::mir::ssa::{
    MirBuilder, MirConstant, MirInstruction, MirOperand, MirOperation, MirStorageKind, MirValueOrigin, callee_name_matches,
    expr_helpers::lower_resolved_callee,
    infer_builder_operand_type, lower_callee_operand,
    value_semantics::{ensure_layout_for_type, layout_id_for_type, storage_kind_for_type},
};

use super::{
    apply_payload_to_sum_type_args, concretize_variant_constructor_types, instantiated_payload_for_call, refine_auto_apply_type,
    refine_auto_sum_instantiation_args, type_args_from_sum_shaped, unresolved_variant_labels,
};

impl MirBuilder {
    /// 所有 intrinsic lowering 分支统一降级为普通 Call。
    ///
    /// 核心合同：intrinsic 专用 opcode 只允许由 adaptor 链接进来的后端专用绑定产生；
    /// 源码 `[intrinsic(...)]` 自由函数、语言 builtin intrinsic 候选（只挂 intrinsic 身份）
    /// 一律走普通 Call 路径，让 planner adaptor / backend TargetExecutionPlan 链路消费。
    /// 当前没有 adaptor 链接进来的后端绑定，因此所有 intrinsic 调用直接 `return None`
    /// 让主 lowering 循环走 Call 分支。后续 adaptor 真能链接 opcode 时再在此收敛。
    pub(crate) fn try_lower_ref_deref_intrinsic(
        &mut self,
        _resolved: Option<&HirResolvedCall>,
        _callee: &MirOperand,
        _arguments: &[MirOperand],
    ) -> Option<MirOperand> {
        None
    }

    /// `T?` ???????? sum ??? `Nominal{ItemId}`?
    /// ??? `Named("Option")` ????? variant ???? `call.sum_owner`?
    pub(crate) fn is_option_type(&self, actual_type: &ValkyrieType) -> bool {
        match actual_type {
            ValkyrieType::Nullable(_) => true,
            ValkyrieType::Nominal { declaration, .. } => {
                crate::compiler::mir::ssa::MirSumDeclaration::find_by_declaration(&self.sum_types, *declaration).is_some()
            }
            _ => false,
        }
    }

    /// Some/Fine/Fail 短名命中但缺少 sum 身份时失败关闭，禁止回落 Invoke。
    pub(crate) fn fail_unresolved_sum_variant_call(&mut self, resolved: Option<&HirResolvedCall>) -> MirOperand {
        let (sum_type, variant) = unresolved_variant_labels(resolved);
        self.diagnostics.push(crate::compiler::mir::ssa::MirDiagnostic::UnresolvedVariantIdentity { sum_type, variant });
        MirOperand::Constant(MirConstant::Unit)
    }

    pub(crate) fn option_payload_type(actual_type: &ValkyrieType) -> Option<ValkyrieType> {
        match actual_type {
            ValkyrieType::Nullable(inner) => Some(*inner.clone()),
            ValkyrieType::Nominal { arguments, .. } => arguments.first().cloned(),
            ValkyrieType::Apply(_, args) => args.first().cloned(),
            _ => None,
        }
    }

    pub(crate) fn option_uses_generic_payload(actual_type: &ValkyrieType) -> bool {
        Self::option_payload_type(actual_type).is_some_and(|payload| matches!(payload, ValkyrieType::Named(name) if name.as_str() == "T"))
    }

    /// ??????sum declaration ItemId ??????????? `Named`??
    pub(crate) fn sum_apply_type(&self, declaration: nyar_types::ItemId, type_args: &[ValkyrieType]) -> Option<ValkyrieType> {
        let _ = crate::compiler::mir::ssa::MirSumDeclaration::find_by_declaration(&self.sum_types, declaration)?;
        Some(ValkyrieType::Nominal { declaration, arguments: type_args.to_vec() })
    }

    /// 仅从 impl 实参或 sum declaration 的 Generic binder 补全 Result 类型参数；禁止发明 `Named(T/E)`。
    pub(crate) fn pad_result_type_args(&self, type_args: Vec<ValkyrieType>) -> Vec<ValkyrieType> {
        if type_args.len() >= 2 {
            return type_args;
        }
        let error_arg = self.impl_type_arguments.get(1).cloned().or_else(|| {
            self.result_declaration.and_then(|declaration| {
                crate::compiler::mir::ssa::MirSumDeclaration::find_by_declaration(&self.sum_types, declaration)
                    .and_then(|sum| sum.generics.get(1).map(|generic| ValkyrieType::Generic(generic.clone())))
            })
        });
        match type_args.len() {
            0 => {
                if self.impl_type_arguments.len() >= 2 {
                    self.impl_type_arguments.clone()
                }
                else if let Some(declaration) = self.result_declaration {
                    crate::compiler::mir::ssa::MirSumDeclaration::find_by_declaration(&self.sum_types, declaration)
                        .map(|sum| sum.generics.iter().map(|generic| ValkyrieType::Generic(generic.clone())).collect())
                        .unwrap_or_default()
                }
                else {
                    Vec::new()
                }
            }
            1 => {
                if let Some(error) = error_arg {
                    vec![type_args[0].clone(), error]
                }
                else {
                    type_args
                }
            }
            _ => type_args,
        }
    }

    pub(crate) fn sum_instantiation_args(
        &self,
        declaration: nyar_types::ItemId,
        contextual: &ValkyrieType,
        explicit_type_args: &[ValkyrieType],
    ) -> Vec<ValkyrieType> {
        if !explicit_type_args.is_empty() {
            return explicit_type_args.to_vec();
        }
        let from_context = type_args_from_sum_shaped(contextual);
        if !from_context.is_empty() {
            return from_context;
        }
        crate::compiler::mir::ssa::MirSumDeclaration::find_by_declaration(&self.sum_types, declaration)
            .map(|sum| sum.generics.iter().map(|generic| ValkyrieType::Generic(generic.clone())).collect())
            .unwrap_or_default()
    }

    pub(crate) fn try_lower_option_is_some(&mut self, receiver: MirOperand, resolved: Option<&HirResolvedCall>) -> Option<MirOperand> {
        let call = resolved?;
        let (owner, variant_declaration) = call.sum_owner.zip(call.sum_variant)?;
        let actual_type = infer_builder_operand_type(&receiver, &self.value_types).filter(|ty| self.is_option_type(ty))?;
        let (nominal, variant) = self.sum_identity_by_declarations(owner, variant_declaration)?;
        let value = self.next_value(MirValueOrigin::CallResult);
        self.push_instruction(
            MirOperation::SumVariantIs { nominal, type_args: type_args_from_sum_shaped(&actual_type), variant, object: receiver },
            vec![value],
        );
        self.value_types.insert(value, ValkyrieType::Boolean);
        Some(MirOperand::Value(value))
    }

    pub(crate) fn option_shaped_type(
        &self,
        receiver: &MirOperand,
        hint: Option<&ValkyrieType>,
        payload_hint: Option<&ValkyrieType>,
    ) -> Option<ValkyrieType> {
        infer_builder_operand_type(receiver, &self.value_types)
            .filter(|ty| self.is_option_type(ty))
            .or_else(|| hint.cloned().filter(|ty| self.is_option_type(ty)))
            .or_else(|| {
                payload_hint
                    .and_then(|payload| self.option_declaration.and_then(|declaration| self.sum_apply_type(declaration, &[payload.clone()])))
            })
    }

    /// Option ????????HIR ???? extractor ????
    pub(crate) fn is_option_unwrap_call(resolved: Option<&HirResolvedCall>) -> bool {
        resolved.is_some_and(|call| matches!(call.domain, HirCallableDomain::Extractor) || call.extractor_payload_type.is_some())
    }

    pub(crate) fn is_option_is_some_call(&self, resolved: Option<&HirResolvedCall>) -> bool {
        resolved.is_some_and(|call| {
            matches!(call.return_type, ValkyrieType::Boolean) && call.parameter_types.first().is_some_and(|ty| self.is_option_type(ty))
        })
    }

    pub(crate) fn try_lower_option_unwrap(
        &mut self,
        receiver: MirOperand,
        resolved: Option<&HirResolvedCall>,
        hint: Option<&ValkyrieType>,
        payload_hint: Option<&ValkyrieType>,
    ) -> Option<MirOperand> {
        let call = resolved?;
        let (owner, variant_declaration) = call.sum_owner.zip(call.sum_variant)?;
        let actual_type = self.option_shaped_type(&receiver, hint, payload_hint)?;
        let payload_type =
            call.extractor_payload_type.clone().or_else(|| Self::option_payload_type(&actual_type)).or_else(|| payload_hint.cloned())?;
        let (nominal, variant) = self.sum_identity_by_declarations(owner, variant_declaration)?;
        if let MirOperand::Value(receiver_ref) = &receiver {
            if self.value_types.get(receiver_ref).is_none_or(|ty| !self.is_option_type(ty)) {
                if let Some(option_ty) = self.sum_apply_type(owner, &[payload_type.clone()]) {
                    self.value_types.insert(*receiver_ref, option_ty);
                }
            }
        }
        let value = self.next_value(MirValueOrigin::CallResult);
        self.push_instruction(
            MirOperation::SumPayloadGet {
                nominal,
                type_args: type_args_from_sum_shaped(&actual_type),
                variant,
                payload_type: payload_type.clone(),
                object: receiver,
            },
            vec![value],
        );
        self.value_types.insert(value, payload_type);
        Some(MirOperand::Value(value))
    }

    pub(crate) fn try_lower_option_none_constructor(
        &mut self,
        resolved: Option<&HirResolvedCall>,
        expected_type: Option<&ValkyrieType>,
        explicit_generic_arguments: &[ValkyrieType],
    ) -> Option<MirOperand> {
        let call = resolved?;
        if call.domain != HirCallableDomain::Constructor {
            return None;
        }
        let (owner, variant_declaration) = call.sum_owner.zip(call.sum_variant)?;
        let contextual = expected_type
            .cloned()
            .or_else(|| (!explicit_generic_arguments.is_empty()).then(|| self.sum_apply_type(owner, explicit_generic_arguments)).flatten())
            .unwrap_or_else(|| call.return_type.clone());
        if !self.is_option_type(&contextual) && contextual.nominal_declaration() != Some(owner) && explicit_generic_arguments.is_empty() {
            return None;
        }
        let type_args = self.sum_instantiation_args(owner, &contextual, explicit_generic_arguments);
        let return_type = self.sum_apply_type(owner, &type_args).unwrap_or_else(|| call.return_type.clone());
        let (nominal, variant) = self.sum_identity_by_declarations(owner, variant_declaration)?;
        let value = self.next_value(MirValueOrigin::CallResult);
        self.push_instruction(MirOperation::SumNew { nominal, type_args, variant, payload_type: None, payload: None }, vec![value]);
        self.value_types.insert(value, return_type);
        Some(MirOperand::Value(value))
    }

    pub(crate) fn try_lower_result_variant_constructor(
        &mut self,
        _variant: &str,
        payload_expr: &HirExpr,
        resolved: Option<&HirResolvedCall>,
        expected_type: Option<&ValkyrieType>,
    ) -> Option<MirOperand> {
        let call = resolved?;
        if call.domain != HirCallableDomain::Constructor {
            return None;
        }
        let (owner, variant_declaration) = call.sum_owner.zip(call.sum_variant)?;
        let (nominal, variant_id) = self.sum_identity_by_declarations(owner, variant_declaration)?;
        // 调用方短名仅供诊断；payload/实参位置只认 sum_owner + binder。
        let contextual = expected_type.cloned().unwrap_or_else(|| call.return_type.clone());
        let payload_hint = call.parameter_types.first().cloned().or_else(|| instantiated_payload_for_call(&self.sum_types, call, &contextual));
        let payload_operand = self.lower_expr_to_operand_with_hint(payload_expr, payload_hint.as_ref());
        let payload_type = infer_builder_operand_type(&payload_operand, &self.value_types).or(payload_hint);
        let mut emitted_type_args = self.pad_result_type_args(self.sum_instantiation_args(owner, &contextual, &[]));
        if let Some(payload) = payload_type.as_ref() {
            if let Some(sum) = crate::compiler::mir::ssa::MirSumDeclaration::find_by_declaration(&self.sum_types, owner) {
                if let Some(variant_decl) = sum.variants.iter().find(|item| item.declaration == Some(variant_declaration)) {
                    apply_payload_to_sum_type_args(sum, variant_decl, &mut emitted_type_args, payload);
                }
            }
        }
        let return_type = if type_args_from_sum_shaped(&contextual).is_empty() && !emitted_type_args.is_empty() {
            self.sum_apply_type(owner, &emitted_type_args).unwrap_or_else(|| contextual.clone())
        }
        else {
            contextual.clone()
        };
        let (return_type, payload_type) =
            concretize_variant_constructor_types(&return_type, payload_type, expected_type, &[payload_operand.clone()], &self.value_types);
        let emitted_type_args = self.pad_result_type_args(if type_args_from_sum_shaped(&return_type).is_empty() {
            emitted_type_args
        }
        else {
            type_args_from_sum_shaped(&return_type)
        });
        let value = self.next_value(MirValueOrigin::CallResult);
        self.push_instruction(
            MirOperation::SumNew { nominal, type_args: emitted_type_args, variant: variant_id, payload_type, payload: Some(payload_operand) },
            vec![value],
        );
        self.value_types.insert(value, return_type);
        Some(MirOperand::Value(value))
    }

    pub(crate) fn try_lower_option_some_constructor(
        &mut self,
        payload_expr: &HirExpr,
        resolved: Option<&HirResolvedCall>,
        expected_type: Option<&ValkyrieType>,
    ) -> Option<MirOperand> {
        let call = resolved?;
        if call.domain != HirCallableDomain::Constructor {
            return None;
        }
        let (owner, variant_declaration) = call.sum_owner.zip(call.sum_variant)?;
        let (nominal, variant) = self.sum_identity_by_declarations(owner, variant_declaration)?;
        let payload_hint = call.parameter_types.first().cloned().or_else(|| expected_type.and_then(|ty| Self::option_payload_type(ty)));
        let payload_operand = self.lower_expr_to_operand_with_hint(payload_expr, payload_hint.as_ref());
        let payload_type = infer_builder_operand_type(&payload_operand, &self.value_types).or(payload_hint);
        let contextual = expected_type
            .cloned()
            .or_else(|| payload_type.as_ref().and_then(|payload| self.sum_apply_type(owner, &[payload.clone()])))
            .unwrap_or_else(|| call.return_type.clone());
        let contextual = refine_auto_apply_type(&contextual, payload_type.as_ref());
        let type_args = refine_auto_sum_instantiation_args(&self.sum_instantiation_args(owner, &contextual, &[]), payload_type.as_ref());
        let return_type = if type_args_from_sum_shaped(&contextual).is_empty() && !type_args.is_empty() {
            self.sum_apply_type(owner, &type_args).unwrap_or_else(|| refine_auto_apply_type(&contextual, payload_type.as_ref()))
        }
        else {
            refine_auto_apply_type(&contextual, payload_type.as_ref())
        };
        let emitted_type_args = if !type_args.is_empty() {
            type_args
        }
        else {
            refine_auto_sum_instantiation_args(&type_args_from_sum_shaped(&return_type), payload_type.as_ref())
        };
        let value = self.next_value(MirValueOrigin::CallResult);
        self.push_instruction(
            MirOperation::SumNew {
                nominal,
                type_args: emitted_type_args,
                variant,
                payload_type: payload_type.clone(),
                payload: Some(payload_operand),
            },
            vec![value],
        );
        self.value_types.insert(value, return_type);
        Some(MirOperand::Value(value))
    }

    /// 数组结构操作 intrinsic 的统一降级点。
    ///
    /// 核心合同见文件头：intrinsic 专用 opcode（`ArrayLength`/`ArrayGet`/`ArraySet`）只允许
    /// 由 adaptor 链接进来的后端专用绑定产生；源码 `[intrinsic(...)]` 自由函数与语言 builtin
    /// intrinsic 候选一律走普通 Call 路径，由 planner adaptor / backend TargetExecutionPlan
    /// 链路消费。当前没有 adaptor 链接进来的后端绑定，因此所有数组 intrinsic 调用直接
    /// `return None` 让主 lowering 循环走 Call 分支。
    pub(crate) fn try_lower_array_intrinsics(
        &mut self,
        _resolved: Option<&HirResolvedCall>,
        _arguments: &[MirOperand],
        _expected_type: Option<&ValkyrieType>,
    ) -> Option<MirOperand> {
        None
    }

    /// Positional `Range(a, b)` ??????????`StructNew`???????????`Call`??
    pub(crate) fn try_lower_struct_constructor_call(&mut self, call: &HirResolvedCall, arguments: &[MirOperand]) -> Option<MirOperand> {
        if call.sum_owner.is_some() || call.sum_variant.is_some() {
            return None;
        }
        let declaration_id = call.return_type.nominal_declaration().or(call.declaration)?;
        let declaration = crate::compiler::mir::ssa::MirStruct::find_by_declaration(&self.field_declarations, declaration_id)?;
        if declaration.fields.len() != arguments.len() {
            return None;
        }
        let nominal = declaration.nominal;
        let fields = declaration.fields.iter().map(|field| field.id).zip(arguments.iter().cloned()).collect::<Vec<_>>();
        let value = self.next_value(MirValueOrigin::Temporary);
        self.push_instruction(MirOperation::StructNew { nominal, fields }, vec![value]);
        self.value_types.insert(value, call.return_type.clone());
        Some(MirOperand::Value(value))
    }

    pub(crate) fn field_type_for_semantic_type(&self, ty: &ValkyrieType, field: &str) -> Option<ValkyrieType> {
        let declaration_id = ty.nominal_declaration()?;
        let structure = crate::compiler::mir::ssa::MirStruct::find_by_declaration(&self.field_declarations, declaration_id)?;
        structure.instantiate_field(ty, field)
    }

    /// Resolve `Nyar`/`Valkyrie` function type for a call callee (Value or named Symbol).
    pub(crate) fn function_type_of_callee(&self, callee: &MirOperand) -> Option<FunctionType> {
        match callee {
            MirOperand::Value(value) => match self.value_types.get(value) {
                Some(ValkyrieType::Function(func)) => Some(func.as_ref().clone()),
                _ => None,
            },
            _ => None,
        }
    }

    pub(crate) fn storage_for_type(&self, ty: &ValkyrieType) -> MirStorageKind {
        storage_kind_for_type(ty, &self.aggregate_layouts.value_type_declarations)
    }

    pub(crate) fn storage_for_object_operand(&self, operand: &MirOperand) -> MirStorageKind {
        infer_builder_operand_type(operand, &self.value_types).map(|ty| self.storage_for_type(&ty)).unwrap_or(MirStorageKind::Reference)
    }

    pub(crate) fn layout_id_for_type(&mut self, ty: &ValkyrieType) -> Option<crate::compiler::mir::ssa::LayoutId> {
        if let Some(id) = layout_id_for_type(ty, &self.aggregate_layouts) {
            return Some(id);
        }
        match ty {
            ValkyrieType::Named(name) => None,
            _ if self.storage_for_type(ty) == MirStorageKind::Value => ensure_layout_for_type(&mut self.aggregate_layouts, ty),
            _ => None,
        }
    }

    pub(crate) fn layout_id_for_object_operand(&mut self, operand: &MirOperand) -> Option<crate::compiler::mir::ssa::LayoutId> {
        infer_builder_operand_type(operand, &self.value_types).and_then(|ty| self.layout_id_for_type(&ty))
    }

    pub(crate) fn field_type_for_object_operand(&self, object_operand: &MirOperand, field: &Identifier) -> Option<ValkyrieType> {
        let object_ty = infer_builder_operand_type(object_operand, &self.value_types)?;
        self.field_type_for_semantic_type(&object_ty, field.as_str())
    }
}
