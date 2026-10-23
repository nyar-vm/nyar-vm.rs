use crate::{
    hir::is_nullable_type,
    types::{
        Identifier, NamePath,
        hir::{FunctionType, HirCallableDomain, HirExpr, HirExprKind, HirLiteral, HirResolvedCall, ValkyrieType},
    },
};
use nyar_types::IntrinsicId;

use super::{
    MirBuilder, MirConstant, MirInstruction, MirOperand, MirOperation, MirStorageKind, MirValueOrigin,
    builtin_helpers::resolve_intrinsic_id,
    callee_name_matches,
    expr_helpers::{is_array_shaped_valkyrie_type, lower_resolved_callee, named_type_name, peel_generic_apply},
    infer_builder_operand_type, lower_callee_operand,
    value_semantics::{ensure_layout_for_type, layout_id_for_type, storage_kind_for_type},
};

impl MirBuilder {
    pub(super) fn callee_intrinsic_id(resolved: Option<&HirResolvedCall>, callee: &MirOperand) -> Option<IntrinsicId> {
        // 声明冻结的 intrinsic 身份优先；符号路径反查只作为迁移期通道，服务于
        // 尚未带结构化身份的私有种子（`__array_len` 等）。
        resolved.and_then(|call| call.intrinsic).or_else(|| resolved.and_then(|call| resolve_intrinsic_id(&call.symbol)))
    }

    fn try_lower_ref_deref_intrinsic(
        &mut self,
        resolved: Option<&HirResolvedCall>,
        callee: &MirOperand,
        arguments: &[MirOperand],
    ) -> Option<MirOperand> {
        if Self::callee_intrinsic_id(resolved, callee) != Some(IntrinsicId::RefDeref) {
            return None;
        }
        arguments.first().cloned()
    }

    /// `T?` ???????? sum ??? `Nominal{ItemId}`?
    /// ??? `Named("Option")` ????? variant ???? `call.sum_owner`?
    fn is_option_type(&self, actual_type: &ValkyrieType) -> bool {
        match actual_type {
            ValkyrieType::Nullable(_) => true,
            ValkyrieType::Nominal { declaration, .. } => super::MirSumDeclaration::find_by_declaration(&self.sum_types, *declaration).is_some(),
            _ => false,
        }
    }

    /// Some/Fine/Fail 短名命中但缺少 sum 身份时失败关闭，禁止回落 Invoke。
    fn fail_unresolved_sum_variant_call(&mut self, resolved: Option<&HirResolvedCall>) -> MirOperand {
        let (sum_type, variant) = unresolved_variant_labels(resolved);
        self.diagnostics.push(super::MirDiagnostic::UnresolvedVariantIdentity { sum_type, variant });
        MirOperand::Constant(MirConstant::Unit)
    }

    fn option_payload_type(actual_type: &ValkyrieType) -> Option<ValkyrieType> {
        match actual_type {
            ValkyrieType::Nullable(inner) => Some(*inner.clone()),
            ValkyrieType::Nominal { arguments, .. } => arguments.first().cloned(),
            ValkyrieType::Apply(_, args) => args.first().cloned(),
            _ => None,
        }
    }

    fn option_uses_generic_payload(actual_type: &ValkyrieType) -> bool {
        Self::option_payload_type(actual_type).is_some_and(|payload| matches!(payload, ValkyrieType::Named(name) if name.as_str() == "T"))
    }

    /// ??????sum declaration ItemId ??????????? `Named`??
    fn sum_apply_type(&self, declaration: nyar_types::ItemId, type_args: &[ValkyrieType]) -> Option<ValkyrieType> {
        let _ = super::MirSumDeclaration::find_by_declaration(&self.sum_types, declaration)?;
        Some(ValkyrieType::Nominal { declaration, arguments: type_args.to_vec() })
    }

    /// 仅从 impl 实参或 sum declaration 的 Generic binder 补全 Result 类型参数；禁止发明 `Named(T/E)`。
    fn pad_result_type_args(&self, type_args: Vec<ValkyrieType>) -> Vec<ValkyrieType> {
        if type_args.len() >= 2 {
            return type_args;
        }
        let error_arg = self.impl_type_arguments.get(1).cloned().or_else(|| {
            self.result_declaration.and_then(|declaration| {
                super::MirSumDeclaration::find_by_declaration(&self.sum_types, declaration)
                    .and_then(|sum| sum.generics.get(1).map(|generic| ValkyrieType::Generic(generic.clone())))
            })
        });
        match type_args.len() {
            0 => {
                if self.impl_type_arguments.len() >= 2 {
                    self.impl_type_arguments.clone()
                }
                else if let Some(declaration) = self.result_declaration {
                    super::MirSumDeclaration::find_by_declaration(&self.sum_types, declaration)
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

    fn sum_instantiation_args(
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
        super::MirSumDeclaration::find_by_declaration(&self.sum_types, declaration)
            .map(|sum| sum.generics.iter().map(|generic| ValkyrieType::Generic(generic.clone())).collect())
            .unwrap_or_default()
    }

    fn infer_array_element_type(&self, array: &MirOperand) -> Option<ValkyrieType> {
        infer_builder_operand_type(array, &self.value_types).and_then(|ty| match ty {
            ValkyrieType::Array(inner) => Some(*inner),
            ValkyrieType::Apply(base, args) if named_type_name(base.as_ref()) == Some("Array") => args.first().cloned(),
            ValkyrieType::FixedArray { element, .. } => Some(*element),
            _ => None,
        })
    }

    fn try_lower_option_is_some(&mut self, receiver: MirOperand, resolved: Option<&HirResolvedCall>) -> Option<MirOperand> {
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

    fn option_shaped_type(
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
    fn is_option_unwrap_call(resolved: Option<&HirResolvedCall>) -> bool {
        resolved.is_some_and(|call| matches!(call.domain, HirCallableDomain::Extractor) || call.extractor_payload_type.is_some())
    }

    fn is_option_is_some_call(&self, resolved: Option<&HirResolvedCall>) -> bool {
        resolved.is_some_and(|call| {
            matches!(call.return_type, ValkyrieType::Boolean) && call.parameter_types.first().is_some_and(|ty| self.is_option_type(ty))
        })
    }

    fn try_lower_option_unwrap(
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

    fn try_lower_option_none_constructor(
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

    fn try_lower_result_variant_constructor(
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
            if let Some(sum) = super::MirSumDeclaration::find_by_declaration(&self.sum_types, owner) {
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

    fn try_lower_option_some_constructor(
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

    fn emit_array_length_operand(&mut self, array: MirOperand, expected_type: Option<&ValkyrieType>) -> MirOperand {
        let value = self.next_value(MirValueOrigin::CallResult);
        // results 必须挂在指令上，否则 wasm `array.len` 结果槽为空。
        self.push_instruction(MirOperation::ArrayLength { array }, vec![value]);
        // 语言合同是 `usize`：优先消费调用点/绑定注解的 Nominal，其次声明表中的 usize ItemId，
        // 禁止默认写出 `Named("usize")` 再与 `Nominal{ItemId}` 形成双 TypeId（CAN015 StoreVar）。
        let length_ty = expected_type
            .filter(|ty| super::expr_helpers::is_integer_literal_formal_type(ty, &self.field_declarations))
            .cloned()
            .or_else(|| self.resolved_usize_nominal_type())
            .unwrap_or_else(|| ValkyrieType::Named(Identifier::new("usize")));
        self.value_types.insert(value, length_ty);
        MirOperand::Value(value)
    }

    /// 从已收集的聚合声明取 `usize` 的 `Nominal{ItemId}`；跨包导入后应可命中。
    fn resolved_usize_nominal_type(&self) -> Option<ValkyrieType> {
        self.field_declarations.iter().find_map(|item| {
            let declaration = item.declaration?;
            if item.name.as_str() != "usize" {
                return None;
            }
            Some(ValkyrieType::Nominal { declaration, arguments: Vec::new() })
        })
    }

    fn try_lower_array_len_intrinsic(
        &mut self,
        resolved: Option<&HirResolvedCall>,
        callee: &MirOperand,
        arguments: &[MirOperand],
        expected_type: Option<&ValkyrieType>,
    ) -> Option<MirOperand> {
        if Self::callee_intrinsic_id(resolved, callee) != Some(IntrinsicId::ArrayLen) {
            return None;
        }
        let array = arguments.first().cloned()?;
        let hint = resolved
            .map(|call| &call.return_type)
            .filter(|ty| super::expr_helpers::is_integer_literal_formal_type(ty, &self.field_declarations))
            .or(expected_type);
        Some(self.emit_array_length_operand(array, hint))
    }

    fn try_lower_array_get_on_array(
        &mut self,
        array: MirOperand,
        ordinal: MirOperand,
        resolved: Option<&HirResolvedCall>,
        expected_type: Option<&ValkyrieType>,
    ) -> Option<MirOperand> {
        let array_ty = infer_builder_operand_type(&array, &self.value_types)
            .or_else(|| resolved.and_then(|call| call.parameter_types.first()).filter(|ty| is_array_shaped_valkyrie_type(ty)).cloned())?;
        if !is_array_shaped_valkyrie_type(&array_ty) {
            return None;
        }
        let element_type = self.infer_array_element_type(&array)?;
        let value = self.next_value(MirValueOrigin::CallResult);
        self.push_instruction(MirOperation::ArrayGet { array, index: ordinal }, vec![value]);
        let return_type = resolved
            .map(|call| call.return_type.clone())
            .or_else(|| expected_type.cloned())
            .filter(|ty| !self.is_option_type(ty))
            .unwrap_or(element_type);
        self.value_types.insert(value, return_type);
        Some(MirOperand::Value(value))
    }

    fn try_lower_array_get_intrinsic(
        &mut self,
        resolved: Option<&HirResolvedCall>,
        callee: &MirOperand,
        arguments: &[MirOperand],
        expected_type: Option<&ValkyrieType>,
    ) -> Option<MirOperand> {
        if Self::callee_intrinsic_id(resolved, callee) != Some(IntrinsicId::ArrayGet) {
            return None;
        }
        // Option ?????????????????+ `Option` ????????????
        let wraps_option = resolved
            .map(|call| self.is_option_type(&call.return_type))
            .unwrap_or_else(|| expected_type.is_some_and(|ty| self.is_option_type(ty)));
        if wraps_option {
            return None;
        }
        let array = arguments.first().cloned()?;
        let ordinal = arguments.get(1).cloned()?;
        self.try_lower_array_get_on_array(array, ordinal, resolved, expected_type)
    }

    /// ?? callee ?? overload ????[`IntrinsicId::ArraySet`] ?????
    fn try_lower_array_set_intrinsic(
        &mut self,
        resolved: Option<&HirResolvedCall>,
        callee: &MirOperand,
        arguments: &[MirOperand],
        _expected_type: Option<&ValkyrieType>,
    ) -> Option<MirOperand> {
        if Self::callee_intrinsic_id(resolved, callee) != Some(IntrinsicId::ArraySet) || arguments.len() != 3 {
            return None;
        }
        let array = arguments[0].clone();
        let array_ty = infer_builder_operand_type(&array, &self.value_types)
            .or_else(|| resolved.and_then(|call| call.parameter_types.first()).filter(|ty| is_array_shaped_valkyrie_type(ty)).cloned())?;
        if !is_array_shaped_valkyrie_type(&array_ty) {
            return None;
        }
        self.push_instruction(
            MirOperation::ArraySet { array: arguments[0].clone(), index: arguments[1].clone(), value: arguments[2].clone() },
            Vec::new(),
        );
        Some(MirOperand::Constant(MirConstant::Unit))
    }

    /// ?? callee ?? overload ????[`IntrinsicId::ArrayPush`] ?????
    /// ?????? `push` ??????ADR 0013???
    fn try_lower_array_push_intrinsic(
        &mut self,
        resolved: Option<&HirResolvedCall>,
        callee: &MirOperand,
        arguments: &[MirOperand],
        expected_type: Option<&ValkyrieType>,
    ) -> Option<MirOperand> {
        if Self::callee_intrinsic_id(resolved, callee) != Some(IntrinsicId::ArrayPush) || arguments.len() != 2 {
            return None;
        }
        let array = arguments[0].clone();
        let array_ty = infer_builder_operand_type(&array, &self.value_types)
            .or_else(|| expected_type.filter(|ty| is_array_shaped_valkyrie_type(ty)).cloned())?;
        if !is_array_shaped_valkyrie_type(&array_ty) {
            return None;
        }
        let value = self.next_value(MirValueOrigin::CallResult);
        self.push_instruction(MirOperation::ArrayPush { array: arguments[0].clone(), value: arguments[1].clone() }, vec![value]);
        let return_type = resolved
            .map(|call| call.return_type.clone())
            .filter(|ty| is_array_shaped_valkyrie_type(ty))
            .or_else(|| expected_type.filter(|ty| is_array_shaped_valkyrie_type(ty)).cloned())
            .unwrap_or(array_ty);
        self.value_types.insert(value, return_type);
        Some(MirOperand::Value(value))
    }

    fn try_lower_array_intrinsics(
        &mut self,
        resolved: Option<&HirResolvedCall>,
        callee: &MirOperand,
        arguments: &[MirOperand],
        expected_type: Option<&ValkyrieType>,
    ) -> Option<MirOperand> {
        self.try_lower_array_len_intrinsic(resolved, callee, arguments, expected_type)
            .or_else(|| self.try_lower_array_get_intrinsic(resolved, callee, arguments, expected_type))
            .or_else(|| self.try_lower_array_set_intrinsic(resolved, callee, arguments, expected_type))
            .or_else(|| self.try_lower_array_push_intrinsic(resolved, callee, arguments, expected_type))
    }

    /// Positional `Range(a, b)` ??????????`StructNew`???????????`Call`??
    fn try_lower_struct_constructor_call(&mut self, call: &HirResolvedCall, arguments: &[MirOperand]) -> Option<MirOperand> {
        if call.sum_owner.is_some() || call.sum_variant.is_some() {
            return None;
        }
        let declaration_id = call.return_type.nominal_declaration().or(call.declaration)?;
        let declaration = super::MirStruct::find_by_declaration(&self.field_declarations, declaration_id)?;
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

    fn field_type_for_semantic_type(&self, ty: &ValkyrieType, field: &str) -> Option<ValkyrieType> {
        let declaration_id = ty.nominal_declaration()?;
        let structure = super::MirStruct::find_by_declaration(&self.field_declarations, declaration_id)?;
        structure.instantiate_field(ty, field)
    }

    /// Resolve `Nyar`/`Valkyrie` function type for a call callee (Value or named Symbol).
    fn function_type_of_callee(&self, callee: &MirOperand) -> Option<FunctionType> {
        match callee {
            MirOperand::Value(value) => match self.value_types.get(value) {
                Some(ValkyrieType::Function(func)) => Some(func.as_ref().clone()),
                _ => None,
            },
            _ => None,
        }
    }

    fn storage_for_type(&self, ty: &ValkyrieType) -> MirStorageKind {
        storage_kind_for_type(ty, &self.aggregate_layouts.value_type_declarations)
    }

    fn storage_for_object_operand(&self, operand: &MirOperand) -> MirStorageKind {
        infer_builder_operand_type(operand, &self.value_types).map(|ty| self.storage_for_type(&ty)).unwrap_or(MirStorageKind::Reference)
    }

    fn layout_id_for_type(&mut self, ty: &ValkyrieType) -> Option<super::LayoutId> {
        if let Some(id) = layout_id_for_type(ty, &self.aggregate_layouts) {
            return Some(id);
        }
        match ty {
            ValkyrieType::Named(name) => None,
            _ if self.storage_for_type(ty) == MirStorageKind::Value => ensure_layout_for_type(&mut self.aggregate_layouts, ty),
            _ => None,
        }
    }

    fn layout_id_for_object_operand(&mut self, operand: &MirOperand) -> Option<super::LayoutId> {
        infer_builder_operand_type(operand, &self.value_types).and_then(|ty| self.layout_id_for_type(&ty))
    }

    pub(super) fn field_type_for_object_operand(&self, object_operand: &MirOperand, field: &Identifier) -> Option<ValkyrieType> {
        let object_ty = infer_builder_operand_type(object_operand, &self.value_types)?;
        self.field_type_for_semantic_type(&object_ty, field.as_str())
    }

    pub(super) fn lower_expr_to_operand(&mut self, expr: &HirExpr) -> MirOperand {
        self.lower_expr_to_operand_with_hint(expr, None)
    }

    pub(super) fn lower_expr_to_operand_with_hint(&mut self, expr: &HirExpr, expected_type: Option<&ValkyrieType>) -> MirOperand {
        match &expr.kind {
            HirExprKind::Literal(literal) => {
                let (constant, ty) = lower_literal(literal, expected_type);
                // 调用点形参为 `Nominal{usize}` 时，overload 已特化字面量；MIR 必须写出同一类型，
                // 否则 SMIR007 仍见 Integer32 vs Nominal。
                let ty = match (literal, expected_type) {
                    (HirLiteral::Integer64(_), Some(expected))
                        if super::expr_helpers::is_integer_literal_formal_type(expected, &self.field_declarations) =>
                    {
                        Some(expected.clone())
                    }
                    _ => ty,
                };
                let value = self.next_value(MirValueOrigin::Literal);
                self.push_instruction(MirOperation::LoadConstant { constant, ty: ty.clone() }, vec![value]);
                if let Some(ty) = ty {
                    self.value_types.insert(value, ty);
                }
                MirOperand::Value(value)
            }
            HirExprKind::Variable(identifier) => {
                if let Some(bound) = self.bindings.get(identifier.name.as_str()).cloned() {
                    return bound;
                }
                self.diagnostics.push(super::MirDiagnostic::UnresolvedCallableIdentity { symbol: identifier.name.to_string() });
                MirOperand::Constant(MirConstant::Unit)
            }
            HirExprKind::Path(path) => {
                let value = self.next_value(MirValueOrigin::Path);
                // `null` is a language-level nullable value (`T?`), not the
                // nominal `Option<T>::None` variant. Preserve the contextual
                // nullable type on the SSA value so validation and backends
                // never need to infer it from the symbol spelling.
                if path.parts().len() == 1 && path.parts()[0].as_str() == "null" {
                    if let Some(ty) =
                        expected_type.cloned().or_else(|| is_nullable_type(&self.current_return_type).then(|| self.current_return_type.clone()))
                    {
                        self.value_types.insert(value, ty);
                    }
                }
                MirOperand::Value(value)
            }
            HirExprKind::Call { callee, args, resolved, .. } => {
                let (explicit_generic_arguments, callee) = peel_generic_apply(callee.as_ref());
                if super::expr_helpers::callee_matches_name(callee, resolved.as_ref(), "panic") {
                    let payload = if args.len() == 1 { Some(self.lower_expr_to_operand(&args[0].value)) } else { None };
                    return self.lower_uncaught_raise(payload);
                }
                // HIR may lower `expr.unwrap()` to `unwrap(expr)` (functional call).
                // ????Option ??????? Extractor ??????????????????
                if args.len() == 1 && Self::is_option_unwrap_call(resolved.as_ref()) {
                    let receiver_operand = self.lower_expr_to_operand(&args[0].value);
                    let payload_hint = resolved.as_ref().map(|call| call.return_type.clone()).or_else(|| expected_type.cloned());
                    let hint =
                        infer_builder_operand_type(&receiver_operand, &self.value_types).filter(|ty| self.is_option_type(ty)).or_else(|| {
                            let declaration = resolved.as_ref().and_then(|call| call.sum_owner).or(self.option_declaration)?;
                            payload_hint.as_ref().and_then(|payload| self.sum_apply_type(declaration, &[payload.clone()]))
                        });
                    if let Some(operand) =
                        self.try_lower_option_unwrap(receiver_operand.clone(), resolved.as_ref(), hint.as_ref(), payload_hint.as_ref())
                    {
                        return operand;
                    }
                }
                // 一元 payload 变体构造：只认已冻结 sum_owner/sum_variant；按 owner binder 元数分流，禁止 Some/Fine/Fail 短名入口。
                if args.len() == 1 {
                    if let Some(call) = resolved.as_ref() {
                        if call.domain == crate::types::hir::HirCallableDomain::Constructor {
                            if let Some(owner) = call.sum_owner.filter(|_| call.sum_variant.is_some()) {
                                let binary = super::MirSumDeclaration::find_by_declaration(&self.sum_types, owner)
                                    .is_some_and(|sum| sum.generics.len() == 2);
                                return if binary {
                                    self.try_lower_result_variant_constructor("", &args[0].value, resolved.as_ref(), expected_type)
                                        .unwrap_or_else(|| self.fail_unresolved_sum_variant_call(resolved.as_ref()))
                                }
                                else {
                                    self.try_lower_option_some_constructor(&args[0].value, resolved.as_ref(), expected_type)
                                        .unwrap_or_else(|| self.fail_unresolved_sum_variant_call(resolved.as_ref()))
                                };
                            }
                        }
                    }
                }
                // 禁止按 callee 名 `tuple` 补结构语义；须等真实 Tuple AST/HIR/类型身份落地。
                if callee_name_matches(&callee.kind, "tuple") {
                    self.diagnostics.push(super::MirDiagnostic::UnresolvedCallableIdentity { symbol: "tuple".to_string() });
                    return MirOperand::Constant(MirConstant::Unit);
                }
                if let Some((receiver_operand, _method_name)) =
                    self.extract_method_call(callee).filter(|_| resolved.as_ref().is_some_and(|call| call.has_receiver))
                {
                    let param_types = resolved.as_ref().map(|call| call.parameter_types.as_slice());
                    // Skip receiver slot (index 0) when binding hints for explicit args.
                    let mut arguments = args
                        .iter()
                        .enumerate()
                        .map(|(index, arg)| {
                            let hint = param_types.and_then(|params| params.get(index + 1));
                            self.lower_expr_to_operand_with_hint(&arg.value, hint)
                        })
                        .collect::<Vec<_>>();
                    arguments.insert(0, receiver_operand.clone());
                    if args.is_empty() && self.is_option_is_some_call(resolved.as_ref()) {
                        if self.option_shaped_type(&receiver_operand, None, None).is_some() {
                            if let Some(operand) = self.try_lower_option_is_some(receiver_operand.clone(), resolved.as_ref()) {
                                return operand;
                            }
                        }
                    }
                    if args.is_empty() && Self::is_option_unwrap_call(resolved.as_ref()) {
                        let payload_hint = resolved.as_ref().map(|call| call.return_type.clone()).or_else(|| expected_type.cloned());
                        // ??????Option ??????????Option ?????? payload ????
                        let hint = infer_builder_operand_type(&receiver_operand, &self.value_types)
                            .filter(|ty| self.is_option_type(ty))
                            .or_else(|| {
                                let declaration = resolved.as_ref().and_then(|call| call.sum_owner).or(self.option_declaration)?;
                                payload_hint.as_ref().and_then(|payload| self.sum_apply_type(declaration, &[payload.clone()]))
                            });
                        if let Some(operand) =
                            self.try_lower_option_unwrap(receiver_operand.clone(), resolved.as_ref(), hint.as_ref(), payload_hint.as_ref())
                        {
                            return operand;
                        }
                    }
                    let resolved_call = resolved.as_ref().expect("Semantic MIR requires a resolved instance call contract");
                    let has_requirement = resolved_call.requirement.is_some();
                    let callee = if has_requirement {
                        MirOperand::Constant(MirConstant::Unit)
                    }
                    else {
                        let Some(callee) = lower_resolved_callee(resolved_call)
                        else {
                            self.diagnostics
                                .push(super::MirDiagnostic::UnresolvedCallableIdentity { symbol: resolved_call.symbol.to_string() });
                            return MirOperand::Constant(MirConstant::Unit);
                        };
                        callee
                    };
                    if !has_requirement {
                        if let Some(operand) = self.try_lower_array_intrinsics(resolved.as_ref(), &callee, &arguments, expected_type) {
                            return operand;
                        }
                    }
                    let return_type = self.return_type_for_resolved_call(&callee, resolved_call);
                    let operand = self.push_call_returning(callee, arguments, return_type);
                    if let Some(req) = &resolved_call.requirement {
                        if let Some(last) = self.instructions.last() {
                            self.pending_requirements.push((last.id, req.clone()));
                        }
                    }
                    return operand;
                }
                // `obj.field(args)` where `obj.field` is a function-typed field (e.g.
                // `FilterIterator._predicate`). Lower as indirect call: load the field
                // value via FieldGet, then `Call { dispatch: Indirect, callee: <value> }`.
                // This lets backends emit `Delegate::DynamicInvoke` / `call_ref` etc.
                // instead of treating the field name as a static method symbol.
                if let HirExprKind::FieldAccess { object, field, field_id } = &callee.kind {
                    let receiver_operand = self.lower_expr_to_operand(object);
                    let field_ty = self.field_type_for_object_operand(&receiver_operand, field);
                    if matches!(field_ty, Some(ValkyrieType::Function(_))) {
                        let Some(field_id) = *field_id
                        else {
                            self.diagnostics.push(super::MirDiagnostic::UnresolvedFieldIdentity { field: field.to_string() });
                            return MirOperand::Constant(MirConstant::Unit);
                        };
                        let callee_value = self.next_value(MirValueOrigin::Temporary);
                        self.push_instruction(MirOperation::FieldGet { object: receiver_operand, field: field_id }, vec![callee_value]);
                        if let Some(field_ty) = field_ty {
                            self.value_types.insert(callee_value, field_ty);
                        }
                        let param_types = resolved.as_ref().map(|call| call.parameter_types.as_slice());
                        let arguments = args
                            .iter()
                            .enumerate()
                            .map(|(index, arg)| {
                                let hint = param_types.and_then(|params| params.get(index));
                                self.lower_expr_to_operand_with_hint(&arg.value, hint)
                            })
                            .collect::<Vec<_>>();
                        let return_type =
                            resolved.as_ref().expect("function-typed field call requires resolved call contract").return_type.clone();
                        // `unit` ?????? value ??BPHYS001???
                        return self.push_call_returning(MirOperand::Value(callee_value), arguments, return_type);
                    }

                    // `obj.field.method()` where `obj` is not yet in bindings (e.g. nested
                    // temporaries) still must lower as a receiver Call, not a dotted Symbol.
                    let has_receiver = resolved.as_ref().is_some_and(|call| call.has_receiver);
                    let param_types = resolved.as_ref().map(|call| call.parameter_types.as_slice());
                    let mut arguments = args
                        .iter()
                        .enumerate()
                        .map(|(index, arg)| {
                            let hint = param_types.and_then(|params| params.get(index + usize::from(has_receiver)));
                            self.lower_expr_to_operand_with_hint(&arg.value, hint)
                        })
                        .collect::<Vec<_>>();
                    if has_receiver {
                        arguments.insert(0, receiver_operand.clone());
                    }
                    let resolved_call = resolved.as_ref().expect("Semantic MIR requires a resolved field receiver call contract");
                    let Some(callee) = lower_resolved_callee(resolved_call)
                    else {
                        self.diagnostics.push(super::MirDiagnostic::UnresolvedCallableIdentity { symbol: resolved_call.symbol.to_string() });
                        return MirOperand::Constant(MirConstant::Unit);
                    };
                    let return_type = self.return_type_for_resolved_call(&callee, resolved_call);
                    // `unit` ?????? value ??BPHYS001???
                    return self.push_call_returning(callee, arguments, return_type);
                }
                let is_prefix_not = callee_name_matches(&callee.kind, "prefix !");
                let param_types = resolved.as_ref().map(|call| call.parameter_types.as_slice());
                // Lower args before unite constructor paths so `Some(v)` keeps payload hints.
                let mut arguments = Vec::with_capacity(args.len());
                for (index, arg) in args.iter().enumerate() {
                    let hint_owned: Option<ValkyrieType> = param_types
                        .and_then(|params| params.get(index).cloned())
                        .or_else(|| {
                            (param_types.is_none() && expected_type.is_some() && matches!(arg.value.kind, HirExprKind::Call { .. }))
                                .then(|| expected_type.cloned())
                                .flatten()
                        })
                        .or_else(|| is_prefix_not.then_some(ValkyrieType::Boolean))
                        .or_else(|| {
                            if index == 0 || arguments.is_empty() {
                                return None;
                            }
                            let first_ty = infer_builder_operand_type(&arguments[0], &self.value_types)?;
                            match first_ty {
                                ValkyrieType::Array(inner) => Some(*inner),
                                _ => None,
                            }
                        });
                    arguments.push(self.lower_expr_to_operand_with_hint(&arg.value, hint_owned.as_ref()));
                }
                // Unite constructors must lower before static callee projection.
                if let Some(call) = resolved.as_ref() {
                    if call.domain == HirCallableDomain::Constructor {
                        if args.is_empty() {
                            if let Some(operand) =
                                self.try_lower_option_none_constructor(Some(call), expected_type, &explicit_generic_arguments)
                            {
                                return operand;
                            }
                        }
                        if let Some(operand) = self.try_lower_struct_constructor_call(call, &arguments) {
                            return operand;
                        }
                        if let Some((sum_type, type_args, variant, payload_type, payload)) =
                            sum_new_parts_from_constructor(call, &arguments, &self.sum_types)
                        {
                            let value = self.next_value(MirValueOrigin::CallResult);
                            let Some((owner, variant_declaration)) = call.sum_owner.zip(call.sum_variant)
                            else {
                                self.diagnostics.push(super::MirDiagnostic::UnresolvedVariantIdentity { sum_type, variant });
                                return MirOperand::Constant(MirConstant::Unit);
                            };
                            let Some((nominal, variant_id)) = self.sum_identity_by_declarations(owner, variant_declaration)
                            else {
                                return MirOperand::Constant(MirConstant::Unit);
                            };
                            let (return_type, payload_type) = concretize_variant_constructor_types(
                                &call.return_type,
                                payload_type,
                                expected_type,
                                &arguments,
                                &self.value_types,
                            );
                            self.push_instruction(
                                MirOperation::SumNew { nominal, type_args, variant: variant_id, payload_type, payload },
                                vec![value],
                            );
                            self.value_types.insert(value, return_type);
                            return MirOperand::Value(value);
                        }
                        let (sum_type, variant) = unresolved_variant_labels(Some(call));
                        self.diagnostics.push(super::MirDiagnostic::UnresolvedVariantIdentity { sum_type, variant });
                        return MirOperand::Constant(MirConstant::Unit);
                    }
                }
                let deferred_member = resolved.as_ref().and_then(|call| call.requirement.clone());
                let has_requirement = deferred_member.is_some();
                let callee = if has_requirement {
                    MirOperand::Constant(MirConstant::Unit)
                }
                else {
                    match lower_callee_operand(callee, resolved.as_ref(), self) {
                    Some(callee) => callee,
                    None if resolved.as_ref().is_some_and(|call| call.domain == HirCallableDomain::Constructor) => {
                        let call = resolved.as_ref().expect("constructor call");
                        match lower_resolved_callee(call) {
                            Some(callee) => callee,
                            None => {
                                self.diagnostics.push(super::MirDiagnostic::UnresolvedCallableIdentity {
                                    symbol: call.symbol.to_string(),
                                });
                                return MirOperand::Constant(MirConstant::Unit);
                            }
                        }
                    }
                    None => {
                        self.diagnostics.push(super::MirDiagnostic::UnresolvedCallableIdentity {
                            symbol: "construct missing HIR callable contract".to_string(),
                        });
                        return MirOperand::Constant(MirConstant::Unit);
                    }
                    }
                };
                if !has_requirement {
                    if let Some(operand) = self.try_lower_ref_deref_intrinsic(resolved.as_ref(), &callee, &arguments) {
                        return operand;
                    }
                    if let Some(operand) = self.try_lower_null_intrinsics(resolved.as_ref(), &callee, &arguments) {
                        return operand;
                    }
                    if let Some(operand) = self.try_lower_array_intrinsics(resolved.as_ref(), &callee, &arguments, expected_type) {
                        return operand;
                    }
                }
                let function_ty = self.function_type_of_callee(&callee);
                if arguments.len() == 1 && !has_requirement {
                    if self.is_option_is_some_call(resolved.as_ref()) && self.option_shaped_type(&arguments[0], None, None).is_some() {
                        if let Some(operand) = self.try_lower_option_is_some(arguments[0].clone(), resolved.as_ref()) {
                            return operand;
                        }
                    }
                }
                // ???`Call` ?? `push_call_returning`???? intrinsic ?????????
                // `unit` ?????? value ??BPHYS001???
                let Some(return_type) = resolved
                    .as_ref()
                    .map(|call| call.return_type.clone())
                    .or_else(|| self.declared_return_type_for_call(&callee, resolved.as_ref()))
                    .or_else(|| function_ty.map(|func| func.return_type))
                else {
                    self.diagnostics
                        .push(super::MirDiagnostic::UnresolvedValueType { context: format!("call missing return type fact: {callee:?}") });
                    return MirOperand::Constant(MirConstant::Unit);
                };
                let return_type = super::expr_helpers::refine_call_return_type(return_type, expected_type);
                let operand = self.push_call_returning(callee, arguments, return_type);
                if let Some(req) = deferred_member {
                    if let Some(last) = self.instructions.last() {
                        self.pending_requirements.push((last.id, req));
                    }
                }
                operand
            }
            HirExprKind::ArrayNew { element_type, length } => {
                let length_operand = self.lower_expr_to_operand(length);
                let array_type = ValkyrieType::Array(Box::new(element_type.clone()));
                let value = self.next_value(MirValueOrigin::Temporary);
                self.push_instruction(
                    MirOperation::ArrayNew {
                        array_type: array_type.clone(),
                        length: length_operand,
                        initialization: super::ArrayInitialization::Default,
                    },
                    vec![value],
                );
                self.value_types.insert(value, array_type);
                MirOperand::Value(value)
            }
            HirExprKind::ArrayLiteral { items } => {
                // ????????ArrayFromElements???? Array ?? array_type ????
                let array_type = match expected_type {
                    Some(ValkyrieType::FixedArray { element, length }) => {
                        ValkyrieType::FixedArray { element: element.clone(), length: *length }
                    }
                    Some(ValkyrieType::Array(item)) => ValkyrieType::Array(item.clone()),
                    _ => {
                        let element_type = infer_array_literal_element_type(items, None);
                        ValkyrieType::Array(Box::new(element_type))
                    }
                };
                let element_hint = match &array_type {
                    ValkyrieType::FixedArray { element, .. } | ValkyrieType::Array(element) => Some(element.as_ref()),
                    _ => None,
                };
                let elements = items.iter().map(|item| self.lower_expr_to_operand_with_hint(item, element_hint)).collect::<Vec<_>>();
                let array_value = self.next_value(MirValueOrigin::Temporary);
                self.push_instruction(MirOperation::ArrayFromElements { array_type: array_type.clone(), elements }, vec![array_value]);
                self.value_types.insert(array_value, array_type);
                MirOperand::Value(array_value)
            }
            HirExprKind::TupleLiteral { items } => {
                let fields = items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        let hint = expected_type.and_then(|ty| match ty {
                            ValkyrieType::Tuple(element_types) => element_types.get(index),
                            _ => None,
                        });
                        self.lower_expr_to_operand_with_hint(item, hint)
                    })
                    .collect::<Vec<_>>();
                let tuple_type = match expected_type {
                    Some(ValkyrieType::Tuple(element_types)) if element_types.len() == items.len() => {
                        ValkyrieType::Tuple(element_types.clone())
                    }
                    _ => ValkyrieType::Tuple(
                        fields
                            .iter()
                            .map(|operand| infer_builder_operand_type(operand, &self.value_types).unwrap_or(ValkyrieType::AutoType))
                            .collect(),
                    ),
                };
                let tuple_value = self.next_value(MirValueOrigin::Temporary);
                self.push_instruction(MirOperation::TupleNew { fields }, vec![tuple_value]);
                self.value_types.insert(tuple_value, tuple_type);
                MirOperand::Value(tuple_value)
            }
            HirExprKind::Construct { path, args, resolved } => {
                let name = path.parts().last().cloned().unwrap_or_else(|| Identifier::new("_"));
                let struct_type_name = self.struct_new_owner_name(&name);
                let mut fields: Vec<(nyar_types::FieldId, MirOperand)> = Vec::with_capacity(args.len());
                let mut field_values = Vec::with_capacity(args.len());
                for (arg_index, arg) in args.iter().enumerate() {
                    if let HirExprKind::FieldInit { name: field_name, value, field_id } = &arg.kind {
                        // Empty `[]` without a hint defaults to `i32[]` (see
                        // `infer_array_literal_element_type`). Pass the struct field type so
                        // `visited: [utf8] = []` lowers as `string[]`, not `int32[]`.
                        // 字段身份只消费 HIR 冻结结果；缺失即结构化失败。
                        let owner = self.struct_construct_result_type(&name, resolved.as_ref());
                        let field_ty = self
                            .field_type_for_semantic_type(&owner, field_name.as_str())
                            .or_else(|| resolved.as_ref().and_then(|call| call.parameter_types.get(arg_index).cloned()))
                            .map(|ty| super::resolve_self_type_with_owner(&ty, self.impl_owner_type.as_ref()));
                        let value_operand = self.lower_expr_to_operand_with_hint(value, field_ty.as_ref());
                        // StructNew SMIR010 compares operand `value_types` (after
                        // concretize) to aggregate layout field types.  Hints steer
                        // `[]` element typing but do not update `value_types`; without
                        // this, `ArrayList { _items: [] }` stays `i32[]` while layout
                        // keeps `Array(Generic(T))`.
                        if let (Some(declared_ty), MirOperand::Value(value_ref)) = (&field_ty, &value_operand) {
                            self.value_types.insert(*value_ref, declared_ty.clone());
                        }
                        let Some(field_id) = *field_id
                        else {
                            self.diagnostics.push(super::MirDiagnostic::UnresolvedFieldIdentity { field: field_name.to_string() });
                            return MirOperand::Constant(MirConstant::Unit);
                        };
                        field_values.push(value_operand.clone());
                        fields.push((field_id, value_operand));
                    }
                }
                // Unite/enum constructors lower as `SumNew` only when HIR already closed
                // `sum_owner` / `sum_variant` facts. Do not rediscover variant identity
                // from spellings in MIR.
                if let Some(call) = resolved.as_ref() {
                    if call.domain == HirCallableDomain::Constructor {
                        if let Some((owner, variant_declaration)) = call.sum_owner.zip(call.sum_variant) {
                            let value = self.next_value(MirValueOrigin::CallResult);
                            let ctor_return = call.return_type.clone();
                            let type_args = type_args_from_sum_shaped(&ctor_return);
                            let payload_type = call.parameter_types.first().cloned();
                            let payload = match field_values.len() {
                                0 => None,
                                1 => Some(field_values[0].clone()),
                                _ => field_values.first().cloned(),
                            };
                            let Some((nominal, variant_id)) = self.sum_identity_by_declarations(owner, variant_declaration)
                            else {
                                let (sum_type, variant) = unresolved_variant_labels(Some(call));
                                self.diagnostics.push(super::MirDiagnostic::UnresolvedVariantIdentity { sum_type, variant });
                                return MirOperand::Constant(MirConstant::Unit);
                            };
                            let (return_type, payload_type) = concretize_variant_constructor_types(
                                &ctor_return,
                                payload_type,
                                expected_type,
                                &field_values,
                                &self.value_types,
                            );
                            self.push_instruction(
                                MirOperation::SumNew { nominal, type_args, variant: variant_id, payload_type, payload },
                                vec![value],
                            );
                            self.value_types.insert(value, return_type);
                            return MirOperand::Value(value);
                        }
                        if call.sum_owner.is_some() || call.sum_variant.is_some() {
                            let (sum_type, variant) = unresolved_variant_labels(Some(call));
                            self.diagnostics.push(super::MirDiagnostic::UnresolvedVariantIdentity { sum_type, variant });
                            return MirOperand::Constant(MirConstant::Unit);
                        }
                    }
                }
                let value = self.next_value(MirValueOrigin::Temporary);
                let result_type = self.struct_construct_result_type(&name, resolved.as_ref());
                let Some(declaration_id) = result_type.nominal_declaration().or_else(|| resolved.as_ref().and_then(|call| call.declaration))
                else {
                    self.diagnostics.push(super::MirDiagnostic::UnresolvedNominalIdentity { type_name: struct_type_name });
                    return MirOperand::Constant(MirConstant::Unit);
                };
                let Some(nominal) = self.nominal_identity_for_declaration(declaration_id)
                else {
                    self.diagnostics.push(super::MirDiagnostic::UnresolvedNominalIdentity { type_name: struct_type_name });
                    return MirOperand::Constant(MirConstant::Unit);
                };
                self.push_instruction(MirOperation::StructNew { nominal, fields }, vec![value]);
                self.value_types.insert(value, result_type);
                MirOperand::Value(value)
            }
            HirExprKind::FieldAccess { object, field, field_id } => {
                let object_operand = self.lower_singleton_field_object(object).unwrap_or_else(|| self.lower_expr_to_operand(object));
                // 禁止按字段名 `length` 改写成 ArrayLength；数组长度只能来自
                // 已在 HIR 解析的 IntrinsicId::ArrayLen Call / 明确 ArrayLength 节点。
                let Some(field_id) = *field_id
                else {
                    self.diagnostics.push(super::MirDiagnostic::UnresolvedFieldIdentity { field: field.to_string() });
                    return MirOperand::Constant(MirConstant::Unit);
                };
                self.lower_object_field_get_operand(object_operand, field_id, field)
            }
            HirExprKind::StoreField { object, field, value, field_id } => {
                let object_operand = self.lower_singleton_field_object(object).unwrap_or_else(|| self.lower_expr_to_operand(object));
                let storage = self.storage_for_object_operand(&object_operand);
                let layout_id = self.layout_id_for_object_operand(&object_operand);
                let field_type = self.field_type_for_object_operand(&object_operand, field);
                let value_operand = self.lower_expr_to_operand_with_hint(value, field_type.as_ref());
                let Some(field_id) = *field_id
                else {
                    self.diagnostics.push(super::MirDiagnostic::UnresolvedFieldIdentity { field: field.to_string() });
                    return MirOperand::Constant(MirConstant::Unit);
                };
                self.instructions.push(MirInstruction::from_operation(MirOperation::FieldSet {
                    object: object_operand,
                    field: field_id,
                    value: value_operand,
                }));
                MirOperand::Constant(MirConstant::Unit)
            }
            HirExprKind::Return(value) => {
                let return_ty = self.current_return_type.clone();
                let terminand = value.as_deref().map(|e| self.lower_expr_to_operand_with_hint(e, Some(&return_ty)));
                self.lower_explicit_return(terminand);
                MirOperand::Constant(MirConstant::Unit)
            }
            HirExprKind::Assign { target, value } => {
                let name = target.as_str().to_string();
                let existing = self.bindings.get(&name).cloned();
                let expected = existing.as_ref().and_then(|operand| infer_builder_operand_type(operand, &self.value_types));
                let operand = self.lower_expr_to_operand_with_hint(value, expected.as_ref());
                if let Some(ty) = infer_builder_operand_type(&operand, &self.value_types) {
                    if self.storage_for_type(&ty) == MirStorageKind::Value {
                        let layout_id = ensure_layout_for_type(&mut self.aggregate_layouts, &ty);
                        if let Some(layout_id) = layout_id {
                            let dest = self.next_value(MirValueOrigin::LetBinding { name: name.clone() });
                            self.push_instruction(
                                MirOperation::AggregateCopy { source: operand.clone(), dest: MirOperand::Value(dest) },
                                vec![dest],
                            );
                            self.value_types.insert(dest, ty);
                            self.bindings.insert(name, MirOperand::Value(dest));
                            return MirOperand::Constant(MirConstant::Unit);
                        }
                    }
                }
                let new_value = self.next_value(MirValueOrigin::LetBinding { name: name.clone() });
                self.push_instruction(MirOperation::StoreVar { name: name.clone(), value: operand.clone(), ty: None }, vec![new_value]);
                if let Some(ty) = infer_builder_operand_type(&operand, &self.value_types) {
                    self.value_types.insert(new_value, ty);
                }
                self.bindings.insert(name, MirOperand::Value(new_value));
                MirOperand::Constant(MirConstant::Unit)
            }
            HirExprKind::If { condition, then_branch, else_branch } => self.lower_if_expr(condition, then_branch, else_branch, expected_type),
            HirExprKind::IfLet { pattern, scrutinee, then_branch, else_branch } => {
                self.lower_if_let_expr(pattern, scrutinee, then_branch, else_branch)
            }
            HirExprKind::Block(body) => self.lower_block_expr(body),
            HirExprKind::Loop { label, pattern, iterator, condition, body, .. } => {
                self.lower_loop_expr(label, pattern, iterator, condition, body)
            }
            HirExprKind::Break { label, expr } => self.lower_break_expr(label, expr),
            HirExprKind::Continue { label } => self.lower_continue_expr(label),
            HirExprKind::Match { scrutinee, arms } => self.lower_match_expr(scrutinee, arms, expected_type),
            HirExprKind::Case { scrutinee, arms } => self.lower_case_expr(scrutinee, arms, expected_type),
            HirExprKind::Yield(value) => self.lower_yield_expr(value.as_deref()),
            HirExprKind::YieldFrom(value) => self.lower_yield_from_expr(value),
            HirExprKind::Await(value) => self.lower_await_expr(value),
            HirExprKind::Awake(value) => self.lower_awake_expr(value),
            HirExprKind::BlockOn(value) => self.lower_block_on_expr(value),
            HirExprKind::Raise(value) => self.lower_raise_expr(value),
            HirExprKind::Resume(value) => self.lower_resume_expr(value),
            HirExprKind::Catch { expr, arms } => self.lower_catch_expr(expr, arms),
            HirExprKind::TryPropagate { inner, resolved_ok_variant, .. } => self.lower_try_propagate_expr(inner, resolved_ok_variant.as_ref()),
            HirExprKind::TryScope { is_optional, is_forced, result_type, body } => {
                self.lower_try_scope_expr(*is_optional, *is_forced, result_type, body)
            }
            HirExprKind::AnonymousClass { class_name: Some(class_name), fields, captures, .. } => {
                self.lower_anonymous_class_expr(class_name, fields, captures)
            }
            HirExprKind::Fallthrough => self.lower_fallthrough_expr(),
            HirExprKind::Cast { expr, ty } => {
                let operand = self.lower_expr_to_operand(expr);
                if let MirOperand::Value(value_ref) = operand {
                    if self.value_types.get(&value_ref) == Some(ty) {
                        return MirOperand::Value(value_ref);
                    }
                    // `expr as T` 必须保留为 Cast：Copy 要求源/结果同 TypeId，不能用来偷换类型。
                    let casted = self.next_value(MirValueOrigin::Temporary);
                    self.push_instruction(MirOperation::Cast { source: MirOperand::Value(value_ref), ty: ty.clone() }, vec![casted]);
                    self.value_types.insert(casted, ty.clone());
                    MirOperand::Value(casted)
                }
                else {
                    operand
                }
            }
            _ => {
                self.diagnostics
                    .push(super::MirDiagnostic::UnsupportedExpression { span: expr.span.clone(), kind: format!("{:?}", expr.kind) });
                let value = self.next_value(MirValueOrigin::Temporary);
                self.instructions.push(MirInstruction::from_operation(MirOperation::LoadConstant {
                    constant: MirConstant::Unit,
                    ty: Some(ValkyrieType::Unit),
                }));
                self.value_types.insert(value, ValkyrieType::Unit);
                MirOperand::Value(value)
            }
        }
    }
}

pub(super) fn infer_array_literal_element_type(items: &[HirExpr], hint: Option<&ValkyrieType>) -> ValkyrieType {
    if let Some(hint) = hint {
        return hint.clone();
    }
    match items.first().map(|item| &item.kind) {
        Some(HirExprKind::Literal(HirLiteral::Bool(_))) => ValkyrieType::Boolean,
        Some(HirExprKind::Literal(HirLiteral::String(_))) => ValkyrieType::Utf8,
        Some(HirExprKind::Literal(HirLiteral::Float64(_))) => ValkyrieType::Float64,
        Some(HirExprKind::Literal(HirLiteral::Integer64(_))) => ValkyrieType::Integer32 { signed: true },
        _ => ValkyrieType::Integer32 { signed: true },
    }
}

pub(super) fn lower_literal(literal: &HirLiteral, expected_type: Option<&ValkyrieType>) -> (MirConstant, Option<ValkyrieType>) {
    match literal {
        HirLiteral::Integer64(value) => (
            MirConstant::Int(*value),
            Some(match expected_type {
                Some(
                    ty @ (ValkyrieType::Integer8 { .. }
                    | ValkyrieType::Integer16 { .. }
                    | ValkyrieType::Integer32 { .. }
                    | ValkyrieType::Integer64 { .. }
                    | ValkyrieType::Integer128 { .. }),
                ) => ty.clone(),
                // Named 数值形参（如 `usize`）在无 MirStruct 表时也可直接特化；Nominal 由调用方用声明表覆盖。
                Some(ty @ ValkyrieType::Named(_)) if super::expr_helpers::is_integer_literal_formal_type(ty, &[]) => ty.clone(),
                _ if *value >= i32::MIN as i64 && *value <= i32::MAX as i64 => ValkyrieType::Integer32 { signed: true },
                _ => ValkyrieType::Integer64 { signed: true },
            }),
        ),
        HirLiteral::Float64(value) => (MirConstant::Float64(*value), Some(ValkyrieType::Float64)),
        HirLiteral::Bool(value) => (MirConstant::Bool(*value), Some(ValkyrieType::Boolean)),
        HirLiteral::String(value) => (
            MirConstant::Utf8(
                value
                    .segments
                    .iter()
                    .map(|segment| match segment {
                        crate::types::hir::HirStringSegment::Text(text) => text.clone(),
                        crate::types::hir::HirStringSegment::Interpolation { expr, .. } => {
                            format!("${{{}}}", render_interpolation_expr(expr))
                        }
                    })
                    .collect::<String>(),
            ),
            Some(ValkyrieType::Utf8),
        ),
        HirLiteral::Unit => (MirConstant::Unit, Some(ValkyrieType::Unit)),
    }
}

fn render_interpolation_expr(expr: &HirExpr) -> String {
    match &expr.kind {
        HirExprKind::Variable(identifier) => identifier.name.to_string(),
        HirExprKind::Path(path) => path.to_string(),
        HirExprKind::Literal(HirLiteral::Integer64(value)) => value.to_string(),
        HirExprKind::Literal(HirLiteral::Bool(value)) => value.to_string(),
        HirExprKind::Literal(HirLiteral::String(value)) => value
            .segments
            .iter()
            .map(|segment| match segment {
                crate::types::hir::HirStringSegment::Text(text) => text.clone(),
                crate::types::hir::HirStringSegment::Interpolation { .. } => "${...}".to_string(),
            })
            .collect(),
        HirExprKind::Call { callee, .. } => format!("{}(...)", render_interpolation_expr(callee)),
        _ => "...".to_string(),
    }
}

/// Prefer the contextual sum apply when both sides already share Nominal declaration identity.
/// payload 须由调用方经 `parameter_types` / `instantiate_payload` 提供，禁止变体短名认亲。
fn concretize_variant_constructor_types(
    ctor_return: &ValkyrieType,
    payload_type: Option<ValkyrieType>,
    expected_type: Option<&ValkyrieType>,
    arguments: &[MirOperand],
    value_types: &std::collections::BTreeMap<super::MirValueRef, ValkyrieType>,
) -> (ValkyrieType, Option<ValkyrieType>) {
    let return_type = match expected_type {
        Some(expected) if same_sum_owner(ctor_return, expected) => expected.clone(),
        _ => ctor_return.clone(),
    };
    let from_argument = arguments.first().and_then(|argument| infer_builder_operand_type(argument, value_types));
    let payload_type = payload_type.or(from_argument);
    (return_type, payload_type)
}

fn same_sum_owner(left: &ValkyrieType, right: &ValkyrieType) -> bool {
    match (left.nominal_declaration(), right.nominal_declaration()) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// 失败诊断只引用已冻结的声明身份；缺失时给出稳定占位，禁止从拼写重发现 variant。
fn unresolved_variant_labels(resolved: Option<&HirResolvedCall>) -> (String, String) {
    let sum_type = resolved.and_then(|call| call.sum_owner).map_or_else(|| "<unresolved sum owner>".to_string(), |owner| owner.to_string());
    let variant =
        resolved.and_then(|call| call.sum_variant).map_or_else(|| "<unresolved sum variant>".to_string(), |variant| variant.to_string());
    (sum_type, variant)
}

/// 按 sum_owner + sum_variant + 已冻结实参代入 payload；禁止 Fine/Some/Fail 拼写选槽。
fn instantiated_payload_for_call(
    sum_types: &[super::MirSumDeclaration],
    call: &HirResolvedCall,
    sum_type: &ValkyrieType,
) -> Option<ValkyrieType> {
    let owner = call.sum_owner?;
    let variant_declaration = call.sum_variant?;
    let sum = super::MirSumDeclaration::find_by_declaration(sum_types, owner)?;
    let variant = sum.variants.iter().find(|item| item.declaration == Some(variant_declaration))?;
    let args = type_args_from_sum_shaped(sum_type);
    if args.is_empty() {
        return call.parameter_types.first().cloned().or_else(|| variant.payload_type());
    }
    match sum.instantiate_payload(variant, &args) {
        Some(payload) => payload.or_else(|| call.parameter_types.first().cloned()),
        None => call.parameter_types.first().cloned(),
    }
}

/// 将构造器 payload 写回其声明 binder 对应的 type arg 槽位。
fn apply_payload_to_sum_type_args(
    sum: &super::MirSumDeclaration,
    variant: &crate::mir::MirSumVariant,
    type_args: &mut Vec<ValkyrieType>,
    payload: &ValkyrieType,
) {
    let Some(declared_payload) = variant.payload_type()
    else {
        return;
    };
    let ValkyrieType::Generic(generic) = declared_payload
    else {
        return;
    };
    let Some(binder) = generic.binder
    else {
        return;
    };
    let Some(index) = sum.generics.iter().position(|item| item.binder == Some(binder))
    else {
        return;
    };
    if index < type_args.len() {
        type_args[index] = payload.clone();
    }
}

/// Map a resolved unite/enum constructor call onto `SumNew` parts.
fn sum_new_parts_from_constructor(
    call: &HirResolvedCall,
    arguments: &[MirOperand],
    sum_types: &[super::MirSumDeclaration],
) -> Option<(String, Vec<ValkyrieType>, String, Option<ValkyrieType>, Option<MirOperand>)> {
    let owner = call.sum_owner?;
    let variant_declaration = call.sum_variant?;
    let sum = sum_types.iter().find(|item| item.declaration == Some(owner))?;
    let variant = sum.variants.iter().find(|item| item.declaration == Some(variant_declaration))?;
    let (sum_type, type_args) = match &call.return_type {
        ValkyrieType::Nominal { declaration, arguments } if Some(*declaration) == sum.declaration => (sum.name.clone(), arguments.clone()),
        _ => return None,
    };
    if call.return_type.nominal_declaration() != sum.declaration {
        return None;
    }
    let payload_type = call.parameter_types.first().cloned();
    let payload = match arguments.len() {
        0 => None,
        1 => Some(arguments[0].clone()),
        _ => arguments.first().cloned(),
    };
    Some((sum_type, type_args, variant.name.clone(), payload_type, payload))
}

pub(super) fn type_args_from_sum_shaped(ty: &ValkyrieType) -> Vec<ValkyrieType> {
    match ty {
        ValkyrieType::Nominal { arguments, .. } => arguments.clone(),
        ValkyrieType::Apply(_, args) => args.clone(),
        ValkyrieType::Nullable(inner) => vec![(**inner).clone()],
        _ => Vec::new(),
    }
}

/// ?????? overload ????`Apply(Option, [AutoType])`?? payload ??????
fn refine_auto_sum_instantiation_args(type_args: &[ValkyrieType], payload_type: Option<&ValkyrieType>) -> Vec<ValkyrieType> {
    type_args
        .iter()
        .map(|arg| if matches!(arg, ValkyrieType::AutoType) { payload_type.cloned().unwrap_or_else(|| arg.clone()) } else { arg.clone() })
        .collect()
}

fn refine_auto_apply_type(ty: &ValkyrieType, payload_type: Option<&ValkyrieType>) -> ValkyrieType {
    match ty {
        ValkyrieType::Nominal { declaration, arguments } if arguments.iter().any(|arg| matches!(arg, ValkyrieType::AutoType)) => {
            ValkyrieType::Nominal { declaration: *declaration, arguments: refine_auto_sum_instantiation_args(arguments, payload_type) }
        }
        ValkyrieType::Apply(base, args) if args.iter().any(|arg| matches!(arg, ValkyrieType::AutoType)) => {
            ValkyrieType::Apply(base.clone(), refine_auto_sum_instantiation_args(args, payload_type))
        }
        _ => ty.clone(),
    }
}
