//! Elixir 代码生成实现

use nyar_types::{Id, Loc, NyarContext, Vfs};
use oak_elixir::ast::{ElixirRoot, Item, Expr, Statement, Module, Function, Block};
// use chomsky_uir::IKun;

/// 将 Elixir AST 根节点转换为 Nyar IR
pub fn lower_elixir_root<V: Vfs>(ast: &ElixirRoot, ctx: &mut NyarContext<V>) -> Id {
    let mut items = Vec::new();
    for item in &ast.items {
        items.push(lower_item(item, ctx));
    }
    let loc = Loc::default();
    ctx.builder().module("rusty-elixir-program", items, loc)
}

fn lower_item<V: Vfs>(item: &Item, ctx: &mut NyarContext<V>) -> Id {
    match item {
        Item::Module(m) => lower_module(m, ctx),
        Item::Function(f) => lower_function(f, ctx),
        Item::Statement(s) => lower_statement(s, ctx),
    }
}

fn lower_module<V: Vfs>(m: &Module, ctx: &mut NyarContext<V>) -> Id {
    let mut items = Vec::new();
    for item in &m.items {
        items.push(lower_item(item, ctx));
    }
    let loc = range_to_loc(m.span.clone(), ctx.source_id);
    ctx.builder().module(&m.name.name, items, loc)
}

fn lower_function<V: Vfs>(f: &Function, ctx: &mut NyarContext<V>) -> Id {
    let params: Vec<String> = f.params.iter().map(|p| p.name.name.clone()).collect();
    let body = lower_block(&f.body, ctx);
    let loc = range_to_loc(f.span.clone(), ctx.source_id);
    
    // 在 Nyar IR 中，函数可以表示为 Lambda 之后导出，或者直接作为一个定义
    let lambda = ctx.builder().lambda(params, body, loc);
    ctx.builder().export(&f.name.name, lambda, loc)
}

fn lower_block<V: Vfs>(block: &Block, ctx: &mut NyarContext<V>) -> Id {
    let mut stmts = Vec::new();
    for stmt in &block.statements {
        stmts.push(lower_statement(stmt, ctx));
    }
    let loc = range_to_loc(block.span.clone(), ctx.source_id);
    ctx.builder().block(stmts, loc)
}

fn lower_statement<V: Vfs>(stmt: &Statement, ctx: &mut NyarContext<V>) -> Id {
    match stmt {
        Statement::Let { name, expr, span } => {
            let val = lower_expr(expr, ctx);
            let loc = range_to_loc(span.clone(), ctx.source_id);
            ctx.builder().assign(&name.name, val, loc)
        }
        Statement::ExprStmt { expr, .. } => {
            lower_expr(expr, ctx)
        }
    }
}

fn lower_expr<V: Vfs>(expr: &Expr, ctx: &mut NyarContext<V>) -> Id {
    match expr {
        Expr::Ident(id) => {
            let loc = range_to_loc(id.span.clone(), ctx.source_id);
            ctx.builder().symbol(&id.name, loc)
        }
        Expr::Atom { value, span } => {
            let loc = range_to_loc(span.clone(), ctx.source_id);
            // Elixir 原子可以用 Symbol 表示
            ctx.builder().symbol(value, loc)
        }
        Expr::Number { value, span } => {
            let loc = range_to_loc(span.clone(), ctx.source_id);
            if let Ok(i) = value.parse::<i64>() {
                ctx.builder().int(i, loc)
            } else if let Ok(f) = value.parse::<f64>() {
                ctx.builder().float(f, loc)
            } else {
                ctx.builder().int(0, loc)
            }
        }
        Expr::String { value, span } => {
            let loc = range_to_loc(span.clone(), ctx.source_id);
            ctx.builder().string(value, loc)
        }
        Expr::Bool { value, span } => {
            let loc = range_to_loc(span.clone(), ctx.source_id);
            ctx.builder().bool(*value, loc)
        }
        Expr::Unary { op, expr, span } => {
            let val = lower_expr(expr, ctx);
            let loc = range_to_loc(span.clone(), ctx.source_id);
            // 这里需要根据 op 映射到对应的扩展操作
            ctx.builder().extension(&format!("unary_{:?}", op), vec![val], loc)
        }
        Expr::Binary { left, op, right, span } => {
            let l = lower_expr(left, ctx);
            let r = lower_expr(right, ctx);
            let loc = range_to_loc(span.clone(), ctx.source_id);
            // 简单映射一些常见操作
            match format!("{:?}", op).as_str() {
                "Plus" => ctx.builder().add_op(l, r, loc),
                "Minus" => ctx.builder().sub_op(l, r, loc),
                "Mul" => ctx.builder().mul_op(l, r, loc),
                "Div" => ctx.builder().div_op(l, r, loc),
                _ => ctx.builder().binary_op(&format!("bin_{:?}", op), l, r, loc),
            }
        }
        Expr::Match { left, right, span } => {
            let l = lower_expr(left, ctx);
            let r = lower_expr(right, ctx);
            let loc = range_to_loc(span.clone(), ctx.source_id);
            // Elixir match = is often an assignment or pattern match
            // Here we treat it as an assign if left is Ident
            if let Expr::Ident(id) = &**left {
                ctx.builder().assign(&id.name, r, loc)
            } else {
                ctx.builder().extension("match", vec![l, r], loc)
            }
        }
        Expr::Call { callee, args, span } => {
            let func = lower_expr(callee, ctx);
            let arguments: Vec<Id> = args.iter().map(|a| lower_expr(a, ctx)).collect();
            let loc = range_to_loc(span.clone(), ctx.source_id);
            ctx.builder().call(func, arguments, loc)
        }
        Expr::Field { receiver, field, span } => {
            let obj = lower_expr(receiver, ctx);
            let loc = range_to_loc(span.clone(), ctx.source_id);
            // 字段访问可以用 extension 或者特定的 get_field
            let field_sym = ctx.builder().symbol(&field.name, loc);
            ctx.builder().extension("get_field", vec![obj, field_sym], loc)
        }
        Expr::Attribute { name, span } => {
            let loc = range_to_loc(span.clone(), ctx.source_id);
            // 模块属性可以用 extension "get_attribute"
            let attr_sym = ctx.builder().symbol(&name.name, loc);
            ctx.builder().extension("get_attribute", vec![attr_sym], loc)
        }
        Expr::Index { receiver, index, span } => {
            let obj = lower_expr(receiver, ctx);
            let idx = lower_expr(index, ctx);
            let loc = range_to_loc(span.clone(), ctx.source_id);
            ctx.builder().extension("get_index", vec![obj, idx], loc)
        }
        Expr::Paren { expr, .. } => lower_expr(expr, ctx),
        Expr::List { items, span } => {
            let elements: Vec<Id> = items.iter().map(|i| lower_expr(i, ctx)).collect();
            let loc = range_to_loc(span.clone(), ctx.source_id);
            ctx.builder().extension("list", elements, loc)
        }
        Expr::Tuple { items, span } => {
            let elements: Vec<Id> = items.iter().map(|i| lower_expr(i, ctx)).collect();
            let loc = range_to_loc(span.clone(), ctx.source_id);
            ctx.builder().extension("tuple", elements, loc)
        }
        Expr::Map { items, span } => {
            let elements: Vec<Id> = items.iter().map(|i| lower_expr(i, ctx)).collect();
            let loc = range_to_loc(span.clone(), ctx.source_id);
            ctx.builder().extension("map", elements, loc)
        }
        Expr::Block(b) => lower_block(b, ctx),
    }
}

fn range_to_loc(range: core::range::Range<usize>, source_id: u32) -> Loc {
    Loc::new(source_id, range.start as u32, range.end as u32)
}
