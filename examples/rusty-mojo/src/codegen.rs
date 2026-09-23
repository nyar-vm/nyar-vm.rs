use nyar_types::{Id, Loc, NyarContext, Vfs};
use oak_mojo::ast::{MojoExpression, MojoLiteral, MojoStatement};
use chomsky_uir::ConstraintAnalysis;

/// Mojo 代码生成器
pub struct MojoCodegen<'a, 'b, V: Vfs> {
    ctx: &'a mut NyarContext<'b, V, ConstraintAnalysis>,
}

impl<'a, 'b, V: Vfs> MojoCodegen<'a, 'b, V> {
    /// 创建新的代码生成器
    pub fn new(ctx: &'a mut NyarContext<'b, V, ConstraintAnalysis>) -> Self {
        Self { ctx }
    }

    /// 转换多个语句
    pub fn lower_statements(&mut self, statements: &[MojoStatement]) -> Id {
        let mut ids = Vec::new();
        for stmt in statements {
            if let Some(id) = self.lower_statement(stmt) {
                ids.push(id);
            }
        }
        let loc = Loc::default();
        self.ctx.builder().block(ids, loc)
    }

    /// 转换单个语句
    pub fn lower_statement(&mut self, stmt: &MojoStatement) -> Option<Id> {
        let loc = Loc::default();
        match stmt {
            MojoStatement::Function { name, params, body, .. } => {
                let mut body_ids = Vec::new();
                for s in body {
                    if let Some(id) = self.lower_statement(s) {
                        body_ids.push(id);
                    }
                }
                let param_names = params.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
                Some(self.ctx.builder().function(name, param_names, body_ids, loc))
            }
            MojoStatement::Variable { name, value, .. } => {
                let val_id = if let Some(v) = value {
                    self.lower_expression(v)
                } else {
                    let mut b = self.ctx.builder();
                    b.extension("nil", vec![], loc)
                };
                Some(self.ctx.builder().assign(name, val_id, loc))
            }
            MojoStatement::Assignment { target, value } => {
                let target_id = self.lower_expression(target);
                let val_id = self.lower_expression(value);
                Some(self.ctx.builder().assign_to_id(target_id, val_id, loc))
            }
            MojoStatement::If { condition, then_body, else_body } => {
                let cond_id = self.lower_expression(condition);
                let then_id = self.lower_statements(then_body);
                let else_id = else_body.as_ref().map(|body| self.lower_statements(body));
                Some(self.ctx.builder().if_(cond_id, then_id, else_id, loc))
            }
            MojoStatement::While { condition, body } => {
                let cond_id = self.lower_expression(condition);
                let body_id = self.lower_statements(body);
                Some(self.ctx.builder().while_loop(cond_id, body_id, loc))
            }
            MojoStatement::For { variable, iterable, body } => {
                let iter_id = self.lower_expression(iterable);
                let body_id = self.lower_statements(body);
                let mut b = self.ctx.builder();
                let var_id = b.symbol(variable, loc);
                // Mojo for loops are essentially iterative
                Some(b.extension("for_loop", vec![var_id, iter_id, body_id], loc))
            }
            MojoStatement::Return(expr) => {
                let val_id = expr.as_ref().map(|e| self.lower_expression(e))
                    .unwrap_or_else(|| {
                        let mut b = self.ctx.builder();
                        b.extension("nil", vec![], loc)
                    });
                Some(self.ctx.builder().return_(val_id, loc))
            }
            MojoStatement::Expression(expr) => {
                Some(self.lower_expression(expr))
            }
        }
    }

    /// 转换表达式
    pub fn lower_expression(&mut self, expr: &MojoExpression) -> Id {
        let loc = Loc::default();
        match expr {
            MojoExpression::Literal(lit) => {
                let mut b = self.ctx.builder();
                match lit {
                    MojoLiteral::Int(v) => b.int(*v, loc),
                    MojoLiteral::Float(v) => b.float(*v, loc),
                    MojoLiteral::String(v) => b.string(v, loc),
                    MojoLiteral::Bool(v) => b.bool(*v, loc),
                    MojoLiteral::None => b.extension("nil", vec![], loc),
                }
            }
            MojoExpression::Identifier(name) => {
                self.ctx.builder().symbol(name, loc)
            }
            MojoExpression::Binary { left, op, right } => {
                let lhs = self.lower_expression(left);
                let rhs = self.lower_expression(right);
                let mut b = self.ctx.builder();
                match op.as_str() {
                    "+" => b.add_op(lhs, rhs, loc),
                    "-" => b.sub_op(lhs, rhs, loc),
                    "*" => b.mul_op(lhs, rhs, loc),
                    "/" => b.div_op(lhs, rhs, loc),
                    "==" => b.eq_op(lhs, rhs, loc),
                    "!=" => b.ne_op(lhs, rhs, loc),
                    "<" => b.lt_op(lhs, rhs, loc),
                    "<=" => b.le_op(lhs, rhs, loc),
                    ">" => b.gt_op(lhs, rhs, loc),
                    ">=" => b.ge_op(lhs, rhs, loc),
                    _ => b.binary_op(op, lhs, rhs, loc),
                }
            }
            MojoExpression::Unary { op, right } => {
                let rhs = self.lower_expression(right);
                let mut b = self.ctx.builder();
                match op.as_str() {
                    "-" => b.extension("neg", vec![rhs], loc),
                    "!" | "not" => b.extension("not", vec![rhs], loc),
                    _ => b.extension(op, vec![rhs], loc),
                }
            }
            MojoExpression::Call { callee, args } => {
                let func_id = self.lower_expression(callee);
                let arg_ids = args.iter().map(|a| self.lower_expression(a)).collect();
                self.ctx.builder().call(func_id, arg_ids, loc)
            }
        }
    }
}
