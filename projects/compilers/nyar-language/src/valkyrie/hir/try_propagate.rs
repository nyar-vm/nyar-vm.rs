//! `expr?` 操作数的分类与编译期校验。

use std::collections::BTreeMap;

use crate::types::{
    Identifier, NamePath,
    hir::{HirBlock, HirExpr, HirExprKind, HirFunction, HirModule, HirPattern, HirStatement, HirStatementKind, HirStruct, ValkyrieType},
};
use std_data::text::valkyrie::ParseError;

/// `?` 操作数形态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TryPropagateKind {
    Nullable(ValkyrieType),
    /// `Option<T>`：`None` 早退，`Some` 解出 payload。
    Option(ValkyrieType),
    Result {
        ok: ValkyrieType,
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

/// 判断模块是否声明了 unity `Result`。
pub fn module_declares_result(module: &HirModule) -> bool {
    module.enums.iter().any(|enum_def| enum_def.is_unity && enum_def.name.as_str() == "Result")
}

/// 分类 `?` 的操作数类型。
pub fn classify_try_operand(operand_type: &ValkyrieType, module: &HirModule) -> Option<TryPropagateKind> {
    if let Some(payload) = nullable_payload_type(operand_type) {
        return Some(TryPropagateKind::Nullable(payload));
    }
    if let ValkyrieType::Apply(base, arguments) = operand_type {
        if let ValkyrieType::Named(name) = base.as_ref() {
            // `Option<T>` 是核心 unite；按类型形态识别，不要求本文件再声明一遍。
            if name.as_str() == "Option" && arguments.len() == 1 {
                return Some(TryPropagateKind::Option(arguments[0].clone()));
            }
            if name.as_str() == "Result" && arguments.len() == 2 && module_declares_result(module) {
                return Some(TryPropagateKind::Result { ok: arguments[0].clone(), err: arguments[1].clone() });
            }
        }
    }
    None
}

fn result_type(ok: &ValkyrieType, err: &ValkyrieType) -> ValkyrieType {
    ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Result"))), vec![ok.clone(), err.clone()])
}

fn result_err_type(ty: &ValkyrieType) -> Option<ValkyrieType> {
    match ty {
        ValkyrieType::Apply(base, arguments) if arguments.len() == 2 => {
            if let ValkyrieType::Named(name) = base.as_ref() {
                if name.as_str() == "Result" {
                    return Some(arguments[1].clone());
                }
            }
            None
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
        HirExprKind::Call { resolved, .. } => resolved.as_ref().map(|call| call.return_type.clone()),
        HirExprKind::Construct { name, .. } => Some(ValkyrieType::Named(name.clone())),
        HirExprKind::TryPropagate(inner) => infer_expr_type(inner, locals).and_then(|ty| try_propagate_result_type(&ty)),
        HirExprKind::Block(block) => block.expr.as_ref().and_then(|expr| infer_expr_type(expr, locals)),
        _ => None,
    }
}

fn try_propagate_result_type(operand_type: &ValkyrieType) -> Option<ValkyrieType> {
    nullable_payload_type(operand_type).or_else(|| match operand_type {
        ValkyrieType::Apply(base, arguments) => {
            if let ValkyrieType::Named(name) = base.as_ref() {
                if name.as_str() == "Option" && arguments.len() == 1 {
                    return Some(arguments[0].clone());
                }
                if name.as_str() == "Result" && arguments.len() == 2 {
                    return Some(arguments[0].clone());
                }
            }
            None
        }
        _ => None,
    })
}

fn validate_try_propagate_expr(
    expr: &HirExpr,
    enclosing_return: &ValkyrieType,
    module: &HirModule,
    locals: &BTreeMap<String, ValkyrieType>,
    try_scope_depth: usize,
) -> Result<(), ParseError> {
    let HirExprKind::TryPropagate(inner) = &expr.kind
    else {
        return Ok(());
    };
    let Some(operand_type) = infer_expr_type(inner, locals)
    else {
        return Ok(());
    };
    let Some(kind) = classify_try_operand(&operand_type, module)
    else {
        return Err(ParseError::invalid(format!("`?` cannot be applied to `{}`", render_type_name(&operand_type))));
    };
    match kind {
        TryPropagateKind::Nullable(payload) => {
            if try_scope_depth == 0 && !is_nullable_type(enclosing_return) {
                return Err(ParseError::invalid(format!(
                    "`?` on nullable `{}` requires enclosing return compatible with nullable or a `try` scope; got `{}`",
                    render_type_name(&payload),
                    render_type_name(enclosing_return)
                )));
            }
        }
        TryPropagateKind::Option(payload) => {
            if try_scope_depth == 0 && !is_option_apply_type(enclosing_return) {
                return Err(ParseError::invalid(format!(
                    "`?` on `Option<{}>` requires enclosing `Option` return or a `try` scope; got `{}`",
                    render_type_name(&payload),
                    render_type_name(enclosing_return)
                )));
            }
        }
        TryPropagateKind::Result { ok, err } => {
            let expected = result_type(&ok, &err);
            if !result_types_compatible(enclosing_return, &expected) {
                return Err(ParseError::invalid(format!(
                    "`?` on `Result<{}, {}>` requires enclosing return `Result<_, {}>`; got `{}`",
                    render_type_name(&ok),
                    render_type_name(&err),
                    render_type_name(&err),
                    render_type_name(enclosing_return)
                )));
            }
            if let Some(enclosing_err) = result_err_type(enclosing_return) {
                if enclosing_err != err {
                    return Err(ParseError::invalid(format!(
                        "`?` error type `{}` is incompatible with enclosing return error `{}`",
                        render_type_name(&err),
                        render_type_name(&enclosing_err)
                    )));
                }
            }
        }
    }
    validate_expr_try_propagate(inner, enclosing_return, module, locals, try_scope_depth)
}

fn result_types_compatible(enclosing: &ValkyrieType, expected: &ValkyrieType) -> bool {
    enclosing == expected
        || matches!(
            (enclosing, expected),
            (
                ValkyrieType::Apply(base_a, args_a),
                ValkyrieType::Apply(base_b, args_b)
            ) if base_a == base_b
                && args_a.len() == 2
                && args_b.len() == 2
                && args_a[1] == args_b[1]
        )
}

fn validate_expr_try_propagate(
    expr: &HirExpr,
    enclosing_return: &ValkyrieType,
    module: &HirModule,
    locals: &BTreeMap<String, ValkyrieType>,
    try_scope_depth: usize,
) -> Result<(), ParseError> {
    match &expr.kind {
        HirExprKind::TryPropagate(_) => validate_try_propagate_expr(expr, enclosing_return, module, locals, try_scope_depth)?,
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
) -> Result<(), ParseError> {
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
) -> Result<(), ParseError> {
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

fn validate_function_try_propagate(function: &HirFunction, module: &HirModule) -> Result<(), ParseError> {
    let mut locals = BTreeMap::new();
    for param in &function.params {
        locals.insert(param.name.name.to_string(), param.ty.clone());
    }
    validate_block_try_propagate(&function.body, &function.return_type, module, &locals, 0)
}

/// 校验模块内全部 `?` 表达式。
pub fn validate_try_propagate_module(module: &HirModule) -> Result<(), ParseError> {
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

/// `Fine` / `Fail` 变体名，与文档 unite 定义一致。
pub fn fine_variant_path() -> NamePath {
    NamePath::new(vec![Identifier::new("Fine")])
}

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

/// 供校验 / MIR 判断 `Option<T>` 形态时复用。
pub fn is_option_apply_type(ty: &ValkyrieType) -> bool {
    matches!(
        ty,
        ValkyrieType::Apply(base, arguments) if arguments.len() == 1 && matches!(base.as_ref(), ValkyrieType::Named(name) if name.as_str() == "Option")
    )
}

/// 占位：供 MIR 判断 Result 形态时复用。
pub fn is_result_apply_type(ty: &ValkyrieType) -> bool {
    matches!(
        ty,
        ValkyrieType::Apply(base, arguments) if arguments.len() == 2 && matches!(base.as_ref(), ValkyrieType::Named(name) if name.as_str() == "Result")
    )
}
