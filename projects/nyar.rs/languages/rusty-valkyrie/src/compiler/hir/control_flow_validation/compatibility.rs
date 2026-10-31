use std::collections::{BTreeMap, BTreeSet};

use crate::compiler::{
    hir::{
        is_nullable_type, is_option_apply_type, is_result_apply_type,
        lowering::{
            HirError,
            errors::{ControlFlowOp, LoopJumpOperation, PropagateProtocol, RequiredContext},
        },
        nullable_payload_type,
    },
    semantic::control_flow::{ControlFlowContext, TryScopeData},
    types::{
        Identifier, NamePath,
        hir::{
            HirBlock, HirExpr, HirExprKind, HirFunction, HirLiteral, HirMatchArm, HirModule, HirPattern, HirStatement, HirStatementKind,
            ValkyrieType,
        },
    },
};

use nyar_types::{DiagnosticSource, DiagnosticSpan};

use super::{control_flow_source, expr_diagnostic_span};

use crate::compiler::hir::control_flow_validation::state::HirValidationState;

/// 判断控制流校验中期望类型与实参类型是否兼容（不按名字回退 builtin / 泛型 / Self）。
pub fn control_flow_value_type_compatible(expected: &ValkyrieType, actual: &ValkyrieType) -> bool {
    control_flow_value_type_compatible_with_primitives(expected, actual, None)
}

pub fn control_flow_value_type_compatible_with_primitives(
    expected: &ValkyrieType,
    actual: &ValkyrieType,
    state: Option<&HirValidationState>,
) -> bool {
    expected == actual
        || matches!(expected, ValkyrieType::AutoType)
        || matches!(actual, ValkyrieType::AutoType)
        || integer_carriers_share_identity(expected, actual)
        || primitive_named_equivalent(expected, actual)
        || array_types_compatible(expected, actual)
        || nullable_return_compatible(expected, actual)
        || nominal_apply_types_compatible(expected, actual)
        || state.is_some_and(|state| primitive_boolean_compatible(expected, actual, state))
}

/// 检查两个类型是否是同一个 primitive numeric 的不同表示形式：
/// `Named("i32")` ↔ `Integer32 { .. }`、`Named("f64")` ↔ `Float64` 等。
///
/// 身份键带上有符号性：`i32` 与 `u32` 是同一宽度的不同名义类型，不得因表示形式合并。
/// `bool` 不在表内：布尔的名义等价只承认 `boolean_declarations` 证据
/// （见 `primitive_boolean_compatible`），未绑定声明的 `Named("bool")` 不得顶替 builtin Boolean。
fn primitive_named_equivalent(a: &ValkyrieType, b: &ValkyrieType) -> bool {
    fn identity_of(ty: &ValkyrieType) -> Option<(u32, Option<bool>)> {
        Some(match ty {
            ValkyrieType::Integer8 { signed } => (1, Some(*signed)),
            ValkyrieType::Integer16 { signed } => (2, Some(*signed)),
            ValkyrieType::Integer32 { signed } => (3, Some(*signed)),
            ValkyrieType::Integer64 { signed } => (4, Some(*signed)),
            ValkyrieType::Integer128 { signed } => (5, Some(*signed)),
            ValkyrieType::Float32 => (6, None),
            ValkyrieType::Float64 => (7, None),
            ValkyrieType::Named(name) => match name.as_str() {
                "i8" => (1, Some(true)),
                "i16" => (2, Some(true)),
                "i32" | "int" | "Int" => (3, Some(true)),
                "i64" | "long" | "Long" => (4, Some(true)),
                "i128" => (5, Some(true)),
                "f32" | "float" | "Float" => (6, None),
                "f64" | "double" | "Double" => (7, None),
                _ => return None,
            },
            _ => return None,
        })
    }
    identity_of(a).is_some_and(|identity_a| identity_of(b).is_some_and(|identity_b| identity_a == identity_b))
}

pub fn primitive_boolean_compatible(expected: &ValkyrieType, actual: &ValkyrieType, state: &HirValidationState) -> bool {
    let is_bool_nominal =
        |ty: &ValkyrieType| matches!(ty, ValkyrieType::Nominal { declaration, .. } if state.boolean_declarations.contains(declaration));
    (is_bool_nominal(expected) && matches!(actual, ValkyrieType::Boolean))
        || (is_bool_nominal(actual) && matches!(expected, ValkyrieType::Boolean))
}

pub fn nominal_apply_types_compatible(expected: &ValkyrieType, actual: &ValkyrieType) -> bool {
    match (expected, actual) {
        (
            ValkyrieType::Nominal { declaration: expected_decl, arguments: expected_args },
            ValkyrieType::Nominal { declaration: actual_decl, arguments: actual_args },
        ) if expected_decl == actual_decl && expected_args.len() == actual_args.len() => {
            expected_args.iter().zip(actual_args).all(|(expected, actual)| type_parameters_compatible(expected, actual))
        }
        _ => false,
    }
}

pub fn type_parameters_compatible(expected: &ValkyrieType, actual: &ValkyrieType) -> bool {
    match (expected, actual) {
        (ValkyrieType::Generic(expected), ValkyrieType::Generic(actual)) if expected.name == actual.name => true,
        _ => expected == actual,
    }
}

pub fn nullable_return_compatible(expected: &ValkyrieType, actual: &ValkyrieType) -> bool {
    match (nullable_payload_type(expected), nullable_payload_type(actual)) {
        (Some(lhs), Some(rhs)) => lhs == rhs,
        (Some(lhs), None) => lhs == *actual,
        (None, Some(rhs)) => *expected == rhs,
        _ => false,
    }
}

pub fn array_types_compatible(expected: &ValkyrieType, actual: &ValkyrieType) -> bool {
    match (expected, actual) {
        (ValkyrieType::Array(expected), ValkyrieType::Array(actual)) => {
            expected == actual || matches!(expected.as_ref(), ValkyrieType::AutoType) || matches!(actual.as_ref(), ValkyrieType::AutoType)
        }
        _ => false,
    }
}
///
/// 字面量的静态形态只有默认宽度 `i64`，字面量本身不携带宽度事实；期望类型是数值类型时
/// 按期望类型取值。判定与 `overload.rs::specialize_call_literal_exprs` 完全同源：
/// 同一份数值名集合（`Named("i32")` 与 `Nominal{ItemId}` 都算数值），同一条件（仅整数字面量）。
pub fn integer_literal_binds_to(expected: &ValkyrieType, value: Option<&HirExpr>, state: &HirValidationState) -> bool {
    if !matches!(value.map(|value| &value.kind), Some(HirExprKind::Literal(HirLiteral::Integer64(_)))) {
        return false;
    }
    match expected {
        ValkyrieType::Nominal { declaration, .. } => state.numeric_declarations.contains(declaration),
        other => crate::compiler::hir::type_relation::is_numeric_type_name_of(other),
    }
}

pub fn is_integer_type(ty: &ValkyrieType) -> bool {
    matches!(
        ty,
        ValkyrieType::Integer8 { .. }
            | ValkyrieType::Integer16 { .. }
            | ValkyrieType::Integer32 { .. }
            | ValkyrieType::Integer64 { .. }
            | ValkyrieType::Integer128 { .. }
    )
}

/// 整数 carrier 的互容只承认同一名义身份（ADR 0009/0013）：位宽与有符号性都必须相同。
///
/// 不同档位（如 `i32` 与 `i64`、`i32` 与 `u32`）不得因「都是整数」互相顶替。
/// 整数字面量的宽度由期望类型决定，走 `integer_literal_binds_to`，不在这里放宽。
fn integer_carriers_share_identity(expected: &ValkyrieType, actual: &ValkyrieType) -> bool {
    if !is_integer_type(expected) || !is_integer_type(actual) {
        return false;
    }
    integer_carrier_identity(expected) == integer_carrier_identity(actual)
}

/// 整数 carrier 名义身份的构成因子：位宽 + 有符号性。
fn integer_carrier_identity(ty: &ValkyrieType) -> Option<(u16, bool)> {
    Some(match ty {
        ValkyrieType::Integer8 { signed } => (8, *signed),
        ValkyrieType::Integer16 { signed } => (16, *signed),
        ValkyrieType::Integer32 { signed } => (32, *signed),
        ValkyrieType::Integer64 { signed } => (64, *signed),
        ValkyrieType::Integer128 { signed } => (128, *signed),
        _ => return None,
    })
}

pub fn return_without_value_compatible(expected: &ValkyrieType) -> bool {
    matches!(expected, ValkyrieType::Unit | ValkyrieType::Void | ValkyrieType::AutoType)
}

pub fn future_resume_type(ty: &ValkyrieType) -> Option<ValkyrieType> {
    match ty {
        ValkyrieType::Apply(base, arguments) if arguments.len() == 1 && matches!(named_type_name(base), Some("Future" | "Promise")) => {
            arguments.first().cloned()
        }
        _ => None,
    }
}
///
/// witness table 机制不区分 dyn trait 与 impl trait，因此函数返回值可直接写
/// `Iterator<T>` 或 `Generator<T>`，本函数统一提取其类型参数作为 yield 值的期望类型。
pub fn generator_yield_type(ty: &ValkyrieType) -> Option<ValkyrieType> {
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

pub fn named_type_name(ty: &ValkyrieType) -> Option<&str> {
    match ty {
        ValkyrieType::Named(name) => Some(name.as_str()),
        ValkyrieType::Apply(base, _) => named_type_name(base),
        _ => None,
    }
}

pub fn display_type(ty: &ValkyrieType) -> String {
    match ty {
        ValkyrieType::Void => "void".to_string(),
        ValkyrieType::Unit => "unit".to_string(),
        ValkyrieType::Boolean => "bool".to_string(),
        ValkyrieType::Integer8 { signed } => integer_type_name(*signed, 8),
        ValkyrieType::Integer16 { signed } => integer_type_name(*signed, 16),
        ValkyrieType::Integer32 { signed } => integer_type_name(*signed, 32),
        ValkyrieType::Integer64 { signed } => integer_type_name(*signed, 64),
        ValkyrieType::Integer128 { signed } => integer_type_name(*signed, 128),
        ValkyrieType::Float32 => "f32".to_string(),
        ValkyrieType::Float64 => "f64".to_string(),
        ValkyrieType::Character => "char".to_string(),
        ValkyrieType::Utf8 => "utf8".to_string(),
        ValkyrieType::Utf16 => "utf16".to_string(),
        ValkyrieType::Named(name) => name.to_string(),
        ValkyrieType::QualifiedPath { namespace, name } => ValkyrieType::qualified_path_label(namespace, name),
        ValkyrieType::Nominal { declaration, arguments } => {
            if arguments.is_empty() {
                format!("Nominal({declaration:?})")
            }
            else {
                format!("Nominal({declaration:?})<{}>", arguments.iter().map(display_type).collect::<Vec<_>>().join(", "))
            }
        }
        ValkyrieType::Apply(base, arguments) => {
            format!("{}<{}>", display_type(base), arguments.iter().map(display_type).collect::<Vec<_>>().join(", "))
        }
        ValkyrieType::Generic(generic) => format!("{}[{:?}]", generic.name, generic.binder),
        ValkyrieType::Function(function) => format!(
            "micro({}) -> {}",
            function.params.iter().map(display_type).collect::<Vec<_>>().join(", "),
            display_type(&function.return_type)
        ),
        ValkyrieType::Tuple(items) => format!("({})", items.iter().map(display_type).collect::<Vec<_>>().join(", ")),
        ValkyrieType::Row(row) => format!(
            "{{ {} }}",
            row.methods
                .iter()
                .map(|method| {
                    format!(
                        "{}({}) -> {}",
                        method.name,
                        method.params.iter().map(display_type).collect::<Vec<_>>().join(", "),
                        display_type(&method.return_type)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ValkyrieType::Array(item) => format!("[{}]", display_type(item)),
        ValkyrieType::FixedArray { element, length } => {
            format!("[{}; {}]", display_type(element), length)
        }
        ValkyrieType::TypeLambda(lambda) => format!(
            "type lambda({}) -> {}",
            lambda.params.iter().map(|item| item.name.to_string()).collect::<Vec<_>>().join(", "),
            display_type(&lambda.body)
        ),
        ValkyrieType::TraitObject(object) => {
            format!("{}<{}>", object.trait_path, object.type_arguments.iter().map(display_type).collect::<Vec<_>>().join(", "))
        }
        ValkyrieType::Associated(associated) => {
            format!("{}::{}", display_type(&associated.base), associated.name)
        }
        ValkyrieType::AutoType => "auto".to_string(),
        ValkyrieType::SelfType => "Self".to_string(),
        ValkyrieType::Nullable(payload) => format!("{}?", display_type(payload)),
        ValkyrieType::Union(members) => members.iter().map(display_type).collect::<Vec<_>>().join(" | "),
        ValkyrieType::Intersection(members) => members.iter().map(display_type).collect::<Vec<_>>().join(" & "),
    }
}

pub fn integer_type_name(signed: bool, bits: u16) -> String {
    if signed { format!("i{bits}") } else { format!("u{bits}") }
}
