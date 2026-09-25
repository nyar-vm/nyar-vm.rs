use crate::{
    hir::is_nullable_type,
    types::{
        Identifier, NamePath,
        hir::{FunctionType, HirCallableDomain, HirExpr, HirExprKind, HirLiteral, HirResolvedCall, ValkyrieType},
    },
};
use nyar_types::NyarType;

use super::{
    MirBuilder, MirConstant, MirInstruction, MirOperand, MirOperation, MirStorageKind, MirTerminator, MirValueOrigin,
    builtin_helpers::{
        array_index_call_output_type, intrinsic_opcode_for_operator, intrinsic_opcode_output_type, is_array_len_intrinsic_symbol,
        is_ref_deref_intrinsic_symbol,
    },
    callee_name_matches,
    expr_helpers::{peel_generic_apply, reject_text_operator_for_numeric_args, named_type_name},
    infer_builder_operand_type, lower_callee_operand,
    value_semantics::{
        ensure_layout_for_type, ensure_named_aggregate_layout, layout_id_for_type, storage_kind_for_named_type, storage_kind_for_type,
    },
};

impl MirBuilder {
    fn try_lower_ref_deref_intrinsic(
        &mut self,
        resolved: Option<&HirResolvedCall>,
        callee: &MirOperand,
        arguments: &[MirOperand],
    ) -> Option<MirOperand> {
        let is_intrinsic = resolved
            .map(|call| is_ref_deref_intrinsic_symbol(&call.symbol))
            .unwrap_or_else(|| matches!(callee, MirOperand::Symbol(symbol) if is_ref_deref_intrinsic_symbol(symbol)));
        if !is_intrinsic {
            return None;
        }
        arguments.first().cloned()
    }

    fn try_lower_option_is_some(&mut self, receiver: MirOperand) -> Option<MirOperand> {
        let actual_type = infer_builder_operand_type(&receiver, &self.value_types)?;
        let sum_name = match &actual_type {
            ValkyrieType::Nullable(_) => self.sum_types.iter().find(|sum| sum.name == "Option").map(|sum| sum.name.clone()),
            ValkyrieType::Named(name) if name.as_str() == "Option" => {
                self.sum_types.iter().find(|sum| sum.name == "Option").map(|sum| sum.name.clone())
            }
            ValkyrieType::Apply(base, _) if named_type_name(base.as_ref()) == Some("Option") => {
                self.sum_types.iter().find(|sum| sum.name == "Option").map(|sum| sum.name.clone())
            }
            _ => None,
        }?;
        let value = self.next_value(MirValueOrigin::CallResult);
        self.instructions.push(MirInstruction::from_operation(MirOperation::SumVariantIs {
            sum_type: sum_name,
            type_args: type_args_from_sum_shaped(&actual_type),
            variant: "Some".to_string(),
            object: receiver,
        }));
        self.value_types.insert(value, ValkyrieType::Boolean);
        Some(MirOperand::Value(value))
    }

    fn try_lower_array_len_intrinsic(
        &mut self,
        resolved: Option<&HirResolvedCall>,
        callee: &MirOperand,
        arguments: &[MirOperand],
        expected_type: Option<&ValkyrieType>,
    ) -> Option<MirOperand> {
        let is_intrinsic = resolved
            .map(|call| is_array_len_intrinsic_symbol(&call.symbol))
            .unwrap_or_else(|| matches!(callee, MirOperand::Symbol(symbol) if is_array_len_intrinsic_symbol(symbol)));
        if !is_intrinsic {
            return None;
        }
        let array = arguments.first().cloned()?;
        let value = self.next_value(MirValueOrigin::CallResult);
        self.instructions.push(MirInstruction::from_operation(MirOperation::ArrayLength { array }));
        let return_type = resolved
            .map(|call| call.return_type.clone())
            .or_else(|| expected_type.cloned())
            .unwrap_or_else(|| ValkyrieType::Named(Identifier::new("usize")));
        self.value_types.insert(value, return_type);
        Some(MirOperand::Value(value))
    }

    fn field_type_for_semantic_type(&self, ty: &ValkyrieType, field: &str) -> Option<ValkyrieType> {
        match ty {
            ValkyrieType::Named(name) => self
                .lookup_struct_field_type(name.as_str(), field)
                .or_else(|| self.field_type_from_layout(layout_id_for_type(ty, &self.aggregate_layouts), field)),
            ValkyrieType::Apply(base, arguments) => self
                .field_type_for_semantic_type(base, field)
                .or_else(|| arguments.iter().find_map(|argument| self.field_type_for_semantic_type(argument, field))),
            ValkyrieType::Tuple(elements) => {
                let mut result = None;
                for element in elements {
                    let Some(candidate) = self.field_type_for_semantic_type(element, field)
                    else {
                        continue;
                    };
                    if let Some(existing) = &result {
                        if existing != &candidate {
                            return None;
                        }
                    }
                    else {
                        result = Some(candidate);
                    }
                }
                result
            }
            _ => None,
        }
    }

    pub(super) fn field_type_from_layout(&self, layout_id: Option<super::LayoutId>, field: &str) -> Option<ValkyrieType> {
        let layout = self.aggregate_layouts.layouts.iter().find(|layout| Some(layout.id) == layout_id)?;
        let field = layout.fields.iter().find(|field_layout| field_layout.name == field)?;
        match &field.ty {
            NyarType::Unit => Some(ValkyrieType::Unit),
            NyarType::Boolean => Some(ValkyrieType::Boolean),
            NyarType::Utf8 => Some(ValkyrieType::Utf8),
            NyarType::Utf16 => Some(ValkyrieType::Utf16),
            NyarType::Integer8 { signed } => Some(ValkyrieType::Integer8 { signed: *signed }),
            NyarType::Integer16 { signed } => Some(ValkyrieType::Integer16 { signed: *signed }),
            NyarType::Integer32 { signed } => Some(ValkyrieType::Integer32 { signed: *signed }),
            NyarType::Integer64 { signed } => Some(ValkyrieType::Integer64 { signed: *signed }),
            NyarType::Float32 => Some(ValkyrieType::Float32),
            NyarType::Float64 => Some(ValkyrieType::Float64),
            NyarType::Named(name) => Some(ValkyrieType::Named(Identifier::new(name.as_str()))),
            NyarType::Integer128 { signed } => Some(ValkyrieType::Integer128 { signed: *signed }),
            NyarType::Character => Some(ValkyrieType::Character),
            NyarType::Array(item) => Some(ValkyrieType::Array(Box::new(Self::nyar_field_type(item)?))),
            NyarType::FixedArray { element, length } => {
                Some(ValkyrieType::FixedArray { element: Box::new(Self::nyar_field_type(element)?), length: *length })
            }
            NyarType::Tuple(items) => Some(ValkyrieType::Tuple(items.iter().map(Self::nyar_field_type).collect::<Option<Vec<_>>>()?)),
            NyarType::Apply(base, args) => Some(ValkyrieType::Apply(
                Box::new(Self::nyar_field_type(base)?),
                args.iter().map(Self::nyar_field_type).collect::<Option<Vec<_>>>()?,
            )),
            _ => None,
        }
    }

    fn nyar_field_type(ty: &NyarType) -> Option<ValkyrieType> {
        match ty {
            NyarType::Unit => Some(ValkyrieType::Unit),
            NyarType::Boolean => Some(ValkyrieType::Boolean),
            NyarType::Utf8 => Some(ValkyrieType::Utf8),
            NyarType::Utf16 => Some(ValkyrieType::Utf16),
            NyarType::Character => Some(ValkyrieType::Character),
            NyarType::Integer8 { signed } => Some(ValkyrieType::Integer8 { signed: *signed }),
            NyarType::Integer16 { signed } => Some(ValkyrieType::Integer16 { signed: *signed }),
            NyarType::Integer32 { signed } => Some(ValkyrieType::Integer32 { signed: *signed }),
            NyarType::Integer64 { signed } => Some(ValkyrieType::Integer64 { signed: *signed }),
            NyarType::Integer128 { signed } => Some(ValkyrieType::Integer128 { signed: *signed }),
            NyarType::Float32 => Some(ValkyrieType::Float32),
            NyarType::Float64 => Some(ValkyrieType::Float64),
            NyarType::Named(name) => Some(ValkyrieType::Named(Identifier::new(name.as_str()))),
            NyarType::Array(item) => Some(ValkyrieType::Array(Box::new(Self::nyar_field_type(item)?))),
            NyarType::FixedArray { element, length } => {
                Some(ValkyrieType::FixedArray { element: Box::new(Self::nyar_field_type(element)?), length: *length })
            }
            NyarType::Tuple(items) => Some(ValkyrieType::Tuple(items.iter().map(Self::nyar_field_type).collect::<Option<Vec<_>>>()?)),
            NyarType::Apply(base, args) => Some(ValkyrieType::Apply(
                Box::new(Self::nyar_field_type(base)?),
                args.iter().map(Self::nyar_field_type).collect::<Option<Vec<_>>>()?,
            )),
            _ => None,
        }
    }

    fn struct_name_for_operand(&self, operand: &MirOperand) -> Option<String> {
        infer_builder_operand_type(operand, &self.value_types).and_then(|ty| match ty {
            ValkyrieType::Named(name) => Some(name.to_string()),
            ValkyrieType::Apply(base, _) => match base.as_ref() {
                ValkyrieType::Named(name) => Some(name.to_string()),
                _ => None,
            },
            _ => None,
        })
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

    fn storage_for_layout_id(&self, layout_id: Option<super::LayoutId>, fallback: MirStorageKind) -> MirStorageKind {
        layout_id
            .and_then(|id| self.aggregate_layouts.layouts.iter().find(|layout| layout.id == id).map(|layout| layout.storage))
            .unwrap_or(fallback)
    }

    fn layout_id_for_type(&mut self, ty: &ValkyrieType) -> Option<super::LayoutId> {
        if let Some(id) = layout_id_for_type(ty, &self.aggregate_layouts) {
            return Some(id);
        }
        match ty {
            ValkyrieType::Named(name) => {
                let simple = name.as_str();
                if matches!(simple, "any" | "null" | "object" | "Self" | "__auto" | "__opaque") {
                    return None;
                }
                let fields = self.struct_field_layouts.get(simple)?;
                if fields.is_empty() {
                    return None;
                }
                let storage = self.storage_for_type(ty);
                Some(ensure_named_aggregate_layout(&mut self.aggregate_layouts, simple, storage, fields))
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
        match object_ty {
            ValkyrieType::Utf8 if field.as_str() == "_repr" => Some(ValkyrieType::Array(Box::new(ValkyrieType::Integer8 { signed: false }))),
            ValkyrieType::Utf16 if field.as_str() == "_repr" => Some(ValkyrieType::Array(Box::new(ValkyrieType::Integer16 { signed: false }))),
            ValkyrieType::Named(name) => self.lookup_struct_field_type(name.as_str(), field.as_str()),
            ValkyrieType::Apply(base, arguments) => match base.as_ref() {
                ValkyrieType::Named(name) => self
                    .lookup_struct_field_type(name.as_str(), field.as_str())
                    .map(|field_ty| Self::substitute_first_generic(field_ty, arguments.as_slice()))
                    .or_else(|| arguments.iter().find_map(|argument| self.field_type_for_semantic_type(argument, field.as_str()))),
                _ => arguments.iter().find_map(|argument| self.field_type_for_semantic_type(argument, field.as_str())),
            },
            // A nominal structure can arrive from the HIR normalizer wrapped in
            // a one-element tuple. It is still the same semantic aggregate for
            // field lookup; do not make each backend rediscover this shape.
            ValkyrieType::Tuple(elements) if elements.len() == 1 => match &elements[0] {
                ValkyrieType::Named(name) => self
                    .lookup_struct_field_type(name.as_str(), field.as_str())
                    .or_else(|| self.field_type_from_layout(layout_id_for_type(&elements[0], &self.aggregate_layouts), field.as_str())),
                ValkyrieType::Apply(base, arguments) => match base.as_ref() {
                    ValkyrieType::Named(name) => self
                        .lookup_struct_field_type(name.as_str(), field.as_str())
                        .map(|field_ty| Self::substitute_first_generic(field_ty, arguments.as_slice()))
                        .or_else(|| self.field_type_from_layout(layout_id_for_type(&elements[0], &self.aggregate_layouts), field.as_str())),
                    _ => None,
                },
                _ => None,
            },
            ValkyrieType::Tuple(elements) => {
                let mut candidate = None;
                for element in elements {
                    let field_type = match element {
                        ValkyrieType::Named(name) => self.lookup_struct_field_type(name.as_str(), field.as_str()),
                        ValkyrieType::Apply(base, _) => match base.as_ref() {
                            ValkyrieType::Named(name) => self.lookup_struct_field_type(name.as_str(), field.as_str()),
                            _ => None,
                        },
                        _ => None,
                    };
                    let Some(field_type) = field_type
                    else {
                        continue;
                    };
                    if let Some(existing) = &candidate {
                        if existing != &field_type {
                            return None;
                        }
                    }
                    else {
                        candidate = Some(field_type);
                    }
                }
                candidate
            }
            _ => None,
        }
    }

    pub(super) fn unique_struct_field_type(&self, field: &str) -> Option<ValkyrieType> {
        let mut candidate: Option<ValkyrieType> = None;
        for fields in self.struct_field_layouts.values() {
            for (name, ty) in fields {
                if name != field {
                    continue;
                }
                match &candidate {
                    None => candidate = Some(ty.clone()),
                    Some(existing) if existing == ty => {}
                    Some(_) => return None,
                }
            }
        }
        candidate
    }

    fn substitute_first_generic(ty: ValkyrieType, arguments: &[ValkyrieType]) -> ValkyrieType {
        let Some(first) = arguments.first()
        else {
            return ty;
        };
        match ty {
            ValkyrieType::Generic(_) => first.clone(),
            ValkyrieType::Array(item) => ValkyrieType::Array(Box::new(Self::substitute_first_generic(*item, arguments))),
            ValkyrieType::FixedArray { element, length } => {
                ValkyrieType::FixedArray { element: Box::new(Self::substitute_first_generic(*element, arguments)), length }
            }
            ValkyrieType::Apply(base, args) => ValkyrieType::Apply(
                Box::new(Self::substitute_first_generic(*base, arguments)),
                args.into_iter().map(|arg| Self::substitute_first_generic(arg, arguments)).collect(),
            ),
            ValkyrieType::Tuple(items) => {
                ValkyrieType::Tuple(items.into_iter().map(|item| Self::substitute_first_generic(item, arguments)).collect())
            }
            other => other,
        }
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
                if let Some(result) = self.try_lower_singleton_static_call(callee, args) {
                    return result;
                }
                if callee_name_matches(&callee.kind, "tuple") {
                    let fields = args.iter().map(|arg| self.lower_expr_to_operand(&arg.value)).collect::<Vec<_>>();
                    let element_types = resolved
                        .as_ref()
                        .and_then(|call| match &call.return_type {
                            ValkyrieType::Tuple(types) => Some(types.clone()),
                            _ => None,
                        })
                        .unwrap_or_else(|| {
                            fields
                                .iter()
                                .map(|operand| infer_builder_operand_type(operand, &self.value_types).unwrap_or(ValkyrieType::Unit))
                                .collect()
                        });
                    let tuple_type =
                        resolved.as_ref().map(|call| call.return_type.clone()).unwrap_or_else(|| ValkyrieType::Tuple(element_types.clone()));
                    let storage = MirStorageKind::Value;
                    let layout_id = self.layout_id_for_type(&tuple_type);
                    let value = self.next_value(MirValueOrigin::Temporary);
                    self.instructions.push(MirInstruction::from_operation(MirOperation::TupleNew { fields }));
                    self.value_types.insert(value, tuple_type);
                    return MirOperand::Value(value);
                }
                if let Some((receiver_operand, method_name)) = self.extract_method_call(callee) {
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
                    if method_name.as_str() == "is_some" && args.is_empty() {
                        if let Some(operand) = self.try_lower_option_is_some(receiver_operand.clone()) {
                            return operand;
                        }
                    }
                    // ADR 0010: no dispatch/witness/evidence/intrinsic/parameter_types on Call.
                    let (callee_symbol, return_type) = match resolved.as_ref() {
                        Some(call) => (call.symbol.clone(), Some(call.return_type.clone())),
                        None => {
                            eprintln!(
                                "[mir] unresolved receiver call; lowering `{}` as diagnostic static symbol (ADR 0008)",
                                method_name.as_str()
                            );
                            (NamePath::new(vec![method_name.clone()]), None)
                        }
                    };
                    let callee = MirOperand::Symbol(callee_symbol);
                    if let Some(operand) = self.try_lower_ref_deref_intrinsic(resolved.as_ref(), &callee, &arguments) {
                        return operand;
                    }
                    if let Some(operand) = self.try_lower_array_len_intrinsic(resolved.as_ref(), &callee, &arguments, expected_type) {
                        return operand;
                    }
                    let value = self.next_value(MirValueOrigin::CallResult);
                    self.instructions.push(MirInstruction::from_operation(MirOperation::Call { callee, arguments }));
                    if let Some(return_type) = return_type {
                        self.value_types.insert(value, return_type);
                    }
                    return MirOperand::Value(value);
                }
                // `obj.field(args)` where `obj.field` is a function-typed field (e.g.
                // `FilterIterator._predicate`). Lower as indirect call: load the field
                // value via FieldGet, then `Call { dispatch: Indirect, callee: <value> }`.
                // This lets backends emit `Delegate::DynamicInvoke` / `call_ref` etc.
                // instead of treating the field name as a static method symbol.
                if let HirExprKind::FieldAccess { object, field } = &callee.kind {
                    let receiver_operand = self.lower_expr_to_operand(object);
                    let layout_id = self.layout_id_for_object_operand(&receiver_operand);
                    let field_ty = self
                        .field_type_for_object_operand(&receiver_operand, field)
                        .or_else(|| self.field_type_from_layout(layout_id, field.as_str()));
                    if matches!(field_ty, Some(ValkyrieType::Function(_))) {
                        let storage = self.storage_for_layout_id(layout_id, self.storage_for_object_operand(&receiver_operand));
                        let callee_value = self.next_value(MirValueOrigin::Temporary);
                        self.instructions.push(MirInstruction::from_operation(MirOperation::FieldGet {
                            object: receiver_operand,
                            field: field.to_string(),
                        }));
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
                            resolved.as_ref().map(|call| call.return_type.clone()).or_else(|| self.return_types.get(field.as_str()).cloned());
                        let value = self.next_value(MirValueOrigin::CallResult);
                        self.instructions
                            .push(MirInstruction::from_operation(MirOperation::Call { callee: MirOperand::Value(callee_value), arguments }));
                        if let Some(return_type) = return_type {
                            self.value_types.insert(value, return_type);
                        }
                        else if let Some(expected_type) = expected_type.cloned() {
                            self.value_types.insert(value, expected_type);
                        }
                        return MirOperand::Value(value);
                    }

                    // `obj.field.method()` where `obj` is not yet in bindings (e.g. nested
                    // temporaries) still must lower as a receiver Call, not a dotted Symbol.
                    let param_types = resolved.as_ref().map(|call| call.parameter_types.as_slice());
                    let mut arguments = args
                        .iter()
                        .enumerate()
                        .map(|(index, arg)| {
                            let hint = param_types.and_then(|params| params.get(index + 1));
                            self.lower_expr_to_operand_with_hint(&arg.value, hint)
                        })
                        .collect::<Vec<_>>();
                    arguments.insert(0, receiver_operand.clone());
                    let parameter_types = resolved.as_ref().map(|call| {
                        let mut types = call.parameter_types.clone();
                        if types.len() + 1 == arguments.len() {
                            if let Some(receiver_type) = infer_builder_operand_type(&receiver_operand, &self.value_types) {
                                types.insert(0, receiver_type);
                            }
                        }
                        types
                    });
                    let callee = MirOperand::Symbol(
                        resolved.as_ref().expect("Semantic MIR requires a resolved field receiver call contract").symbol.clone(),
                    );
                    if let Some(operand) = self.try_lower_ref_deref_intrinsic(resolved.as_ref(), &callee, &arguments) {
                        return operand;
                    }
                    if let Some(operand) = self.try_lower_array_len_intrinsic(resolved.as_ref(), &callee, &arguments, expected_type) {
                        return operand;
                    }
                    let value = self.next_value(MirValueOrigin::CallResult);
                    self.instructions.push(MirInstruction::from_operation(MirOperation::Call { callee, arguments }));
                    if let Some(return_type) = resolved
                        .as_ref()
                        .map(|call| call.return_type.clone())
                        .or_else(|| self.return_types.get(field.as_str()).cloned())
                        .or_else(|| expected_type.cloned())
                    {
                        self.value_types.insert(value, return_type);
                    }
                    return MirOperand::Value(value);
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
                            let (return_type, payload_type) = concretize_variant_constructor_types(
                                &call.return_type,
                                payload_type,
                                &variant,
                                expected_type,
                                &arguments,
                                &self.value_types,
                            );
                            self.instructions.push(MirInstruction::from_operation(MirOperation::SumNew {
                                sum_type,
                                type_args,
                                variant,
                                payload_type,
                                payload,
                            }));
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
                        if path.parts().last().is_some_and(|part| part.as_str() == "is_some") {
                            if let Some(operand) = self.try_lower_option_is_some(arguments[0].clone()) {
                                return operand;
                            }
                        }
                    }
                }
                if let Some(operand) = self.try_lower_array_len_intrinsic(resolved.as_ref(), &callee, &arguments, expected_type) {
                    return operand;
                }
                // ADR 0010: Call is only { callee, arguments }. No intrinsic/dispatch/generic side-channels.
                let value = self.next_value(MirValueOrigin::CallResult);
                self.instructions
                    .push(MirInstruction::from_operation(MirOperation::Call { callee: callee.clone(), arguments: arguments.clone() }));
                if let Some(ty) = array_index_call_output_type(&arguments, &self.value_types)
                    .or_else(|| function_ty.map(|func| func.return_type))
                    .or_else(|| resolved.as_ref().map(|call| call.return_type.clone()))
                    .or_else(|| match &callee {
                        MirOperand::Symbol(path) => self
                            .return_types
                            .get(&path.to_string())
                            .cloned()
                            .or_else(|| path.parts().last().and_then(|name| self.return_types.get(name.as_str()).cloned())),
                        _ => None,
                    })
                    .or_else(|| expected_type.cloned())
                {
                    self.value_types.insert(value, ty);
                }
                MirOperand::Value(value)
            }
            HirExprKind::ArrayNew { element_type, length } => {
                let length_operand = self.lower_expr_to_operand(length);
                let array_type = ValkyrieType::Array(Box::new(element_type.clone()));
                let value = self.next_value(MirValueOrigin::Temporary);
                self.instructions.push(MirInstruction::from_operation(MirOperation::ArrayNew {
                    array_type: array_type.clone(),
                    length: length_operand,
                    initialization: super::ArrayInitialization::Default,
                }));
                self.value_types.insert(value, array_type);
                MirOperand::Value(value)
            }
            HirExprKind::ArrayLiteral { items } => {
                // ADR 0011: surface literal → ArrayFromElements; fixed vs Array from array_type only.
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
                self.instructions
                    .push(MirInstruction::from_operation(MirOperation::ArrayFromElements { array_type: array_type.clone(), elements }));
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
                            .and_then(|variant| variant.payload_type.as_ref().and_then(Self::nyar_field_type))
                    });
                    let payload = match field_values.len() {
                        0 => None,
                        1 => Some(field_values[0].clone()),
                        _ => field_values.first().cloned(),
                    };
                    let variant = name.to_string();
                    let (return_type, payload_type) = concretize_variant_constructor_types(
                        &ctor_return,
                        payload_type,
                        &variant,
                        expected_type,
                        &field_values,
                        &self.value_types,
                    );
                    self.instructions.push(MirInstruction::from_operation(MirOperation::SumNew {
                        sum_type,
                        type_args,
                        variant,
                        payload_type,
                        payload,
                    }));
                    self.value_types.insert(value, return_type);
                    return MirOperand::Value(value);
                }
                let storage = storage_kind_for_named_type(&struct_type_name, &self.struct_is_value_type);
                let field_names: Vec<&str> = fields.iter().map(|(field_name, _)| field_name.as_str()).collect();
                let layout_id = self
                    .aggregate_layouts
                    .layouts
                    .iter()
                    .find(|layout| {
                        layout.name == struct_type_name.as_str()
                            && field_names.len() == layout.fields.len()
                            && field_names.iter().all(|field_name| layout.fields.iter().any(|field| field.name == *field_name))
                    })
                    .map(|layout| layout.id)
                    .or_else(|| {
                        let field_types = fields
                            .iter()
                            .map(|(field_name, operand)| {
                                let ty = self
                                    .lookup_struct_field_type(struct_type_name.as_str(), field_name)
                                    .map(|ty| super::resolve_self_type_with_owner(&ty, self.impl_owner_type.as_ref()))
                                    .or_else(|| infer_builder_operand_type(operand, &self.value_types))
                                    .unwrap_or(ValkyrieType::Unit);
                                (field_name.clone(), ty)
                            })
                            .collect::<Vec<_>>();
                        Some(ensure_named_aggregate_layout(
                            &mut self.aggregate_layouts,
                            struct_type_name.as_str(),
                            storage,
                            &field_types,
                        ))
                    });
                let value = self.next_value(MirValueOrigin::Temporary);
                self.push_instruction(MirOperation::StructNew { type_name: struct_type_name, fields }, vec![value]);
                self.value_types.insert(value, self.struct_construct_result_type(name, resolved.as_ref()));
                MirOperand::Value(value)
            }
            HirExprKind::FieldAccess { object, field } => {
                let object_operand = self.lower_singleton_field_object(object).unwrap_or_else(|| self.lower_expr_to_operand(object));
                // `.length` as ArrayLen must arrive as a resolved HIR CallContract with
                // IntrinsicOpcode::ArrayLen. Do not recover intrinsics from FieldAccess
                // (ADR 0008: no upward inference from short names).
                if let Some(struct_name) = self.struct_name_for_operand(&object_operand) {
                    return self.lower_object_field_operand(object_operand, &struct_name, field);
                }
                let layout_id = self.layout_id_for_object_operand(&object_operand);
                let storage = self.storage_for_layout_id(layout_id, self.storage_for_object_operand(&object_operand));
                let field_ty = self.field_type_for_object_operand(&object_operand, field);
                let value = self.next_value(MirValueOrigin::Temporary);
                self.instructions
                    .push(MirInstruction::from_operation(MirOperation::FieldGet { object: object_operand, field: field.to_string() }));
                if let Some(field_ty) = field_ty
                    .or_else(|| self.field_type_from_layout(layout_id, field.as_str()))
                    .or_else(|| self.return_types.get(field.as_str()).cloned())
                {
                    self.value_types.insert(value, field_ty);
                }
                MirOperand::Value(value)
            }
            HirExprKind::StoreField { object, field, value } => {
                let object_operand = self.lower_singleton_field_object(object).unwrap_or_else(|| self.lower_expr_to_operand(object));
                let storage = self.storage_for_object_operand(&object_operand);
                let layout_id = self.layout_id_for_object_operand(&object_operand);
                let value_operand = self.lower_expr_to_operand(value);
                self.instructions.push(MirInstruction::from_operation(MirOperation::FieldSet {
                    object: object_operand,
                    field: field.to_string(),
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
                // Value-type assign uses AggregateCopy. On reassignment, copy into the
                // existing binding's ValueId (loop-carried / pre-match home) — a fresh
                // dest leaves later same-body peeks on the old SSA (parse_von separator Fail).
                if let Some(ty) = infer_builder_operand_type(&operand, &self.value_types) {
                    if self.storage_for_type(&ty) == MirStorageKind::Value {
                        let layout_id = ensure_layout_for_type(&mut self.aggregate_layouts, &ty);
                        if let Some(layout_id) = layout_id {
                            let dest = match existing {
                                Some(MirOperand::Value(id)) => id,
                                _ => self.next_value(MirValueOrigin::LetBinding { name: name.clone() }),
                            };
                            self.instructions.push(MirInstruction::from_operation(MirOperation::AggregateCopy {
                                source: operand.clone(),
                                dest: MirOperand::Value(dest),
                            }));
                            // 复用已有 ValueId 时不要覆盖已注册的类型：
                            // `let mut i: usize = 0; while ... { i = i + 1 }` 中，
                            // `i + 1` 的 HIR 重载可能被误绑为 Utf8Text（Self-matching
                            // false positive），若用 `insert` 覆盖会污染循环 header
                            // 参数的类型注册，触发 JVM VerifyError。新创建的 ValueId
                            // 在表中不存在，`or_insert` 会插入推断类型。
                            self.value_types.entry(dest).or_insert(ty);
                            self.bindings.insert(name, MirOperand::Value(dest));
                            return MirOperand::Constant(MirConstant::Unit);
                        }
                    }
                }
                // Reference / scalar mutables: StoreVar into the named home. Keep the
                // existing ValueId when reassigning so match/if-restored bindings and
                // CLR block-param locals observe the write in the same body.
                let new_value = match existing {
                    Some(MirOperand::Value(id)) => id,
                    _ => self.next_value(MirValueOrigin::LetBinding { name: name.clone() }),
                };
                self.instructions.push(MirInstruction::from_operation(MirOperation::StoreVar {
                    name: name.clone(),
                    value: operand.clone(),
                    ty: None,
                }));
                if let Some(ty) = infer_builder_operand_type(&operand, &self.value_types) {
                    // 复用已有 ValueId 时不要覆盖已注册的类型：循环中 `i = i + 1`
                    // 的 HIR 重载可能被误绑为 Utf8Text，覆盖会污染循环 header
                    // 参数的类型注册。新创建的 ValueId 在表中不存在，`or_insert`
                    // 会插入推断类型。
                    self.value_types.entry(new_value).or_insert(ty);
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
    /// Requires a contextual Option/Result-shaped type (return hint or
    /// `current_return_type`) and a registered sum layout whose matching
    /// variant has no payload. Bound locals still win over this path.
    fn try_lower_nullary_sum_variant(&mut self, variant_name: &str, expected_type: Option<&ValkyrieType>) -> Option<MirOperand> {
        let contextual = expected_type.cloned().or_else(|| {
            (is_option_shaped(&self.current_return_type) || is_result_shaped(&self.current_return_type))
                .then(|| self.current_return_type.clone())
        })?;
        if !(is_option_shaped(&contextual) || is_result_shaped(&contextual)) {
            return None;
        }
        let preferred_owner = sum_owner_name(&contextual);
        let sum_type = self
            .sum_types
            .iter()
            .find(|sum| {
                preferred_owner.is_some_and(|owner| sum.name.as_str() == owner)
                    && sum.variants.iter().any(|variant| variant.name == variant_name && variant.payload_type.is_none())
            })
            .map(|sum| preferred_owner.map(str::to_string).unwrap_or_else(|| sum.name.clone()))
            .or_else(|| {
                let matches: Vec<String> = self
                    .sum_types
                    .iter()
                    .filter(|sum| sum.variants.iter().any(|variant| variant.name == variant_name && variant.payload_type.is_none()))
                    .map(|sum| sum.name.clone())
                    .collect();
                (matches.len() == 1).then(|| matches.into_iter().next().unwrap())
            })?;
        let value = self.next_value(MirValueOrigin::CallResult);
        let (return_type, payload_type) =
            concretize_variant_constructor_types(&contextual, None, variant_name, Some(&contextual), &[], &self.value_types);
        let type_args = type_args_from_sum_shaped(&contextual);
        self.instructions.push(MirInstruction::from_operation(MirOperation::SumNew {
            sum_type,
            type_args,
            variant: variant_name.to_string(),
            payload_type,
            payload: None,
        }));
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
    sum_types: &[nyar_types::SumTypeLayout],
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
