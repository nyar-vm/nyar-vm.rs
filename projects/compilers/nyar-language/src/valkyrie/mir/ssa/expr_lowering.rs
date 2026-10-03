use std::collections::BTreeMap;

use crate::{
    hir::is_nullable_type,
    types::{
        Identifier, NamePath,
        hir::{FunctionType, HirCallableDomain, HirExpr, HirExprKind, HirLiteral, HirResolvedCall, ValkyrieType},
    },
};
use nyar_types::IntrinsicId;

use super::{
    MirBuilder, MirConstant, MirInstruction, MirOperand, MirOperation, MirStorageKind, MirTerminator, MirValueOrigin, MirValueRef,
    builtin_helpers::{
        array_index_call_output_type, intrinsic_opcode_for_operator, intrinsic_opcode_output_type, language_operator_call_return_type,
        resolve_intrinsic_id,
    },
    callee_name_matches,
    expr_helpers::{
        is_array_shaped_valkyrie_type, named_type_name, peel_generic_apply,
        qualify_instance_method_symbol, receiver_method_owner_name, reject_text_operator_for_numeric_args,
    },
    infer_builder_operand_type, lower_callee_operand,
    value_semantics::{
        ensure_layout_for_type, layout_id_for_type, storage_kind_for_named_type, storage_kind_for_type,
    },
};

impl MirBuilder {
    fn callee_intrinsic_id(resolved: Option<&HirResolvedCall>, callee: &MirOperand) -> Option<IntrinsicId> {
        resolved
            .and_then(|call| resolve_intrinsic_id(&call.symbol))
            .or_else(|| match callee {
                MirOperand::Symbol(symbol) => resolve_intrinsic_id(symbol),
                _ => None,
            })
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

    fn option_sum_name(actual_type: &ValkyrieType) -> Option<&'static str> {
        match actual_type {
            ValkyrieType::Nullable(_) => Some("Option"),
            ValkyrieType::Named(name) if name.as_str() == "Option" => Some("Option"),
            ValkyrieType::Apply(base, _) if named_type_name(base.as_ref()) == Some("Option") => Some("Option"),
            _ => None,
        }
    }

    fn option_payload_type(actual_type: &ValkyrieType) -> Option<ValkyrieType> {
        match actual_type {
            ValkyrieType::Nullable(inner) => Some(*inner.clone()),
            ValkyrieType::Apply(_, args) => args.first().cloned(),
            _ => None,
        }
    }

    fn option_uses_generic_payload(actual_type: &ValkyrieType) -> bool {
        Self::option_payload_type(actual_type).is_some_and(|payload| {
            matches!(payload, ValkyrieType::Named(name) if name.as_str() == "T")
        })
    }

    fn infer_array_element_type(&self, array: &MirOperand) -> Option<ValkyrieType> {
        infer_builder_operand_type(array, &self.value_types).and_then(|ty| match ty {
            ValkyrieType::Array(inner) => Some(*inner),
            ValkyrieType::Apply(base, args) if named_type_name(base.as_ref()) == Some("Array") => args.first().cloned(),
            ValkyrieType::FixedArray { element, .. } => Some(*element),
            _ => None,
        })
    }

    fn try_lower_option_is_some(&mut self, receiver: MirOperand) -> Option<MirOperand> {
        let actual_type = infer_builder_operand_type(&receiver, &self.value_types).filter(|ty| Self::option_sum_name(ty).is_some())?;
        let sum_name = Self::option_sum_name(&actual_type)?.to_string();
        let variant = self.variant_id(&sum_name, "Some")?;
        let value = self.next_value(MirValueOrigin::CallResult);
        self.push_instruction(
            MirOperation::SumVariantIs {
                sum_type: sum_name,
                type_args: type_args_from_sum_shaped(&actual_type),
                variant,
                object: receiver,
            },
            vec![value],
        );
        self.value_types.insert(value, ValkyrieType::Boolean);
        Some(MirOperand::Value(value))
    }

    fn option_shaped_type(
        receiver: &MirOperand,
        value_types: &BTreeMap<MirValueRef, ValkyrieType>,
        hint: Option<&ValkyrieType>,
        payload_hint: Option<&ValkyrieType>,
    ) -> Option<ValkyrieType> {
        infer_builder_operand_type(receiver, value_types)
            .filter(|ty| Self::option_sum_name(ty).is_some())
            .or_else(|| hint.cloned().filter(|ty| Self::option_sum_name(ty).is_some()))
            .or_else(|| {
                payload_hint.map(|payload| {
                    ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Option"))), vec![payload.clone()])
                })
            })
    }

    /// Option 结构操作只接受 HIR 已解析的 extractor 合同。
    fn is_option_unwrap_call(resolved: Option<&HirResolvedCall>) -> bool {
        resolved.is_some_and(|call| matches!(call.domain, HirCallableDomain::Extractor) || call.extractor_payload_type.is_some())
    }

    fn is_option_is_some_call(resolved: Option<&HirResolvedCall>) -> bool {
        resolved.is_some_and(|call| {
            matches!(call.return_type, ValkyrieType::Boolean)
                && call.parameter_types.first().is_some_and(|ty| Self::option_sum_name(ty).is_some())
        })
    }

    fn try_lower_option_unwrap(
        &mut self,
        receiver: MirOperand,
        hint: Option<&ValkyrieType>,
        payload_hint: Option<&ValkyrieType>,
    ) -> Option<MirOperand> {
        let actual_type = Self::option_shaped_type(&receiver, &self.value_types, hint, payload_hint)?;
        let sum_name = Self::option_sum_name(&actual_type)?.to_string();
        let payload_type = Self::option_payload_type(&actual_type).or_else(|| payload_hint.cloned())?;
        let variant = self.variant_id(&sum_name, "Some")?;
        if let MirOperand::Value(receiver_ref) = &receiver {
            if self.value_types.get(receiver_ref).is_none_or(|ty| Self::option_sum_name(ty).is_none()) {
                self.value_types.insert(
                    *receiver_ref,
                    ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Option"))), vec![payload_type.clone()]),
                );
            }
        }
        let value = self.next_value(MirValueOrigin::CallResult);
        self.push_instruction(
            MirOperation::SumPayloadGet {
                sum_type: sum_name,
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
        let return_type = resolved
            .map(|call| call.return_type.clone())
            .or_else(|| expected_type.cloned())
            .or_else(|| {
                (!explicit_generic_arguments.is_empty()).then(|| {
                    ValkyrieType::Apply(
                        Box::new(ValkyrieType::Named(Identifier::new("Option"))),
                        explicit_generic_arguments.to_vec(),
                    )
                })
            })?;
        if Self::option_sum_name(&return_type).is_none() {
            return None;
        }
        let variant = self.variant_id("Option", "None")?;
        let value = self.next_value(MirValueOrigin::CallResult);
        self.push_instruction(
            MirOperation::SumNew {
                sum_type: "Option".to_string(),
                type_args: type_args_from_sum_shaped(&return_type),
                variant,
                payload_type: None,
                payload: None,
            },
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
        let variant = self.variant_id("Option", "Some")?;
        let payload_operand = self.lower_expr_to_operand(payload_expr);
        let payload_type = infer_builder_operand_type(&payload_operand, &self.value_types)
            .or_else(|| resolved.and_then(|call| call.parameter_types.first().cloned()));
        let return_type = resolved
            .map(|call| call.return_type.clone())
            .or_else(|| expected_type.cloned())
            .or_else(|| {
                payload_type.clone().map(|payload| {
                    ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Option"))), vec![payload])
                })
            })?;
        let value = self.next_value(MirValueOrigin::CallResult);
        self.push_instruction(
            MirOperation::SumNew {
                sum_type: "Option".to_string(),
                type_args: type_args_from_sum_shaped(&return_type),
                variant,
                payload_type: payload_type.clone(),
                payload: Some(payload_operand),
            },
            vec![value],
        );
        self.value_types.insert(value, return_type);
        Some(MirOperand::Value(value))
    }

    fn emit_array_length_operand(&mut self, array: MirOperand) -> MirOperand {
        let value = self.next_value(MirValueOrigin::CallResult);
        // 必须绑定 results：`from_operation` 空 results 会让 wasm 端 `array.len` 后丢弃并返回 0。
        self.push_instruction(MirOperation::ArrayLength { array }, vec![value]);
        self.value_types.insert(value, ValkyrieType::Named(Identifier::new("usize")));
        MirOperand::Value(value)
    }

    /// `receiver._items.length` and similar: SSA types may be erased on the `_items`
    /// temp while HIR still knows the field is array-shaped (`ArrayList.length` body).
    fn try_emit_array_length_field_access(&mut self, object: &HirExpr, object_operand: &MirOperand) -> Option<MirOperand> {
        if let Some(ty) = infer_builder_operand_type(object_operand, &self.value_types) {
            if is_array_shaped_valkyrie_type(&ty) {
                return Some(self.emit_array_length_operand(object_operand.clone()));
            }
        }
        if let HirExprKind::FieldAccess { object: receiver, field } = &object.kind {
            let receiver_operand = self.lower_expr_to_operand(receiver);
            if let Some(items_ty) = self.field_type_for_object_operand(&receiver_operand, field) {
                if is_array_shaped_valkyrie_type(&items_ty) {
                    return Some(self.emit_array_length_operand(object_operand.clone()));
                }
            }
        }
        None
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
        let value = self.next_value(MirValueOrigin::CallResult);
        self.push_instruction(MirOperation::ArrayLength { array }, vec![value]);
        let return_type = resolved
            .map(|call| call.return_type.clone())
            .or_else(|| expected_type.cloned())
            .unwrap_or_else(|| ValkyrieType::Named(Identifier::new("usize")));
        self.value_types.insert(value, return_type);
        Some(MirOperand::Value(value))
    }

    fn try_lower_array_get_on_array(
        &mut self,
        array: MirOperand,
        ordinal: MirOperand,
        resolved: Option<&HirResolvedCall>,
        expected_type: Option<&ValkyrieType>,
        wraps_option: bool,
    ) -> Option<MirOperand> {
        let array_element_type = self.infer_array_element_type(&array);
        let one = MirOperand::Constant(MirConstant::Int(1));
        let index_value = self.next_value(MirValueOrigin::CallResult);
        self.push_instruction(
            MirOperation::Call {
                callee: MirOperand::Symbol(NamePath::new(vec![Identifier::new("infix -")])),
                arguments: vec![ordinal, one],
            },
            vec![index_value],
        );
        self.value_types.insert(index_value, ValkyrieType::Integer32 { signed: true });
        let element_value = self.next_value(MirValueOrigin::CallResult);
        self.push_instruction(
            MirOperation::ArrayGet { array, index: MirOperand::Value(index_value) },
            vec![element_value],
        );
        let element_type = array_element_type
            .clone()
            .or_else(|| expected_type.and_then(|ty| Self::option_payload_type(ty)));
        if let Some(element_type) = element_type.clone() {
            self.value_types.insert(element_value, element_type);
        }
        if wraps_option {
            let payload_type = element_type
                .clone()
                .or(array_element_type.clone())
                .or_else(|| resolved.and_then(|call| Self::option_payload_type(&call.return_type)));
            let return_type = expected_type
                .cloned()
                .filter(|ty| Self::option_sum_name(ty).is_some())
                .or_else(|| {
                    resolved
                        .map(|call| call.return_type.clone())
                        .filter(|ty| Self::option_sum_name(ty).is_some() && !Self::option_uses_generic_payload(ty))
                })
                .or_else(|| {
                    payload_type.clone().map(|payload| {
                        ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Option"))), vec![payload])
                    })
                })?;
            let option_value = self.next_value(MirValueOrigin::CallResult);
            let variant = self.variant_id("Option", "Some")?;
            self.push_instruction(
                MirOperation::SumNew {
                    sum_type: "Option".to_string(),
                    type_args: type_args_from_sum_shaped(&return_type),
                    variant,
                    payload_type,
                    payload: Some(MirOperand::Value(element_value)),
                },
                vec![option_value],
            );
            self.value_types.insert(option_value, return_type);
            return Some(MirOperand::Value(option_value));
        }
        let return_type = resolved
            .map(|call| call.return_type.clone())
            .or_else(|| expected_type.cloned())
            .or(element_type)
            .unwrap_or_else(|| ValkyrieType::Named(Identifier::new("i32")));
        self.value_types.insert(element_value, return_type);
        Some(MirOperand::Value(element_value))
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
        // Option 包装由返回类型合同决定，不按 `Array.get` 方法名猜。
        let wraps_option = resolved
            .map(|call| Self::option_sum_name(&call.return_type).is_some())
            .unwrap_or_else(|| expected_type.is_some_and(|ty| Self::option_sum_name(ty).is_some()));
        let array = arguments.first().cloned()?;
        let ordinal = arguments.get(1).cloned()?;
        self.try_lower_array_get_on_array(array, ordinal, resolved, expected_type, wraps_option)
    }

    /// 仅当 callee 已由 overload 绑定为 [`IntrinsicId::ArrayPush`] 时降低。
    /// 禁止按表面名 `push` 字符串特判（ADR 0013）。
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
        // 使用已解析合同上的符号（IntrinsicId 路径），不得再拼写表面名。
        let callee_symbol = resolved
            .map(|call| call.symbol.clone())
            .or_else(|| match callee {
                MirOperand::Symbol(path) => Some(path.clone()),
                _ => None,
            })?;
        self.push_instruction(
            MirOperation::Call {
                callee: MirOperand::Symbol(callee_symbol),
                arguments: arguments.to_vec(),
            },
            vec![value],
        );
        let return_type = resolved
            .map(|call| call.return_type.clone())
            .filter(|ty| is_array_shaped_valkyrie_type(ty))
            .or_else(|| expected_type.filter(|ty| is_array_shaped_valkyrie_type(ty)).cloned())
            .unwrap_or(array_ty);
        self.value_types.insert(value, return_type);
        Some(MirOperand::Value(value))
    }

    fn field_type_for_semantic_type(&self, ty: &ValkyrieType, field: &str) -> Option<ValkyrieType> {
        let base = match ty { ValkyrieType::Apply(base, _) => base.as_ref(), other => other };
        let ValkyrieType::Named(owner) = base else { return None; };
        let mut declarations = self.field_declarations.iter().filter(|declaration| declaration.qualified_name() == owner.as_str());
        let declaration = declarations.next()?;
        if declarations.next().is_some() { return None; }
        declaration.instantiate_field(ty, field)
    }

    /// Resolve `Nyar`/`Valkyrie` function type for a call callee (Value or named Symbol).
    fn function_type_of_callee(&self, callee: &MirOperand) -> Option<FunctionType> {
        match callee {
            MirOperand::Value(value) => match self.value_types.get(value) {
                Some(ValkyrieType::Function(func)) => Some(func.as_ref().clone()),
                _ => None,
            },
            MirOperand::Symbol(path) if path.parts().len() == 1 => {
                let name = path.parts()[0].as_str();
                if let Some(MirOperand::Value(value)) = self.bindings.get(name) {
                    return match self.value_types.get(value) {
                        Some(ValkyrieType::Function(func)) => Some(func.as_ref().clone()),
                        _ => None,
                    };
                }
                self.values.iter().find_map(|value| match &value.origin {
                    MirValueOrigin::Parameter { name: n, .. }
                    | MirValueOrigin::LetBinding { name: n }
                    | MirValueOrigin::BlockParameter { name: n, .. }
                        if n == name =>
                    {
                        match self.value_types.get(&value.id) {
                            Some(ValkyrieType::Function(func)) => Some(func.as_ref().clone()),
                            _ => None,
                        }
                    }
                    _ => None,
                })
            }
            _ => None,
        }
    }

    fn storage_for_type(&self, ty: &ValkyrieType) -> MirStorageKind {
        let value_names = self.aggregate_layouts.value_type_names.iter().map(|name| Identifier::new(name)).collect();
        storage_kind_for_type(ty, &value_names)
    }

    fn storage_for_object_operand(&self, operand: &MirOperand) -> MirStorageKind {
        infer_builder_operand_type(operand, &self.value_types).map(|ty| self.storage_for_type(&ty)).unwrap_or(MirStorageKind::Reference)
    }

    fn layout_id_for_type(&mut self, ty: &ValkyrieType) -> Option<super::LayoutId> {
        if let Some(id) = layout_id_for_type(ty, &self.aggregate_layouts) {
            return Some(id);
        }
        match ty {
            ValkyrieType::Named(name) => {
                None
            }
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
                // Bare nullary unite arms (`return None`) arrive as unbound
                // Variable, not Constructor Call. Emit typed SumNew so SMIR007
                // sees Option/Result SSA types instead of untyped Symbol.
                if let Some(operand) = self.try_lower_nullary_sum_variant(identifier.name.as_str(), expected_type) {
                    return operand;
                }
                MirOperand::Symbol(NamePath::new(vec![identifier.name.clone()]))
            }
            HirExprKind::Path(path) => {
                if path.parts().len() == 1 {
                    let name = path.parts()[0].as_str();
                    if let Some(operand) = self.try_lower_nullary_sum_variant(name, expected_type) {
                        return operand;
                    }
                }
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
            HirExprKind::Call { callee, args, resolved } => {
                let (explicit_generic_arguments, callee) = peel_generic_apply(callee.as_ref());
                // HIR may lower `expr.unwrap()` to `unwrap(expr)` (functional call).
                // 先要求 Option 形接收者，再按 Extractor 合同（或迁移显示名）进入结构操作。
                if args.len() == 1 && Self::is_option_unwrap_call(resolved.as_ref()) {
                    let receiver_operand = self.lower_expr_to_operand(&args[0].value);
                    let payload_hint = resolved
                        .as_ref()
                        .map(|call| call.return_type.clone())
                        .or_else(|| expected_type.cloned());
                    let hint = infer_builder_operand_type(&receiver_operand, &self.value_types)
                        .filter(|ty| Self::option_sum_name(ty).is_some())
                        .or_else(|| {
                            payload_hint.as_ref().map(|payload| {
                                ValkyrieType::Apply(
                                    Box::new(ValkyrieType::Named(Identifier::new("Option"))),
                                    vec![payload.clone()],
                                )
                            })
                        });
                    if let Some(operand) =
                        self.try_lower_option_unwrap(receiver_operand.clone(), hint.as_ref(), payload_hint.as_ref())
                    {
                        return operand;
                    }
                }
                if callee_name_matches(&callee.kind, "Some") && args.len() == 1 {
                    if let Some(operand) = self.try_lower_option_some_constructor(&args[0].value, resolved.as_ref(), expected_type) {
                        return operand;
                    }
                }
                if callee_name_matches(&callee.kind, "option_none") && args.is_empty() {
                    if let Some(operand) =
                        self.try_lower_option_none_constructor(resolved.as_ref(), expected_type, &explicit_generic_arguments)
                    {
                        return operand;
                    }
                }
                if callee_name_matches(&callee.kind, "tuple") {
                    let fields = args.iter().map(|arg| self.lower_expr_to_operand(&arg.value)).collect::<Vec<_>>();
                    let element_types = if let Some(types) = resolved.as_ref().and_then(|call| match &call.return_type {
                        ValkyrieType::Tuple(types) => Some(types.clone()),
                        _ => None,
                    }) {
                        types
                    }
                    else {
                        let Some(types) = fields.iter().map(|operand| infer_builder_operand_type(operand, &self.value_types)).collect::<Option<Vec<_>>>() else {
                            self.diagnostics.push(super::MirDiagnostic::UnresolvedValueType { context: "tuple 构造元素".to_string() });
                            return MirOperand::Constant(MirConstant::Unit);
                        };
                        types
                    };
                    let tuple_type =
                        resolved.as_ref().map(|call| call.return_type.clone()).unwrap_or_else(|| ValkyrieType::Tuple(element_types.clone()));
                    let storage = MirStorageKind::Value;
                    let layout_id = self.layout_id_for_type(&tuple_type);
                    let value = self.next_value(MirValueOrigin::Temporary);
                    self.instructions.push(MirInstruction::from_operation(MirOperation::TupleNew { fields }));
                    self.value_types.insert(value, tuple_type);
                    return MirOperand::Value(value);
                }
                if let Some((receiver_operand, _method_name)) = self.extract_method_call(callee)
                    .filter(|_| resolved.as_ref().is_some_and(|call| call.has_receiver))
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
                    if args.is_empty() && Self::is_option_is_some_call(resolved.as_ref()) {
                        if Self::option_shaped_type(&receiver_operand, &self.value_types, None, None).is_some() {
                            if let Some(operand) = self.try_lower_option_is_some(receiver_operand.clone()) {
                                return operand;
                            }
                        }
                    }
                    if args.is_empty() && Self::is_option_unwrap_call(resolved.as_ref()) {
                        let payload_hint = resolved
                            .as_ref()
                            .map(|call| call.return_type.clone())
                            .or_else(|| expected_type.cloned());
                        // 仅采纳已是 Option 形的接收者类型；非 Option 推断不得挡住 payload 回退。
                        let hint = infer_builder_operand_type(&receiver_operand, &self.value_types)
                            .filter(|ty| Self::option_sum_name(ty).is_some())
                            .or_else(|| {
                                payload_hint.as_ref().map(|payload| {
                                    ValkyrieType::Apply(
                                        Box::new(ValkyrieType::Named(Identifier::new("Option"))),
                                        vec![payload.clone()],
                                    )
                                })
                            });
                        if let Some(operand) =
                            self.try_lower_option_unwrap(receiver_operand.clone(), hint.as_ref(), payload_hint.as_ref())
                        {
                            return operand;
                        }
                    }
                    // Call 不得携带 dispatch / witness / evidence / intrinsic / parameter_types。
                    let (callee_symbol, return_type) = qualify_instance_method_symbol(resolved.as_ref());
                    let callee = MirOperand::Symbol(callee_symbol);
                    if let Some(operand) = self.try_lower_ref_deref_intrinsic(resolved.as_ref(), &callee, &arguments) {
                        return operand;
                    }
                    if let Some(operand) = self.try_lower_array_len_intrinsic(resolved.as_ref(), &callee, &arguments, expected_type) {
                        return operand;
                    }
                    if let Some(operand) = self.try_lower_array_get_intrinsic(resolved.as_ref(), &callee, &arguments, expected_type) {
                        return operand;
                    }
                    if let Some(operand) = self.try_lower_array_push_intrinsic(resolved.as_ref(), &callee, &arguments, expected_type) {
                        return operand;
                    }
                    return self.push_call_returning(callee, arguments, return_type.expect("resolved instance call return type"));
                }
                // `obj.field(args)` where `obj.field` is a function-typed field (e.g.
                // `FilterIterator._predicate`). Lower as indirect call: load the field
                // value via FieldGet, then `Call { dispatch: Indirect, callee: <value> }`.
                // This lets backends emit `Delegate::DynamicInvoke` / `call_ref` etc.
                // instead of treating the field name as a static method symbol.
                if let HirExprKind::FieldAccess { object, field } = &callee.kind {
                    let receiver_operand = self.lower_expr_to_operand(object);
                    let field_ty = self.field_type_for_object_operand(&receiver_operand, field);
                    if matches!(field_ty, Some(ValkyrieType::Function(_))) {
                        let callee_value = self.next_value(MirValueOrigin::Temporary);
                        self.push_instruction(
                            MirOperation::FieldGet {
                                object: receiver_operand,
                                field: field.clone(),
                            },
                            vec![callee_value],
                        );
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
                        let return_type = resolved
                            .as_ref()
                            .expect("函数类型字段调用必须有已解析调用合同")
                            .return_type
                            .clone();
                        // `unit` 不得占用物理 value 槽（BPHYS001）。
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
                    let callee = MirOperand::Symbol(
                        resolved.as_ref().expect("Semantic MIR requires a resolved field receiver call contract").symbol.clone(),
                    );
                    if let Some(operand) = self.try_lower_ref_deref_intrinsic(resolved.as_ref(), &callee, &arguments) {
                        return operand;
                    }
                    if let Some(operand) = self.try_lower_array_len_intrinsic(resolved.as_ref(), &callee, &arguments, expected_type) {
                        return operand;
                    }
                    if let Some(operand) = self.try_lower_array_get_intrinsic(resolved.as_ref(), &callee, &arguments, expected_type) {
                        return operand;
                    }
                    if let Some(operand) = self.try_lower_array_push_intrinsic(resolved.as_ref(), &callee, &arguments, expected_type) {
                        return operand;
                    }
                    let return_type = resolved
                        .as_ref()
                        .expect("字段调用必须有已解析调用合同")
                        .return_type
                        .clone();
                    // `unit` 不得占用物理 value 槽（BPHYS001）。
                    return self.push_call_returning(callee, arguments, return_type);
                }
                let is_prefix_not = callee_name_matches(&callee.kind, "prefix !");
                let callee = lower_callee_operand(callee, resolved.as_ref(), self);
                let param_types = resolved.as_ref().map(|call| call.parameter_types.as_slice());
                // Lower args left-to-right so `push([T], Variant { … })` can hint the variant
                // Construct with element type T (shared names like Label/Goto/Field).
                let mut arguments = Vec::with_capacity(args.len());
                for (index, arg) in args.iter().enumerate() {
                    let hint_owned: Option<ValkyrieType> = param_types
                        .and_then(|params| params.get(index).cloned())
                        .or_else(|| {
                            // Preserve contextual typing for a nested call when
                            // overload resolution did not provide a formal
                            // parameter record. The context comes from the
                            // enclosing semantic expression, never from a
                            // backend stack shape or a callee name.
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
                let callee = reject_text_operator_for_numeric_args(callee, &arguments, &self.value_types);
                // `Fine(x)` / `Fail(e)` / `Some(v)` resolve as Constructor calls.
                // Emit `SumNew` so SMIR003 does not demand a fake `Result.Fail`
                // function registry entry — unite construction is not a call.
                if let Some(call) = resolved.as_ref() {
                    if call.domain == HirCallableDomain::Constructor {
                        if let Some((sum_type, type_args, variant, payload_type, payload)) =
                            sum_new_parts_from_constructor(call, &arguments, &self.sum_types)
                        {
                            let value = self.next_value(MirValueOrigin::CallResult);
                            let Some(variant_id) = self.variant_id(&sum_type, &variant) else {
                                return MirOperand::Constant(MirConstant::Unit);
                            };
                            let (return_type, payload_type) = concretize_variant_constructor_types(
                                &call.return_type,
                                payload_type,
                                &variant,
                                expected_type,
                                &arguments,
                                &self.value_types,
                            );
                            self.push_instruction(
                                MirOperation::SumNew { sum_type, type_args, variant: variant_id, payload_type, payload },
                                vec![value],
                            );
                            self.value_types.insert(value, return_type);
                            return MirOperand::Value(value);
                        }
                    }
                }
                let function_ty = self.function_type_of_callee(&callee);
                if let Some(operand) = self.try_lower_ref_deref_intrinsic(resolved.as_ref(), &callee, &arguments) {
                    return operand;
                }
                if arguments.len() == 1 {
                    if let MirOperand::Symbol(path) = &callee {
                        let surface = path.parts().last().map(|part| part.as_str());
                        if Self::is_option_is_some_call(resolved.as_ref())
                            && Self::option_shaped_type(&arguments[0], &self.value_types, None, None).is_some()
                        {
                            if let Some(operand) = self.try_lower_option_is_some(arguments[0].clone()) {
                                return operand;
                            }
                        }
                    }
                }
                if let Some(operand) = self.try_lower_array_len_intrinsic(resolved.as_ref(), &callee, &arguments, expected_type) {
                    return operand;
                }
                if let Some(operand) = self.try_lower_array_get_intrinsic(resolved.as_ref(), &callee, &arguments, expected_type) {
                    return operand;
                }
                if let Some(operand) = self.try_lower_array_push_intrinsic(resolved.as_ref(), &callee, &arguments, expected_type) {
                    return operand;
                }
                // Call 仅含 { callee, arguments }；禁止 intrinsic / dispatch / generic 旁路。
                // `unit` 不得占用物理 value 槽（BPHYS001）——一律经 `push_call_returning`。
                let Some(return_type) = array_index_call_output_type(&arguments, &self.value_types)
                    .or_else(|| function_ty.map(|func| func.return_type))
                    .or_else(|| resolved.as_ref().map(|call| call.return_type.clone()))
                    .or_else(|| match &callee {
                        MirOperand::Symbol(path) => language_operator_call_return_type(path, &arguments, &self.value_types),
                        _ => None,
                    })
                    .or_else(|| expected_type.cloned())
                else {
                    self.diagnostics.push(super::MirDiagnostic::UnresolvedValueType { context: format!("调用结果 `{:?}`", callee) });
                    return MirOperand::Constant(MirConstant::Unit);
                };
                self.push_call_returning(callee, arguments, return_type)
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
                // 表面字面量 → ArrayFromElements；定长与 Array 只由 array_type 区分。
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
                self.push_instruction(
                    MirOperation::ArrayFromElements { array_type: array_type.clone(), elements },
                    vec![array_value],
                );
                self.value_types.insert(array_value, array_type);
                MirOperand::Value(array_value)
            }
            HirExprKind::Construct { path, name, args, resolved } => {
                let struct_type_name = self.struct_new_owner_name(name);
                let mut fields = Vec::with_capacity(args.len());
                let mut field_values = Vec::with_capacity(args.len());
                for (arg_index, arg) in args.iter().enumerate() {
                    if let HirExprKind::FieldInit { name: field_name, value } = &arg.kind {
                        // Empty `[]` without a hint defaults to `i32[]` (see
                        // `infer_array_literal_element_type`). Pass the struct field type so
                        // `visited: [utf8] = []` lowers as `string[]`, not `int32[]`.
                        // Prefer the nominal field registry.  For generated
                        // structures whose declaration is imported through a
                        // resolved constructor, carry the selected formal type
                        // by field position as explicit HIR metadata; do not
                        // infer a sum from the variant name.
                        let field_ty = self
                            .lookup_struct_field_type(struct_type_name.as_str(), field_name.as_str())
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
                        field_values.push(value_operand.clone());
                        fields.push((field_name.to_string(), value_operand));
                    }
                }
                // Unite/enum constructors (`Fail(e)`, `Integer { value }`) lower as
                // `SumNew`, not static Calls — there is no `Result.Fail` FunctionDef.
                let is_sum_variant = self.sum_types.iter().any(|sum| sum.variants.iter().any(|variant| variant.name == name.as_str()));
                if is_sum_variant {
                    let value = self.next_value(MirValueOrigin::CallResult);
                    let ctor_return = resolved
                        .as_ref()
                        .map(|call| call.return_type.clone())
                        .or_else(|| expected_type.cloned())
                        .or_else(|| {
                            if path.parts().len() >= 2 {
                                return path.parts().get(path.parts().len() - 2).cloned().map(ValkyrieType::Named);
                            }
                            let owners: Vec<&str> = self
                                .sum_types
                                .iter()
                                .filter(|sum| sum.variants.iter().any(|variant| variant.name == name.as_str()))
                                .map(|sum| sum.name.as_str())
                                .collect();
                            if owners.len() == 1 { Some(ValkyrieType::Named(Identifier::new(owners[0]))) } else { None }
                        })
                        .unwrap_or_else(|| ValkyrieType::Named(name.clone()));
                    let sum_type = match &ctor_return {
                        ValkyrieType::Named(n) => n.to_string(),
                        ValkyrieType::Apply(base, _) => match base.as_ref() {
                            ValkyrieType::Named(n) => n.to_string(),
                            _ => name.to_string(),
                        },
                        _ => {
                            if path.parts().len() >= 2 {
                                path.parts()[path.parts().len() - 2].to_string()
                            }
                            else {
                                self.sum_types
                                    .iter()
                                    .find(|sum| sum.variants.iter().any(|variant| variant.name == name.as_str()))
                                    .map(|sum| sum.name.to_string())
                                    .unwrap_or_else(|| name.to_string())
                            }
                        }
                    };
                    let type_args = type_args_from_sum_shaped(&ctor_return);
                    let payload_type = resolved.as_ref().and_then(|call| call.parameter_types.first().cloned()).or_else(|| {
                        self.sum_types
                            .iter()
                            .find(|sum| sum.name.as_str() == sum_type)
                            .and_then(|sum| sum.variants.iter().find(|variant| variant.name == name.as_str()))
                            .and_then(|variant| variant.payload_type())
                    });
                    let payload = match field_values.len() {
                        0 => None,
                        1 => Some(field_values[0].clone()),
                        _ => field_values.first().cloned(),
                    };
                    let variant = name.to_string();
                    let Some(variant_id) = self.variant_id(&sum_type, &variant) else {
                        return MirOperand::Constant(MirConstant::Unit);
                    };
                    let (return_type, payload_type) = concretize_variant_constructor_types(
                        &ctor_return,
                        payload_type,
                        &variant,
                        expected_type,
                        &field_values,
                        &self.value_types,
                    );
                    self.push_instruction(
                        MirOperation::SumNew { sum_type, type_args, variant: variant_id, payload_type, payload },
                        vec![value],
                    );
                    self.value_types.insert(value, return_type);
                    return MirOperand::Value(value);
                }
                let value = self.next_value(MirValueOrigin::Temporary);
                self.push_instruction(MirOperation::StructNew { type_name: NamePath::new(vec![Identifier::new(&struct_type_name)]), fields: fields.into_iter().map(|(name, value)| (Identifier::new(&name), value)).collect() }, vec![value]);
                self.value_types.insert(value, self.struct_construct_result_type(name, resolved.as_ref()));
                MirOperand::Value(value)
            }
            HirExprKind::FieldAccess { object, field } => {
                let object_operand = self.lower_singleton_field_object(object).unwrap_or_else(|| self.lower_expr_to_operand(object));
                // 数组长度属性：仅当对象已是 array-shaped 时映射到 ArrayLength（结构操作）。
                // 词素 `length` 是迁移期表面语法；正式合同为 IntrinsicId::ArrayLen / FieldId。
                // 禁止对非数组对象按短名猜 ArrayLen。
                if field.as_str() == "length" {
                    if let Some(operand) = self.try_emit_array_length_field_access(object, &object_operand) {
                        return operand;
                    }
                }
                self.lower_object_field_operand(object_operand, field)
            }
            HirExprKind::StoreField { object, field, value } => {
                let object_operand = self.lower_singleton_field_object(object).unwrap_or_else(|| self.lower_expr_to_operand(object));
                let storage = self.storage_for_object_operand(&object_operand);
                let layout_id = self.layout_id_for_object_operand(&object_operand);
                let field_type = self.field_type_for_object_operand(&object_operand, field);
                let value_operand = self.lower_expr_to_operand_with_hint(value, field_type.as_ref());
                self.instructions.push(MirInstruction::from_operation(MirOperation::FieldSet {
                    object: object_operand,
                    field: field.clone(),
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
                            self.push_instruction(MirOperation::AggregateCopy {
                                source: operand.clone(),
                                dest: MirOperand::Value(dest),
                            }, vec![dest]);
                            self.value_types.insert(dest, ty);
                            self.bindings.insert(name, MirOperand::Value(dest));
                            return MirOperand::Constant(MirConstant::Unit);
                        }
                    }
                }
                let new_value = self.next_value(MirValueOrigin::LetBinding { name: name.clone() });
                self.push_instruction(MirOperation::StoreVar {
                    name: name.clone(),
                    value: operand.clone(),
                    ty: None,
                }, vec![new_value]);
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
            HirExprKind::Match { scrutinee, arms } => self.lower_match_expr(scrutinee, arms),
            HirExprKind::Case { scrutinee, arms } => self.lower_case_expr(scrutinee, arms),
            HirExprKind::Yield(value) => self.lower_yield_expr(value.as_deref()),
            HirExprKind::YieldFrom(value) => self.lower_yield_from_expr(value),
            HirExprKind::Await(value) => self.lower_await_expr(value),
            HirExprKind::Awake(value) => self.lower_awake_expr(value),
            HirExprKind::BlockOn(value) => self.lower_block_on_expr(value),
            HirExprKind::Raise(value) => self.lower_raise_expr(value),
            HirExprKind::Resume(value) => self.lower_resume_expr(value),
            HirExprKind::Catch { expr, arms } => self.lower_catch_expr(expr, arms),
            HirExprKind::TryPropagate(expr) => self.lower_try_propagate_expr(expr),
            HirExprKind::TryScope { is_optional, is_forced, result_type, body } => {
                self.lower_try_scope_expr(*is_optional, *is_forced, result_type, body)
            }
            HirExprKind::AnonymousClass { class_name: Some(class_name), fields, captures, .. } => {
                self.lower_anonymous_class_expr(class_name, fields, captures)
            }
            HirExprKind::Fallthrough => self.lower_fallthrough_expr(),
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

    /// Lower an unbound nullary unite arm (`None`) to typed `SumNew`.
    ///
    /// Prefer contextual Option/Result-shaped type（形参 hint / 当前返回类型）。
    /// 无 hint 时，若全仓仅有一个匹配的无载荷变体所有者（典型：`None`→`Option`），
    /// 仍发出 `SumNew`，避免调用点留下裸 `Symbol("None")` 触发 BPHYS004。
    /// Bound locals still win over this path.
    fn try_lower_nullary_sum_variant(&mut self, variant_name: &str, expected_type: Option<&ValkyrieType>) -> Option<MirOperand> {
        let contextual = expected_type.cloned().or_else(|| {
            (is_option_shaped(&self.current_return_type) || is_result_shaped(&self.current_return_type))
                .then(|| self.current_return_type.clone())
        });
        let preferred_owner = contextual.as_ref().and_then(sum_owner_name);
        let sum_type = contextual
            .as_ref()
            .and_then(|contextual| {
                if !(is_option_shaped(contextual) || is_result_shaped(contextual)) {
                    return None;
                }
                self.sum_types
                    .iter()
                    .find(|sum| {
                        preferred_owner.is_some_and(|owner| sum.name.as_str() == owner)
                            && sum.variants.iter().any(|variant| variant.name == variant_name && variant.fields.is_empty())
                    })
                    .map(|sum| preferred_owner.map(str::to_string).unwrap_or_else(|| sum.name.clone()))
            })
            .or_else(|| {
                let matches: Vec<String> = self
                    .sum_types
                    .iter()
                    .filter(|sum| sum.variants.iter().any(|variant| variant.name == variant_name && variant.fields.is_empty()))
                    .map(|sum| sum.name.clone())
                    .collect();
                (matches.len() == 1).then(|| matches.into_iter().next().unwrap())
            })?;
        let contextual = contextual.unwrap_or_else(|| {
            // 无 hint：用唯一所有者构造最小 Apply（`Option`/`Result`），payload 槽留 Auto。
            ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new(sum_type.as_str()))), vec![ValkyrieType::AutoType])
        });
        if !(is_option_shaped(&contextual) || is_result_shaped(&contextual)) {
            return None;
        }
        let value = self.next_value(MirValueOrigin::CallResult);
        let (return_type, payload_type) =
            concretize_variant_constructor_types(&contextual, None, variant_name, Some(&contextual), &[], &self.value_types);
        let type_args = type_args_from_sum_shaped(&contextual);
        let variant = self.variant_id(&sum_type, variant_name)?;
        self.push_instruction(
            MirOperation::SumNew {
                sum_type: sum_type.clone(),
                type_args,
                variant,
                payload_type,
                payload: None,
            },
            vec![value],
        );
        self.value_types.insert(value, return_type);
        Some(MirOperand::Value(value))
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
                Some(ValkyrieType::Integer32 { signed }) => ValkyrieType::Integer32 { signed: *signed },
                Some(ValkyrieType::Integer64 { signed }) => ValkyrieType::Integer64 { signed: *signed },
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

/// Prefer the contextual Result/Option apply (`Result<Plan, E>`) over the
/// constructor's still-generic `Result<T, E>` so SMIR007 return checks match.
/// Fine/Fail also accept alias returns (`VonParseResult<T>`) that expand to the
/// same Result unite — do not require identical owner spelling.
fn concretize_variant_constructor_types(
    ctor_return: &ValkyrieType,
    payload_type: Option<ValkyrieType>,
    variant: &str,
    expected_type: Option<&ValkyrieType>,
    arguments: &[MirOperand],
    value_types: &std::collections::BTreeMap<super::MirValueRef, ValkyrieType>,
) -> (ValkyrieType, Option<ValkyrieType>) {
    let return_type = match expected_type {
        Some(expected) if should_prefer_expected_sum_return(variant, ctor_return, expected) => expected.clone(),
        _ => ctor_return.clone(),
    };
    let from_expected = payload_type_for_variant(&return_type, variant)
        .or_else(|| expected_type.and_then(|expected| payload_type_for_variant(expected, variant)));
    let from_argument = arguments.first().and_then(|argument| infer_builder_operand_type(argument, value_types));
    let payload_type = from_expected.or(from_argument).or(payload_type);
    (return_type, payload_type)
}

fn should_prefer_expected_sum_return(variant: &str, ctor_return: &ValkyrieType, expected: &ValkyrieType) -> bool {
    if same_sum_owner(ctor_return, expected) {
        return true;
    }
    // Built-in Result/Option arms: constructor may still say `Result<T, E>` while
    // the function is typed as a Result alias (`VonParseResult<T>`).
    matches!(variant, "Fine" | "Fail" | "Some" | "None" | "Ok" | "Err")
        && (is_result_shaped(expected) || is_result_shaped(ctor_return) || is_option_shaped(expected) || is_option_shaped(ctor_return))
}

fn is_result_shaped(ty: &ValkyrieType) -> bool {
    match ty {
        ValkyrieType::Named(name) => name.as_str() == "Result" || name.as_str().ends_with("Result"),
        ValkyrieType::Apply(base, _) => is_result_shaped(base),
        _ => false,
    }
}

fn is_option_shaped(ty: &ValkyrieType) -> bool {
    match ty {
        ValkyrieType::Named(name) => matches!(name.as_str(), "Option" | "Nullable"),
        ValkyrieType::Apply(base, _) => is_option_shaped(base),
        ValkyrieType::Nullable(_) => true,
        _ => false,
    }
}

/// When returning/calling under a Result/Option alias (`VonParseResult<T>`),
/// keep the contextual apply spelling so SMIR007 matches the function signature
/// instead of the expanded `Result<T, E>` owner.
fn prefer_contextual_sum_alias(ty: ValkyrieType, expected: Option<&ValkyrieType>) -> ValkyrieType {
    let Some(expected) = expected
    else {
        return ty;
    };
    if (is_result_shaped(&ty) && is_result_shaped(expected)) || (is_option_shaped(&ty) && is_option_shaped(expected)) {
        return expected.clone();
    }
    ty
}

fn same_sum_owner(left: &ValkyrieType, right: &ValkyrieType) -> bool {
    match (sum_owner_name(left), sum_owner_name(right)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

fn sum_owner_name(ty: &ValkyrieType) -> Option<&str> {
    match ty {
        ValkyrieType::Named(name) => Some(name.as_str()),
        ValkyrieType::Apply(base, _) => match base.as_ref() {
            ValkyrieType::Named(name) => Some(name.as_str()),
            _ => None,
        },
        _ => None,
    }
}

fn payload_type_for_variant(sum_type: &ValkyrieType, variant: &str) -> Option<ValkyrieType> {
    let ValkyrieType::Apply(_, args) = sum_type
    else {
        return None;
    };
    match variant {
        "Fine" | "Some" | "Left" | "Ok" => args.first().cloned(),
        "Fail" | "Right" | "Err" => {
            if args.len() >= 2 {
                args.get(1).cloned()
            }
            else {
                // One-arg Result aliases (`VonParseResult<T>`) keep Fail payload
                // as VonDiagnostic by convention at the call site; prefer argument.
                None
            }
        }
        _ => None,
    }
}

/// Map a resolved unite/enum constructor call onto `SumNew` parts.
fn sum_new_parts_from_constructor(
    call: &HirResolvedCall,
    arguments: &[MirOperand],
    sum_types: &[super::MirSumDeclaration],
) -> Option<(String, Vec<ValkyrieType>, String, Option<ValkyrieType>, Option<MirOperand>)> {
    let variant = call.symbol.parts().last()?.to_string();
    let (sum_type, type_args) = match &call.return_type {
        ValkyrieType::Named(name) => (name.to_string(), Vec::new()),
        ValkyrieType::Apply(base, args) => match base.as_ref() {
            ValkyrieType::Named(name) => (name.to_string(), args.clone()),
            _ => (call.symbol.parts().first().map(|part| part.to_string())?, Vec::new()),
        },
        _ => {
            let sum_type = if call.symbol.parts().len() >= 2 {
                call.symbol.parts()[call.symbol.parts().len() - 2].to_string()
            }
            else {
                sum_types.iter().find(|sum| sum.variants.iter().any(|item| item.name == variant)).map(|sum| sum.name.clone())?
            };
            (sum_type, Vec::new())
        }
    };
    // Only treat as SumNew when the variant exists on a known sum layout, or
    // the callee path is already `Owner.Variant`.
    // Fine/Fail/Some/None are language Result/Option arms — emit SumNew even
    // when the owning unite layout was not copied into this fragment's sum_types.
    let known = call.symbol.parts().len() >= 2
        || sum_types.iter().any(|sum| sum.name.as_str() == sum_type && sum.variants.iter().any(|item| item.name == variant))
        || matches!(variant.as_str(), "Fine" | "Fail" | "Some" | "None" | "Ok" | "Err");
    if !known {
        return None;
    }
    let payload_type = call.parameter_types.first().cloned();
    let payload = match arguments.len() {
        0 => None,
        1 => Some(arguments[0].clone()),
        _ => arguments.first().cloned(),
    };
    Some((sum_type, type_args, variant, payload_type, payload))
}

pub(super) fn type_args_from_sum_shaped(ty: &ValkyrieType) -> Vec<ValkyrieType> {
    match ty {
        ValkyrieType::Apply(_, args) => args.clone(),
        ValkyrieType::Nullable(inner) => vec![(**inner).clone()],
        _ => Vec::new(),
    }
}
