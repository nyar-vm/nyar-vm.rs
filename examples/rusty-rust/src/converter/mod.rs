//! Oak Rust AST 到 Chomsky UIR 的转换器

use chomsky_types::Loc;
use chomsky_uir::{Analysis, IKun, Id, IntentBuilder};
use core::range::Range;
use oak_rust::ast as oak_ast;
use oak_rust::lexer::RustTokenType;

// Helper to convert Range<usize> to Loc
fn span_to_loc(span: Range<usize>) -> Loc {
    Loc::new(0, span.start as u32, span.end as u32)
}

fn stmt_span(stmt: &oak_ast::Statement) -> Range<usize> {
    match stmt {
        oak_ast::Statement::Let { span, .. } => span.clone(),
        oak_ast::Statement::Return { span, .. } => span.clone(),
        oak_ast::Statement::ExprStmt { span, .. } => span.clone(),
        oak_ast::Statement::Break { span, .. } => span.clone(),
        oak_ast::Statement::Continue { span, .. } => span.clone(),
        oak_ast::Statement::Item(item) => match item {
            oak_ast::Item::Function(f) => f.span.clone(),
            oak_ast::Item::Struct(s) => s.span.clone(),
            oak_ast::Item::Impl(i) => i.span.clone(),
            oak_ast::Item::Trait(t) => t.span.clone(),
            oak_ast::Item::Module(m) => m.span.clone(),
            oak_ast::Item::Use(u) => u.span.clone(),
            oak_ast::Item::ExternBlock(e) => e.span.clone(),
            _ => Range { start: 0, end: 0 },
        },
    }
}

/// 将 Oak Rust AST 根节点转换为 Chomsky UIR
pub fn convert_root<A: Analysis<IKun>>(
    root: &oak_ast::RustRoot,
    builder: &mut IntentBuilder<A>,
) -> Id {
    let mut items = Vec::new();
    for item in &root.items {
        items.push(convert_item(item, builder));
    }
    builder.extension("module", items, Loc::unknown())
}

fn convert_item<A: Analysis<IKun>>(item: &oak_ast::Item, builder: &mut IntentBuilder<A>) -> Id {
    match item {
        oak_ast::Item::Function(f) => convert_function(f, builder),
        oak_ast::Item::Struct(s) => convert_struct(s, builder),
        _ => builder.string(&format!("unsupported_item: {:?}", item), Loc::unknown()),
    }
}

fn convert_function<A: Analysis<IKun>>(f: &oak_ast::Function, builder: &mut IntentBuilder<A>) -> Id {
    let loc = span_to_loc(f.span.clone());

    // Params
    let params: Vec<String> = f.params.iter().map(|p| p.name.name.clone()).collect();

    // Body
    let body_id = convert_block(&f.body, builder);

    // Function definition: assign name = lambda
    let lambda = builder.lambda(params, body_id, loc);
    builder.assign(&f.name.name, lambda, loc)
}

fn convert_struct<A: Analysis<IKun>>(s: &oak_ast::Struct, builder: &mut IntentBuilder<A>) -> Id {
    let loc = span_to_loc(s.span.clone());
    let mut fields = Vec::new();
    for field in &s.fields {
        let f_id = builder.symbol(&field.name.name, span_to_loc(field.span.clone()));
        fields.push(f_id);
    }
    // Struct definition as Extension? Or just a symbol declaration?
    // Using extension "struct_def"
    let mut args = vec![builder.symbol(&s.name.name, loc)];
    args.extend(fields);
    builder.extension("struct_def", args, loc)
}

fn convert_block<A: Analysis<IKun>>(block: &oak_ast::Block, builder: &mut IntentBuilder<A>) -> Id {
    let loc = span_to_loc(block.span.clone());
    let stmts = block
        .statements
        .iter()
        .map(|s| convert_statement(s, builder))
        .collect();
    builder.seq(stmts, loc)
}

fn convert_statement<A: Analysis<IKun>>(
    stmt: &oak_ast::Statement,
    builder: &mut IntentBuilder<A>,
) -> Id {
    let loc = span_to_loc(stmt_span(stmt));
    match stmt {
        oak_ast::Statement::Let { name, expr, .. } => {
            let value = if let Some(e) = expr {
                convert_expr(e, builder)
            } else {
                builder.seq(vec![], loc)
            };
            builder.assign(&name.name, value, loc)
        }
        oak_ast::Statement::Return { expr, .. } => {
            let value = if let Some(e) = expr {
                convert_expr(e, builder)
            } else {
                builder.seq(vec![], loc)
            };
            builder.extension("return", vec![value], loc)
        }
        oak_ast::Statement::ExprStmt { expr, .. } => convert_expr(expr, builder),
        oak_ast::Statement::Break { .. } => builder.extension("break", vec![], loc),
        oak_ast::Statement::Continue { .. } => builder.extension("continue", vec![], loc),
        oak_ast::Statement::Item(item) => convert_item(item, builder),
    }
}

fn extract_name(expr: &oak_ast::Expr) -> Option<String> {
    match expr {
        oak_ast::Expr::Ident(id) => Some(id.name.clone()),
        oak_ast::Expr::Field {
            receiver, field, ..
        } => {
            let r = extract_name(receiver)?;
            Some(format!("{}.{}", r, field.name))
        }
        _ => None,
    }
}

fn convert_expr<A: Analysis<IKun>>(expr: &oak_ast::Expr, builder: &mut IntentBuilder<A>) -> Id {
    // Note: Loc is tricky to get from expr ref if not stored.
    // oak_ast::Expr usually has span.
    let loc = Loc::unknown(); // Simplified for now as Expr might not expose span easily in this match context
                              // Actually oak_ast::Expr variants have span.

    match expr {
        oak_ast::Expr::Ident(id) => builder.symbol(&id.name, span_to_loc(id.span.clone())),
        oak_ast::Expr::Literal { value, span } => {
            // Basic parsing of literal string to type?
            if let Ok(i) = value.parse::<i64>() {
                builder.constant(i, span_to_loc(span.clone()))
            } else if value.starts_with('"') {
                builder.string(value, span_to_loc(span.clone()))
            } else {
                builder.string(value, span_to_loc(span.clone()))
            }
        }
        oak_ast::Expr::Bool { value, span } => builder.bool(*value, span_to_loc(span.clone())),
        oak_ast::Expr::Binary {
            left,
            op,
            right,
            span,
        } => {
            let loc = span_to_loc(span.clone());
            let op_name = match op {
                RustTokenType::Plus => "Add",
                RustTokenType::Minus => "Sub",
                RustTokenType::Star => "Mul",
                RustTokenType::Slash => "Div",
                RustTokenType::EqEq => "Equal",
                RustTokenType::Ne => "NotEqual",
                RustTokenType::Lt => "Less",
                RustTokenType::Le => "LessEqual",
                RustTokenType::Gt => "Greater",
                RustTokenType::Ge => "GreaterEqual",
                RustTokenType::AndAnd => "And",
                RustTokenType::OrOr => "Or",
                _ => "UnknownOp",
            };
            let l = convert_expr(left, builder);
            let r = convert_expr(right, builder);
            builder.binary_op(op_name, l, r, loc)
        }
        oak_ast::Expr::Call { callee, args, span } => {
            let loc = span_to_loc(span.clone());
            if let Some(name) = extract_name(callee) {
                let arg_ids: Vec<Id> = args.iter().map(|a| convert_expr(a, builder)).collect();
                if name == "println" || name == "println!" {
                    return builder.cross_lang_call("nyar", "std::io", "println", arg_ids, loc);
                } else if name == "print" || name == "print!" {
                    return builder.cross_lang_call("nyar", "std::io", "print", arg_ids, loc);
                }
                let func_id = builder.symbol(&name, loc);
                builder.call(func_id, arg_ids, loc)
            } else {
                let func_id = builder.symbol("anonymous_call", loc);
                let arg_ids = args.iter().map(|a| convert_expr(a, builder)).collect();
                builder.call(func_id, arg_ids, loc)
            }
        }
        oak_ast::Expr::If {
            condition,
            then_block,
            else_block,
            span,
        } => {
            let loc = span_to_loc(span.clone());
            let cond = convert_expr(condition, builder);
            let t = convert_block(then_block, builder);
            let f = if let Some(eb) = else_block {
                convert_block(eb, builder)
            } else {
                builder.constant(0, loc)
            };
            builder.branch(cond, t, f, loc)
        }
        oak_ast::Expr::While {
            condition,
            body,
            span,
        } => {
            let loc = span_to_loc(span.clone());
            let cond = convert_expr(condition, builder);
            let b = convert_block(body, builder);
            builder.while_loop(cond, b, loc)
        }
        oak_ast::Expr::For {
            var,
            iter,
            body,
            span,
        } => {
            let loc = span_to_loc(span.clone());
            let var_id = builder.symbol(&var.name, span_to_loc(var.span.clone()));
            let iter_id = convert_expr(iter, builder);
            let body_id = convert_block(body, builder);
            builder.extension("for", vec![var_id, iter_id, body_id], loc)
        }
        oak_ast::Expr::Field {
            receiver,
            field,
            span,
        } => {
            let loc = span_to_loc(span.clone());
            let r = convert_expr(receiver, builder);
            let f = builder.symbol(&field.name, span_to_loc(field.span.clone()));
            builder.extension("field_access", vec![r, f], loc)
        }
        oak_ast::Expr::Index {
            receiver,
            index,
            span,
        } => {
            let loc = span_to_loc(span.clone());
            let r = convert_expr(receiver, builder);
            let i = convert_expr(index, builder);
            builder.extension("index_access", vec![r, i], loc)
        }
        oak_ast::Expr::Array { elements, span } => {
            let loc = span_to_loc(span.clone());
            let elems = elements.iter().map(|e| convert_expr(e, builder)).collect();
            builder.extension("array_literal", elems, loc)
        }
        oak_ast::Expr::Struct { path, fields, span } => {
            let loc = span_to_loc(span.clone());
            let mut args = vec![builder.string(path, loc)];
            for f in fields {
                args.push(builder.string(&f.name.name, span_to_loc(f.span.clone())));
                args.push(convert_expr(&f.expr, builder));
            }
            builder.extension("struct_instantiation", args, loc)
        }
        _ => builder.string("unsupported_expr", loc),
    }
}
