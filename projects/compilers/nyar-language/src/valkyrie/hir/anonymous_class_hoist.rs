//! 将匿名 class 表达式提升为合成 `HirStruct`，并填充闭包捕获。

use std::collections::BTreeMap;

use crate::types::{
    Identifier,
    hir::{
        HirBlock, HirDocumentation, HirExpr, HirExprKind, HirField, HirModule, HirParent, HirStatement, HirStatementKind, HirStruct,
        HirVisibility, ValkyrieType,
    },
};

use super::CaptureAnalyzer;

static ANON_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

struct HoistContext {
    pending_structs: Vec<HirStruct>,
    namespace: Vec<Identifier>,
}

/// 遍历模块，提升全部匿名 class 并注册合成 struct。
pub fn hoist_anonymous_classes(module: &mut HirModule) {
    let mut context = HoistContext { pending_structs: Vec::new(), namespace: module.name.parts().to_vec() };

    for function in &mut module.functions {
        let mut locals = locals_from_params(&function.params);
        hoist_block(&mut function.body, &mut context, &mut locals);
    }

    let struct_count = module.structs.len();
    for si in 0..struct_count {
        let class_name = module.structs[si].name.clone();
        let method_count = module.structs[si].methods.len();
        for mi in 0..method_count {
            let mut locals = locals_from_params(&module.structs[si].methods[mi].params);
            locals.insert("self".to_string(), ValkyrieType::Named(class_name.clone()));
            hoist_block(&mut module.structs[si].methods[mi].body, &mut context, &mut locals);
        }
    }

    module.structs.extend(context.pending_structs);
}

fn locals_from_params(params: &[crate::types::hir::HirParam]) -> BTreeMap<String, ValkyrieType> {
    params.iter().map(|param| (param.name.name.to_string(), param.ty.clone())).collect()
}

fn hoist_block(block: &mut HirBlock, context: &mut HoistContext, locals: &mut BTreeMap<String, ValkyrieType>) {
    for statement in &mut block.statements {
        hoist_statement(statement, context, locals);
    }
    if let Some(expr) = &mut block.expr {
        hoist_expr(expr, context, locals);
    }
}

fn hoist_statement(statement: &mut HirStatement, context: &mut HoistContext, locals: &mut BTreeMap<String, ValkyrieType>) {
    match &mut statement.kind {
        HirStatementKind::Let { pattern, ty, initializer, .. } => {
            if let Some(initializer) = initializer {
                hoist_expr(initializer, context, locals);
            }
            if let Some(ty) = ty {
                bind_pattern_local(pattern, ty, locals);
            }
            else if let Some(initializer) = initializer {
                if let Some(inferred) = infer_simple_expr_type(initializer, locals) {
                    bind_pattern_local(pattern, &inferred, locals);
                }
            }
        }
        HirStatementKind::Expr(expr) => hoist_expr(expr, context, locals),
    }
}

fn hoist_expr(expr: &mut HirExpr, context: &mut HoistContext, locals: &BTreeMap<String, ValkyrieType>) {
    if let HirExprKind::AnonymousClass { is_value_type, parents, fields, methods, captures, class_name } = &mut expr.kind {
        if class_name.is_none() {
            let synthetic = next_anon_name();
            let mut analyzer = CaptureAnalyzer::new();
            for (name, ty) in locals {
                analyzer.add_var(name, ty.clone(), false);
            }
            for (_, value) in fields.iter() {
                collect_captures(value, &mut analyzer);
            }
            for method in methods.iter() {
                collect_block_captures(&method.body, &mut analyzer, locals);
            }
            *captures = analyzer.into_captures();

            let mut struct_fields = captures
                .iter()
                .map(|capture| HirField {
                    name: capture_field_name(&capture.identifier.name),
                    doc: HirDocumentation::default(),
                    ty: capture.ty.clone(),
                    visibility: HirVisibility::private(),
                    is_mutable: false,
                })
                .collect::<Vec<_>>();

            for (name, init) in fields.iter() {
                struct_fields.push(HirField {
                    name: name.clone(),
                    doc: HirDocumentation::default(),
                    ty: infer_simple_expr_type(init, locals).unwrap_or(ValkyrieType::Unit),
                    visibility: HirVisibility::private(),
                    is_mutable: false,
                });
            }

            let hir_parents = parents.clone();

            context.pending_structs.push(HirStruct {
                name: synthetic.clone(),
                namespace: context.namespace.clone(),
                parents: hir_parents,
                fields: struct_fields,
                methods: methods.clone(),
                is_value_type: *is_value_type,
                ..HirStruct::default()
            });
            *class_name = Some(synthetic);
        }
        for (_, value) in fields {
            hoist_expr(value, context, locals);
        }
        for method in methods {
            let mut nested_locals = locals.clone();
            hoist_block(&mut method.body, context, &mut nested_locals);
        }
        return;
    }

    match &mut expr.kind {
        HirExprKind::Call { callee, args, .. } => {
            hoist_expr(callee, context, locals);
            for arg in args {
                hoist_expr(&mut arg.value, context, locals);
            }
        }
        HirExprKind::Block(block) => hoist_block(block, context, &mut locals.clone()),
        HirExprKind::If { condition, then_branch, else_branch } => {
            hoist_expr(condition, context, locals);
            hoist_block(then_branch, context, &mut locals.clone());
            if let Some(else_branch) = else_branch {
                hoist_block(else_branch, context, &mut locals.clone());
            }
        }
        HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
            hoist_expr(scrutinee, context, locals);
            for arm in arms {
                hoist_expr(&mut arm.body, context, locals);
            }
        }
        HirExprKind::Lambda { body, .. } => hoist_block(body, context, &mut locals.clone()),
        HirExprKind::Return(Some(value))
        | HirExprKind::Yield(Some(value))
        | HirExprKind::Resume(value)
        | HirExprKind::TryPropagate(value)
        | HirExprKind::Raise(value)
        | HirExprKind::Await(value)
        | HirExprKind::Awake(value)
        | HirExprKind::BlockOn(value)
        | HirExprKind::YieldFrom(value) => hoist_expr(value, context, locals),
        HirExprKind::Assign { value, .. } | HirExprKind::FieldAccess { object: value, .. } => hoist_expr(value, context, locals),
        HirExprKind::StoreField { object, value, .. } => {
            hoist_expr(object, context, locals);
            hoist_expr(value, context, locals);
        }
        HirExprKind::Construct { args, .. } => {
            for arg in args {
                hoist_expr(arg, context, locals);
            }
        }
        HirExprKind::ArrayLiteral { items } => {
            for item in items {
                hoist_expr(item, context, locals);
            }
        }
        HirExprKind::ArrayNew { length, .. } => hoist_expr(length, context, locals),
        _ => {}
    }
}

fn next_anon_name() -> Identifier {
    let id = ANON_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Identifier::new(&format!("__anon_{id}"))
}

fn capture_field_name(name: &Identifier) -> Identifier {
    Identifier::new(&format!("__cap_{}", name.as_str()))
}

fn collect_block_captures(block: &HirBlock, analyzer: &mut CaptureAnalyzer, outer_locals: &BTreeMap<String, ValkyrieType>) {
    let mut locals = outer_locals.clone();
    for statement in &block.statements {
        collect_statement_captures(statement, analyzer, &mut locals);
    }
    if let Some(expr) = &block.expr {
        collect_captures(expr, analyzer);
    }
}

fn collect_statement_captures(statement: &HirStatement, analyzer: &mut CaptureAnalyzer, locals: &mut BTreeMap<String, ValkyrieType>) {
    match &statement.kind {
        HirStatementKind::Let { pattern, ty, initializer, .. } => {
            if let Some(initializer) = initializer {
                collect_captures(initializer, analyzer);
            }
            if let Some(ty) = ty {
                bind_pattern_local(pattern, ty, locals);
            }
        }
        HirStatementKind::Expr(expr) => collect_captures(expr, analyzer),
    }
}

fn collect_captures(expr: &HirExpr, analyzer: &mut CaptureAnalyzer) {
    match &expr.kind {
        HirExprKind::Variable(identifier) => analyzer.access_var(identifier.name.as_str(), false),
        HirExprKind::Call { callee, args, .. } => {
            collect_captures(callee, analyzer);
            for arg in args {
                collect_captures(&arg.value, analyzer);
            }
        }
        HirExprKind::Block(block) => {
            for statement in &block.statements {
                if let HirStatementKind::Expr(expr) = &statement.kind {
                    collect_captures(expr, analyzer);
                }
            }
            if let Some(expr) = &block.expr {
                collect_captures(expr, analyzer);
            }
        }
        HirExprKind::If { condition, then_branch, else_branch } => {
            collect_captures(condition, analyzer);
            if let Some(expr) = &then_branch.expr {
                collect_captures(expr, analyzer);
            }
            if let Some(else_branch) = else_branch {
                if let Some(expr) = &else_branch.expr {
                    collect_captures(expr, analyzer);
                }
            }
        }
        HirExprKind::AnonymousClass { fields, methods, .. } => {
            for (_, value) in fields {
                collect_captures(value, analyzer);
            }
            for method in methods {
                if let Some(expr) = &method.body.expr {
                    collect_captures(expr, analyzer);
                }
            }
        }
        _ => {}
    }
}

fn bind_pattern_local(pattern: &crate::types::hir::HirPattern, ty: &ValkyrieType, locals: &mut BTreeMap<String, ValkyrieType>) {
    match pattern {
        crate::types::hir::HirPattern::Variable(identifier) => {
            locals.insert(identifier.name.to_string(), ty.clone());
        }
        crate::types::hir::HirPattern::Bind { identifier, pattern } => {
            bind_pattern_local(pattern, ty, locals);
            locals.insert(identifier.name.to_string(), ty.clone());
        }
        _ => {}
    }
}

fn infer_simple_expr_type(expr: &HirExpr, locals: &BTreeMap<String, ValkyrieType>) -> Option<ValkyrieType> {
    match &expr.kind {
        HirExprKind::Literal(literal) => Some(match literal {
            crate::types::hir::HirLiteral::Integer64(_) => ValkyrieType::Integer64 { signed: true },
            crate::types::hir::HirLiteral::Float64(_) => ValkyrieType::Float64,
            crate::types::hir::HirLiteral::Bool(_) => ValkyrieType::Boolean,
            crate::types::hir::HirLiteral::Unit => ValkyrieType::Unit,
            crate::types::hir::HirLiteral::String(_) => ValkyrieType::Named(Identifier::new("utf8")),
        }),
        HirExprKind::Variable(identifier) => locals.get(identifier.name.as_str()).cloned(),
        HirExprKind::AnonymousClass { class_name: Some(name), .. } => Some(ValkyrieType::Named(name.clone())),
        _ => None,
    }
}
