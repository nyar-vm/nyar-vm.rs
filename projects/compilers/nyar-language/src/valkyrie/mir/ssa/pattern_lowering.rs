use std::collections::BTreeSet;

use crate::types::{
    Identifier, NamePath,
    hir::{HirExpr, HirExprKind, HirExtractorPattern, HirLiteral, HirPattern, HirResolvedCall, ValkyrieType},
};
use nyar_types::NyarType;

use super::{
    MirBuilder, MirConstant, MirDiagnostic, MirInstruction, MirOperand, MirOperation, MirStorageKind, MirTerminator, MirValueOrigin,
    MirValueRef, callee_name_matches, infer_builder_operand_type, lower_literal, named_type_name, plain_type_pattern_matches,
    value_semantics::{
        LayoutId, ensure_layout_for_type, ensure_named_aggregate_layout, ensure_unite_tagged_layout, layout_id_for_type,
        storage_kind_for_named_type, storage_kind_for_type,
    },
};
use crate::hir::nullable_payload_type;

impl MirBuilder {
    pub(super) fn record_static_binding(&mut self, pattern: &HirPattern, initializer: Option<&HirExpr>) {
        match pattern {
            HirPattern::Variable(identifier) => {
                if let Some(expr) = initializer {
                    self.static_bindings.insert(identifier.name.to_string(), expr.clone());
                }
                else {
                    self.static_bindings.remove(identifier.name.as_str());
                }
            }
            HirPattern::Tuple(items) => {
                for item in items {
                    self.record_static_binding(item, None);
                }
            }
            HirPattern::Extractor(extractor) => match extractor {
                HirExtractorPattern::Array { prefix, suffix, .. } => {
                    for item in prefix {
                        self.record_static_binding(item, None);
                    }
                    for item in suffix {
                        self.record_static_binding(item, None);
                    }
                }
                HirExtractorPattern::Constructor { fields, .. } => {
                    for field in fields {
                        self.record_static_binding(field, None);
                    }
                }
            },
            _ => {}
        }
    }

    pub(super) fn bind_pattern_from_expr(&mut self, pattern: &HirPattern, expr: &HirExpr, ty: Option<ValkyrieType>) {
        let resolved_expr = self.resolve_static_expr(expr).unwrap_or_else(|| expr.clone());
        match pattern {
            HirPattern::Tuple(items) => {
                if let Some(tuple_items) = tuple_literal_exprs(&resolved_expr) {
                    for (item_pattern, item_expr) in items.iter().zip(tuple_items.iter()) {
                        self.bind_pattern_from_expr(item_pattern, item_expr, None);
                    }
                    return;
                }
            }
            HirPattern::Wildcard => {
                let _ = self.lower_expr_to_operand(&resolved_expr);
                return;
            }
            _ => {}
        }

        let operand = self.lower_expr_to_operand_with_hint(&resolved_expr, ty.as_ref());
        self.bind_pattern_from_operand(pattern, operand, ty);
    }

    pub(super) fn bind_pattern_from_operand(&mut self, pattern: &HirPattern, operand: MirOperand, ty: Option<ValkyrieType>) {
        self.bind_pattern_from_operand_with_payload(pattern, operand, ty, None);
    }

    pub(super) fn bind_pattern_from_operand_with_payload(
        &mut self,
        pattern: &HirPattern,
        operand: MirOperand,
        ty: Option<ValkyrieType>,
        extractor_payload: Option<MirOperand>,
    ) {
        match pattern {
            HirPattern::Wildcard => {}
            HirPattern::Else | HirPattern::Literal(_) | HirPattern::Range { .. } | HirPattern::Name(_) | HirPattern::Type(_) => {}
            HirPattern::Variable(identifier) => {
                let name = identifier.name.to_string();
                let value = self.next_value(MirValueOrigin::LetBinding { name: name.clone() });
                let operand_ty = infer_builder_operand_type(&operand, &self.value_types);
                // Prefer a concrete SSA type on the operand over a generic unite parameter
                // hint such as `Named("E")` from HIR extractor_payload_type.
                let inferred_type = match (ty.clone(), operand_ty) {
                    (Some(hint), Some(concrete)) if looks_like_unbound_type_parameter(&hint) => Some(concrete),
                    (hint, concrete) => hint.or(concrete),
                };
                if let Some(ref inferred_type) = inferred_type {
                    let value_names = self.aggregate_layouts.value_type_names.iter().map(|item| Identifier::new(item)).collect();
                    if storage_kind_for_type(inferred_type, &value_names) == MirStorageKind::Value {
                        if let Some(layout_id) = ensure_layout_for_type(&mut self.aggregate_layouts, inferred_type) {
                            self.instructions.push(MirInstruction::from_operation(MirOperation::AggregateCopy {
                                source: operand,
                                dest: MirOperand::Value(value),
                            }));
                            self.value_types.insert(value, inferred_type.clone());
                            self.bindings.insert(name, MirOperand::Value(value));
                            return;
                        }
                    }
                }
                self.instructions.push(MirInstruction::from_operation(MirOperation::StoreVar {
                    name: name.clone(),
                    value: operand,
                    ty: inferred_type.clone(),
                }));
                if let Some(inferred_type) = inferred_type {
                    self.value_types.insert(value, inferred_type);
                }
                self.bindings.insert(name, MirOperand::Value(value));
            }
            HirPattern::TypedBind { identifier, ty: pattern_type } => {
                let typed_hint = ty.or_else(|| pattern_type.parts().last().cloned().map(ValkyrieType::Named));
                self.bind_pattern_from_operand_with_payload(&HirPattern::Variable(identifier.clone()), operand, typed_hint, None);
            }
            HirPattern::Tuple(items) => {
                // Tuple patterns can arrive through an extractor payload. In
                // that form the direct binding hint is intentionally absent,
                // but the payload operand still carries the canonical SSA
                // type. Preserve that type before creating tuple_get_N so a
                // backend never has to rediscover the tuple layout.
                let tuple_ty = ty
                    .clone()
                    .or_else(|| infer_builder_operand_type(&operand, &self.value_types))
                    .or_else(|| extractor_payload.as_ref().and_then(|payload| infer_builder_operand_type(payload, &self.value_types)));
                for (index, item_pattern) in items.iter().enumerate() {
                    let extracted = self.extract_tuple_field_operand(&operand, index, tuple_ty.as_ref());
                    self.bind_pattern_from_operand_with_payload(item_pattern, MirOperand::Value(extracted), None, None);
                }
            }
            HirPattern::Extractor(extractor) => {
                // Empty-field unite arms match by `tag` only — do not FieldGet `payload`
                // (nullary payload is null; CLR also rejects untagged unite payload gets).
                if matches!(extractor, HirExtractorPattern::Constructor { fields, .. } if fields.is_empty()) {
                    return;
                }
                let payload = match extractor_payload {
                    Some(payload) => payload,
                    None => match extractor {
                        HirExtractorPattern::Constructor { resolved: Some(resolved), .. }
                        | HirExtractorPattern::Array { resolved: Some(resolved), .. } => {
                            self.lower_extractor_call_operand(resolved, operand.clone())
                        }
                        _ => {
                            self.bind_unknown_pattern_bindings(&HirPattern::Extractor(extractor.clone()));
                            return;
                        }
                    },
                };
                self.bind_extractor_pattern_from_payload(extractor, payload);
            }
            HirPattern::Object { fields, rest, .. } => {
                // Unite 花括号臂（`Include { value }`）经 probe 传来 payload 时：
                // 变体字段直接绑定到 payload（单字段变体 payload 已是 `value` 的类型），
                // 不可再按 scrutinee（WitDefinition）做 struct FieldGet。
                if let Some(payload) = extractor_payload {
                    let payload_ty = infer_builder_operand_type(&payload, &self.value_types);
                    if fields.len() <= 1 {
                        for (_, field_pattern) in fields {
                            self.bind_pattern_from_operand_with_payload(field_pattern, payload.clone(), payload_ty.clone(), None);
                        }
                    }
                    else {
                        for (slot, (_, field_pattern)) in fields.iter().enumerate() {
                            let extracted = self.lower_extractor_payload_slot_operand(payload.clone(), slot, payload_ty.clone());
                            self.bind_pattern_from_operand_with_payload(field_pattern, extracted, None, None);
                        }
                    }
                    if let Some(rest) = rest {
                        self.bind_pattern_from_operand_with_payload(&HirPattern::Variable(rest.clone()), payload, payload_ty, None);
                    }
                }
                else {
                    self.bind_object_pattern_from_operand(fields, rest.as_ref(), operand);
                }
            }
            HirPattern::Bind { identifier, pattern } => {
                self.bind_pattern_from_operand_with_payload(pattern, operand.clone(), ty.clone(), extractor_payload.clone());
                self.bind_pattern_from_operand_with_payload(&HirPattern::Variable(identifier.clone()), operand, ty, None);
            }
            HirPattern::Mut(pattern) | HirPattern::Pin { pattern, .. } => {
                self.bind_pattern_from_operand_with_payload(pattern, operand, ty, extractor_payload);
            }
            // OR patterns are handled by `lower_or_pattern_match_operand` for
            // the predicate. They introduce no single payload binding unless
            // represented by an explicit Bind pattern, so the binding phase
            // must not manufacture residual MIR for them.
            HirPattern::Or(_) => {}
            _ => {
                let value = self.next_value(MirValueOrigin::Temporary);
                // Keep the residual instruction well-typed so the shared
                // contract can report SMIR008 instead of masking it as SMIR001.
                self.value_types.insert(value, ValkyrieType::Boolean);
                self.instructions.push(MirInstruction::from_operation(
                    // Preserve an unsupported pattern as structured residual MIR.
                    // The shared Semantic MIR contract rejects it as SMIR008;
                    // do not disguise the residual as a fake symbol.
                    MirOperation::PatternMatch { value: operand.clone(), pattern: pattern.clone() },
                ));
            }
        }
    }

    pub(super) fn lower_pattern_match_operand(&mut self, pattern: &HirPattern, value: MirOperand) -> MirOperand {
        match pattern {
            HirPattern::Wildcard | HirPattern::Variable(_) | HirPattern::Else => MirOperand::Constant(MirConstant::Bool(true)),
            HirPattern::Literal(literal) => self.lower_literal_pattern_match_operand(literal, value),
            HirPattern::Range { start, end, inclusive_end } => {
                self.lower_range_pattern_match_operand(start.as_ref(), end.as_ref(), *inclusive_end, value)
            }
            HirPattern::Tuple(items) => self.lower_tuple_pattern_match_operand(items, value),
            HirPattern::Extractor(extractor) => self.lower_extractor_pattern_match_operand(extractor, value),
            HirPattern::Name(name) => self.lower_name_pattern_match_operand(name, value),
            HirPattern::Type(name) => self.lower_type_pattern_match_operand(name, value),
            HirPattern::TypedBind { ty, .. } => self.lower_type_pattern_match_operand(ty, value),
            HirPattern::Object { name, fields, .. } => self.lower_object_pattern_match_operand(name, fields, value),
            HirPattern::Or(patterns) => self.lower_or_pattern_match_operand(patterns, value),
            HirPattern::Bind { pattern, .. } => self.lower_pattern_match_operand(pattern, value),
            HirPattern::Mut(pattern) => self.lower_pattern_match_operand(pattern, value),
            HirPattern::Pin { pattern, .. } => self.lower_pattern_match_operand(pattern, value),
        }
    }

    pub(super) fn lower_pattern_match_probe(&mut self, pattern: &HirPattern, value: MirOperand) -> (MirOperand, Option<MirOperand>) {
        match pattern {
            HirPattern::Extractor(extractor) => {
                // Tagged unite/enum：按 tag 判别（空字段与带字段均适用）。
                if let Some((condition, payload)) = self.try_lower_sum_constructor_tag_match(extractor, value.clone()) {
                    return (condition, payload);
                }
                let Some(resolved) = extractor_resolved_call(extractor)
                else {
                    return (self.lower_fallback_pattern_match(HirPattern::Extractor(extractor.clone()), value), None);
                };
                let nullable = self.lower_extractor_call_operand(resolved, value.clone());
                let matched = self.lower_nullable_some_operand(nullable.clone());
                let condition = self.lower_extractor_nested_pattern_match_operand(extractor, nullable.clone(), matched, resolved);
                (condition, Some(nullable))
            }
            // `case Include { value }:` 解析为 Object（花括号）。对 unite scrutinee 必须
            // tag 比较 + payload 提取，否则 `type_pattern_matches(WitDefinition, Include)`
            // → Bool(false)，绑定落到 unsupported_pattern → JVM `iconst_0; getfield`。
            HirPattern::Object { name: Some(name), fields, rest: None } => {
                if let Some((condition, payload)) = self.try_lower_sum_object_tag_match(name, fields, value.clone()) {
                    return (condition, payload);
                }
                (self.lower_pattern_match_operand(pattern, value), None)
            }
            _ => (self.lower_pattern_match_operand(pattern, value), None),
        }
    }

    pub(super) fn lower_type_pattern_match_operand(&mut self, name: &NamePath, value: MirOperand) -> MirOperand {
        let Some(actual_type) = infer_builder_operand_type(&value, &self.value_types)
        else {
            return self.lower_fallback_pattern_match(HirPattern::Type(name.clone()), value);
        };

        MirOperand::Constant(MirConstant::Bool(self.type_pattern_matches(&actual_type, name)))
    }

    pub(super) fn lower_name_pattern_match_operand(&mut self, name: &NamePath, value: MirOperand) -> MirOperand {
        let Some(actual_type) = infer_builder_operand_type(&value, &self.value_types)
        else {
            return self.lower_fallback_pattern_match(HirPattern::Name(name.clone()), value);
        };

        // Nullary unite/enum arm (`case StringLiteral:`): runtime `tag` compare from
        // `sum_types` — never `plain_type_pattern_matches` → Bool(false).
        if let Some(condition) = self.try_lower_sum_variant_tag_match(&actual_type, name, value.clone()) {
            return condition;
        }

        MirOperand::Constant(MirConstant::Bool(plain_type_pattern_matches(&actual_type, name)))
    }

    /// Match `case Variant:` against a sum scrutinee by loading `tag` and comparing it to the
    /// variant discriminant from [`Self::sum_types`]. Disambiguates shared names (`EndOfFile`)
    /// via the scrutinee's concrete sum type — no cross-sum guessing.
    fn try_lower_sum_variant_tag_match(
        &mut self,
        actual_type: &ValkyrieType,
        pattern_name: &NamePath,
        value: MirOperand,
    ) -> Option<MirOperand> {
        let type_name = match actual_type {
            ValkyrieType::Nullable(_) => Some("Option"),
            _ => named_type_name(actual_type),
        }?;
        let variant_name = pattern_name.parts().last()?.as_str();
        if pattern_name.parts().len() >= 2 {
            let owner = pattern_name.parts().first()?.as_str();
            if !sum_type_name_matches(type_name, owner) {
                return None;
            }
        }

        let sum = self.sum_types.iter().find(|sum| sum_type_name_matches(&sum.name, type_name))?;
        let variant = sum.variants.iter().find(|variant| variant.name == variant_name)?;
        let _ = variant.tag;
        let sum_name = sum.name.clone();
        let type_args = super::expr_lowering::type_args_from_sum_shaped(actual_type);
        let condition = self.next_value(MirValueOrigin::Temporary);
        self.push_instruction(
            MirOperation::SumVariantIs {
                sum_type: sum_name,
                type_args,
                variant: variant_name.to_string(),
                object: value,
            },
            vec![condition],
        );
        self.value_types.insert(condition, ValkyrieType::Boolean);
        Some(MirOperand::Value(condition))
    }

    pub(super) fn lower_tuple_pattern_match_operand(&mut self, items: &[HirPattern], value: MirOperand) -> MirOperand {
        let Some(ValkyrieType::Tuple(types)) = infer_builder_operand_type(&value, &self.value_types)
        else {
            return self.lower_fallback_pattern_match(HirPattern::Tuple(items.to_vec()), value);
        };
        if items.len() != types.len() {
            return MirOperand::Constant(MirConstant::Bool(false));
        }

        let mut accumulated: Option<MirOperand> = None;
        for (index, item_pattern) in items.iter().enumerate() {
            let extracted = self.extract_tuple_field_operand(&value, index, Some(&ValkyrieType::Tuple(types.clone())));
            let item_match = self.lower_pattern_match_operand(item_pattern, MirOperand::Value(extracted));
            accumulated = Some(self.merge_pattern_match_operands(accumulated, item_match));
        }

        accumulated.unwrap_or(MirOperand::Constant(MirConstant::Bool(true)))
    }

    /// extractor 命中条件：先对空字段 unite 臂做 `tag` 比较；带 payload 的臂仍走 `T?`/`null`。
    pub(super) fn lower_extractor_pattern_match_operand(&mut self, extractor: &HirExtractorPattern, value: MirOperand) -> MirOperand {
        if let Some((condition, _)) = self.try_lower_sum_constructor_tag_match(extractor, value.clone()) {
            return condition;
        }
        let Some(resolved) = extractor_resolved_call(extractor)
        else {
            return self.lower_fallback_pattern_match(HirPattern::Extractor(extractor.clone()), value);
        };
        let nullable = self.lower_extractor_call_operand(resolved, value.clone());
        let matched = self.lower_nullable_some_operand(nullable.clone());
        self.lower_extractor_nested_pattern_match_operand(extractor, nullable, matched, resolved)
    }

    /// Constructor extractor on a sum scrutinee → `SumVariantIs`；带字段时再 `SumPayloadGet`。
    fn try_lower_sum_constructor_tag_match(
        &mut self,
        extractor: &HirExtractorPattern,
        value: MirOperand,
    ) -> Option<(MirOperand, Option<MirOperand>)> {
        let HirExtractorPattern::Constructor { name, fields, resolved, .. } = extractor
        else {
            return None;
        };
        let actual_type = infer_builder_operand_type(&value, &self.value_types)?;
        let type_name = match &actual_type {
            ValkyrieType::Nullable(_) => Some("Option"),
            _ => named_type_name(&actual_type),
        }?;
        if !self.sum_types.iter().any(|sum| sum_type_name_matches(&sum.name, type_name)) {
            return None;
        }
        let condition = self.try_lower_sum_variant_tag_match(&actual_type, name, value.clone())?;
        if fields.is_empty() {
            return Some((condition, None));
        }
        // Prefer sum-layout payload typing (substitutes Result Apply args). HIR-resolved
        // extractor metadata often still carries unbound `T`/`E` and must not win.
        // Never fall through to FieldGet `payload` against a payload-struct layout.
        let payload = if let Some(payload) = self.lower_sum_payload_operand(&actual_type, name, value.clone()) {
            payload
        }
        else if let Some(resolved) = resolved.as_ref() {
            self.try_lower_result_option_extractor(resolved, value.clone())
                .or_else(|| self.lower_sum_payload_operand(&actual_type, name, value.clone()))?
        }
        else {
            return None;
        };
        Some((condition, Some(payload)))
    }

    /// Object pattern `Variant { field }` on a sum scrutinee → same tag + payload lowering.
    fn try_lower_sum_object_tag_match(
        &mut self,
        name: &NamePath,
        fields: &[(Identifier, HirPattern)],
        value: MirOperand,
    ) -> Option<(MirOperand, Option<MirOperand>)> {
        let actual_type = infer_builder_operand_type(&value, &self.value_types)?;
        let type_name = match &actual_type {
            ValkyrieType::Nullable(_) => Some("Option"),
            _ => named_type_name(&actual_type),
        }?;
        if !self.sum_types.iter().any(|sum| sum_type_name_matches(&sum.name, type_name)) {
            return None;
        }
        let condition = self.try_lower_sum_variant_tag_match(&actual_type, name, value.clone())?;
        if fields.is_empty() {
            return Some((condition, None));
        }
        let payload = self.lower_sum_payload_operand(&actual_type, name, value)?;
        Some((condition, Some(payload)))
    }

    /// Emit `SumPayloadGet` for a sum scrutinee and register the variant payload type.
    fn lower_sum_payload_operand(&mut self, actual_type: &ValkyrieType, variant_name: &NamePath, value: MirOperand) -> Option<MirOperand> {
        let type_name = named_type_name(actual_type)?;
        let variant_simple = variant_name.parts().last()?.as_str();
        let (canonical_sum_name, payload_ty) = self.sum_types.iter().find_map(|sum| {
            if !sum_type_name_matches(&sum.name, type_name) {
                return None;
            }
            let variant = sum.variants.iter().find(|variant| variant.name == variant_simple)?;
            let payload_ty = variant.payload_type.as_ref().and_then(sum_payload_to_valkyrie)?;
            Some((sum.name.clone(), payload_ty))
        })?;
        let payload_ty = match actual_type {
            ValkyrieType::Apply(_, args) if matches!(variant_simple, "Fine" | "Some" | "Left" | "Ok") => {
                args.first().cloned().unwrap_or(payload_ty)
            }
            ValkyrieType::Apply(_, args) if matches!(variant_simple, "Fail" | "Right" | "Err") => {
                if args.len() >= 2 {
                    args.get(1).cloned().unwrap_or(payload_ty)
                }
                else {
                    payload_ty
                }
            }
            _ => payload_ty,
        };
        let output = self.next_value(MirValueOrigin::Temporary);
        self.push_instruction(
            MirOperation::SumPayloadGet {
            // Alias scrutinees (`VonParseResult`) must still name the declared unite (`Result`).
            sum_type: canonical_sum_name,
            type_args: super::expr_lowering::type_args_from_sum_shaped(actual_type),
            variant: variant_simple.to_string(),
            payload_type: payload_ty.clone(),
            object: value,
            },
            vec![output],
        );
        self.value_types.insert(output, payload_ty);
        Some(MirOperand::Value(output))
    }

    /// `case Fine(x)` / `Fail(e)` / `Some(v)` must use SumPayloadGet — never FieldGet
    /// against a payload structure layout (that caused SMIR010 on LegionSourceClosurePlan).
    fn try_lower_result_option_extractor(&mut self, resolved: &HirResolvedCall, value: MirOperand) -> Option<MirOperand> {
        let variant = resolved.symbol.parts().last()?.as_str();
        if !matches!(variant, "Fine" | "Fail" | "Some" | "None" | "Ok" | "Err") {
            return None;
        }
        let object_ty = infer_builder_operand_type(&value, &self.value_types)?;
        let sum_type = match &object_ty {
            ValkyrieType::Named(name) if name.as_str() == "Result" || name.as_str().ends_with("Result") => "Result".to_string(),
            ValkyrieType::Apply(base, _) => match base.as_ref() {
                ValkyrieType::Named(name) if name.as_str() == "Result" || name.as_str().ends_with("Result") => "Result".to_string(),
                ValkyrieType::Named(name) if matches!(name.as_str(), "Option" | "Nullable") => "Option".to_string(),
                _ => return None,
            },
            ValkyrieType::Named(name) if matches!(name.as_str(), "Option" | "Nullable") => "Option".to_string(),
            ValkyrieType::Nullable(_) => "Option".to_string(),
            _ => return None,
        };
        if variant == "None" {
            return Some(MirOperand::Constant(MirConstant::Unit));
        }
        // Prefer the concrete Apply substitution on the scrutinee. HIR often keeps
        // `extractor_payload_type = Named("T"|"E")` from the generic unite definition,
        // which must not override `Result<i32, VonDiagnostic>` → VonDiagnostic.
        let payload_ty = match (&object_ty, variant) {
            (ValkyrieType::Apply(_, args), "Fine" | "Some" | "Ok") => args.first().cloned(),
            (ValkyrieType::Apply(_, args), "Fail" | "Err") => {
                if args.len() >= 2 {
                    args.get(1).cloned()
                }
                else {
                    Some(ValkyrieType::Named(Identifier::new("VonDiagnostic")))
                }
            }
            (ValkyrieType::Nullable(inner), "Some") => Some((**inner).clone()),
            _ => resolved.extractor_payload_type.clone(),
        }?;
        let output = self.next_value(MirValueOrigin::Temporary);
        self.push_instruction(
            MirOperation::SumPayloadGet {
                sum_type,
                type_args: super::expr_lowering::type_args_from_sum_shaped(&object_ty),
                variant: variant.to_string(),
                payload_type: payload_ty.clone(),
                object: value,
            },
            vec![output],
        );
        self.value_types.insert(output, payload_ty);
        Some(MirOperand::Value(output))
    }

    pub(super) fn lower_literal_pattern_match_operand(&mut self, literal: &HirLiteral, value: MirOperand) -> MirOperand {
        let (constant, expected_type) = lower_literal(literal, None);
        match literal {
            HirLiteral::Bool(expected) => {
                let Some(actual_type) = infer_builder_operand_type(&value, &self.value_types)
                else {
                    return self.lower_fallback_pattern_match(HirPattern::Literal(literal.clone()), value);
                };
                if actual_type != ValkyrieType::Boolean {
                    return MirOperand::Constant(MirConstant::Bool(false));
                }
                if *expected { value } else { self.lower_logical_not_operand(value) }
            }
            HirLiteral::Integer64(_) | HirLiteral::Float64(_) => {
                let Some(expected_type) = expected_type
                else {
                    return self.lower_fallback_pattern_match(HirPattern::Literal(literal.clone()), value);
                };
                let Some(actual_type) = infer_builder_operand_type(&value, &self.value_types)
                else {
                    return self.lower_fallback_pattern_match(HirPattern::Literal(literal.clone()), value);
                };
                if actual_type != expected_type {
                    return MirOperand::Constant(MirConstant::Bool(false));
                }
                self.lower_eq_constant_operand(value, constant, &actual_type)
            }
            HirLiteral::Unit => {
                if matches!(infer_builder_operand_type(&value, &self.value_types), Some(ValkyrieType::Unit)) {
                    MirOperand::Constant(MirConstant::Bool(true))
                }
                else {
                    MirOperand::Constant(MirConstant::Bool(false))
                }
            }
            HirLiteral::String(_) => {
                let Some(expected_type) = expected_type
                else {
                    return self.lower_fallback_pattern_match(HirPattern::Literal(literal.clone()), value);
                };
                let Some(actual_type) = infer_builder_operand_type(&value, &self.value_types)
                else {
                    return self.lower_fallback_pattern_match(HirPattern::Literal(literal.clone()), value);
                };
                if actual_type != expected_type {
                    return MirOperand::Constant(MirConstant::Bool(false));
                }

                match (value, constant) {
                    (MirOperand::Constant(MirConstant::Utf8(actual)), MirConstant::Utf8(expected)) => {
                        MirOperand::Constant(MirConstant::Bool(actual == expected))
                    }
                    (value, _) => self.lower_fallback_pattern_match(HirPattern::Literal(literal.clone()), value),
                }
            }
        }
    }

    pub(super) fn lower_range_pattern_match_operand(
        &mut self,
        start: Option<&HirLiteral>,
        end: Option<&HirLiteral>,
        inclusive_end: bool,
        value: MirOperand,
    ) -> MirOperand {
        let Some(actual_type) = infer_builder_operand_type(&value, &self.value_types)
        else {
            return self.lower_fallback_pattern_match(HirPattern::Range { start: start.cloned(), end: end.cloned(), inclusive_end }, value);
        };

        if !matches!(
            actual_type,
            ValkyrieType::Integer8 { .. }
                | ValkyrieType::Integer16 { .. }
                | ValkyrieType::Integer32 { .. }
                | ValkyrieType::Integer64 { .. }
                | ValkyrieType::Integer128 { .. }
                | ValkyrieType::Float32
                | ValkyrieType::Float64
        ) {
            return self.lower_fallback_pattern_match(HirPattern::Range { start: start.cloned(), end: end.cloned(), inclusive_end }, value);
        }

        let mut accumulated: Option<MirOperand> = None;
        if let Some(start_literal) = start {
            let (start_constant, _) = lower_literal(start_literal, Some(&actual_type));
            // value >= start ? !(value < start)
            let lt_start = self.lower_lt_constant_operand(value.clone(), start_constant, &actual_type);
            let lower_bound = self.lower_logical_not_operand(lt_start);
            accumulated = Some(self.merge_pattern_match_operands(accumulated, lower_bound));
        }
        if let Some(end_literal) = end {
            let (end_constant, _) = lower_literal(end_literal, Some(&actual_type));
            let upper_bound = if inclusive_end {
                // value <= end ? !(end < value)
                let lt_end = self.lower_lt_operands(MirOperand::Constant(end_constant), value, &actual_type);
                self.lower_logical_not_operand(lt_end)
            }
            else {
                self.lower_lt_constant_operand(value, end_constant, &actual_type)
            };
            accumulated = Some(self.merge_pattern_match_operands(accumulated, upper_bound));
        }
        accumulated.unwrap_or(MirOperand::Constant(MirConstant::Bool(true)))
    }

    pub(super) fn lower_object_pattern_match_operand(
        &mut self,
        name: &Option<NamePath>,
        fields: &[(Identifier, HirPattern)],
        value: MirOperand,
    ) -> MirOperand {
        let Some(actual_type) = infer_builder_operand_type(&value, &self.value_types)
        else {
            return self.lower_fallback_pattern_match(HirPattern::Object { name: name.clone(), fields: fields.to_vec(), rest: None }, value);
        };

        if let Some(name) = name {
            if !self.type_pattern_matches(&actual_type, name) {
                return MirOperand::Constant(MirConstant::Bool(false));
            }
        }

        if fields.is_empty() {
            return MirOperand::Constant(MirConstant::Bool(true));
        }

        let Some(struct_name) = named_type_name(&actual_type)
        else {
            return MirOperand::Constant(MirConstant::Bool(false));
        };

        if !self.has_known_struct_layout(struct_name) {
            return self.lower_fallback_pattern_match(HirPattern::Object { name: name.clone(), fields: fields.to_vec(), rest: None }, value);
        }

        let mut accumulated: Option<MirOperand> = None;
        for (field_name, field_pattern) in fields {
            if !self.struct_has_field(struct_name, field_name.as_str()) {
                return MirOperand::Constant(MirConstant::Bool(false));
            }
            let field_operand = self.lower_object_field_operand(value.clone(), struct_name, field_name);
            let field_match = self.lower_pattern_match_operand(field_pattern, field_operand);
            accumulated = Some(self.merge_pattern_match_operands(accumulated, field_match));
        }

        accumulated.unwrap_or(MirOperand::Constant(MirConstant::Bool(true)))
    }

    pub(super) fn lower_or_pattern_match_operand(&mut self, patterns: &[HirPattern], value: MirOperand) -> MirOperand {
        let mut accumulated: Option<MirOperand> = None;
        for pattern in patterns {
            let candidate = self.lower_pattern_match_operand(pattern, value.clone());
            accumulated = Some(self.merge_pattern_match_any_operands(accumulated, candidate));
        }
        accumulated.unwrap_or(MirOperand::Constant(MirConstant::Bool(false)))
    }

    pub(super) fn has_known_struct_layout(&self, struct_name: &str) -> bool {
        self.struct_field_layouts.contains_key(struct_name) || self.struct_parent_index.contains_key(struct_name)
    }

    fn lower_extractor_nested_pattern_match_operand(
        &mut self,
        extractor: &HirExtractorPattern,
        payload: MirOperand,
        matched: MirOperand,
        resolved: &HirResolvedCall,
    ) -> MirOperand {
        let mut accumulated = Some(matched);
        match extractor {
            HirExtractorPattern::Constructor { fields, .. } => {
                for (slot, field_pattern) in fields.iter().enumerate() {
                    let extracted =
                        self.lower_extractor_payload_slot_operand(payload.clone(), slot, self.infer_extractor_slot_type(resolved, slot));
                    let item_match = self.lower_pattern_match_operand(field_pattern, extracted);
                    accumulated = Some(self.merge_pattern_match_operands(accumulated, item_match));
                }
            }
            HirExtractorPattern::Array { prefix, suffix, .. } => {
                for (slot, field_pattern) in prefix.iter().enumerate() {
                    let extracted =
                        self.lower_extractor_payload_slot_operand(payload.clone(), slot, self.infer_extractor_slot_type(resolved, slot));
                    let item_match = self.lower_pattern_match_operand(field_pattern, extracted);
                    accumulated = Some(self.merge_pattern_match_operands(accumulated, item_match));
                }
                let suffix_base = prefix.len()
                    + match extractor {
                        HirExtractorPattern::Array { rest, .. } if rest.is_some() => 1,
                        _ => 0,
                    };
                for (index, field_pattern) in suffix.iter().enumerate() {
                    let slot = suffix_base + index;
                    let extracted =
                        self.lower_extractor_payload_slot_operand(payload.clone(), slot, self.infer_extractor_slot_type(resolved, slot));
                    let item_match = self.lower_pattern_match_operand(field_pattern, extracted);
                    accumulated = Some(self.merge_pattern_match_operands(accumulated, item_match));
                }
            }
        }
        accumulated.unwrap_or(MirOperand::Constant(MirConstant::Bool(true)))
    }

    /// Layout id for unite `tag`/`payload` FieldGet: nominal sum first, never the
    /// Fine/Fail payload struct type argument.
    fn layout_id_for_sum_field_access(&mut self, actual_type: &ValkyrieType, sum_name: &str) -> Option<LayoutId> {
        let ensured = ensure_unite_tagged_layout(&mut self.aggregate_layouts, sum_name);
        let sum_named = ValkyrieType::Named(Identifier::new(sum_name));
        layout_id_for_type(&sum_named, &self.aggregate_layouts)
            .or(Some(ensured))
            .or_else(|| layout_id_for_type(actual_type, &self.aggregate_layouts))
            .and_then(|id| {
                let layout = self.aggregate_layouts.layouts.iter().find(|layout| layout.id == id)?;
                if layout.fields.iter().any(|field| field.name == "tag") || layout.fields.iter().any(|field| field.name == "payload") {
                    Some(id)
                }
                else {
                    None
                }
            })
            .or(Some(ensured))
    }

    pub(super) fn lower_extractor_call_operand(&mut self, resolved: &HirResolvedCall, value: MirOperand) -> MirOperand {
        use crate::types::hir::HirCallableDomain;
        if resolved.domain == HirCallableDomain::Extractor {
            if let Some(payload) = self.try_lower_result_option_extractor(resolved, value.clone()) {
                return payload;
            }
            let object_ty = infer_builder_operand_type(&value, &self.value_types);
            // If the scrutinee is still a Result/Option-shaped Apply/Named, prefer
            // SumPayloadGet even when the dedicated helper returned None (e.g. odd
            // extractor metadata). Never FieldGet `payload` on the Fine payload struct.
            if let Some(ref ty) = object_ty {
                if let Some(type_name) = named_type_name(ty) {
                    if self.sum_types.iter().any(|sum| sum_type_name_matches(&sum.name, type_name))
                        || type_name == "Result"
                        || type_name.ends_with("Result")
                        || matches!(type_name, "Option" | "Nullable")
                    {
                        let variant = resolved.symbol.parts().last().map(|part| part.as_str()).unwrap_or("");
                        if let Some(payload) = self.lower_sum_payload_operand(ty, &NamePath::new(vec![Identifier::new(variant)]), value.clone())
                        {
                            return payload;
                        }
                    }
                }
            }
            // Unite/enum extractor: MIR FieldGet of the shared `payload` slot (not a MethodDef Call).
            let output = self.next_value(MirValueOrigin::Temporary);
            let storage = match object_ty.as_ref() {
                Some(ValkyrieType::Named(name)) => storage_kind_for_named_type(name.as_str(), &self.struct_is_value_type),
                Some(ValkyrieType::Apply(base, _)) => match base.as_ref() {
                    ValkyrieType::Named(name) => storage_kind_for_named_type(name.as_str(), &self.struct_is_value_type),
                    _ => MirStorageKind::Reference,
                },
                _ => MirStorageKind::Reference,
            };
            let layout_id = object_ty.as_ref().and_then(|ty| {
                let sum_name = named_type_name(ty)
                    .and_then(|name| self.sum_types.iter().find(|sum| sum_type_name_matches(&sum.name, name)).map(|sum| sum.name.clone()));
                let candidate = match sum_name.as_deref() {
                    Some(sum_name) => self.layout_id_for_sum_field_access(ty, sum_name),
                    None => layout_id_for_type(ty, &self.aggregate_layouts),
                };
                candidate.and_then(|id| {
                    let has_payload = self
                        .aggregate_layouts
                        .layouts
                        .iter()
                        .find(|layout| layout.id == id)
                        .is_some_and(|layout| layout.fields.iter().any(|field| field.name == "payload"));
                    has_payload.then_some(id)
                })
            });
            let storage = layout_id
                .and_then(|id| self.aggregate_layouts.layouts.iter().find(|layout| layout.id == id).map(|layout| layout.storage))
                .unwrap_or(storage);
            self.instructions.push(MirInstruction::from_operation(MirOperation::FieldGet { object: value, field: "payload".to_string() }));
            let payload_ty = resolved.extractor_payload_type.clone().unwrap_or_else(|| resolved.return_type.clone());
            self.value_types.insert(output, payload_ty);
            return MirOperand::Value(output);
        }
        let output = self.next_value(MirValueOrigin::Temporary);
        self.instructions.push(MirInstruction::from_operation(MirOperation::Call {
            callee: MirOperand::Symbol(resolved.symbol.clone()),
            arguments: vec![value.clone()],
        }));
        self.value_types.insert(output, resolved.return_type.clone());
        MirOperand::Value(output)
    }

    fn lower_nullable_some_operand(&mut self, nullable: MirOperand) -> MirOperand {
        let is_null = self.lower_static_call("is_null", vec![nullable], MirValueOrigin::Temporary);
        self.value_types.insert(is_null, ValkyrieType::Boolean);
        self.lower_logical_not_operand(MirOperand::Value(is_null))
    }

    fn lower_class_payload_operand(&mut self, handle: MirOperand) -> MirOperand {
        let value = self.lower_static_call("__ref_deref", vec![handle], MirValueOrigin::Temporary);
        MirOperand::Value(value)
    }

    fn lower_extractor_matched_operand(&mut self, nullable: MirOperand) -> MirOperand {
        self.lower_nullable_some_operand(nullable)
    }

    fn lower_extractor_payload_slot_operand(&mut self, payload: MirOperand, slot: usize, expected_type: Option<ValkyrieType>) -> MirOperand {
        let payload_type = infer_builder_operand_type(&payload, &self.value_types);
        match payload_type.as_ref() {
            Some(ValkyrieType::Tuple(_)) => {
                let extracted = self.lower_static_call(&format!("tuple_get_{slot}"), vec![payload], MirValueOrigin::Temporary);
                if let Some(expected_type) = expected_type {
                    self.value_types.insert(extracted, expected_type);
                }
                MirOperand::Value(extracted)
            }
            _ if slot == 0 => {
                // Single-field extractor payload: the value is the payload itself — no synthetic Call.
                // Do not clobber a concrete SSA type (e.g. VonDiagnostic from Result Apply) with the
                // generic unite parameter (`Named("E")`) from HIR extractor_payload_type.
                if let (Some(expected_type), MirOperand::Value(value)) = (expected_type, &payload) {
                    self.value_types.entry(*value).or_insert(expected_type);
                }
                payload
            }
            _ => {
                let extracted = self.lower_static_call(&format!("tuple_get_{slot}"), vec![payload], MirValueOrigin::Temporary);
                if let Some(expected_type) = expected_type {
                    self.value_types.insert(extracted, expected_type);
                }
                MirOperand::Value(extracted)
            }
        }
    }

    fn infer_extractor_slot_type(&self, resolved: &HirResolvedCall, slot: usize) -> Option<ValkyrieType> {
        let payload_type = resolved.extractor_payload_type.clone().or_else(|| nullable_payload_type(&resolved.return_type))?;
        match payload_type {
            ValkyrieType::Tuple(items) => items.get(slot).cloned(),
            _ if slot == 0 => Some(payload_type.clone()),
            _ => None,
        }
    }

    pub(super) fn lower_object_field_operand(&mut self, value: MirOperand, struct_name: &str, field_name: &Identifier) -> MirOperand {
        let fallback_storage = storage_kind_for_named_type(struct_name, &self.struct_is_value_type);
        let object = if self.struct_is_value_type.get(struct_name) == Some(&false) { self.lower_class_payload_operand(value) } else { value };
        let output = self.next_value(MirValueOrigin::Temporary);
        let ty = ValkyrieType::Named(Identifier::new(struct_name));
        let layout_id = layout_id_for_type(&ty, &self.aggregate_layouts).or_else(|| {
            if matches!(struct_name, "any" | "null" | "object" | "Self" | "__auto" | "__opaque") {
                return None;
            }
            // Imported / late-discovered structs: materialize layout from field table.
            let fields = self.struct_field_layouts.get(struct_name)?;
            if fields.is_empty() {
                return None;
            }
            Some(ensure_named_aggregate_layout(&mut self.aggregate_layouts, struct_name, fallback_storage, fields))
        });
        let storage = layout_id
            .and_then(|id| self.aggregate_layouts.layouts.iter().find(|layout| layout.id == id).map(|layout| layout.storage))
            .unwrap_or(fallback_storage);
        self.instructions.push(MirInstruction::from_operation(MirOperation::FieldGet { object, field: field_name.to_string() }));
        if let Some(field_type) = self
            .lookup_struct_field_type(struct_name, field_name.as_str())
            .or_else(|| self.field_type_from_layout(layout_id, field_name.as_str()))
        {
            self.value_types.insert(output, field_type);
        }
        MirOperand::Value(output)
    }

    pub(super) fn lower_eq_constant_operand(&mut self, lhs: MirOperand, rhs: MirConstant, ty: &ValkyrieType) -> MirOperand {
        let _ = (lhs, rhs, ty);
        MirOperand::Constant(MirConstant::Bool(false))
    }

    pub(super) fn lower_lt_constant_operand(&mut self, lhs: MirOperand, rhs: MirConstant, ty: &ValkyrieType) -> MirOperand {
        self.lower_lt_operands(lhs, MirOperand::Constant(rhs), ty)
    }

    pub(super) fn lower_lt_operands(&mut self, lhs: MirOperand, rhs: MirOperand, ty: &ValkyrieType) -> MirOperand {
        let _ = (lhs, rhs, ty);
        MirOperand::Constant(MirConstant::Bool(false))
    }

    /// Short-circuit `!value`: `if value { false } else { true }`.
    pub(super) fn lower_logical_not_operand(&mut self, value: MirOperand) -> MirOperand {
        let pre_bindings = self.bindings.clone();
        let cond_block = self.current_block;
        let then_block = self.new_block("not_then");
        let else_block = self.new_block("not_else");
        let merge_block = self.new_block("not_merge");
        let mut exit_value = None;

        self.current_block = cond_block;
        self.terminate(MirTerminator::Branch { condition: value, then_target: then_block, else_target: else_block });
        self.flush_block("not_cond");

        self.current_block = then_block;
        self.bindings = pre_bindings.clone();
        self.ensure_branch_exit_parameter(merge_block, &mut exit_value, Some(ValkyrieType::Boolean));
        self.terminate(MirTerminator::Jump { target: merge_block, arguments: vec![MirOperand::Constant(MirConstant::Bool(false))] });
        self.flush_block("not_then");

        self.current_block = else_block;
        self.bindings = pre_bindings.clone();
        self.ensure_branch_exit_parameter(merge_block, &mut exit_value, Some(ValkyrieType::Boolean));
        self.terminate(MirTerminator::Jump { target: merge_block, arguments: vec![MirOperand::Constant(MirConstant::Bool(true))] });
        self.flush_block("not_else");

        self.current_block = merge_block;
        self.bindings = pre_bindings;
        exit_value.map(MirOperand::Value).unwrap_or(MirOperand::Constant(MirConstant::Bool(false)))
    }

    /// Short-circuit `lhs && rhs`: `if lhs { rhs } else { false }`.
    pub(super) fn lower_logical_and_operand(&mut self, lhs: MirOperand, rhs: MirOperand) -> MirOperand {
        let pre_bindings = self.bindings.clone();
        let cond_block = self.current_block;
        let then_block = self.new_block("and_then");
        let else_block = self.new_block("and_else");
        let merge_block = self.new_block("and_merge");
        let mut exit_value = None;

        self.current_block = cond_block;
        self.terminate(MirTerminator::Branch { condition: lhs, then_target: then_block, else_target: else_block });
        self.flush_block("and_cond");

        self.current_block = then_block;
        self.bindings = pre_bindings.clone();
        self.ensure_branch_exit_parameter(merge_block, &mut exit_value, Some(ValkyrieType::Boolean));
        self.terminate(MirTerminator::Jump { target: merge_block, arguments: vec![rhs] });
        self.flush_block("and_then");

        self.current_block = else_block;
        self.bindings = pre_bindings.clone();
        self.ensure_branch_exit_parameter(merge_block, &mut exit_value, Some(ValkyrieType::Boolean));
        self.terminate(MirTerminator::Jump { target: merge_block, arguments: vec![MirOperand::Constant(MirConstant::Bool(false))] });
        self.flush_block("and_else");

        self.current_block = merge_block;
        self.bindings = pre_bindings;
        exit_value.map(MirOperand::Value).unwrap_or(MirOperand::Constant(MirConstant::Bool(false)))
    }

    /// Short-circuit `lhs || rhs`: `if lhs { true } else { rhs }`.
    pub(super) fn lower_logical_or_operand(&mut self, lhs: MirOperand, rhs: MirOperand) -> MirOperand {
        let pre_bindings = self.bindings.clone();
        let cond_block = self.current_block;
        let then_block = self.new_block("or_then");
        let else_block = self.new_block("or_else");
        let merge_block = self.new_block("or_merge");
        let mut exit_value = None;

        self.current_block = cond_block;
        self.terminate(MirTerminator::Branch { condition: lhs, then_target: then_block, else_target: else_block });
        self.flush_block("or_cond");

        self.current_block = then_block;
        self.bindings = pre_bindings.clone();
        self.ensure_branch_exit_parameter(merge_block, &mut exit_value, Some(ValkyrieType::Boolean));
        self.terminate(MirTerminator::Jump { target: merge_block, arguments: vec![MirOperand::Constant(MirConstant::Bool(true))] });
        self.flush_block("or_then");

        self.current_block = else_block;
        self.bindings = pre_bindings.clone();
        self.ensure_branch_exit_parameter(merge_block, &mut exit_value, Some(ValkyrieType::Boolean));
        self.terminate(MirTerminator::Jump { target: merge_block, arguments: vec![rhs] });
        self.flush_block("or_else");

        self.current_block = merge_block;
        self.bindings = pre_bindings;
        exit_value.map(MirOperand::Value).unwrap_or(MirOperand::Constant(MirConstant::Bool(false)))
    }

    pub(super) fn merge_pattern_match_operands(&mut self, lhs: Option<MirOperand>, rhs: MirOperand) -> MirOperand {
        match (lhs, rhs) {
            (None, rhs) => rhs,
            (Some(MirOperand::Constant(MirConstant::Bool(false))), _) => MirOperand::Constant(MirConstant::Bool(false)),
            (Some(_), MirOperand::Constant(MirConstant::Bool(false))) => MirOperand::Constant(MirConstant::Bool(false)),
            (Some(MirOperand::Constant(MirConstant::Bool(true))), rhs) => rhs,
            (Some(lhs), MirOperand::Constant(MirConstant::Bool(true))) => lhs,
            (Some(lhs), rhs) => self.lower_logical_and_operand(lhs, rhs),
        }
    }

    pub(super) fn merge_pattern_match_any_operands(&mut self, lhs: Option<MirOperand>, rhs: MirOperand) -> MirOperand {
        match (lhs, rhs) {
            (None, rhs) => rhs,
            (Some(MirOperand::Constant(MirConstant::Bool(true))), _) => MirOperand::Constant(MirConstant::Bool(true)),
            (Some(_), MirOperand::Constant(MirConstant::Bool(true))) => MirOperand::Constant(MirConstant::Bool(true)),
            (Some(MirOperand::Constant(MirConstant::Bool(false))), rhs) => rhs,
            (Some(lhs), MirOperand::Constant(MirConstant::Bool(false))) => lhs,
            (Some(lhs), rhs) => self.lower_logical_or_operand(lhs, rhs),
        }
    }

    pub(super) fn lookup_struct_field_type(&self, struct_name: &str, field_name: &str) -> Option<ValkyrieType> {
        self.lookup_struct_field_type_recursive(struct_name, field_name, &mut BTreeSet::new())
    }

    pub(super) fn lookup_struct_field_type_recursive(
        &self,
        struct_name: &str,
        field_name: &str,
        visiting: &mut BTreeSet<String>,
    ) -> Option<ValkyrieType> {
        if !visiting.insert(struct_name.to_string()) {
            return None;
        }

        if let Some(field_type) = self
            .struct_field_layouts
            .get(struct_name)
            .and_then(|fields| fields.iter().find(|(name, _)| name == field_name))
            .map(|(_, ty)| ty.clone())
        {
            visiting.remove(struct_name);
            return Some(field_type);
        }

        // Imported/generated HIR can retain a qualified struct key while the
        // operand carries only its nominal tail. Resolve that identity only
        // when the suffix is unique and structurally agrees; never infer from
        // the field name alone.
        let suffix = format!(".{struct_name}");
        let mut qualified_match = None;
        for (candidate_name, fields) in &self.struct_field_layouts {
            if !candidate_name.ends_with(&suffix) {
                continue;
            }
            if let Some((field_name, field_type)) = fields.iter().find(|(name, _)| name == field_name) {
                if let Some((existing_name, existing_type)) = &qualified_match {
                    if existing_type != field_type || existing_name != candidate_name {
                        qualified_match = None;
                        break;
                    }
                }
                else {
                    qualified_match = Some((candidate_name.clone(), field_type.clone()));
                }
            }
        }
        if let Some((_, field_type)) = qualified_match {
            visiting.remove(struct_name);
            return Some(field_type);
        }

        let result = self
            .struct_parent_index
            .get(struct_name)
            .and_then(|parents| parents.iter().find_map(|parent| self.lookup_struct_field_type_recursive(parent, field_name, visiting)));
        visiting.remove(struct_name);
        result
    }

    pub(super) fn struct_has_field(&self, struct_name: &str, field_name: &str) -> bool {
        self.lookup_struct_field_type(struct_name, field_name).is_some()
    }

    pub(super) fn type_pattern_matches(&self, actual_type: &ValkyrieType, pattern_name: &NamePath) -> bool {
        let Some(expected_name) = pattern_name.parts().last().map(|identifier| identifier.as_str())
        else {
            return false;
        };

        match actual_type {
            ValkyrieType::Named(name) => self.named_or_parent_matches(name.as_str(), expected_name),
            ValkyrieType::Apply(base, _) => self.type_pattern_matches(base, pattern_name),
            _ => plain_type_pattern_matches(actual_type, pattern_name),
        }
    }

    pub(super) fn named_or_parent_matches(&self, actual_name: &str, expected_name: &str) -> bool {
        if actual_name == expected_name {
            return true;
        }

        self.struct_inherits_from(actual_name, expected_name, &mut BTreeSet::new())
    }

    pub(super) fn struct_inherits_from(&self, actual_name: &str, expected_name: &str, visiting: &mut BTreeSet<String>) -> bool {
        if !visiting.insert(actual_name.to_string()) {
            return false;
        }

        let found = self.struct_parent_index.get(actual_name).is_some_and(|parents| {
            parents.iter().any(|parent| parent == expected_name || self.struct_inherits_from(parent, expected_name, visiting))
        });
        visiting.remove(actual_name);
        found
    }

    /// Extractor ???? extractor payload??????????????????
    pub(super) fn bind_extractor_pattern_from_payload(&mut self, extractor: &HirExtractorPattern, payload: MirOperand) {
        let resolved = extractor_resolved_call(extractor);
        match extractor {
            HirExtractorPattern::Constructor { fields, .. } => {
                for (slot, field_pattern) in fields.iter().enumerate() {
                    let mut slot_type = resolved.and_then(|resolved| self.infer_extractor_slot_type(resolved, slot));
                    let payload_ty = infer_builder_operand_type(&payload, &self.value_types);
                    if slot_type.as_ref().is_some_and(looks_like_unbound_type_parameter) {
                        slot_type = payload_ty.clone();
                    }
                    else if slot_type.is_none() {
                        slot_type = payload_ty;
                    }
                    let extracted = self.lower_extractor_payload_slot_operand(payload.clone(), slot, slot_type.clone());
                    // Preserve extractor payload/slot types on bindings (`Fine(plan)` → `plan`
                    // must stay `LegionSourceClosurePlan`) so later `plan.files.length()` can
                    // recover ArrayLen instead of emitting an untyped Call (SMIR002).
                    self.bind_pattern_from_operand(field_pattern, extracted, slot_type);
                }
            }
            HirExtractorPattern::Array { prefix, rest, suffix, .. } => {
                let item_hint = resolved.and_then(|resolved| {
                    resolved.extractor_payload_type.as_ref().and_then(|ty| match ty {
                        ValkyrieType::Tuple(items) => items.first().cloned(),
                        ValkyrieType::Array(item) => Some(*item.clone()),
                        other => Some(other.clone()),
                    })
                });
                for (slot, field_pattern) in prefix.iter().enumerate() {
                    let extracted = self.lower_extractor_payload_slot_operand(payload.clone(), slot, item_hint.clone());
                    self.bind_pattern_from_operand(field_pattern, extracted, item_hint.clone());
                }
                let mut next_slot = prefix.len();
                if let Some(rest) = rest {
                    let rest_type = item_hint.clone().map(|item| ValkyrieType::Array(Box::new(item)));
                    let extracted = self.lower_extractor_payload_slot_operand(payload.clone(), next_slot, rest_type.clone());
                    self.bind_pattern_from_operand(&HirPattern::Variable(rest.clone()), extracted, rest_type);
                    next_slot += 1;
                }
                for (index, field_pattern) in suffix.iter().enumerate() {
                    let extracted = self.lower_extractor_payload_slot_operand(payload.clone(), next_slot + index, item_hint.clone());
                    self.bind_pattern_from_operand(field_pattern, extracted, item_hint.clone());
                }
            }
        }
    }

    pub(super) fn bind_object_pattern_from_operand(
        &mut self,
        fields: &[(Identifier, HirPattern)],
        rest: Option<&crate::types::hir::HirIdentifier>,
        operand: MirOperand,
    ) {
        let Some(actual_type) = infer_builder_operand_type(&operand, &self.value_types)
        else {
            for (_, field_pattern) in fields {
                self.bind_unknown_pattern_bindings(field_pattern);
            }
            if let Some(rest) = rest {
                self.bind_unknown_pattern_bindings(&HirPattern::Variable(rest.clone()));
            }
            return;
        };

        let Some(struct_name) = named_type_name(&actual_type)
        else {
            for (_, field_pattern) in fields {
                self.bind_unknown_pattern_bindings(field_pattern);
            }
            if let Some(rest) = rest {
                self.bind_unknown_pattern_bindings(&HirPattern::Variable(rest.clone()));
            }
            return;
        };

        if !self.has_known_struct_layout(struct_name) {
            for (_, field_pattern) in fields {
                self.bind_unknown_pattern_bindings(field_pattern);
            }
            if let Some(rest) = rest {
                self.bind_unknown_pattern_bindings(&HirPattern::Variable(rest.clone()));
            }
            return;
        }

        for (field_name, field_pattern) in fields {
            if !self.struct_has_field(struct_name, field_name.as_str()) {
                self.bind_unknown_pattern_bindings(field_pattern);
                continue;
            }

            let extracted = self.lower_object_field_operand(operand.clone(), struct_name, field_name);
            self.bind_pattern_from_operand(field_pattern, extracted, None);
        }
        if let Some(rest) = rest {
            self.bind_pattern_from_operand(&HirPattern::Variable(rest.clone()), operand, Some(actual_type));
        }
    }

    pub(super) fn bind_unknown_pattern_bindings(&mut self, pattern: &HirPattern) {
        match pattern {
            HirPattern::Wildcard | HirPattern::Else => {}
            HirPattern::Variable(identifier) => {
                // Unknown pattern bindings have no semantic operand. Do not
                // manufacture a symbol that a backend could mistake for a
                // real declaration; keep the failure structured and fail
                // closed at the shared contract boundary.
                let _ = identifier;
            }
            HirPattern::Tuple(items) => {
                for item in items {
                    self.bind_unknown_pattern_bindings(item);
                }
            }
            HirPattern::Extractor(extractor) => match extractor {
                HirExtractorPattern::Array { prefix, rest, suffix, .. } => {
                    for item in prefix {
                        self.bind_unknown_pattern_bindings(item);
                    }
                    if let Some(rest) = rest {
                        self.bind_unknown_pattern_bindings(&HirPattern::Variable(rest.clone()));
                    }
                    for item in suffix {
                        self.bind_unknown_pattern_bindings(item);
                    }
                }
                HirExtractorPattern::Constructor { fields, .. } => {
                    for field in fields {
                        self.bind_unknown_pattern_bindings(field);
                    }
                }
            },
            HirPattern::Object { fields, rest, .. } => {
                for (_, field_pattern) in fields {
                    self.bind_unknown_pattern_bindings(field_pattern);
                }
                if let Some(rest) = rest {
                    self.bind_unknown_pattern_bindings(&HirPattern::Variable(rest.clone()));
                }
            }
            HirPattern::TypedBind { identifier, .. } => {
                self.bind_unknown_pattern_bindings(&HirPattern::Variable(identifier.clone()));
            }
            HirPattern::Bind { pattern, .. } | HirPattern::Mut(pattern) | HirPattern::Pin { pattern, .. } => {
                self.bind_unknown_pattern_bindings(pattern);
            }
            _ => {}
        }
    }

    pub(super) fn extract_tuple_field_operand(&mut self, operand: &MirOperand, index: usize, tuple_ty: Option<&ValkyrieType>) -> MirValueRef {
        if let (MirOperand::Value(value), Some(tuple_ty)) = (operand, tuple_ty) {
            // Keep the producer's semantic tuple type attached to the
            // operand itself. `tuple_get_N` is a physical projection, but
            // its owner layout must still come from Semantic MIR metadata.
            self.value_types.insert(*value, tuple_ty.clone());
            let _ = ensure_layout_for_type(&mut self.aggregate_layouts, tuple_ty);
        }
        let extracted = self.lower_static_call(&format!("tuple_get_{index}"), vec![operand.clone()], MirValueOrigin::Temporary);
        if let Some(ValkyrieType::Tuple(types)) = tuple_ty {
            if let Some(elem_ty) = types.get(index) {
                self.value_types.insert(extracted, elem_ty.clone());
            }
        }
        extracted
    }

    /// ?????? lowering ????????????? fallback ????
    ///
    /// ??????????????????? lowering?extractor ? resolved?
    /// scrutinee ?????tuple/object ???????????????
    /// `MirDiagnostic::PatternLoweringFailed` ????? `Bool(false)`?
    /// ???? emit ??? trap ? `PatternMatch` ???
    pub(super) fn lower_fallback_pattern_match(&mut self, pattern: HirPattern, _value: MirOperand) -> MirOperand {
        self.diagnostics
            .push(MirDiagnostic::PatternLoweringFailed { pattern, reason: "???? lowering ? MIR ?????????? extractor ??".to_string() });
        MirOperand::Constant(MirConstant::Bool(false))
    }
}

fn extractor_resolved_call(extractor: &HirExtractorPattern) -> Option<&HirResolvedCall> {
    match extractor {
        HirExtractorPattern::Constructor { resolved, .. } | HirExtractorPattern::Array { resolved, .. } => resolved.as_ref(),
    }
}

/// Map sum-variant `NyarType` payload back to a MIR `ValkyrieType` for SSA typing.
fn sum_payload_to_valkyrie(ty: &NyarType) -> Option<ValkyrieType> {
    match ty {
        NyarType::Named(name) => Some(ValkyrieType::Named(Identifier::new(name.as_str()))),
        NyarType::Utf8 => Some(ValkyrieType::Utf8),
        NyarType::Utf16 => Some(ValkyrieType::Utf16),
        NyarType::Boolean => Some(ValkyrieType::Boolean),
        NyarType::Unit => Some(ValkyrieType::Unit),
        NyarType::Integer32 { signed } => Some(ValkyrieType::Integer32 { signed: *signed }),
        NyarType::Integer64 { signed } => Some(ValkyrieType::Integer64 { signed: *signed }),
        NyarType::Integer16 { signed } => Some(ValkyrieType::Integer16 { signed: *signed }),
        NyarType::Integer8 { signed } => Some(ValkyrieType::Integer8 { signed: *signed }),
        NyarType::Float32 => Some(ValkyrieType::Float32),
        NyarType::Float64 => Some(ValkyrieType::Float64),
        NyarType::Character => Some(ValkyrieType::Character),
        NyarType::Array(inner) => Some(ValkyrieType::Array(Box::new(sum_payload_to_valkyrie(inner)?))),
        NyarType::Tuple(items) => {
            let mapped = items.iter().map(sum_payload_to_valkyrie).collect::<Option<Vec<_>>>()?;
            Some(ValkyrieType::Tuple(mapped))
        }
        _ => None,
    }
}

fn tuple_literal_exprs(expr: &HirExpr) -> Option<Vec<HirExpr>> {
    match &expr.kind {
        HirExprKind::Call { callee, args, .. } if callee_name_matches(&callee.kind, "tuple") => {
            Some(args.iter().map(|arg| arg.value.clone()).collect())
        }
        _ => None,
    }
}

/// Exact or dotted/underscored suffix match (`std.data.text.von.VonTokenKind` ↔ `VonTokenKind`).
/// Also maps type aliases such as `VonParseResult` onto the `Result` unite layout.
fn sum_type_name_matches(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let boundary_suffix = |hay: &str, needle: &str| -> bool {
        if !hay.ends_with(needle) || hay.len() <= needle.len() {
            return false;
        }
        matches!(hay.as_bytes().get(hay.len() - needle.len() - 1).copied(), Some(b'.' | b'_'))
    };
    if boundary_suffix(a, b) || boundary_suffix(b, a) {
        return true;
    }
    // `type VonParseResult<T> = Result<T, E>` keeps the alias name on Apply; Fine/Fail
    // must still resolve against the Result unite in `sum_types`.
    let result_alias = |name: &str| name == "Result" || (name.ends_with("Result") && name != "Result");
    (a == "Result" && result_alias(b)) || (b == "Result" && result_alias(a))
}

/// HIR extractor metadata often retains unite type parameters (`T` / `E`) instead of the
/// concrete Apply substitution. Those names must not win over an already-concrete SSA type.
fn looks_like_unbound_type_parameter(ty: &ValkyrieType) -> bool {
    match ty {
        ValkyrieType::Named(name) => {
            let text = name.as_str();
            !text.is_empty()
                && text.chars().all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
                && text.chars().next().is_some_and(|ch| ch.is_ascii_uppercase())
                && text.len() <= 3
        }
        _ => false,
    }
}
