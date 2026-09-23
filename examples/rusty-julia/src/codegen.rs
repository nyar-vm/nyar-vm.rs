//! Julia 代码生成实现

use nyar_types::{Id, Loc, NyarContext, Vfs};
use oak_julia::ast::{JuliaExpression, JuliaStatement, JuliaRoot};
use chomsky_uir::ConstraintAnalysis;

/// Julia 到 UIR 的转换器
pub struct UirConverter<'a, 'b, V: Vfs> {
    ctx: &'a mut NyarContext<'b, V, ConstraintAnalysis>,
}

impl<'a, 'b, V: Vfs> UirConverter<'a, 'b, V> {
    /// 创建新的转换器
    pub fn new(ctx: &'a mut NyarContext<'b, V, ConstraintAnalysis>) -> Self {
        Self { ctx }
    }

    /// 转换根节点
    pub fn convert_root(&mut self, root: &JuliaRoot) -> Id {
        let mut items = Vec::new();
        for stmt in &root.statements {
            if let Some(node) = self.convert_statement(stmt) {
                items.push(node);
            }
        }
        self.ctx.builder().module("main", items, Loc::default())
    }

    /// 转换语句
    pub fn convert_statement(&mut self, stmt: &JuliaStatement) -> Option<Id> {
        match stmt {
            JuliaStatement::Function(func) => {
                self.ctx.scopes.push_scope();
                let mut body = Vec::new();
                for s in &func.body {
                    if let Some(node) = self.convert_statement(s) {
                        body.push(node);
                    }
                }
                self.ctx.scopes.pop_scope();
                let func_id = self.ctx.builder().function(&func.name, Vec::new(), body, Loc::default());
                Some(func_id)
            }
            JuliaStatement::If { condition, then_body, else_body } => {
                let cond_id = self.convert_expression(condition);
                let then_ids = then_body.iter().filter_map(|s| self.convert_statement(s)).collect::<Vec<_>>();
                let then_id = self.ctx.builder().block(then_ids, Loc::default());
                let else_id = else_body.as_ref().map(|body| {
                    let ids = body.iter().filter_map(|s| self.convert_statement(s)).collect::<Vec<_>>();
                    self.ctx.builder().block(ids, Loc::default())
                });
                Some(self.ctx.builder().if_(cond_id, then_id, else_id, Loc::default()))
            }
            JuliaStatement::For { variable, iterable, body } => {
                let var_id = self.ctx.builder().symbol(variable, Loc::default());
                let iter_id = self.convert_expression(iterable);
                let body_ids = body.iter().filter_map(|s| self.convert_statement(s)).collect::<Vec<_>>();
                let body_id = self.ctx.builder().block(body_ids, Loc::default());
                Some(self.ctx.builder().extension("for", vec![var_id, iter_id, body_id], Loc::default()))
            }
            JuliaStatement::Expression(expr) => Some(self.convert_expression(expr)),
            JuliaStatement::Error => None,
        }
    }

    /// 转换表达式
    pub fn convert_expression(&mut self, expr: &JuliaExpression) -> Id {
        let loc = Loc::default();
        match expr {
            JuliaExpression::Identifier(name) => {
                self.ctx.builder().symbol(name, loc)
            }
            JuliaExpression::Literal(val) => {
                let unquoted = if (val.starts_with('"') && val.ends_with('"')) || (val.starts_with('\'') && val.ends_with('\'')) {
                    &val[1..val.len() - 1]
                } else {
                    val
                };
                self.ctx.builder().string(unquoted, loc)
            }
            JuliaExpression::Binary { left, op, right } => {
                let lhs = self.convert_expression(left);
                let rhs = self.convert_expression(right);
                self.ctx.builder().extension(op, vec![lhs, rhs], loc)
            }
            JuliaExpression::Call { callee, arguments } => {
                let args = arguments.iter().map(|arg| self.convert_expression(arg)).collect();
                if let JuliaExpression::Identifier(name) = &**callee {
                    match name.as_str() {
                        "println" => return self.ctx.builder().cross_lang_call("nyar", "io", "println", args, loc),
                        "print" => return self.ctx.builder().cross_lang_call("nyar", "io", "print", args, loc),
                        _ => {}
                    }
                }
                let callee_id = self.convert_expression(callee);
                self.ctx.builder().call(callee_id, args, loc)
            }
        }
    }
}
