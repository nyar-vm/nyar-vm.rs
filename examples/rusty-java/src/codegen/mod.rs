//! Java 到 Nyar UIR 的转换器

use chomsky_types::Loc;
use chomsky_uir::{Analysis, IKun, IntentBuilder, Id};
use nyar_aot::NyarContext;
use nyar_types::NyarError;
use oak_java::ast::*;
use oak_vfs::Vfs;

/// Java 到 UIR 的转换器
pub struct JavaUirConverter<'a, 'b, V: Vfs, A: Analysis<IKun> = ()> {
    ctx: &'a mut NyarContext<'b, V, A>,
}

impl<'a, 'b, V: Vfs, A: Analysis<IKun>> JavaUirConverter<'a, 'b, V, A> {
    /// 创建新的转换器
    pub fn new(ctx: &'a mut NyarContext<'b, V, A>) -> Self {
        Self { ctx }
    }

    /// 辅助方法：创建位置信息
    fn loc(&self) -> Loc {
        Loc::default()
    }

    fn builder(&mut self) -> IntentBuilder<'_, A> {
        self.ctx.builder()
    }
}

impl<'a, 'b, V: Vfs, A: Analysis<IKun>> JavaUirConverter<'a, 'b, V, A> {
    /// 将 Java AST 转换为 UIR 树
    pub fn convert_to_id(ast: &JavaRoot, ctx: &'a mut NyarContext<'b, V, A>) -> Id {
        let mut converter = JavaUirConverter::new(ctx);
        converter.convert_root(ast).unwrap_or_else(|_| converter.ctx.builder().constant(0, Loc::new(0, 0, 0)))
    }

    /// 转换根节点
    pub fn convert_root(&mut self, root: &JavaRoot) -> Result<Id, NyarError> {
        let mut items = Vec::new();
        for item in &root.items {
            match item {
                Item::Class(class) => {
                    items.push(self.convert_class(class)?);
                }
                Item::Interface(interface) => {
                    items.push(self.convert_interface(interface)?);
                }
                Item::Package(pkg) => {
                    let loc = self.loc();
                    let name_id = self.builder().string(&pkg.name, loc);
                    items.push(self.builder().extension("package", vec![name_id], loc));
                }
                Item::Import(imp) => {
                    let loc = self.loc();
                    let path_id = self.builder().string(&imp.path, loc);
                    let is_static_id = self.builder().bool(imp.is_static, loc);
                    items.push(self.builder().extension("import", vec![path_id, is_static_id], loc));
                }
                _ => {}
            }
        }
        if items.is_empty() {
            let loc = self.loc();
            Ok(self.builder().seq(vec![], loc))
        } else {
            let loc = self.loc();
            Ok(self.builder().seq(items, loc))
        }
    }

    fn convert_class(&mut self, class: &ClassDeclaration) -> Result<Id, NyarError> {
        let mut members = Vec::new();
        for member in &class.members {
            match member {
                Member::Method(method) => {
                    members.push(self.convert_method(method)?);
                }
                Member::Field(field) => {
                    members.push(self.convert_field(field)?);
                }
                Member::Constructor(ctor) => {
                    members.push(self.convert_constructor(ctor)?);
                }
            }
        }
        let loc = self.loc();
        let name_id = self.builder().string(&class.name, loc);
        let loc = self.loc();
        let members_id = self.builder().seq(members, loc);

        let mut modifiers = Vec::new();
        for m in &class.modifiers {
            let loc = self.loc();
            modifiers.push(self.builder().string(m, loc));
        }
        let loc = self.loc();
        let modifiers_id = self.builder().seq(modifiers, loc);

        let extends_id = if let Some(ext) = &class.extends {
            let loc = self.loc();
            self.builder().string(ext, loc)
        } else {
            let loc = self.loc();
            self.builder().constant(0, loc) // Use 0 or null as placeholder
        };

        let mut implements = Vec::new();
        for i in &class.implements {
            let loc = self.loc();
            implements.push(self.builder().string(i, loc));
        }
        let loc = self.loc();
        let implements_id = self.builder().seq(implements, loc);

        let loc = self.loc();
        Ok(self.builder().extension(
            "class",
            vec![name_id, modifiers_id, extends_id, implements_id, members_id],
            loc,
        ))
    }

    fn convert_interface(&mut self, interface: &InterfaceDeclaration) -> Result<Id, NyarError> {
        let mut members = Vec::new();
        for member in &interface.members {
            match member {
                Member::Method(method) => {
                    members.push(self.convert_method(method)?);
                }
                Member::Field(field) => {
                    members.push(self.convert_field(field)?);
                }
                Member::Constructor(ctor) => {
                    members.push(self.convert_constructor(ctor)?);
                }
            }
        }
        let loc = self.loc();
        let name_id = self.builder().string(&interface.name, loc);
        let loc = self.loc();
        let members_id = self.builder().seq(members, loc);

        let mut modifiers = Vec::new();
        for m in &interface.modifiers {
            let loc = self.loc();
            modifiers.push(self.builder().string(m, loc));
        }
        let loc = self.loc();
        let modifiers_id = self.builder().seq(modifiers, loc);

        let mut extends = Vec::new();
        for e in &interface.extends {
            let loc = self.loc();
            extends.push(self.builder().string(e, loc));
        }
        let loc = self.loc();
        let extends_id = self.builder().seq(extends, loc);

        let loc = self.loc();
        Ok(self.builder().extension(
            "interface",
            vec![name_id, modifiers_id, extends_id, members_id],
            loc,
        ))
    }

    fn convert_field(&mut self, field: &FieldDeclaration) -> Result<Id, NyarError> {
        let loc = self.loc();
        let name_id = self.builder().string(&field.name, loc);
        let loc = self.loc();
        let type_id = self.builder().string(&field.r#type, loc);

        let mut modifiers = Vec::new();
        for m in &field.modifiers {
            let loc = self.loc();
            modifiers.push(self.builder().string(m, loc));
        }
        let loc = self.loc();
        let modifiers_id = self.builder().seq(modifiers, loc);

        let loc = self.loc();
        Ok(self.builder().extension("field", vec![name_id, type_id, modifiers_id], loc))
    }

    fn convert_constructor(&mut self, ctor: &ConstructorDeclaration) -> Result<Id, NyarError> {
        let body_id = self.convert_block(&ctor.body)?;
        let loc = self.loc();
        let name_id = self.builder().string(&ctor.name, loc);

        let mut param_ids = Vec::new();
        for param in &ctor.parameters {
            param_ids.push(self.convert_parameter(param)?);
        }
        let loc = self.loc();
        let params_id = self.builder().seq(param_ids, loc);

        let mut modifiers = Vec::new();
        for m in &ctor.modifiers {
            let loc = self.loc();
            modifiers.push(self.builder().string(m, loc));
        }
        let loc = self.loc();
        let modifiers_id = self.builder().seq(modifiers, loc);

        let loc = self.loc();
        Ok(self.builder().extension(
            "constructor",
            vec![name_id, modifiers_id, params_id, body_id],
            loc,
        ))
    }

    fn convert_method(&mut self, method: &MethodDeclaration) -> Result<Id, NyarError> {
        let body_id = self.convert_block(&method.body)?;
        let loc = self.loc();
        let name_id = self.builder().string(&method.name, loc);
        let loc = self.loc();
        let ret_id = self.builder().string(&method.return_type, loc);

        let mut param_ids = Vec::new();
        for param in &method.parameters {
            param_ids.push(self.convert_parameter(param)?);
        }
        let loc = self.loc();
        let params_id = self.builder().seq(param_ids, loc);

        let mut modifiers = Vec::new();
        for m in &method.modifiers {
            let loc = self.loc();
            modifiers.push(self.builder().string(m, loc));
        }
        let loc = self.loc();
        let modifiers_id = self.builder().seq(modifiers, loc);

        // 规范化 method 扩展：[name, modifiers, params, return_type, body]
        let loc = self.loc();
        Ok(self.builder().extension(
            "method",
            vec![name_id, modifiers_id, params_id, ret_id, body_id],
            loc,
        ))
    }

    fn convert_parameter(&mut self, param: &Parameter) -> Result<Id, NyarError> {
        let loc = self.loc();
        let name_id = self.builder().string(&param.name, loc);
        let loc = self.loc();
        let type_id = self.builder().string(&param.r#type, loc);
        let loc = self.loc();
        Ok(self.builder().extension("parameter", vec![name_id, type_id], loc))
    }

    fn convert_block(&mut self, stmts: &[Statement]) -> Result<Id, NyarError> {
        let mut ids = Vec::new();
        for stmt in stmts {
            ids.push(self.convert_stmt(stmt)?);
        }
        let loc = self.loc();
        Ok(self.builder().seq(ids, loc))
    }

    fn convert_stmt(&mut self, stmt: &Statement) -> Result<Id, NyarError> {
        match stmt {
            Statement::Expression(expr) => self.convert_expr(expr),
            Statement::Return(Some(expr)) => {
                let val = self.convert_expr(expr)?;
                let loc = self.loc();
                Ok(self.builder().return_(val, loc))
            }
            Statement::Return(None) => {
                let loc = self.loc();
                let null_id = self.builder().constant(0, loc);
                let loc = self.loc();
                Ok(self.builder().return_(null_id, loc))
            }
            Statement::Block(stmts) => self.convert_block(stmts),
            Statement::LocalVariable {
                r#type,
                name,
                initializer,
            } => {
                let loc = self.loc();
                let name_id = self.builder().string(name, loc);
                let loc = self.loc();
                let type_id = self.builder().string(r#type, loc);
                let init_id = if let Some(init) = initializer {
                    self.convert_expr(init)?
                } else {
                    let loc = self.loc();
                    self.builder().constant(0, loc)
                };
                let loc = self.loc();
                Ok(self.builder().extension(
                    "local_variable",
                    vec![name_id, type_id, init_id],
                    loc,
                ))
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond_id = self.convert_expr(condition)?;
                let then_id = self.convert_stmt(then_branch)?;
                let else_id = if let Some(eb) = else_branch {
                    self.convert_stmt(eb)?
                } else {
                    let loc = self.loc();
                    self.builder().seq(vec![], loc)
                };
                let loc = self.loc();
                Ok(self.builder().branch(cond_id, then_id, else_id, loc))
            }
            Statement::While { condition, body } => {
                let cond_id = self.convert_expr(condition)?;
                let body_id = self.convert_stmt(body)?;
                let loc = self.loc();
                Ok(self.builder().while_loop(cond_id, body_id, loc))
            }
            Statement::DoWhile { condition, body } => {
                let cond_id = self.convert_expr(condition)?;
                let body_id = self.convert_stmt(body)?;
                let loc = self.loc();
                Ok(self.builder().extension("do_while", vec![cond_id, body_id], loc))
            }
            Statement::For {
                init,
                condition,
                update,
                body,
            } => {
                let init_id = if let Some(i) = init {
                    self.convert_stmt(i)?
                } else {
                    let loc = self.loc();
                    self.builder().seq(vec![], loc)
                };
                let cond_id = if let Some(c) = condition {
                    self.convert_expr(c)?
                } else {
                    let loc = self.loc();
                    self.builder().bool(true, loc)
                };
                let update_id = if let Some(u) = update {
                    self.convert_expr(u)?
                } else {
                    let loc = self.loc();
                    self.builder().seq(vec![], loc)
                };
                let body_id = self.convert_stmt(body)?;
                let loc = self.loc();
                Ok(self.builder().extension("for", vec![init_id, cond_id, update_id, body_id], loc))
            }
            Statement::ForEach {
                item_type,
                item_name,
                iterable,
                body,
            } => {
                let loc = self.loc();
                let type_id = self.builder().string(item_type, loc);
                let loc = self.loc();
                let name_id = self.builder().string(item_name, loc);
                let iterable_id = self.convert_expr(iterable)?;
                let body_id = self.convert_stmt(body)?;
                let loc = self.loc();
                Ok(self.builder().extension("foreach", vec![type_id, name_id, iterable_id, body_id], loc))
            }
            Statement::Switch {
                selector,
                cases,
                default,
            } => {
                let selector_id = self.convert_expr(selector)?;
                let mut case_ids = Vec::new();
                for case in cases {
                    let label_id = self.convert_expr(&case.label)?;
                    let body_id = self.convert_block(&case.body)?;
                    let loc = self.loc();
                    case_ids.push(self.builder().extension("case", vec![label_id, body_id], loc));
                }
                let loc = self.loc();
                let cases_id = self.builder().seq(case_ids, loc);
                let default_id = if let Some(d) = default {
                    self.convert_block(d)?
                } else {
                    let loc = self.loc();
                    self.builder().seq(vec![], loc)
                };
                let loc = self.loc();
                Ok(self.builder().extension("switch", vec![selector_id, cases_id, default_id], loc))
            }
            Statement::Break => {
                let loc = self.loc();
                Ok(self.builder().extension("break", vec![], loc))
            }
            Statement::Continue => {
                let loc = self.loc();
                Ok(self.builder().extension("continue", vec![], loc))
            }
            Statement::Try(try_stmt) => {
                let block_id = self.convert_block(&try_stmt.block)?;
                let mut catch_ids = Vec::new();
                for catch in &try_stmt.catches {
                    let param_id = self.convert_parameter(&catch.parameter)?;
                    let catch_body_id = self.convert_block(&catch.block)?;
                    let loc = self.loc();
                    catch_ids.push(self.builder().extension("catch", vec![param_id, catch_body_id], loc));
                }
                let loc = self.loc();
                let catches_id = self.builder().seq(catch_ids, loc);
                let finally_id = if let Some(f) = &try_stmt.finally {
                    self.convert_block(f)?
                } else {
                    let loc = self.loc();
                    self.builder().seq(vec![], loc)
                };
                let loc = self.loc();
                Ok(self.builder().extension("try", vec![block_id, catches_id, finally_id], loc))
            }
            Statement::Throw(expr) => {
                let expr_id = self.convert_expr(expr)?;
                let loc = self.loc();
                Ok(self.builder().extension("throw", vec![expr_id], loc))
            }
        }
    }

    fn convert_expr(&mut self, expr: &Expression) -> Result<Id, NyarError> {
        match expr {
            Expression::Literal(Literal::Integer(v)) => {
                let loc = self.loc();
                Ok(self.builder().constant(*v, loc))
            }
            Expression::Literal(Literal::Float(v)) => {
                let loc = self.loc();
                Ok(self.builder().float(*v, loc))
            }
            Expression::Literal(Literal::String(s)) => {
                let loc = self.loc();
                Ok(self.builder().string(s, loc))
            }
            Expression::Literal(Literal::Boolean(b)) => {
                let loc = self.loc();
                Ok(self.builder().bool(*b, loc))
            }
            Expression::Identifier(s) => {
                // 特殊处理 System.out
                if s == "System.out" {
                    let loc = self.loc();
                    let system = self.builder().symbol("System", loc);
                    let loc = self.loc();
                    let out = self.builder().symbol("out", loc);
                    let loc = self.loc();
                    return Ok(self.builder().extension("get_field", vec![system, out], loc));
                }
                let loc = self.loc();
                Ok(self.builder().symbol(s, loc))
            }
            Expression::FieldAccess(fa) => {
                let target = self.convert_expr(&fa.target)?;
                let loc = self.loc();
                let name = self.builder().symbol(&fa.name, loc);
                let loc = self.loc();
                Ok(self.builder().extension("get_field", vec![target, name], loc))
            }
            Expression::Binary { left, op, right } => {
                let left_id = self.convert_expr(left)?;
                let right_id = self.convert_expr(right)?;
                let op_name = match op.as_str() {
                    "+" => "add",
                    "-" => "sub",
                    "*" => "mul",
                    "/" => "div",
                    "%" => "rem",
                    "==" => "eq",
                    "!=" => "ne",
                    "<" => "lt",
                    "<=" => "le",
                    ">" => "gt",
                    ">=" => "ge",
                    "&&" => "land",
                    "||" => "lor",
                    "&" => "and",
                    "|" => "or",
                    "^" => "xor",
                    "<<" => "shl",
                    ">>" => "shr",
                    ">>>" => "ushr",
                    "instanceof" => "instanceof",
                    _ => op,
                };
                let loc = self.loc();
                Ok(self.builder().extension(op_name, vec![left_id, right_id], loc))
            }
            Expression::Unary { op, expression } => {
                let expr_id = self.convert_expr(expression)?;
                let op_name = match op.as_str() {
                    "-" => "neg",
                    "!" => "lnot",
                    "~" => "not",
                    _ => op,
                };
                let loc = self.loc();
                Ok(self.builder().extension(op_name, vec![expr_id], loc))
            }
            Expression::Assignment { left, op, right } => {
                let mut val = self.convert_expr(right)?;
                if op != "=" && !op.is_empty() {
                    // 处理复合赋值，如 x += y 转换为 x = x + y
                    let left_val = self.convert_expr(left)?;
                    let base_op = &op[..op.len().saturating_sub(1)];
                    let op_name = match base_op {
                        "+" => "add",
                        "-" => "sub",
                        "*" => "mul",
                        "/" => "div",
                        "%" => "rem",
                        "&" => "and",
                        "|" => "or",
                        "^" => "xor",
                        "<<" => "shl",
                        ">>" => "shr",
                        ">>>" => "ushr",
                        _ => base_op,
                    };
                    let loc = self.loc();
                    val = self.builder().extension(op_name, vec![left_val, val], loc);
                }

                match &**left {
                    Expression::Identifier(name) => {
                        let loc = self.loc();
                        Ok(self.builder().assign(name, val, loc))
                    }
                    _ => {
                        let target = self.convert_expr(left)?;
                        let loc = self.loc();
                        Ok(self.builder().assign_to_id(target, val, loc))
                    }
                }
            }
            Expression::Update { expression, op, is_prefix } => {
                let name = if let Expression::Identifier(name) = &**expression {
                    name.clone()
                } else {
                    return Err(NyarError::Compile("Increment/decrement only supported for identifiers".to_string()));
                };

                let op_name = match op.as_str() {
                    "++" => "add",
                    "--" => "sub",
                    _ => return Err(NyarError::Compile(format!("Unknown update operator: {}", op))),
                };

                let loc = self.loc();
                let one = self.builder().constant(1, loc);
                let loc = self.loc();
                let current_val = self.builder().symbol(&name, loc);
                let loc = self.loc();
                let new_val = self.builder().extension(op_name, vec![current_val, one], loc);
                let loc = self.loc();
                let assign = self.builder().assign(&name, new_val, loc);

                if *is_prefix {
                    // ++x: (x = x + 1, x)
                    let loc = self.loc();
                    let name_id = self.builder().symbol(&name, loc);
                    let loc = self.loc();
                    Ok(self.builder().seq(vec![assign, name_id], loc))
                } else {
                    // x++: (old = x, x = x + 1, old)
                    let temp_name = format!("_tmp_{}", name);
                    let loc = self.loc();
                    let save_old = self.builder().assign(&temp_name, current_val, loc);
                    let loc = self.loc();
                    let return_old = self.builder().symbol(&temp_name, loc);
                    let loc = self.loc();
                    Ok(self.builder().seq(vec![save_old, assign, return_old], loc))
                }
            }
            Expression::MethodCall(call) => {
                let mut arg_ids = Vec::new();
                for arg in &call.arguments {
                    arg_ids.push(self.convert_expr(arg)?);
                }

                // 特殊处理标准库方法
                if let Some(target) = &call.target {
                    match &**target {
                        // System.out.println / print
                        Expression::Identifier(s) if s == "System.out" => {
                            let loc = self.loc();
                            if call.name == "println" {
                                return Ok(self.builder().cross_lang_call("nyar", "std::io", "println", arg_ids, loc));
                            } else if call.name == "print" {
                                return Ok(self.builder().cross_lang_call("nyar", "std::io", "print", arg_ids, loc));
                            }
                        }
                        Expression::FieldAccess(fa) => {
                            if let Expression::Identifier(t) = &*fa.target {
                                if t == "System" && fa.name == "out" {
                                    let loc = self.loc();
                                    if call.name == "println" {
                                        return Ok(self.builder().cross_lang_call("nyar", "std::io", "println", arg_ids, loc));
                                    } else if call.name == "print" {
                                        return Ok(self.builder().cross_lang_call("nyar", "std::io", "print", arg_ids, loc));
                                    }
                                }
                            }
                        }
                        // Math.xxx
                        Expression::Identifier(s) if s == "Math" => {
                            let loc = self.loc();
                            match call.name.as_str() {
                                "sqrt" => return Ok(self.builder().cross_lang_call("nyar", "math", "sqrt", arg_ids, loc)),
                                "abs" => return Ok(self.builder().cross_lang_call("nyar", "math", "abs", arg_ids, loc)),
                                "sin" => return Ok(self.builder().cross_lang_call("nyar", "math", "sin", arg_ids, loc)),
                                "cos" => return Ok(self.builder().cross_lang_call("nyar", "math", "cos", arg_ids, loc)),
                                "tan" => return Ok(self.builder().cross_lang_call("nyar", "math", "tan", arg_ids, loc)),
                                "random" => return Ok(self.builder().cross_lang_call("nyar", "math", "rand", arg_ids, loc)),
                                _ => {}
                            }
                        }
                        // System.currentTimeMillis
                        Expression::Identifier(s) if s == "System" => {
                            if call.name == "currentTimeMillis" {
                                let loc = self.loc();
                                return Ok(self.builder().cross_lang_call("nyar", "time", "now", arg_ids, loc));
                            }
                        }
                        _ => {}
                    }
                }

                let loc = self.loc();
                let name_id = self.builder().symbol(&call.name, loc);
                let loc = self.loc();
                let args_id = self.builder().seq(arg_ids, loc);
                if let Some(target) = &call.target {
                    let target_id = self.convert_expr(target)?;
                    let loc = self.loc();
                    Ok(self.builder().extension(
                        "call",
                        vec![target_id, name_id, args_id],
                        loc,
                    ))
                } else {
                    let loc = self.loc();
                    Ok(self.builder().extension("call", vec![name_id, args_id], loc))
                }
            }
            Expression::New(new_expr) => {
                let loc = self.loc();
                let type_id = self.builder().string(&new_expr.r#type, loc);
                let mut arg_ids = Vec::new();
                for arg in &new_expr.arguments {
                    arg_ids.push(self.convert_expr(arg)?);
                }
                let loc = self.loc();
                let args_id = self.builder().seq(arg_ids, loc);
                let loc = self.loc();
                Ok(self.builder().extension("new", vec![type_id, args_id], loc))
            }
            Expression::This => {
                let loc = self.loc();
                Ok(self.builder().symbol("this", loc))
            }
            Expression::Super => {
                let loc = self.loc();
                Ok(self.builder().symbol("super", loc))
            }
            Expression::ArrayAccess(access) => {
                let target = self.convert_expr(&access.target)?;
                let index = self.convert_expr(&access.index)?;
                let loc = self.loc();
                Ok(self.builder().extension("get_element", vec![target, index], loc))
            }
            Expression::ArrayCreation(creation) => {
                let loc = self.loc();
                let type_id = self.builder().string(&creation.element_type, loc);
                let mut dim_ids = Vec::new();
                for dim in &creation.dimensions {
                    dim_ids.push(self.convert_expr(dim)?);
                }
                let loc = self.loc();
                let dims_id = self.builder().seq(dim_ids, loc);
                let loc = self.loc();
                Ok(self.builder().extension("array_new", vec![type_id, dims_id], loc))
            }
            Expression::Ternary { condition, then_branch, else_branch } => {
                let cond_id = self.convert_expr(condition)?;
                let then_id = self.convert_expr(then_branch)?;
                let else_id = self.convert_expr(else_branch)?;
                let loc = self.loc();
                Ok(self.builder().branch(cond_id, then_id, else_id, loc))
            }
            Expression::Cast { target_type, expression } => {
                let expr_id = self.convert_expr(expression)?;
                let loc = self.loc();
                let type_id = self.builder().string(target_type, loc);
                let loc = self.loc();
                Ok(self.builder().extension("cast", vec![expr_id, type_id], loc))
            }
        }
    }
}