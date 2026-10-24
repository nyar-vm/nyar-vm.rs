//! `expr?` 操作数的分类与编译期校验。

use std::collections::BTreeMap;

use nyar_types::{DiagnosticSource, DiagnosticSpan};

use crate::{
    types::{
        Identifier, NamePath,
        hir::{HirBlock, HirExpr, HirExprKind, HirFunction, HirModule, HirPattern, HirStatement, HirStatementKind, HirStruct, ValkyrieType},
    },
    hir::{HirError, lowering::errors::PropagateProtocol},
};

/// `?` 校验产出的诊断 source。
fn try_source() -> DiagnosticSource {
    DiagnosticSource { id: "<try>".into(), revision: None }
}

/// `?` 表达式自身的 span。
fn try_span(expr: &HirExpr) -> DiagnosticSpan {
    DiagnosticSpan { start: expr.span.span.start as usize, end: expr.span.span.end as usize }
}

/// `?` 操作数形态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TryPropagateKind {
    /// `T?`：值为 null 时早退。
    Nullable(ValkyrieType),
    /// `Option<T>`：`None` 早退，`Some` 解出 payload。
    Option(ValkyrieType),
    /// `Result<T, E>`：错误变体早退，成功变体解出 payload。
    Result {
        /// 成功载荷类型。
        ok: ValkyrieType,
        /// 错误载荷类型。
        err: ValkyrieType,
    },
}

/// 判断类型是否为 nullable（`T?`）。
pub fn is_nullable_type(ty: &ValkyrieType) -> bool {
    matches!(ty, ValkyrieType::Nullable(_))
}

/// 将 `T?` 解析为 payload 类型。
pub fn nullable_payload_type(ty: &ValkyrieType) -> Option<ValkyrieType> {
    match ty {
        ValkyrieType::Nullable(payload) => Some(payload.as_ref().clone()),
        _ => None,
    }
}

/// 按 declaration ItemId 查找 unity 枚举（本模块 / 子模块 / 导入导出）。
fn unity_enum_by_declaration(module: &HirModule, declaration: nyar_types::ItemId) -> Option<&crate::types::hir::HirEnum> {
    module
        .enums
        .iter()
        .chain(module.submodules.iter().flat_map(|child| child.enums.iter()))
        .chain(module.imported_semantic_exports.iter().flat_map(|export| export.enums.iter()))
        .find(|enum_def| enum_def.is_unity && enum_def.declaration == Some(declaration))
}

/// 用已冻结 Nominal declaration + binder 元数分类 `?` 操作数；禁止 Named("Option"|"Result") 短名。
fn classify_unity_try_operand(declaration: nyar_types::ItemId, arguments: &[ValkyrieType], module: &HirModule) -> Option<TryPropagateKind> {
    let enum_def = unity_enum_by_declaration(module, declaration)?;
    match (arguments.len(), enum_def.generics.len()) {
        (1, 1) => Some(TryPropagateKind::Option(arguments[0].clone())),
        (2, 2) => Some(TryPropagateKind::Result { ok: arguments[0].clone(), err: arguments[1].clone() }),
        _ => None,
    }
}

/// 判断模块是否声明了二元 unity（Result 形态）；按 generics 元数，不按短名。
pub fn module_declares_result(module: &HirModule) -> bool {
    module.enums.iter().any(|enum_def| enum_def.is_unity && enum_def.generics.len() == 2)
}

/// 分类 `?` 的操作数类型。
pub fn classify_try_operand(operand_type: &ValkyrieType, module: &HirModule) -> Option<TryPropagateKind> {
    if let Some(payload) = nullable_payload_type(operand_type) {
        return Some(TryPropagateKind::Nullable(payload));
    }
    match operand_type {
        ValkyrieType::Nominal { declaration, arguments } => classify_unity_try_operand(*declaration, arguments, module),
        ValkyrieType::Apply(base, arguments) => match base.as_ref() {
            ValkyrieType::Nominal { declaration, .. } => classify_unity_try_operand(*declaration, arguments, module),
            _ => None,
        },
        _ => None,
    }
}

fn result_err_type(ty: &ValkyrieType) -> Option<ValkyrieType> {
    match ty {
        ValkyrieType::Nominal { arguments, .. } if arguments.len() == 2 => arguments.get(1).cloned(),
        ValkyrieType::Apply(base, arguments) if arguments.len() == 2 && matches!(base.as_ref(), ValkyrieType::Nominal { .. }) => {
            arguments.get(1).cloned()
        }
        _ => None,
    }
}

fn infer_expr_type(expr: &HirExpr, locals: &BTreeMap<String, ValkyrieType>) -> Option<ValkyrieType> {
    match &expr.kind {
        HirExprKind::Literal(literal) => Some(match literal {
            crate::types::hir::HirLiteral::Integer64(_) => ValkyrieType::Integer64 { signed: true },
            crate::types::hir::HirLiteral::Float64(_) => ValkyrieType::Float64,
            crate::types::hir::HirLiteral::String(_) => ValkyrieType::Named(Identifier::new("utf8")),
            crate::types::hir::HirLiteral::Bool(_) => ValkyrieType::Boolean,
            crate::types::hir::HirLiteral::Unit => ValkyrieType::Unit,
        }),
        HirExprKind::Variable(identifier) => locals.get(identifier.name.as_str()).cloned(),
        HirExprKind::Call { resolved, .. } | HirExprKind::Construct { resolved, .. } => resolved.as_ref().map(|call| call.return_type.clone()),
        HirExprKind::TryPropagate { inner, .. } => infer_expr_type(inner, locals).and_then(|ty| try_propagate_result_type(&ty)),
        HirExprKind::Block(block) => block.expr.as_ref().and_then(|expr| infer_expr_type(expr, locals)),
        _ => None,
    }
}

fn try_propagate_result_type(operand_type: &ValkyrieType) -> Option<ValkyrieType> {
    nullable_payload_type(operand_type).or_else(|| match operand_type {
        ValkyrieType::Nominal { arguments, .. } if arguments.len() == 1 || arguments.len() == 2 => arguments.first().cloned(),
        ValkyrieType::Apply(base, arguments) => match base.as_ref() {
            ValkyrieType::Nominal { .. } if arguments.len() == 1 || arguments.len() == 2 => arguments.first().cloned(),
            _ => None,
        },
        _ => None,
    })
}

fn validate_try_propagate_expr(
    expr: &HirExpr,
    enclosing_return: &ValkyrieType,
    module: &HirModule,
    locals: &BTreeMap<String, ValkyrieType>,
    try_scope_depth: usize,
) -> Result<(), HirError> {
    let HirExprKind::TryPropagate { inner, .. } = &expr.kind
    else {
        return Ok(());
    };
    let Some(operand_type) = infer_expr_type(inner, locals)
    else {
        return Ok(());
    };
    let Some(kind) = classify_try_operand(&operand_type, module)
    else {
        return Err(HirError::TryPropagateOperandUnsupported {
            operand_type: render_type_name(&operand_type),
            source: try_source(),
            span: try_span(expr),
        });
    };
    match kind {
        TryPropagateKind::Nullable(_) => {
            if try_scope_depth == 0 && !is_nullable_type(enclosing_return) {
                return Err(HirError::TryPropagateReturnMismatch {
                    protocol: PropagateProtocol::Nullable,
                    return_type: render_type_name(enclosing_return),
                    source: try_source(),
                    span: try_span(expr),
                });
            }
        }
        TryPropagateKind::Option(_) => {
            if try_scope_depth == 0 && !is_option_apply_type(enclosing_return) {
                return Err(HirError::TryPropagateReturnMismatch {
                    protocol: PropagateProtocol::Option,
                    return_type: render_type_name(enclosing_return),
                    source: try_source(),
                    span: try_span(expr),
                });
            }
        }
        TryPropagateKind::Result { err, .. } => {
            if !is_result_apply_type(enclosing_return) {
                return Err(HirError::TryPropagateReturnMismatch {
                    protocol: PropagateProtocol::Result,
                    return_type: render_type_name(enclosing_return),
                    source: try_source(),
                    span: try_span(expr),
                });
            }
            if let Some(enclosing_err) = result_err_type(enclosing_return) {
                if enclosing_err != err {
                    return Err(HirError::TryPropagateErrorTypeMismatch {
                        error_type: render_type_name(&err),
                        return_error: render_type_name(&enclosing_err),
                        source: try_source(),
                        span: try_span(expr),
                    });
                }
            }
        }
    }
    validate_expr_try_propagate(inner, enclosing_return, module, locals, try_scope_depth)
}

fn validate_expr_try_propagate(
    expr: &HirExpr,
    enclosing_return: &ValkyrieType,
    module: &HirModule,
    locals: &BTreeMap<String, ValkyrieType>,
    try_scope_depth: usize,
) -> Result<(), HirError> {
    match &expr.kind {
        HirExprKind::TryPropagate { .. } => validate_try_propagate_expr(expr, enclosing_return, module, locals, try_scope_depth)?,
        HirExprKind::TryScope { body, .. } => validate_block_try_propagate(body, enclosing_return, module, locals, try_scope_depth + 1)?,
        HirExprKind::Call { callee, args, .. } => {
            validate_expr_try_propagate(callee, enclosing_return, module, locals, try_scope_depth)?;
            for arg in args {
                validate_expr_try_propagate(&arg.value, enclosing_return, module, locals, try_scope_depth)?;
            }
        }
        HirExprKind::Block(block) => validate_block_try_propagate(block, enclosing_return, module, locals, try_scope_depth)?,
        HirExprKind::If { condition, then_branch, else_branch } => {
            validate_expr_try_propagate(condition, enclosing_return, module, locals, try_scope_depth)?;
            validate_block_try_propagate(then_branch, enclosing_return, module, locals, try_scope_depth)?;
            if let Some(else_branch) = else_branch {
                validate_block_try_propagate(else_branch, enclosing_return, module, locals, try_scope_depth)?;
            }
        }
        HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
            validate_expr_try_propagate(scrutinee, enclosing_return, module, locals, try_scope_depth)?;
            for arm in arms {
                validate_expr_try_propagate(&arm.body, enclosing_return, module, locals, try_scope_depth)?;
            }
        }
        HirExprKind::Return(Some(value)) | HirExprKind::Yield(Some(value)) => {
            validate_expr_try_propagate(value, enclosing_return, module, locals, try_scope_depth)?;
        }
        _ => {}
    }
    Ok(())
}

fn validate_block_try_propagate(
    block: &HirBlock,
    enclosing_return: &ValkyrieType,
    module: &HirModule,
    locals: &BTreeMap<String, ValkyrieType>,
    try_scope_depth: usize,
) -> Result<(), HirError> {
    let mut locals = locals.clone();
    for statement in &block.statements {
        validate_statement_try_propagate(statement, enclosing_return, module, &mut locals, try_scope_depth)?;
    }
    if let Some(expr) = &block.expr {
        validate_expr_try_propagate(expr, enclosing_return, module, &locals, try_scope_depth)?;
    }
    Ok(())
}

fn validate_statement_try_propagate(
    statement: &HirStatement,
    enclosing_return: &ValkyrieType,
    module: &HirModule,
    locals: &mut BTreeMap<String, ValkyrieType>,
    try_scope_depth: usize,
) -> Result<(), HirError> {
    match &statement.kind {
        HirStatementKind::Let { pattern, ty, initializer, .. } => {
            if let Some(initializer) = initializer {
                validate_expr_try_propagate(initializer, enclosing_return, module, locals, try_scope_depth)?;
            }
            if let Some(ty) = ty {
                bind_pattern_type(pattern, ty, locals);
            }
            else if let Some(initializer) = initializer {
                if let Some(inferred) = infer_expr_type(initializer, locals) {
                    bind_pattern_type(pattern, &inferred, locals);
                }
            }
        }
        HirStatementKind::Expr(expr) => validate_expr_try_propagate(expr, enclosing_return, module, locals, try_scope_depth)?,
    }
    Ok(())
}

fn bind_pattern_type(pattern: &HirPattern, ty: &ValkyrieType, locals: &mut BTreeMap<String, ValkyrieType>) {
    match pattern {
        HirPattern::Variable(identifier) => {
            locals.insert(identifier.name.to_string(), ty.clone());
        }
        HirPattern::Bind { identifier, pattern } => {
            bind_pattern_type(pattern, ty, locals);
            locals.insert(identifier.name.to_string(), ty.clone());
        }
        HirPattern::Mut(pattern) | HirPattern::Pin { pattern, .. } => bind_pattern_type(pattern, ty, locals),
        _ => {}
    }
}

fn validate_function_try_propagate(function: &HirFunction, module: &HirModule) -> Result<(), HirError> {
    let mut locals = BTreeMap::new();
    for param in &function.params {
        locals.insert(param.name.name.to_string(), param.ty.clone());
    }
    validate_block_try_propagate(&function.body, &function.return_type, module, &locals, 0)
}

/// 校验模块内全部 `?` 表达式。
pub fn validate_try_propagate_module(module: &HirModule) -> Result<(), HirError> {
    for function in &module.functions {
        validate_function_try_propagate(function, module)?;
    }
    for class in &module.structs {
        for method in &class.methods {
            validate_function_try_propagate(method, module)?;
        }
    }
    Ok(())
}

fn render_type_name(ty: &ValkyrieType) -> String {
    match ty {
        ValkyrieType::Named(name) => name.to_string(),
        ValkyrieType::Apply(base, arguments) => {
            format!("{}<{}>", render_type_name(base), arguments.iter().map(render_type_name).collect::<Vec<_>>().join(", "))
        }
        ValkyrieType::Nullable(payload) => format!("{}?", render_type_name(payload)),
        ValkyrieType::Union(items) => items.iter().map(render_type_name).collect::<Vec<_>>().join(" | "),
        other => format!("{other:?}"),
    }
}

/// `Fine` 变体的限定路径（与文档 unite 定义一致）。
pub fn fine_variant_path() -> NamePath {
    NamePath::new(vec![Identifier::new("Fine")])
}

/// `Fail` 变体的限定路径（与文档 unite 定义一致）。
pub fn fail_variant_path() -> NamePath {
    NamePath::new(vec![Identifier::new("Fail")])
}

/// 从 unity 声明中查找变体字段布局（用于 MIR payload 提取）。
pub fn variant_field_type(module: &HirModule, variant: &str, field: &str) -> Option<ValkyrieType> {
    module.enums.iter().find_map(|enum_def| {
        enum_def.variants.iter().find_map(|item| {
            if item.name.as_str() == variant { item.fields.iter().find(|f| f.name.as_str() == field).map(|f| f.ty.clone()) } else { None }
        })
    })
}

/// 供校验 / MIR 判断一元 unity 应用形态（冻结后的 `Nominal{…, [T]}`）。
/// 禁止再按 `Named("Option")` 短名认亲。
pub fn is_option_apply_type(ty: &ValkyrieType) -> bool {
    match ty {
        ValkyrieType::Nominal { arguments, .. } if arguments.len() == 1 => true,
        ValkyrieType::Apply(base, arguments) if arguments.len() == 1 => matches!(base.as_ref(), ValkyrieType::Nominal { .. }),
        _ => false,
    }
}

/// 一元 unity 的 payload；裸 Nominal 无参时返回 `None`。
pub fn option_apply_payload_type(ty: &ValkyrieType) -> Option<ValkyrieType> {
    match ty {
        ValkyrieType::Nominal { arguments, .. } if arguments.len() == 1 => arguments.first().cloned(),
        ValkyrieType::Apply(base, arguments) if arguments.len() == 1 && matches!(base.as_ref(), ValkyrieType::Nominal { .. }) => {
            arguments.first().cloned()
        }
        _ => None,
    }
}

/// 二元 unity 变体字段：`Fine` → 第 0 槽，`Fail` → 第 1 槽；按 binder 槽位，不按短名选类型。
pub fn result_variant_payload_type(ty: &ValkyrieType, variant: &str) -> Option<ValkyrieType> {
    let arguments = match ty {
        ValkyrieType::Nominal { arguments, .. } if arguments.len() == 2 => arguments.as_slice(),
        ValkyrieType::Apply(base, arguments) if arguments.len() == 2 && matches!(base.as_ref(), ValkyrieType::Nominal { .. }) => {
            arguments.as_slice()
        }
        _ => return None,
    };
    match variant {
        "Fine" => arguments.first().cloned(),
        "Fail" => arguments.get(1).cloned(),
        _ => None,
    }
}

/// 二元 unity 应用形态（冻结后的 `Nominal{…, [T, E]}`）；禁止 `Named("Result")`。
pub fn is_result_apply_type(ty: &ValkyrieType) -> bool {
    match ty {
        ValkyrieType::Nominal { arguments, .. } if arguments.len() == 2 => true,
        ValkyrieType::Apply(base, arguments) if arguments.len() == 2 => matches!(base.as_ref(), ValkyrieType::Nominal { .. }),
        _ => false,
    }
}
