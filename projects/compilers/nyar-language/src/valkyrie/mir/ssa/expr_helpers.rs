use std::collections::BTreeMap;

use crate::types::{
    Identifier, NamePath,
    hir::{HirCallableDomain, HirExpr, HirExprKind},
};

use super::{MirBuilder, MirConstant, MirOperand, MirValueRef, ValkyrieType};

impl MirBuilder {
    pub(super) fn resolve_static_expr(&self, expr: &HirExpr) -> Option<HirExpr> {
        match &expr.kind {
            HirExprKind::Variable(identifier) => self.static_bindings.get(identifier.name.as_str()).cloned().or_else(|| Some(expr.clone())),
            _ => Some(expr.clone()),
        }
    }

    pub(super) fn resolve_static_iterable_items(&self, expr: &HirExpr) -> Option<Vec<HirExpr>> {
        let resolved = self.resolve_static_expr(expr)?;
        match resolved.kind {
            HirExprKind::ArrayLiteral { items } => Some(items),
            HirExprKind::Call { callee, args, .. } if callee_name_matches(&callee.kind, "array") => {
                Some(args.iter().map(|arg| arg.value.clone()).collect())
            }
            HirExprKind::Call { callee, args, resolved: call_resolved } if callee_name_matches(&callee.kind, "tuple") => {
                Some(vec![HirExpr { kind: HirExprKind::Call { callee, args, resolved: call_resolved }, span: resolved.span }])
            }
            _ => None,
        }
    }

    pub(super) fn extract_method_call(&mut self, callee: &HirExpr) -> Option<(MirOperand, Identifier)> {
        match &callee.kind {
            HirExprKind::FieldAccess { object, field } => {
                // Always lower FieldAccess callees as method calls (receiver + method name).
                // Requiring a root binding incorrectly collapsed nested field chains like
                // `plan.source_closure.package_names.length()` into a dotted Symbol with no args.
                let receiver_operand = self.lower_expr_to_operand(object);
                Some((receiver_operand, field.clone()))
            }
            // Typechecker may flatten `plan.source_closure.package_names.length` into a Path.
            // Only rewrite when the root is a local/param binding — otherwise keep qualified
            // free-function Paths like `std.io.print_line` intact.
            HirExprKind::Path(path) if path.parts().len() >= 2 => {
                let parts = path.parts();
                let root = parts[0].as_str();
                if !self.bindings.contains_key(root) {
                    return None;
                }
                let method_name = parts.last()?.clone();
                let span = callee.span.clone();
                let mut receiver_hir = HirExpr {
                    kind: HirExprKind::Variable(crate::types::hir::HirIdentifier {
                        name: parts[0].clone(),
                        shadow_index: 0,
                        span: span.clone(),
                    }),
                    span: span.clone(),
                };
                for field in &parts[1..parts.len() - 1] {
                    receiver_hir =
                        HirExpr { kind: HirExprKind::FieldAccess { object: Box::new(receiver_hir), field: field.clone() }, span: span.clone() };
                }
                let receiver_operand = self.lower_expr_to_operand(&receiver_hir);
                Some((receiver_operand, method_name))
            }
            _ => None,
        }
    }

    pub(super) fn emit_singleton_instance_operand(&mut self, singleton_name: &str) -> MirOperand {
        let accessor = self.singleton_accessors.get(singleton_name).cloned().unwrap_or_else(|| "instance".to_string());
        let value = self.next_value(super::MirValueOrigin::CallResult);
        self.instructions.push(super::MirInstruction::from_operation(super::MirOperation::Call {
            callee: MirOperand::Symbol(NamePath::new(vec![Identifier::new(singleton_name), Identifier::new(accessor.as_str())])),
            arguments: Vec::new(),
        }));
        self.value_types.insert(value, ValkyrieType::Named(Identifier::new(singleton_name)));
        MirOperand::Value(value)
    }

    pub(super) fn try_lower_singleton_static_call(
        &mut self,
        callee: &HirExpr,
        args: &[crate::types::hir::HirCallArgument],
    ) -> Option<MirOperand> {
        if let Some(result) = self.try_lower_singleton_receiver_call(callee, args) {
            return Some(result);
        }
        let HirExprKind::Path(path) = &callee.kind
        else {
            return None;
        };
        if path.parts().len() < 2 {
            return None;
        }
        let singleton_name = path.parts()[0].as_str();
        if !self.singleton_accessors.contains_key(singleton_name) {
            return None;
        }
        let method_name = path.parts()[1].clone();
        if let Some(accessor) = self.singleton_accessors.get(singleton_name) {
            if method_name.as_str() == accessor.as_str() {
                return Some(self.emit_singleton_instance_operand(singleton_name));
            }
        }
        let instance = self.emit_singleton_instance_operand(singleton_name);
        let mut arguments = args.iter().map(|arg| self.lower_expr_to_operand(&arg.value)).collect::<Vec<_>>();
        arguments.insert(0, instance);
        let parameter_types =
            arguments.iter().map(|argument| infer_builder_operand_type(argument, &self.value_types)).collect::<Option<Vec<_>>>();
        let value = self.next_value(super::MirValueOrigin::CallResult);
        self.instructions.push(super::MirInstruction::from_operation(super::MirOperation::Call {
            callee: MirOperand::Symbol(NamePath::new(vec![Identifier::new(singleton_name), method_name.clone()])),
            arguments,
        }));
        if let Some(return_type) = self.return_types.get(method_name.as_str()).cloned() {
            self.value_types.insert(value, return_type);
        }
        Some(MirOperand::Value(value))
    }

    pub(super) fn try_lower_singleton_receiver_call(
        &mut self,
        callee: &HirExpr,
        args: &[crate::types::hir::HirCallArgument],
    ) -> Option<MirOperand> {
        let method_name = match &callee.kind {
            HirExprKind::Variable(identifier) => identifier.name.clone(),
            HirExprKind::Path(path) if path.parts().len() == 1 => path.parts()[0].clone(),
            _ => return None,
        };
        let receiver = args.first()?;
        let singleton_name = match &receiver.value.kind {
            HirExprKind::Variable(identifier) if self.singleton_accessors.contains_key(identifier.name.as_str()) => identifier.name.as_str(),
            HirExprKind::Path(path) if path.parts().len() == 1 && self.singleton_accessors.contains_key(path.parts()[0].as_str()) => {
                path.parts()[0].as_str()
            }
            _ => return None,
        };
        if self.singleton_accessors.get(singleton_name).is_some_and(|accessor| accessor.as_str() == method_name.as_str()) {
            return Some(self.emit_singleton_instance_operand(singleton_name));
        }
        let instance = self.emit_singleton_instance_operand(singleton_name);
        let mut arguments = args.iter().skip(1).map(|arg| self.lower_expr_to_operand(&arg.value)).collect::<Vec<_>>();
        arguments.insert(0, instance);
        let parameter_types =
            arguments.iter().map(|argument| infer_builder_operand_type(argument, &self.value_types)).collect::<Option<Vec<_>>>();
        let value = self.next_value(super::MirValueOrigin::CallResult);
        self.instructions.push(super::MirInstruction::from_operation(super::MirOperation::Call {
            callee: MirOperand::Symbol(NamePath::new(vec![Identifier::new(singleton_name), method_name.clone()])),
            arguments,
        }));
        if let Some(return_type) = self.return_types.get(method_name.as_str()).cloned() {
            self.value_types.insert(value, return_type);
        }
        Some(MirOperand::Value(value))
    }

    pub(super) fn lower_singleton_field_object(&mut self, object: &HirExpr) -> Option<MirOperand> {
        match &object.kind {
            HirExprKind::Variable(identifier) if self.singleton_accessors.contains_key(identifier.name.as_str()) => {
                Some(self.emit_singleton_instance_operand(identifier.name.as_str()))
            }
            HirExprKind::Path(path) if path.parts().len() == 1 && self.singleton_accessors.contains_key(path.parts()[0].as_str()) => {
                Some(self.emit_singleton_instance_operand(path.parts()[0].as_str()))
            }
            _ => None,
        }
    }
}

// ADR 0010 purge: removed mint_receiver_call_evidence / receiver_uses_witness_dispatch /
// generic_call_facts — those stuffed EvidenceId/GenericFunctionId onto LegacyCall.

pub(super) fn peel_generic_apply(callee: &HirExpr) -> (Vec<ValkyrieType>, &HirExpr) {
    match &callee.kind {
        HirExprKind::GenericApply { callee, arguments } => (arguments.clone(), callee.as_ref()),
        _ => (Vec::new(), callee),
    }
}

pub(super) fn infer_builder_operand_type(operand: &MirOperand, value_types: &BTreeMap<MirValueRef, ValkyrieType>) -> Option<ValkyrieType> {
    match operand {
        MirOperand::Value(value_ref) => value_types.get(value_ref).cloned(),
        MirOperand::Constant(constant) => infer_builder_constant_type(constant),
        MirOperand::Symbol(_) => None,
    }
}

fn valkyrie_type_is_numeric(ty: &ValkyrieType) -> bool {
    matches!(
        ty,
        ValkyrieType::Boolean
            | ValkyrieType::Character
            | ValkyrieType::Integer8 { .. }
            | ValkyrieType::Integer16 { .. }
            | ValkyrieType::Integer32 { .. }
            | ValkyrieType::Integer64 { .. }
            | ValkyrieType::Float32
            | ValkyrieType::Float64
    ) || matches!(
        ty,
        ValkyrieType::Named(name) if matches!(
            name.as_str(),
            "bool" | "char"
                | "i8" | "u8" | "i16" | "u16" | "i32" | "u32" | "i64" | "u64"
                | "i128" | "u128" | "f32" | "f64" | "f128" | "usize" | "isize"
        )
    )
}

/// 若 HIR 把数值算术误绑到 `Utf8Text`/`Utf16Text` 的 `infix +` 等，改回裸运算符名，
/// 让后端走 intrinsic（与数组上误绑 `Utf8Text.length` 的处理对称）。
pub(super) fn reject_text_operator_for_numeric_args(
    callee: MirOperand,
    _arguments: &[MirOperand],
    _value_types: &BTreeMap<MirValueRef, ValkyrieType>,
) -> MirOperand {
    // A nominal callee name is not semantic evidence. Numeric recovery must
    // be decided by the resolved MIR operator contract, never by text names.
    callee
}

pub(super) fn named_type_name(ty: &ValkyrieType) -> Option<&str> {
    match ty {
        ValkyrieType::Named(name) => Some(name.as_str()),
        ValkyrieType::Apply(base, _) => named_type_name(base),
        _ => None,
    }
}

pub(super) fn future_resume_type(ty: &ValkyrieType) -> Option<ValkyrieType> {
    match ty {
        ValkyrieType::Apply(base, arguments) if arguments.len() == 1 && matches!(named_type_name(base), Some("Future" | "Promise")) => {
            arguments.first().cloned()
        }
        _ => None,
    }
}

/// 判断 callee 名称是否匹配预期。
/// 同时支持 `HirExprKind::Path`（多部分路径）和 `HirExprKind::Variable`（单部分名称），
/// 因为 `lower_name_expression` 会将单部分名称降级为 `Variable`。
pub(super) fn callee_name_matches(kind: &HirExprKind, expected: &str) -> bool {
    match kind {
        HirExprKind::Path(path) if path.to_string() == expected => true,
        HirExprKind::Variable(ident) if ident.name.as_str() == expected => true,
        _ => false,
    }
}

pub(super) fn lower_callee_operand(
    expr: &HirExpr,
    resolved: Option<&crate::types::hir::HirResolvedCall>,
    builder: &mut MirBuilder,
) -> MirOperand {
    // Locals / parameters (including function-typed `f`) must stay SSA Values so CLR can
    // emit an indirect invoke. Prefer bindings over any free-function `resolved` symbol.
    if let HirExprKind::Variable(identifier) = &expr.kind {
        if let Some(bound) = builder.bindings.get(identifier.name.as_str()) {
            return bound.clone();
        }
    }
    // Free-function / operator calls only. Instance/method callees must go through
    // `extract_method_call` so MIR keeps the receiver argument for virtual dispatch.
    if let Some(resolved) = resolved {
        let symbol = &resolved.symbol;
        if resolved.domain == HirCallableDomain::Operator {
            let bare = symbol.parts().last().cloned().unwrap_or_else(|| Identifier::new("unknown"));
            return MirOperand::Symbol(NamePath::new(vec![bare]));
        }
        // Never reconstitute a binding-rooted field chain as a zero-arg dotted Symbol.
        if let HirExprKind::Path(path) = &expr.kind {
            if path.parts().len() >= 2 && builder.bindings.contains_key(path.parts()[0].as_str()) {
                let bare = symbol.parts().last().cloned().unwrap_or_else(|| path.parts().last().unwrap().clone());
                return MirOperand::Symbol(NamePath::new(vec![bare]));
            }
        }
        if matches!(&expr.kind, HirExprKind::FieldAccess { .. }) {
            let bare = symbol.parts().last().cloned().unwrap_or_else(|| Identifier::new("unknown"));
            return MirOperand::Symbol(NamePath::new(vec![bare]));
        }
        return MirOperand::Symbol(symbol.clone());
    }
    match &expr.kind {
        HirExprKind::Path(path) if path.parts().len() >= 2 && builder.bindings.contains_key(path.parts()[0].as_str()) => {
            MirOperand::Symbol(NamePath::new(vec![path.parts().last().unwrap().clone()]))
        }
        HirExprKind::Path(path) => MirOperand::Symbol(path.clone()),
        HirExprKind::Variable(identifier) => MirOperand::Symbol(NamePath::new(vec![identifier.name.clone()])),
        HirExprKind::GenericApply { callee, .. } => lower_callee_operand(callee, None, builder),
        // Fallthrough only: Call lowering should have used extract_method_call for FieldAccess.
        HirExprKind::FieldAccess { field, .. } => MirOperand::Symbol(NamePath::new(vec![field.clone()])),
        _ => builder.lower_expr_to_operand(expr),
    }
}

/// 尝试将表达式解析为 `NamePath`。
///
/// 递归处理 `Variable` 与 `FieldAccess` 链，将其组合为完整路径。
/// 例如 `std.iterator.collect_array` 会被解析为
/// `NamePath(["std", "iterator", "collect_array"])`。
/// 若表达式不是纯路径形式（如对象是复杂表达式），返回 `None`。
pub(super) fn try_resolve_as_path(expr: &HirExpr) -> Option<NamePath> {
    match &expr.kind {
        HirExprKind::Variable(identifier) => Some(NamePath::new(vec![identifier.name.clone()])),
        HirExprKind::Path(path) => Some(path.clone()),
        HirExprKind::FieldAccess { object, field } => {
            let mut path = try_resolve_as_path(object)?;
            path.append(field.clone());
            Some(path)
        }
        _ => None,
    }
}

/// 递归提取 `FieldAccess` 链的根变量名。
///
/// 例如 `request.target.length` 的根变量是 `request`。
/// 若表达式不是 `Variable` 与 `FieldAccess` 链，返回 `None`。
/// 用于区分实例方法调用（`obj.method()`）和静态路径调用（`module.function()`）。
pub(super) fn root_variable_name(expr: &HirExpr) -> Option<&str> {
    match &expr.kind {
        HirExprKind::Variable(identifier) => Some(identifier.name.as_str()),
        HirExprKind::FieldAccess { object, .. } => root_variable_name(object),
        _ => None,
    }
}

fn infer_builder_constant_type(constant: &MirConstant) -> Option<ValkyrieType> {
    Some(match constant {
        MirConstant::Int(value) if *value >= i32::MIN as i64 && *value <= i32::MAX as i64 => ValkyrieType::Integer32 { signed: true },
        MirConstant::Int(_) => ValkyrieType::Integer64 { signed: true },
        MirConstant::Float64(_) => ValkyrieType::Float64,
        MirConstant::Bool(_) => ValkyrieType::Boolean,
        MirConstant::Utf8(_) => ValkyrieType::Utf8,
        MirConstant::Utf16(_) => ValkyrieType::Utf16,
        MirConstant::Unit => ValkyrieType::Unit,
    })
}
