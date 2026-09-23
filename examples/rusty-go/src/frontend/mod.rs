use nyar_types::{Id, NyarContext, NyarError, NyarFrontend, Vfs};
use oak_go::{ast, GoBuilder, GoLanguage, GoRoot};
use oak_core::source::SourceText;
use chomsky_uir::{IKun, Analysis};
use chomsky_types::Loc;
use std::ops::Range;

/// Rusty Go 前端实现
#[derive(Default)]
pub struct RustyGoFrontend {
    language: GoLanguage,
}

impl RustyGoFrontend {
    /// 创建新的前端实例
    pub fn new() -> Self {
        Self {
            language: GoLanguage::default(),
        }
    }
}

impl NyarFrontend<(), GoRoot> for RustyGoFrontend {
    type Language = GoLanguage;

    fn parse(&self, source: &str) -> Result<GoRoot, NyarError> {
        use oak_core::Builder;
        let builder = GoBuilder::new(&self.language);
        let source_text = SourceText::new(source.to_string());
        let mut session = oak_core::parser::ParseSession::<GoLanguage>::default();
        let output = builder.build(&source_text, &[], &mut session);

        output.result.map_err(|e| NyarError::Compile(format!("Build error: {:?}", e)))
    }

    fn lower_unified<V: Vfs>(&self, ast: &GoRoot, ctx: &mut NyarContext<'_, V, ()>) -> Id {
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

    fn to_loc(&self, range: Range<usize>) -> Loc {
        Loc::new(0, range.start as u32, range.end as u32)
    }

    fn convert_root(&mut self, root: &GoRoot) -> Id {
        let mut items = Vec::new();
        for decl in &root.declarations {
            items.push(self.convert_declaration(decl));
        }
        let loc = Loc::default();
        self.ctx.builder().module("main", items, loc)
    }

    fn convert_declaration(&mut self, decl: &ast::Declaration) -> Id {
        match decl {
            ast::Declaration::Function(func) => self.convert_function(func),
            ast::Declaration::Variable(var) => self.convert_variable(var),
            ast::Declaration::Const(c) => self.convert_const(c),
            ast::Declaration::Type(t) => self.convert_type(t),
        }
    }

    fn convert_function(&mut self, func: &ast::Function) -> Id {
        let loc = self.to_loc(func.span.clone().into());
        let mut params = Vec::new();
        for p in &func.params {
            params.push(p.name.clone());
        }
        let body = self.convert_block(&func.body);

        let lambda = self.ctx.builder().lambda(params, body, loc.clone());
        self.ctx.builder().export(&func.name, lambda, loc)
    }

    fn convert_type(&mut self, t: &ast::TypeDecl) -> Id {
        let loc = self.to_loc(t.span.clone().into());
        // 注册类型定义，支持接口和结构体
        let name_id = self.ctx.builder().symbol(&t.name, loc.clone());
        let def_id = self.ctx.builder().symbol(&t.definition, loc.clone());
        self.ctx.builder().extension("type_decl", vec![name_id, def_id], loc)
    }

    fn convert_variable(&mut self, var: &ast::Variable) -> Id {
        let loc = self.to_loc(var.span.clone().into());
        let name_node = self.ctx.builder().symbol(&var.name, loc.clone());
        let val = if let Some(v) = &var.value {
            self.convert_expression(v)
        } else {
            self.ctx.builder().extension("default_value", vec![], loc.clone())
        };
        self.ctx.builder().extension("let", vec![name_node, val], loc)
    }

    fn convert_const(&mut self, c: &ast::Const) -> Id {
        let loc = self.to_loc(c.span.clone().into());
        let name_node = self.ctx.builder().symbol(&c.name, loc.clone());
        let val = self.convert_expression(&c.value);
        self.ctx.builder().extension("const", vec![name_node, val], loc)
    }

    fn convert_block(&mut self, block: &ast::Block) -> Id {
        let loc = self.to_loc(block.span.clone().into());
        let mut ids = Vec::new();
        for stmt in &block.statements {
            ids.push(self.convert_statement(stmt));
        }
        self.ctx.builder().block(ids, loc)
    }

    fn convert_statement(&mut self, stmt: &ast::Statement) -> Id {
        let span: Range<usize> = match stmt {
            ast::Statement::Expression(expr) => self.get_expr_span(expr),
            ast::Statement::Assignment { span, .. } => span.clone().into(),
            ast::Statement::Return { span, .. } => span.clone().into(),
            ast::Statement::If { span, .. } => span.clone().into(),
            ast::Statement::For { span, .. } => span.clone().into(),
        };
        let loc = self.to_loc(span);
        match stmt {
            ast::Statement::Expression(expr) => self.convert_expression(expr),
            ast::Statement::Assignment { targets, values, .. } => {
                let mut val_ids = Vec::new();
                for v in values {
                    val_ids.push(self.convert_expression(v));
                }
                
                let mut target_ids = Vec::new();
                for t in targets {
                    target_ids.push(self.ctx.builder().symbol(t, loc.clone()));
                }
                
                if target_ids.len() == 1 && val_ids.len() == 1 {
                    self.ctx.builder().assign_to_id(target_ids[0], val_ids[0], loc)
                } else {
                    let targets_tuple = self.ctx.builder().extension("tuple", target_ids, loc.clone());
                    let values_tuple = self.ctx.builder().extension("tuple", val_ids, loc.clone());
                    self.ctx.builder().extension("multi_assign", vec![targets_tuple, values_tuple], loc)
                }
            }
            ast::Statement::Return { values, .. } => {
                let val = if values.is_empty() {
                    self.ctx.builder().constant(0, loc.clone())
                } else if values.len() == 1 {
                    self.convert_expression(&values[0])
                } else {
                    let mut ids = Vec::new();
                    for v in values {
                        ids.push(self.convert_expression(v));
                    }
                    self.ctx.builder().extension("tuple", ids, loc.clone())
                };
                self.ctx.builder().return_(val, loc)
            }
            ast::Statement::If {
                condition,
                then_block,
                else_block,
                ..
            } => {
                let cond = self.convert_expression(condition);
                let then_id = self.convert_block(then_block);
                let else_id = if let Some(eb) = else_block {
                    self.convert_block(eb)
                } else {
                    self.ctx.builder().constant(0, loc.clone())
                };
                self.ctx.builder().branch(cond, then_id, else_id, loc)
            }
            ast::Statement::For {
                init,
                condition,
                post,
                body,
                ..
            } => {
                let mut init_id = self.ctx.builder().constant(0, loc.clone());
                if let Some(i) = init {
                    init_id = self.convert_statement(i);
                }
                let cond = if let Some(c) = condition {
                    self.convert_expression(c)
                } else {
                    self.ctx.builder().constant(1, loc.clone())
                };
                let mut post_id = self.ctx.builder().constant(0, loc.clone());
                if let Some(p) = post {
                    post_id = self.convert_statement(p);
                }
                let body_id = self.convert_block(body);
                self.ctx.builder().extension("for", vec![init_id, cond, post_id, body_id], loc)
            }
        }
    }

    fn get_expr_span(&self, expr: &ast::Expression) -> Range<usize> {
        match expr {
            ast::Expression::Identifier { span, .. } => span.clone().into(),
            ast::Expression::Literal { span, .. } => span.clone().into(),
            ast::Expression::Binary { span, .. } => span.clone().into(),
            ast::Expression::Call { span, .. } => span.clone().into(),
        }
    }

    fn convert_expression(&mut self, expr: &ast::Expression) -> Id {
        let span = self.get_expr_span(expr);
        let loc = self.to_loc(span);
        match expr {
            ast::Expression::Identifier { name, .. } => {
                self.ctx.builder().symbol(name, loc)
            }
            ast::Expression::Literal { value, .. } => {
                if (value.starts_with('"') && value.ends_with('"')) || (value.starts_with('`') && value.ends_with('`')) {
                    let content = &value[1..value.len() - 1];
                    self.ctx.builder().string(content, loc)
                } else if value == "true" {
                    self.ctx.builder().constant(1, loc)
                } else if value == "false" {
                    self.ctx.builder().constant(0, loc)
                } else if let Ok(n) = value.parse::<i64>() {
                    self.ctx.builder().constant(n, loc)
                } else if let Ok(f) = value.parse::<f64>() {
                    self.ctx.builder().float(f, loc)
                } else {
                    self.ctx.builder().constant(0, loc)
                }
            }
            ast::Expression::Binary { left, op, right, .. } => {
                let l = self.convert_expression(left);
                let r = self.convert_expression(right);
                self.ctx.builder().binary_op(op, l, r, loc)
            }
            ast::Expression::Call { func, args, .. } => {
                let mut arguments = Vec::new();
                for arg in args {
                    arguments.push(self.convert_expression(arg));
                }

                if let ast::Expression::Identifier { name, .. } = &**func {
                    match name.as_str() {
                        "fmt.Println" | "fmt.Printf" | "println" | "print" => {
                            return self.ctx.builder().cross_lang_call("nyar", "std::io", "println", arguments, loc);
                        }
                        "os.Exit" => {
                            return self.ctx.builder().cross_lang_call("nyar", "", "$intrinsic:3", arguments, loc);
                        }
                        "panic" => {
                            return self.ctx.builder().cross_lang_call("nyar", "", "$intrinsic:7", arguments, loc);
                        }
                        "sin" | "Math.sin" | "math.sin" => {
                            return self.ctx.builder().cross_lang_call("nyar", "", "$intrinsic:8", arguments, loc);
                        }
                        "sqrt" | "Math.sqrt" | "math.sqrt" => {
                            return self.ctx.builder().cross_lang_call("nyar", "", "$intrinsic:9", arguments, loc);
                        }
                        _ => {}
                    }
                }

                let f = self.convert_expression(func);
                self.ctx.builder().call(f, arguments, loc)
            }
        }
    }
}
