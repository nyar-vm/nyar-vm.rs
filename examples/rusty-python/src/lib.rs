#![warn(missing_docs)]
//! Rusty Python 语言前端

pub mod runtime;

pub use crate::runtime::RustyPythonRuntime;

use nyar_types::{Id, NyarContext, NyarError, NyarFrontend, Vfs};
use oak_python::ast::{Expression, Literal, PythonRoot, Statement};
use chomsky_uir::{Analysis, IKun};
use chomsky_types::Loc;
use chomsky_uir::egraph::HasDebugInfo;

/// Rusty Python 前端
#[derive(Default)]
pub struct RustyPythonFrontend;

impl RustyPythonFrontend {
    /// 创建新的前端实例
    pub fn new() -> Self {
        Self
    }
}

impl<A: Analysis<IKun> + 'static> NyarFrontend<A, PythonRoot> for RustyPythonFrontend 
where A::Data: HasDebugInfo
{
    type Language = oak_python::PythonLanguage;

    fn parse(&self, source: &str) -> Result<PythonRoot, NyarError> {
        use oak_core::Parser;
        let config = oak_python::PythonLanguage {};
        let parser = oak_python::PythonParser::new(&config);
        let mut cache =
            oak_core::parser::ParseSession::<oak_python::PythonLanguage>::default();
        let parse_result = parser.parse(source, &[], &mut cache);

        let green_node = parse_result
            .result
            .map_err(|e| NyarError::Compile(format!("{:?}", e)))?;

        let builder = oak_python::PythonBuilder::new(&config);
        let source_text = oak_core::source::SourceText::new(source.to_string());
        let ast = builder
            .build_root(green_node, &source_text)
            .map_err(|e| NyarError::Compile(format!("{:?}", e)))?;
        Ok(ast)
    }

    fn lower_unified<V: Vfs>(&self, ast: &PythonRoot, ctx: &mut NyarContext<'_, V, A>) -> Id {
        let mut converter = UirConverter::new(ctx);
        converter.convert_root(ast)
    }
}

struct UirConverter<'a, 'b, V: Vfs, A: Analysis<IKun>> {
    ctx: &'a mut NyarContext<'b, V, A>,
}

impl<'a, 'b, V: Vfs, A: Analysis<IKun>> UirConverter<'a, 'b, V, A> {
    fn new(ctx: &'a mut NyarContext<'b, V, A>) -> Self {
        Self { ctx }
    }

    fn convert_root(&mut self, root: &PythonRoot) -> Id {
        let mut items = Vec::new();
        for stmt in &root.program.statements {
            if let Some(node) = self.convert_statement(stmt) {
                items.push(node);
            }
        }
        self.ctx.builder().module("main", items, Loc::default())
    }

    fn convert_statement(&mut self, stmt: &Statement) -> Option<Id> {
        let loc = Loc::default();
        match stmt {
            Statement::Assignment { target, value } => {
                let val = self.convert_expression(value);
                match target {
                    Expression::Name(name) => {
                        Some(self.ctx.builder().assign(name, val, loc))
                    }
                    Expression::Attribute { value: obj, attr } => {
                        let obj_node = self.convert_expression(obj);
                        let attr_id = self.ctx.builder().symbol(attr, loc.clone());
                        Some(self.ctx.builder().extension("set_field", vec![obj_node, attr_id, val], loc))
                    }
                    _ => {
                        let target_id = self.convert_expression(target);
                        Some(self.ctx.builder().assign_to_id(target_id, val, loc))
                    }
                }
            }
            Statement::AugmentedAssignment {
                target,
                operator,
                value,
            } => {
                let target_node = self.convert_expression(target);
                let value_node = self.convert_expression(value);
                let op_name = match operator {
                    oak_python::ast::AugmentedOperator::Add => "add",
                    oak_python::ast::AugmentedOperator::Sub => "sub",
                    oak_python::ast::AugmentedOperator::Mult => "mul",
                    oak_python::ast::AugmentedOperator::Div => "div",
                    oak_python::ast::AugmentedOperator::FloorDiv => "floordiv",
                    oak_python::ast::AugmentedOperator::Mod => "mod",
                    oak_python::ast::AugmentedOperator::Pow => "pow",
                    oak_python::ast::AugmentedOperator::LShift => "lshift",
                    oak_python::ast::AugmentedOperator::RShift => "rshift",
                    oak_python::ast::AugmentedOperator::BitOr => "bitor",
                    oak_python::ast::AugmentedOperator::BitXor => "bitxor",
                    oak_python::ast::AugmentedOperator::BitAnd => "bitand",
                };
                let result_node = self.ctx.builder().binary_op(op_name, target_node, value_node, loc.clone());
                Some(self.ctx.builder().assign_to_id(target_node, result_node, loc))
            }
            Statement::Expression(expr) => Some(self.convert_expression(expr)),
        Statement::FunctionDef {
            decorators: _,
            name,
            parameters,
            body,
            ..
        } => {
            let actual_name = if name == "__init__" { "initiate" } else { name };
            
            let mut params = Vec::new();
            let mut defaults = Vec::new();
            let mut vararg = None;
            let mut kwarg = None;

            for p in parameters {
                params.push(p.name.clone());
                if let Some(default) = &p.default {
                    defaults.push(self.convert_expression(default));
                }
                if p.is_vararg {
                    vararg = Some(self.ctx.builder().string(&p.name, loc.clone()));
                }
                if p.is_kwarg {
                    kwarg = Some(self.ctx.builder().string(&p.name, loc.clone()));
                }
            }

            let mut body_items = Vec::new();
            for s in body {
                if let Some(node) = self.convert_statement(s) {
                    body_items.push(node);
                }
            }
            let body_id = self.ctx.builder().block(body_items, loc.clone());
            let lam = self.ctx.builder().lambda(params, body_id, loc.clone());
            
            let defaults_id = self.ctx.builder().extension("list", defaults, loc.clone());
            let vararg_id = vararg.unwrap_or_else(|| self.ctx.builder().extension("none", vec![], loc.clone()));
            let kwarg_id = kwarg.unwrap_or_else(|| self.ctx.builder().extension("none", vec![], loc.clone()));

            let name_node = self.ctx.builder().string(actual_name, loc.clone());
            let func_node = self.ctx.builder().extension(
                "python_function",
                vec![
                    name_node,
                    lam,
                    defaults_id,
                    vararg_id,
                    kwarg_id,
                ],
                loc.clone(),
            );
            Some(self.ctx.builder().assign(actual_name, func_node, loc))
        }
        Statement::AsyncFunctionDef {
            decorators: _,
            name,
            parameters,
            body,
            ..
        } => {
            let mut params = Vec::new();
            for p in parameters {
                params.push(p.name.clone());
            }

            let mut body_items = Vec::new();
            for s in body {
                if let Some(node) = self.convert_statement(s) {
                    body_items.push(node);
                }
            }
            let body_id = self.ctx.builder().block(body_items, loc.clone());
            let lam = self.ctx.builder().lambda(params, body_id, loc.clone());
            
            let name_node = self.ctx.builder().string(name, loc.clone());
            let func_node = self.ctx.builder().extension(
                "python_async_function",
                vec![
                    name_node,
                    lam,
                ],
                loc.clone(),
            );
            Some(self.ctx.builder().assign(name, func_node, loc))
        }
            Statement::Return(expr) => {
                let val = expr
                    .as_ref()
                    .map(|e| self.convert_expression(e))
                    .unwrap_or(self.ctx.builder().constant(0, loc.clone()));
                Some(self.ctx.builder().return_(val, loc))
            }
            Statement::If { test, body, orelse } => {
                let cond = self.convert_expression(test);
                let mut then_items = Vec::new();
                for s in body {
                    if let Some(node) = self.convert_statement(s) {
                        then_items.push(node);
                    }
                }
                let then_id = self.ctx.builder().block(then_items, loc.clone());

                let mut else_items = Vec::new();
                for s in orelse {
                    if let Some(node) = self.convert_statement(s) {
                        else_items.push(node);
                    }
                }
                let else_id = self.ctx.builder().block(else_items, loc.clone());

                Some(self.ctx.builder().branch(cond, then_id, else_id, loc))
            }
            Statement::While { test, body, orelse } => {
                let cond = self.convert_expression(test);
                let mut body_items = Vec::new();
                for s in body {
                    if let Some(node) = self.convert_statement(s) {
                        body_items.push(node);
                    }
                }
                let body_id = self.ctx.builder().block(body_items, loc.clone());
                
                let mut else_items = Vec::new();
                for s in orelse {
                    if let Some(node) = self.convert_statement(s) {
                        else_items.push(node);
                    }
                }
                let else_id = self.ctx.builder().block(else_items, loc.clone());
                
                Some(self.ctx.builder().extension("while_else", vec![cond, body_id, else_id], loc))
            }
            Statement::For {
                target,
                iter,
                body,
                orelse,
            } => {
                let target_node = self.convert_expression(target);
                let iter_node = self.convert_expression(iter);
                let mut body_items = Vec::new();
                for s in body {
                    if let Some(node) = self.convert_statement(s) {
                        body_items.push(node);
                    }
                }
                let body_id = self.ctx.builder().block(body_items, loc.clone());
                
                let mut else_items = Vec::new();
                for s in orelse {
                    if let Some(node) = self.convert_statement(s) {
                        else_items.push(node);
                    }
                }
                let else_id = self.ctx.builder().block(else_items, loc.clone());
                
                Some(self.ctx.builder().extension("foreach_else", vec![target_node, iter_node, body_id, else_id], loc))
            }
            Statement::AsyncFor {
                target,
                iter,
                body,
                orelse,
            } => {
                let target_node = self.convert_expression(target);
                let iter_node = self.convert_expression(iter);
                let mut body_items = Vec::new();
                for s in body {
                    if let Some(node) = self.convert_statement(s) {
                        body_items.push(node);
                    }
                }
                let body_id = self.ctx.builder().block(body_items, loc.clone());
                
                let mut else_items = Vec::new();
                for s in orelse {
                    if let Some(node) = self.convert_statement(s) {
                        else_items.push(node);
                    }
                }
                let else_id = self.ctx.builder().block(else_items, loc.clone());
                
                Some(self.ctx.builder().extension("async_foreach_else", vec![target_node, iter_node, body_id, else_id], loc))
            }
            Statement::Pass => Some(self.ctx.builder().constant(0, loc)),
            Statement::Break => Some(self.ctx.builder().extension("break", vec![], loc)),
            Statement::Continue => Some(self.ctx.builder().extension("continue", vec![], loc)),
            Statement::Raise { exc, cause } => {
                let exc_node = exc
                    .as_ref()
                    .map(|e| self.convert_expression(e))
                    .unwrap_or_else(|| self.ctx.builder().constant(0, loc.clone()));
                let cause_node = cause
                    .as_ref()
                    .map(|e| self.convert_expression(e))
                    .unwrap_or_else(|| self.ctx.builder().constant(0, loc.clone()));
                Some(self.ctx.builder().extension("raise", vec![exc_node, cause_node], loc))
            }
            Statement::Assert { test, msg } => {
                let test_node = self.convert_expression(test);
                let msg_node = msg
                    .as_ref()
                    .map(|e| self.convert_expression(e))
                    .unwrap_or_else(|| self.ctx.builder().constant(0, loc.clone()));
                Some(self.ctx.builder().extension("assert", vec![test_node, msg_node], loc))
            }
            Statement::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                let body_items = body.iter().filter_map(|s| self.convert_statement(s)).collect::<Vec<_>>();
                let body_id = self.ctx.builder().block(body_items, loc.clone());

                let mut handler_nodes = Vec::new();
                for handler in handlers {
                    let name_node = if let Some(name) = &handler.name {
                        self.ctx.builder().symbol(name, loc.clone())
                    } else {
                        self.ctx.builder().constant(0, loc.clone())
                    };

                    let type_node = handler
                        .type_
                        .as_ref()
                        .map(|e| self.convert_expression(e))
                        .unwrap_or_else(|| self.ctx.builder().constant(0, loc.clone()));

                    let handler_body_items = handler.body.iter().filter_map(|s| self.convert_statement(s)).collect::<Vec<_>>();
                    let handler_body_id = self.ctx.builder().block(handler_body_items, loc.clone());

                    handler_nodes.push(self.ctx.builder().extension("except", vec![type_node, name_node, handler_body_id], loc.clone()));
                }
                let handlers_id = self.ctx.builder().block(handler_nodes, loc.clone());

                let orelse_items = orelse.iter().filter_map(|s| self.convert_statement(s)).collect::<Vec<_>>();
                let orelse_id = self.ctx.builder().block(orelse_items, loc.clone());

                let final_items = finalbody.iter().filter_map(|s| self.convert_statement(s)).collect::<Vec<_>>();
                let final_id = self.ctx.builder().block(final_items, loc.clone());

                Some(self.ctx.builder().extension("try", vec![body_id, handlers_id, orelse_id, final_id], loc))
            }
            Statement::With { items, body } => {
                let mut item_nodes = Vec::new();
                for item in items {
                    let ctx_expr = self.convert_expression(&item.context_expr);
                    let var_node = if let Some(v) = &item.optional_vars {
                        if let Expression::Name(name) = v {
                            self.ctx.builder().symbol(name, loc.clone())
                        } else {
                            self.convert_expression(v)
                        }
                    } else {
                        self.ctx.builder().constant(0, loc.clone())
                    };
                    item_nodes.push(self.ctx.builder().extension("with_item", vec![ctx_expr, var_node], loc.clone()));
                }
                let items_id = self.ctx.builder().block(item_nodes, loc.clone());

                let body_items = body.iter().filter_map(|s| self.convert_statement(s)).collect::<Vec<_>>();
                let body_id = self.ctx.builder().block(body_items, loc.clone());

                Some(self.ctx.builder().extension("with", vec![items_id, body_id], loc))
            }
            Statement::AsyncWith { items, body } => {
                let mut item_nodes = Vec::new();
                for item in items {
                    let ctx_expr = self.convert_expression(&item.context_expr);
                    let var_node = if let Some(v) = &item.optional_vars {
                        if let Expression::Name(name) = v {
                            self.ctx.builder().symbol(name, loc.clone())
                        } else {
                            self.convert_expression(v)
                        }
                    } else {
                        self.ctx.builder().constant(0, loc.clone())
                    };
                    item_nodes.push(self.ctx.builder().extension("with_item", vec![ctx_expr, var_node], loc.clone()));
                }
                let items_id = self.ctx.builder().block(item_nodes, loc.clone());

                let body_items = body.iter().filter_map(|s| self.convert_statement(s)).collect::<Vec<_>>();
                let body_id = self.ctx.builder().block(body_items, loc.clone());

                Some(self.ctx.builder().extension("async_with", vec![items_id, body_id], loc))
            }
            Statement::Match { subject, cases } => {
                let subject_node = self.convert_expression(subject);
                let mut case_nodes = Vec::new();
                for case in cases {
                    let pattern_node = self.convert_pattern(&case.pattern);
                    let guard_node = case
                        .guard
                        .as_ref()
                        .map(|g| self.convert_expression(g))
                        .unwrap_or_else(|| self.ctx.builder().constant(1, loc.clone()));
                    let body_items = case.body.iter().filter_map(|s| self.convert_statement(s)).collect::<Vec<_>>();
                    let body_id = self.ctx.builder().block(body_items, loc.clone());
                    case_nodes.push(self.ctx.builder().extension(
                        "match_case",
                        vec![pattern_node, guard_node, body_id],
                        loc.clone(),
                    ));
                }
                let cases_id = self.ctx.builder().block(case_nodes, loc.clone());
                Some(self.ctx.builder().extension("match", vec![subject_node, cases_id], loc))
            }
            Statement::Import { names } => {
                let mut last = None;
                for name in names {
                    let module_name = self.ctx.builder().string(&name.name, loc.clone());
                    let module_val = self.ctx.builder().extension("import", vec![module_name], loc.clone());
                    let target_name = name.asname.as_ref().unwrap_or(&name.name);
                    last = Some(self.ctx.builder().assign(target_name, module_val, loc.clone()));
                }
                last
            }
            Statement::ImportFrom { module, names } => {
                let mut last = None;
                let module_str = module.clone().unwrap_or_default();
                let module_name = self.ctx.builder().string(&module_str, loc.clone());
                for name in names {
                    let member_name = self.ctx.builder().string(&name.name, loc.clone());
                    let val = self.ctx.builder().extension("import_from", vec![module_name, member_name], loc.clone());
                    let target_name = name.asname.as_ref().unwrap_or(&name.name);
                    last = Some(self.ctx.builder().assign(target_name, val, loc.clone()));
                }
                last
            }
            Statement::Global { .. } => {
                Some(self.ctx.builder().extension("none", vec![], loc))
            }
            Statement::Nonlocal { .. } => {
                Some(self.ctx.builder().extension("none", vec![], loc))
            }
            Statement::ClassDef { decorators, name, bases, body } => {
                let base_nodes = bases.iter().map(|b| self.convert_expression(b)).collect::<Vec<_>>();

                let mut body_items = Vec::new();
                for s in body {
                    if let Some(node) = self.convert_statement(s) {
                        body_items.push(node);
                    }
                }

                let body_id = self.ctx.builder().block(body_items, loc.clone());

                let bases_id = self.ctx.builder().extension("bases", base_nodes, loc.clone());
                let class_node_vec = vec![self.ctx.builder().symbol(name, loc.clone()), bases_id, body_id];
                let mut class_node = self.ctx.builder().extension("class_def", class_node_vec, loc.clone());

                for dec in decorators.iter().rev() {
                    let dec_node = self.convert_expression(dec);
                    class_node = self.ctx.builder().call(dec_node, vec![class_node], loc.clone());
                }

                Some(self.ctx.builder().assign(name, class_node, loc))
            }
        }
    }

    fn convert_expression(&mut self, expr: &Expression) -> Id {
        let loc = Loc::default();
        match expr {
            Expression::Literal(lit) => match lit {
                Literal::Integer(i) => self.ctx.builder().constant(*i, loc),
                Literal::Float(f) => self.ctx.builder().float(*f, loc),
                Literal::String(s) => self.ctx.builder().string(s, loc),
                Literal::Bytes(b) => {
                    let s = String::from_utf8_lossy(b).to_string();
                    let s_node = self.ctx.builder().string(&s, loc.clone());
                    self.ctx.builder().extension("bytes", vec![s_node], loc)
                }
                Literal::Boolean(b) => self.ctx.builder().bool(*b, loc),
                Literal::None => self.ctx.builder().extension("none", vec![], loc),
            },
            Expression::Name(name) => {
                self.ctx.builder().symbol(name, loc)
            }
            Expression::BinaryOp {
                left,
                operator,
                right,
            } => {
                let left_node = self.convert_expression(left);
                let right_node = self.convert_expression(right);
                let op_name = match operator {
                    oak_python::ast::BinaryOperator::Add => "add",
                    oak_python::ast::BinaryOperator::Sub => "sub",
                    oak_python::ast::BinaryOperator::Mult => "mul",
                    oak_python::ast::BinaryOperator::Div => "div",
                    oak_python::ast::BinaryOperator::FloorDiv => "floordiv",
                    oak_python::ast::BinaryOperator::Mod => "mod",
                    oak_python::ast::BinaryOperator::Pow => "pow",
                    oak_python::ast::BinaryOperator::LShift => "lshift",
                    oak_python::ast::BinaryOperator::RShift => "rshift",
                    oak_python::ast::BinaryOperator::BitOr => "bitor",
                    oak_python::ast::BinaryOperator::BitXor => "bitxor",
                    oak_python::ast::BinaryOperator::BitAnd => "bitand",
                };
                self.ctx.builder().binary_op(op_name, left_node, right_node, loc)
            }
            Expression::UnaryOp { operator, operand } => {
                let operand_node = self.convert_expression(operand);
                let op_name = match operator {
                    oak_python::ast::UnaryOperator::Invert => "invert",
                    oak_python::ast::UnaryOperator::Not => "not",
                    oak_python::ast::UnaryOperator::UAdd => "uadd",
                    oak_python::ast::UnaryOperator::USub => "usub",
                };
                self.ctx.builder().extension(op_name, vec![operand_node], loc)
            }
            Expression::BoolOp { operator, values } => {
                let op_name = match operator {
                    oak_python::ast::BoolOperator::And => "and",
                    oak_python::ast::BoolOperator::Or => "or",
                };
                let nodes = values.iter().map(|v| self.convert_expression(v)).collect();
                self.ctx.builder().extension(op_name, nodes, loc)
            }
            Expression::List { elts } => {
                let nodes = elts.iter().map(|e| self.convert_expression(e)).collect();
                self.ctx.builder().extension("list", nodes, loc)
            }
            Expression::Tuple { elts } => {
                let nodes = elts.iter().map(|e| self.convert_expression(e)).collect();
                self.ctx.builder().extension("tuple", nodes, loc)
            }
            Expression::Dict { keys, values } => {
                let mut nodes = Vec::new();
                for (k, v) in keys.iter().zip(values.iter()) {
                    if let Some(key) = k {
                        let k_node = self.convert_expression(key);
                        let v_node = self.convert_expression(v);
                        nodes.push(self.ctx.builder().extension("dict_item", vec![k_node, v_node], loc.clone()));
                    } else {
                        // **kwargs expansion
                        let v_node = self.convert_expression(v);
                        nodes.push(self.ctx.builder().extension("dict_unpack", vec![v_node], loc.clone()));
                    }
                }
                self.ctx.builder().extension("dict", nodes, loc)
            }
            Expression::Set { elts } => {
                let nodes = elts.iter().map(|e| self.convert_expression(e)).collect();
                self.ctx.builder().extension("set", nodes, loc)
            }
            Expression::ListComp { elt, generators } => {
                let elt_node = self.convert_expression(elt);
                let mut nodes = vec![elt_node];
                for gen in generators {
                    nodes.push(self.convert_comprehension(gen));
                }
                self.ctx.builder().extension("list_comp", nodes, loc)
            }
            Expression::SetComp { elt, generators } => {
                let elt_node = self.convert_expression(elt);
                let mut nodes = vec![elt_node];
                for gen in generators {
                    nodes.push(self.convert_comprehension(gen));
                }
                self.ctx.builder().extension("set_comp", nodes, loc)
            }
            Expression::DictComp { key, value, generators } => {
                let key_node = self.convert_expression(key);
                let val_node = self.convert_expression(value);
                let mut nodes = vec![key_node, val_node];
                for gen in generators {
                    nodes.push(self.convert_comprehension(gen));
                }
                self.ctx.builder().extension("dict_comp", nodes, loc)
            }
            Expression::GeneratorExp { elt, generators } => {
                let elt_node = self.convert_expression(elt);
                let mut nodes = vec![elt_node];
                for gen in generators {
                    nodes.push(self.convert_comprehension(gen));
                }
                self.ctx.builder().extension("generator_exp", nodes, loc)
            }
            Expression::Slice { lower, upper, step } => {
                let lower_node = lower
                    .as_ref()
                    .map(|e| self.convert_expression(e))
                    .unwrap_or_else(|| self.ctx.builder().constant(0, loc.clone()));
                let upper_node = upper
                    .as_ref()
                    .map(|e| self.convert_expression(e))
                    .unwrap_or_else(|| self.ctx.builder().constant(0, loc.clone()));
                let step_node = step
                    .as_ref()
                    .map(|e| self.convert_expression(e))
                    .unwrap_or_else(|| self.ctx.builder().constant(0, loc.clone()));
                self.ctx.builder().extension("slice", vec![lower_node, upper_node, step_node], loc)
            }
            Expression::JoinedStr { values } => {
                let nodes = values.iter().map(|v| self.convert_expression(v)).collect();
                self.ctx.builder().extension("fstring", nodes, loc)
            }
            Expression::FormattedValue { value, .. } => {
                let val_node = self.convert_expression(value);
                self.ctx.builder().extension("formatted_value", vec![val_node], loc)
            }
            Expression::Compare {
                left,
                ops,
                comparators,
            } => {
                let mut current_left = self.convert_expression(left);
                let mut comparisons = Vec::new();
                for (op, right) in ops.iter().zip(comparators.iter()) {
                    let right_node = self.convert_expression(right);
                    let op_name = match op {
                        oak_python::ast::CompareOperator::Eq => "eq",
                        oak_python::ast::CompareOperator::NotEq => "noteq",
                        oak_python::ast::CompareOperator::Lt => "lt",
                        oak_python::ast::CompareOperator::LtE => "lte",
                        oak_python::ast::CompareOperator::Gt => "gt",
                        oak_python::ast::CompareOperator::GtE => "gte",
                        oak_python::ast::CompareOperator::Is => "is",
                        oak_python::ast::CompareOperator::IsNot => "isnot",
                        oak_python::ast::CompareOperator::In => "in",
                        oak_python::ast::CompareOperator::NotIn => "notin",
                    };
                    comparisons.push(self.ctx.builder().binary_op(op_name, current_left, right_node, loc.clone()));
                    current_left = right_node;
                }

                if comparisons.len() == 1 {
                    comparisons.remove(0)
                } else {
                    self.ctx.builder().extension("and", comparisons, loc)
                }
            }
            Expression::Call { func, args, keywords } => {
                let mut has_complex = false;

                for arg in args {
                    if matches!(arg, Expression::Starred { .. }) {
                        has_complex = true;
                        break;
                    }
                }

                for kw in keywords {
                    has_complex = true;
                    if kw.arg.is_none() { // **kwargs
                        break;
                    }
                }

                // 尝试优化为 invoke_method
                if !has_complex {
                    if let Expression::Attribute { value, attr } = &**func {
                        let obj_node = self.convert_expression(value);
                        let method_name = self.ctx.builder().symbol(attr, loc.clone());
                        let arguments = args.iter().map(|arg| self.convert_expression(arg)).collect::<Vec<_>>();
                        let args_ext = self.ctx.builder().extension("args", arguments, loc.clone());
                        return self.ctx.builder().extension("invoke_method", vec![obj_node, method_name, args_ext], loc);
                    }
                }

                let mut arguments = args.iter().map(|arg| self.convert_expression(arg)).collect::<Vec<_>>();
                for kw in keywords {
                    let val = self.convert_expression(&kw.value);
                    let arg = if let Some(arg) = &kw.arg {
                        self.ctx.builder().string(arg, loc.clone())
                    } else {
                        self.ctx.builder().extension("none", vec![], loc.clone())
                    };
                    arguments.push(self.ctx.builder().extension("keyword_arg", vec![arg, val], loc.clone()));
                }

                let f = self.convert_expression(func);
                if has_complex {
                    let args_seq = self.ctx.builder().extension("list", arguments, loc.clone());
                    self.ctx.builder().extension("python_call", vec![f, args_seq], loc)
                } else {
                    self.ctx.builder().call(f, arguments, loc)
                }
            }
            Expression::Attribute { value, attr } => {
                let value_node = self.convert_expression(value);
                let attr_id = self.ctx.builder().symbol(attr, loc.clone());
                self.ctx.builder().extension("get_field", vec![value_node, attr_id], loc)
            }
            Expression::Await(expr) => {
                let node = self.convert_expression(expr);
                self.ctx.builder().extension("await", vec![node], loc)
            }
            Expression::Yield(expr) => {
                let node = if let Some(e) = expr {
                    self.convert_expression(e)
                } else {
                    self.ctx.builder().constant(0, loc.clone())
                };
                self.ctx.builder().extension("yield", vec![node], loc)
            }
            Expression::YieldFrom(expr) => {
                let node = self.convert_expression(expr);
                self.ctx.builder().extension("yield_from", vec![node], loc)
            }
            Expression::Subscript { value, slice } => {
                let value_node = self.convert_expression(value);
                let slice_node = self.convert_expression(slice);
                self.ctx.builder().extension("subscript", vec![value_node, slice_node], loc)
            }
            Expression::Lambda { args, body } => {
                let mut params = Vec::new();
                let mut defaults = Vec::new();
                let mut vararg = None;
                let mut kwarg = None;

                for p in args {
                    params.push(p.name.clone());
                    if let Some(default) = &p.default {
                        defaults.push(self.convert_expression(default));
                    }
                    if p.is_vararg {
                        vararg = Some(self.ctx.builder().string(&p.name, loc.clone()));
                    }
                    if p.is_kwarg {
                        kwarg = Some(self.ctx.builder().string(&p.name, loc.clone()));
                    }
                }

                let body_node = self.convert_expression(body);
                let body_id = self.ctx.builder().block(vec![body_node], loc.clone());
                
                let lam = self.ctx.builder().lambda(params, body_id, loc.clone());
                
                let defaults_id = self.ctx.builder().extension("list", defaults, loc.clone());
                let vararg_id = vararg.unwrap_or_else(|| self.ctx.builder().extension("none", vec![], loc.clone()));
                let kwarg_id = kwarg.unwrap_or_else(|| self.ctx.builder().extension("none", vec![], loc.clone()));

                let name_node = self.ctx.builder().string("lambda", loc.clone());
                self.ctx.builder().extension(
                    "python_function",
                    vec![
                        name_node,
                        lam,
                        defaults_id,
                        vararg_id,
                        kwarg_id,
                    ],
                    loc,
                )
            }
            Expression::IfExp { test, body, orelse } => {
                let cond = self.convert_expression(test);
                let then_node = self.convert_expression(body);
                let else_node = self.convert_expression(orelse);
                self.ctx.builder().extension("if_exp", vec![cond, then_node, else_node], loc)
            }
            Expression::Starred { value, is_double } => {
                let node = self.convert_expression(value);
                let name = if *is_double { "starred_double" } else { "starred" };
                self.ctx.builder().extension(name, vec![node], loc)
            }
        }
    }

    fn convert_comprehension(&mut self, gen: &oak_python::ast::Comprehension) -> Id {
        let loc = Loc::default();
        let target_node = self.convert_expression(&gen.target);
        let iter_node = self.convert_expression(&gen.iter);
        let mut ifs = Vec::new();
        for if_expr in &gen.ifs {
            ifs.push(self.convert_expression(if_expr));
        }
        let ifs_id = self.ctx.builder().block(ifs, loc.clone());
        let mut nodes = vec![target_node, iter_node, ifs_id];
        if gen.is_async {
            nodes.push(self.ctx.builder().constant(1, loc.clone()));
        }
        self.ctx.builder().extension("comprehension", nodes, loc)
    }

    fn convert_pattern(&mut self, pattern: &oak_python::ast::Pattern) -> Id {
        let loc = Loc::default();
        match pattern {
            oak_python::ast::Pattern::Value(expr) => {
                let node = self.convert_expression(expr);
                self.ctx.builder().extension("pattern_value", vec![node], loc)
            }
            oak_python::ast::Pattern::Wildcard => self.ctx.builder().extension("pattern_wildcard", vec![], loc),
            oak_python::ast::Pattern::As { pattern, name } => {
                let inner = if let Some(p) = pattern {
                    self.convert_pattern(p)
                } else {
                    self.ctx.builder().extension("pattern_wildcard", vec![], loc.clone())
                };
                let name_node = self.ctx.builder().symbol(name, loc.clone());
                self.ctx.builder().extension("pattern_as", vec![inner, name_node], loc)
            }
            oak_python::ast::Pattern::Sequence(patterns) => {
                let nodes = patterns.iter().map(|p| self.convert_pattern(p)).collect();
                self.ctx.builder().extension("pattern_sequence", nodes, loc)
            }
            oak_python::ast::Pattern::Mapping { keys, patterns } => {
                let mut nodes = Vec::new();
                for (k, p) in keys.iter().zip(patterns.iter()) {
                    let k_node = self.convert_expression(k);
                    let p_node = self.convert_pattern(p);
                    nodes.push(self.ctx.builder().extension("pattern_mapping_item", vec![k_node, p_node], loc.clone()));
                }
                self.ctx.builder().extension("pattern_mapping", nodes, loc)
            }
            oak_python::ast::Pattern::Class { cls, patterns, keywords, keyword_patterns } => {
                let cls_node = self.convert_expression(cls);
                let mut nodes = vec![cls_node];
                for p in patterns {
                    nodes.push(self.convert_pattern(p));
                }
                for (k, p) in keywords.iter().zip(keyword_patterns.iter()) {
                    let k_node = self.ctx.builder().symbol(k, loc.clone());
                    let p_node = self.convert_pattern(p);
                    nodes.push(self.ctx.builder().extension("pattern_class_keyword", vec![k_node, p_node], loc.clone()));
                }
                self.ctx.builder().extension("pattern_class", nodes, loc)
            }
            oak_python::ast::Pattern::Or(patterns) => {
                let nodes = patterns.iter().map(|p| self.convert_pattern(p)).collect();
                self.ctx.builder().extension("pattern_or", nodes, loc)
            }
        }
    }
}
