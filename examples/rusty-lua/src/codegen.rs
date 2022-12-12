//! Gaia 指令生成器 (已重构为意图树生成器)

use chomsky_uir::{Analysis, IKun, IntentBuilder};
use nyar_types::{Id, Loc, NyarContext, NyarError, Vfs};
use oak_lua::ast::*;

/// Gaia 翻译器，将 Lua AST 转换为意图树
pub struct GaiaTranslator;

impl GaiaTranslator {
    /// 创建新的 Gaia 翻译器
    pub fn new() -> Self {
        Self
    }

    /// 统一的转换接口
    pub fn lower_unified<V: Vfs, A: Analysis<IKun>>(&self, ast: &LuaRoot, ctx: &mut NyarContext<V, A>) -> Id {
        let mut builder = ctx.builder();
        let mut statements = Vec::new();
        for stmt in &ast.statements {
            if let Ok(id) = self.translate_statement(stmt, &mut builder) {
                statements.push(id);
            }
        }

        builder.module("main", statements, Loc::default())
    }

    fn translate_statement<A: Analysis<IKun>>(&self, stmt: &LuaStatement, builder: &mut IntentBuilder<A>) -> Result<Id, NyarError> {
        let loc = Loc::default();
        match stmt {
            LuaStatement::Local(s) => {
                let mut ids = Vec::new();
                for (i, name) in s.names.iter().enumerate() {
                    let val = if let Some(expr) = s.values.get(i) {
                        self.translate_expression(expr, builder)?
                    } else if !s.values.is_empty() && i >= s.values.len() {
                        // Lua multiple assignment: if more targets than values, fill with nil
                        // But if values is empty, it's just local declarations without initialization
                        builder.symbol("nil", loc)
                    } else if s.values.is_empty() {
                        builder.symbol("nil", loc)
                    } else {
                        builder.symbol("nil", loc)
                    };
                    ids.push(builder.assign(name, val, loc));
                }
                Ok(builder.block(ids, loc))
            }
            LuaStatement::Expression(expr) => self.translate_expression(expr, builder),
            LuaStatement::Assignment(s) => {
                let mut ids = Vec::new();
                for (i, target) in s.targets.iter().enumerate() {
                    let val = if let Some(expr) = s.values.get(i) {
                        self.translate_expression(expr, builder)?
                    } else {
                        builder.symbol("nil", loc)
                    };
                    match target {
                        LuaExpression::Identifier(name) => {
                            ids.push(builder.assign(name, val, loc));
                        }
                        LuaExpression::Index(idx) => {
                            let obj = self.translate_expression(&idx.table, builder)?;
                            let key = self.translate_expression(&idx.index, builder)?;
                            ids.push(builder.set_index(obj, key, val, loc));
                        }
                        LuaExpression::Member(mem) => {
                            let obj = self.translate_expression(&mem.table, builder)?;
                            let key = builder.string(&mem.member, loc);
                            ids.push(builder.set_index(obj, key, val, loc));
                        }
                        _ => {
                            // TODO: 更多赋值目标支持
                        }
                    }
                }
                Ok(builder.block(ids, loc))
            }
            LuaStatement::If(s) => {
                let cond = self.translate_expression(&s.condition, builder)?;
                let then_id = self.translate_statements(&s.then_block, builder)?;
                let mut else_id = if let Some(else_block) = &s.else_block {
                    self.translate_statements(else_block, builder)?
                } else {
                    builder.block(vec![], loc)
                };

                for (else_if_cond, else_if_block) in s.else_ifs.iter().rev() {
                    let cond_id = self.translate_expression(else_if_cond, builder)?;
                    let block_id = self.translate_statements(else_if_block, builder)?;
                    else_id = builder.if_(cond_id, block_id, Some(else_id), loc);
                }

                Ok(builder.if_(cond, then_id, Some(else_id), loc))
            }
            LuaStatement::While(s) => {
                let cond = self.translate_expression(&s.condition, builder)?;
                let body = self.translate_statements(&s.block, builder)?;
                Ok(builder.while_(cond, body, loc))
            }
            LuaStatement::Repeat(s) => {
                let body = self.translate_statements(&s.block, builder)?;
                let cond = self.translate_expression(&s.condition, builder)?;
                // repeat until cond => loop { body; if cond break; }
                Ok(builder.extension("repeat_until", vec![body, cond], loc))
            }
            LuaStatement::For(s) => match s {
                LuaForStatement::Numeric { variable, start, end, step, block } => {
                    let start_id = self.translate_expression(start, builder)?;
                    let end_id = self.translate_expression(end, builder)?;
                    let step_id = if let Some(s) = step { self.translate_expression(s, builder)? } else { builder.int(1, loc) };
                    let body = self.translate_statements(block, builder)?;
                    let var_name = builder.string(variable, loc);
                    Ok(builder.extension("for_num", vec![var_name, start_id, end_id, step_id, body], loc))
                }
                LuaForStatement::Generic { variables, iterators, block } => {
                    let mut iters = Vec::new();
                    for it in iterators {
                        iters.push(self.translate_expression(it, builder)?);
                    }
                    let body = self.translate_statements(block, builder)?;
                    let mut var_names = Vec::new();
                    for v in variables {
                        var_names.push(builder.string(v, loc));
                    }
                    let vars_id = builder.extension("vars", var_names, loc);
                    let iters_id = builder.extension("iters", iters, loc);
                    Ok(builder.extension("for_gen", vec![vars_id, iters_id, body], loc))
                }
            },
            LuaStatement::Do(stmts) => self.translate_statements(stmts, builder),
            LuaStatement::Break => Ok(builder.break_(loc)),
            LuaStatement::Goto(label) => {
                let name = builder.string(label, loc);
                Ok(builder.extension("goto", vec![name], loc))
            }
            LuaStatement::Label(label) => {
                let name = builder.string(label, loc);
                Ok(builder.extension("label", vec![name], loc))
            }
            LuaStatement::Function(s) => {
                let func_id = self.translate_function_body(&s.parameters, s.is_vararg, &s.block, builder)?;
                if let Some(receiver) = &s.receiver {
                    // obj:method(args) -> obj.method = function(self, args)
                    let mut name_parts = s.name.clone();
                    let last_name = name_parts.pop().unwrap();
                    let mut obj = builder.symbol(&name_parts[0], loc);
                    for part in &name_parts[1..] {
                        let key = builder.string(part, loc);
                        obj = builder.get_index(obj, key, loc);
                    }
                    let key = builder.string(&last_name, loc);
                    let method_obj = builder.get_index(obj, key, loc);
                    let key = builder.string(receiver, loc);
                    Ok(builder.set_index(method_obj, key, func_id, loc))
                } else {
                    let mut name_parts = s.name.clone();
                    let last_name = name_parts.pop().unwrap();
                    if name_parts.is_empty() {
                        Ok(builder.assign(&last_name, func_id, loc))
                    } else {
                        let mut obj = builder.symbol(&name_parts[0], loc);
                        for part in &name_parts[1..] {
                            let key = builder.string(part, loc);
                            obj = builder.get_index(obj, key, loc);
                        }
                        let key = builder.string(&last_name, loc);
                        Ok(builder.set_index(obj, key, func_id, loc))
                    }
                }
            }
            LuaStatement::Return(s) => {
                let mut vals = Vec::new();
                for expr in &s.values {
                    vals.push(self.translate_expression(expr, builder)?);
                }
                if vals.len() == 1 {
                    Ok(builder.return_(vals[0], loc))
                } else {
                    let tuple = builder.extension("tuple", vals, loc);
                    Ok(builder.return_(tuple, loc))
                }
            }
        }
    }

    fn translate_expression<A: Analysis<IKun>>(&self, expr: &LuaExpression, builder: &mut IntentBuilder<A>) -> Result<Id, NyarError> {
        let loc = Loc::default();
        match expr {
            LuaExpression::Number(n) => Ok(builder.float(*n, loc)),
            LuaExpression::String(s) => Ok(builder.string(s, loc)),
            LuaExpression::Boolean(b) => Ok(builder.bool(*b, loc)),
            LuaExpression::Nil => Ok(builder.symbol("nil", loc)),
            LuaExpression::Identifier(id) => Ok(builder.symbol(id, loc)),
            LuaExpression::Binary(bin) => {
                let left = self.translate_expression(&bin.left, builder)?;
                let right = self.translate_expression(&bin.right, builder)?;
                match bin.op.as_str() {
                    "+" => Ok(builder.add_op(left, right, loc)),
                    "-" => Ok(builder.sub_op(left, right, loc)),
                    "*" => Ok(builder.mul_op(left, right, loc)),
                    "/" => Ok(builder.div_op(left, right, loc)),
                    "%" => Ok(builder.extension("mod", vec![left, right], loc)),
                    "^" => Ok(builder.extension("pow", vec![left, right], loc)),
                    ".." => Ok(builder.extension("concat", vec![left, right], loc)),
                    "==" => Ok(builder.eq_op(left, right, loc)),
                    "~=" => Ok(builder.ne_op(left, right, loc)),
                    "<" => Ok(builder.lt_op(left, right, loc)),
                    ">" => Ok(builder.gt_op(left, right, loc)),
                    "<=" => Ok(builder.le_op(left, right, loc)),
                    ">=" => Ok(builder.ge_op(left, right, loc)),
                    "and" => Ok(builder.binary_op("and", left, right, loc)),
                    "or" => Ok(builder.binary_op("or", left, right, loc)),
                    "&" => Ok(builder.binary_op("bit_and", left, right, loc)),
                    "|" => Ok(builder.binary_op("bit_or", left, right, loc)),
                    "~" => Ok(builder.binary_op("bit_xor", left, right, loc)),
                    "<<" => Ok(builder.binary_op("bit_shl", left, right, loc)),
                    ">>" => Ok(builder.binary_op("bit_shr", left, right, loc)),
                    "//" => Ok(builder.extension("idiv", vec![left, right], loc)),
                    _ => Ok(builder.symbol("nil", loc)),
                }
            }
            LuaExpression::Call(call) => {
                let func = self.translate_expression(&call.function, builder)?;
                let mut args = Vec::new();
                for arg in &call.arguments {
                    args.push(self.translate_expression(arg, builder)?);
                }
                Ok(builder.call(func, args, loc))
            }
            LuaExpression::Unary(un) => {
                let operand = self.translate_expression(&un.operand, builder)?;
                match un.op.as_str() {
                    "-" => Ok(builder.neg_op(operand, loc)),
                    "not" => Ok(builder.not_op(operand, loc)),
                    "#" => Ok(builder.len_op(operand, loc)),
                    "~" => Ok(builder.bit_not_op(operand, loc)),
                    _ => Ok(builder.symbol("nil", loc)),
                }
            }
            LuaExpression::Table(table) => {
                let mut fields = Vec::new();
                for (i, field) in table.fields.iter().enumerate() {
                    match field {
                        LuaTableField::Keyed { key, value } => {
                            let k = self.translate_expression(key, builder)?;
                            let v = self.translate_expression(value, builder)?;
                            fields.push((k, v));
                        }
                        LuaTableField::Named { name, value } => {
                            let k = builder.string(name, loc.clone());
                            let v = self.translate_expression(value, builder)?;
                            fields.push((k, v));
                        }
                        LuaTableField::List { value } => {
                            let k = builder.int((i + 1) as i64, loc.clone());
                            let v = self.translate_expression(value, builder)?;
                            fields.push((k, v));
                        }
                    }
                }
                let mut pairs = Vec::new();
                for (k, v) in fields {
                    pairs.push(builder.extension("pair", vec![k, v], loc.clone()));
                }
                Ok(builder.extension("table", pairs, loc))
            }
            LuaExpression::Function(func) => {
                self.translate_function_body(&func.parameters, func.is_vararg, &func.block, builder)
            }
            LuaExpression::Index(idx) => {
                let obj = self.translate_expression(&idx.table, builder)?;
                let key = self.translate_expression(&idx.index, builder)?;
                Ok(builder.get_index(obj, key, loc))
            }
            LuaExpression::Member(mem) => {
                let obj = self.translate_expression(&mem.table, builder)?;
                let key = builder.string(&mem.member, loc);
                Ok(builder.get_index(obj, key, loc))
            }
            LuaExpression::Vararg => Ok(builder.symbol("...", loc)),
        }
    }

    fn translate_statements<A: Analysis<IKun>>(&self, stmts: &[LuaStatement], builder: &mut IntentBuilder<A>) -> Result<Id, NyarError> {
        let loc = Loc::default();
        let mut ids = Vec::new();
        for stmt in stmts {
            if let Ok(id) = self.translate_statement(stmt, builder) {
                ids.push(id);
            }
        }
        Ok(builder.block(ids, loc))
    }

    fn translate_function_body<A: Analysis<IKun>>(&self, params: &[String], is_vararg: bool, block: &[LuaStatement], builder: &mut IntentBuilder<A>) -> Result<Id, NyarError> {
        let loc = Loc::default();
        let mut param_names = Vec::new();
        for p in params {
            param_names.push(p.clone());
        }
        if is_vararg {
            param_names.push("...".to_string());
        }
        let body = self.translate_statements(block, builder)?;
        Ok(builder.lambda(param_names, body, loc))
    }
}
