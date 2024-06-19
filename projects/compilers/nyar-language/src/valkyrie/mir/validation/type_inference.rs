//! 操作数静态类型推断与调用类型兼容判定。

use crate::{
    mir::{MirBlockRef, MirConstant, MirEffectKind, MirFunction, MirOperand, MirValueOrigin},
    types::hir::ValkyrieType,
};

pub(super) fn infer_operand_static_type(function: &MirFunction, operand: &MirOperand) -> Option<ValkyrieType> {
    match operand {
        MirOperand::Constant(constant) => Some(infer_constant_type(constant)),
        MirOperand::Value(value_ref) => function.value_types.get(value_ref).cloned().or_else(|| {
            let value = function.values.iter().find(|value| value.id == *value_ref)?;
            match &value.origin {
                MirValueOrigin::Parameter { index, .. } => function.param_types.get(*index).cloned(),
                _ => None,
            }
        }),
        MirOperand::Callable(_) => None,
        // 未解析的成员要求没有静态类型：必须先在单态化阶段换成 callable identity。
        MirOperand::Requirement(_) => None,
        MirOperand::Symbol(_) => None,
    }
}

pub(super) fn infer_effect_resume_static_type(
    effect: MirEffectKind,
    payload_type: Option<&ValkyrieType>,
    carrier_type: Option<&ValkyrieType>,
) -> Option<ValkyrieType> {
    match effect {
        MirEffectKind::Yield | MirEffectKind::DelegateYield => Some(ValkyrieType::Unit),
        MirEffectKind::Await | MirEffectKind::AsyncBlock => payload_type.and_then(future_resume_type),
        MirEffectKind::AsyncSpawn => None,
        MirEffectKind::Raise => carrier_type.map(|_ty| ValkyrieType::Void),
    }
}

/// 查找与给定 resume_target 关联的 suspend point 的 carrier_type。
///
/// 校验阶段需要 carrier_type 来推断 `Raise` 路径的 resume 参数类型；该函数从函数的
/// `suspend_points` 中匹配 `resume_target` 一致的挂起点并返回其 carrier_type。
pub(super) fn carrier_type_for_block(_function: &MirFunction, _resume_target: MirBlockRef) -> Option<&ValkyrieType> {
    // suspend_points 不再挂在 Semantic MirFunction 上。
    None
}

pub(super) fn infer_constant_type(constant: &MirConstant) -> ValkyrieType {
    match constant {
        MirConstant::Int(_) => ValkyrieType::Integer64 { signed: true },
        MirConstant::Float64(_) => ValkyrieType::Float64,
        MirConstant::Bool(_) => ValkyrieType::Boolean,
        MirConstant::Utf8(_) => ValkyrieType::Utf8,
        MirConstant::Utf16(_) => ValkyrieType::Utf16,
        MirConstant::Unit => ValkyrieType::Unit,
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

pub(super) fn generator_source_type(ty: &ValkyrieType) -> Option<ValkyrieType> {
    match ty {
        ValkyrieType::Apply(base, arguments)
            if !arguments.is_empty() && matches!(named_type_name(base), Some("Generator" | "Iterator" | "Coroutine")) =>
        {
            arguments.first().cloned()
        }
        ValkyrieType::Named(name) if matches!(name.as_str(), "Generator" | "Iterator" | "Coroutine") => Some(ValkyrieType::Unit),
        _ => None,
    }
}

/// MIR 与源码均使用 `Void` 表达底类型，不保留名称别名。
pub(super) fn mir_return_operand_type_compatible(actual: &ValkyrieType, expected: &ValkyrieType) -> bool {
    actual == expected || bare_named_matches_generic_apply_return(expected, actual)
}

/// Seed callable signatures stay generic in `MirFunction`, while HIR call sites carry instantiated types.
pub(super) fn call_site_type_compatible(declared: &ValkyrieType, actual: &ValkyrieType) -> bool {
    declared == actual || crate::mir::ssa::generic_call_instantiation::type_instantiates_generic_signature(declared, actual)
}

pub(super) fn bare_named_matches_generic_apply_return(expected: &ValkyrieType, actual: &ValkyrieType) -> bool {
    match (expected, actual) {
        (ValkyrieType::Apply(base, _), ValkyrieType::Named(name)) | (ValkyrieType::Named(name), ValkyrieType::Apply(base, _)) => {
            named_type_name(base).is_some_and(|base_name| base_name == name.as_str())
        }
        _ => false,
    }
}

pub(super) fn named_type_name(ty: &ValkyrieType) -> Option<&str> {
    match ty {
        ValkyrieType::Named(name) => Some(name.as_str()),
        ValkyrieType::Apply(base, _) => named_type_name(base),
        _ => None,
    }
}
