//! CSharp 到 Nyar 字节码的翻译器

use chomsky_types::Loc;
use chomsky_uir::{ConstraintAnalysis, EGraph, IKun, IKunTree, IntentBuilder};
use nyar_types::NyarError;
use oak_csharp::ast::*;

/// CSharp 翻译器上下文
///
/// 用于管理翻译过程中的状态，如符号表、E-Graph 构建器等。
pub struct TranslatorContext<'a, A: chomsky_uir::Analysis<IKun> = ()> {
    pub builder: IntentBuilder<'a, A>,
}

impl<'a, A: chomsky_uir::Analysis<IKun>> TranslatorContext<'a, A> {
    pub fn new(egraph: &'a mut EGraph<IKun, A>) -> Self {
        Self {
            builder: IntentBuilder::new(egraph),
        }
    }

    pub fn new_with_builder(builder: IntentBuilder<'a, A>) -> Self {
        Self { builder }
    }

    /// 解析内置函数映射
    pub fn resolve_builtin(&mut self, target: &str, method: &str, args: Vec<chomsky_uir::egraph::Id>, loc: Loc) -> Option<chomsky_uir::egraph::Id> {
        match (target, method) {
            // System.Console
            ("System.Console", "WriteLine") | ("Console", "WriteLine") => {
                Some(self.builder.cross_lang_call("nyar", "std::io", "println", args, loc))
            }
            ("System.Console", "Write") | ("Console", "Write") => {
                Some(self.builder.cross_lang_call("nyar", "std::io", "print", args, loc))
            }
            ("System.Console", "ReadLine") | ("Console", "ReadLine") => {
                Some(self.builder.cross_lang_call("nyar", "std::io", "read_line", args, loc))
            }
            // System.Math
            ("System.Math", "Abs") | ("Math", "Abs") => Some(self.builder.cross_lang_call("nyar", "std::math", "abs", args, loc)),
            ("System.Math", "Sqrt") | ("Math", "Sqrt") => Some(self.builder.cross_lang_call("nyar", "std::math", "sqrt", args, loc)),
            ("System.Math", "Pow") | ("Math", "Pow") => Some(self.builder.cross_lang_call("nyar", "std::math", "pow", args, loc)),
            ("System.Math", "Sin") | ("Math", "Sin") => Some(self.builder.cross_lang_call("nyar", "std::math", "sin", args, loc)),
            ("System.Math", "Cos") | ("Math", "Cos") => Some(self.builder.cross_lang_call("nyar", "std::math", "cos", args, loc)),
            ("System.Math", "Tan") | ("Math", "Tan") => Some(self.builder.cross_lang_call("nyar", "std::math", "tan", args, loc)),
            ("System.Math", "Log") | ("Math", "Log") => Some(self.builder.cross_lang_call("nyar", "std::math", "log", args, loc)),
            ("System.Math", "Exp") | ("Math", "Exp") => Some(self.builder.cross_lang_call("nyar", "std::math", "exp", args, loc)),
            ("System.Math", "Floor") | ("Math", "Floor") => Some(self.builder.cross_lang_call("nyar", "std::math", "floor", args, loc)),
            ("System.Math", "Ceiling") | ("Math", "Ceiling") => Some(self.builder.cross_lang_call("nyar", "std::math", "ceil", args, loc)),
            ("System.Math", "Round") | ("Math", "Round") => Some(self.builder.cross_lang_call("nyar", "std::math", "round", args, loc)),
            ("System.Math", "Min") | ("Math", "Min") => Some(self.builder.cross_lang_call("nyar", "std::math", "min", args, loc)),
            ("System.Math", "Max") | ("Math", "Max") => Some(self.builder.cross_lang_call("nyar", "std::math", "max", args, loc)),
            // System.String
            ("System.String", "Concat") | ("String", "Concat") => Some(self.builder.cross_lang_call("nyar", "std::string", "concat", args, loc)),
            ("System.String", "IsNullOrEmpty") | ("String", "IsNullOrEmpty") => Some(self.builder.cross_lang_call("nyar", "std::string", "is_null_or_empty", args, loc)),
            // System.Convert
            ("System.Convert", "ToInt32") | ("Convert", "ToInt32") => Some(self.builder.cross_lang_call("nyar", "std::convert", "to_int", args, loc)),
            ("System.Convert", "ToDouble") | ("Convert", "ToDouble") => Some(self.builder.cross_lang_call("nyar", "std::convert", "to_float", args, loc)),
            ("System.Convert", "ToString") | ("Convert", "ToString") => Some(self.builder.cross_lang_call("nyar", "std::convert", "to_string", args, loc)),
            // System.Environment
            ("System.Environment", "Exit") | ("Environment", "Exit") => Some(self.builder.cross_lang_call("nyar", "std::os", "exit", args, loc)),
            _ => None,
        }
    }
}

pub struct NyarTranslator;

impl NyarTranslator {
    pub fn new() -> Self {
        Self
    }

    pub fn translate_to_graph(
        &self,
        ast: &CSharpRoot,
        egraph: &mut EGraph<IKun, ConstraintAnalysis>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let mut ctx = TranslatorContext::new(egraph);
        self.translate_root(ast, &mut ctx)
    }

    pub fn translate_to_tree(&self, ast: &CSharpRoot) -> Result<IKunTree, NyarError> {
        let mut egraph = EGraph::<IKun, ConstraintAnalysis>::new();
        let root_id = self.translate_to_graph(ast, &mut egraph)?;

        let extractor =
            chomsky_extract::IKunExtractor::new(&egraph, chomsky_cost::DefaultCostModel::default());

        Ok(extractor.extract(root_id))
    }

    pub fn translate_root<A: chomsky_uir::Analysis<IKun>>(
        &self,
        root: &CSharpRoot,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let mut items = Vec::new();
        for item in &root.items {
            if let Some(id) = self.translate_item(item, ctx)? {
                items.push(id);
            }
        }
        Ok(ctx.builder.module("root", items, Loc::unknown()))
    }

    fn translate_item<A: chomsky_uir::Analysis<IKun>>(
        &self,
        item: &Item,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<Option<chomsky_uir::egraph::Id>, NyarError> {
        match item {
            Item::Namespace(ns) => Ok(Some(self.translate_namespace(ns, ctx)?)),
            Item::Using(using) => {
                let loc = Loc::unknown();
                let path_id = ctx.builder.symbol(&using.path, loc);
                let mut args = vec![path_id];
                if let Some(alias) = &using.alias {
                    args.push(ctx.builder.symbol(alias, loc));
                }
                if using.is_static {
                    Ok(Some(ctx.builder.extension("import_static", args, loc)))
                } else {
                    Ok(Some(ctx.builder.extension("import", args, loc)))
                }
            }
            Item::Class(class) => Ok(Some(self.translate_class(class, ctx)?)),
            Item::Interface(interface) => Ok(Some(self.translate_interface(interface, ctx)?)),
            Item::Struct(struct_decl) => Ok(Some(self.translate_struct(struct_decl, ctx)?)),
            Item::Enum(enum_decl) => Ok(Some(self.translate_enum(enum_decl, ctx)?)),
            Item::Record(record_decl) => Ok(Some(self.translate_record(record_decl, ctx)?)),
            Item::Delegate(delegate) => Ok(Some(self.translate_delegate(delegate, ctx)?)),
        }
    }

    fn translate_namespace<A: chomsky_uir::Analysis<IKun>>(
        &self,
        ns: &NamespaceDeclaration,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let mut items = Vec::new();
        // 处理特性的翻译（可选，取决于 Nyar 是否支持模块特性）
        for _attr in &ns.attributes {
            // items.push(self.translate_attribute(attr, ctx)?);
        }
        for item in &ns.items {
            if let Some(id) = self.translate_item(item, ctx)? {
                items.push(id);
            }
        }
        Ok(ctx.builder.module(&ns.name, items, Loc::unknown()))
    }

    fn translate_class<A: chomsky_uir::Analysis<IKun>>(
        &self,
        class: &ClassDeclaration,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let mut members = Vec::new();
        
        // 处理特性的翻译
        for attr in &class.attributes {
            members.push(self.translate_attribute(attr, ctx)?);
        }

        for member in &class.members {
            match member {
                Member::Method(method) => {
                    members.push(self.translate_method(method, ctx)?);
                }
                Member::Field(field) => {
                    members.push(self.translate_field(field, ctx)?);
                }
                Member::Constructor(ctor) => {
                    members.push(self.translate_method(ctor, ctx)?);
                }
                Member::Property(prop) => {
                    members.push(self.translate_property(prop, ctx)?);
                }
                Member::Indexer(indexer) => {
                    members.push(self.translate_indexer(indexer, ctx)?);
                }
                Member::Event(event) => {
                    members.push(self.translate_event(event, ctx)?);
                }
            }
        }
        
        let loc = Loc::unknown();
        let mut class_id = ctx.builder.module(&class.name, members, loc);

        // 处理泛型参数
        if !class.type_parameters.is_empty() {
            let mut type_params = Vec::new();
            for tp in &class.type_parameters {
                type_params.push(self.translate_type_parameter(tp, ctx)?);
            }
            // 在 Nyar 中，我们可以通过 extension 来标记这是一个泛型模块
            class_id = ctx.builder.extension("generic_class", vec![class_id], loc);
        }

        // 处理修饰符
        for modifier in &class.modifiers {
            match modifier.as_str() {
                "static" => class_id = ctx.builder.extension("static", vec![class_id], loc),
                "abstract" => class_id = ctx.builder.extension("abstract", vec![class_id], loc),
                "sealed" => class_id = ctx.builder.extension("sealed", vec![class_id], loc),
                "public" | "private" | "protected" | "internal" => {
                    let modifier_id = ctx.builder.symbol(modifier, loc);
                    class_id = ctx.builder.extension("visibility", vec![modifier_id, class_id], loc);
                }
                _ => {}
            }
        }

        Ok(class_id)
    }

    fn translate_attribute<A: chomsky_uir::Analysis<IKun>>(
        &self,
        attr: &Attribute,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let loc = Loc::unknown();
        let mut args = Vec::new();
        for arg in &attr.arguments {
            args.push(self.translate_expr(arg, ctx)?);
        }
        let attr_id = ctx.builder.symbol(&attr.name, loc);
        Ok(ctx.builder.extension("attribute", vec![attr_id], loc))
    }

    fn translate_type_parameter<A: chomsky_uir::Analysis<IKun>>(
        &self,
        tp: &TypeParameter,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let loc = Loc::unknown();
        Ok(ctx.builder.symbol(&tp.name, loc))
    }

    fn translate_delegate<A: chomsky_uir::Analysis<IKun>>(
        &self,
        delegate: &DelegateDeclaration,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let loc = Loc::unknown();
        let mut params = Vec::new();
        for p in &delegate.parameters {
            params.push(p.name.clone());
        }
        let body_id = ctx.builder.constant(0, loc);
        let lambda = ctx.builder.lambda(params, body_id, loc);
        Ok(ctx.builder.export(&delegate.name, lambda, loc))
    }

    fn translate_interface<A: chomsky_uir::Analysis<IKun>>(
        &self,
        interface: &InterfaceDeclaration,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let mut members = Vec::new();
        for member in &interface.members {
            if let Member::Method(method) = member {
                members.push(self.translate_method(method, ctx)?);
            }
        }
        Ok(ctx.builder.module(&interface.name, members, Loc::unknown()))
    }

    pub fn translate_struct<A: chomsky_uir::Analysis<IKun>>(
        &self,
        struct_decl: &StructDeclaration,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let mut members = Vec::new();
        for member in &struct_decl.members {
            match member {
                Member::Method(method) => members.push(self.translate_method(method, ctx)?),
                Member::Field(field) => members.push(self.translate_field(field, ctx)?),
                Member::Constructor(ctor) => members.push(self.translate_method(ctor, ctx)?),
                _ => {}
            }
        }
        Ok(ctx.builder.module(&struct_decl.name, members, Loc::unknown()))
    }

    pub fn translate_record<A: chomsky_uir::Analysis<IKun>>(
        &self,
        record_decl: &RecordDeclaration,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let mut members = Vec::new();
        for member in &record_decl.members {
            match member {
                Member::Method(method) => members.push(self.translate_method(method, ctx)?),
                Member::Field(field) => members.push(self.translate_field(field, ctx)?),
                Member::Constructor(ctor) => members.push(self.translate_method(ctor, ctx)?),
                Member::Property(prop) => members.push(self.translate_property(prop, ctx)?),
                _ => {}
            }
        }
        Ok(ctx.builder.module(&record_decl.name, members, Loc::unknown()))
    }

    fn translate_enum<A: chomsky_uir::Analysis<IKun>>(
        &self,
        enum_decl: &EnumDeclaration,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let mut variants = Vec::new();
        let loc = Loc::unknown();
        for variant in &enum_decl.members {
            let value_id = ctx.builder.constant(0, loc);
            variants.push(ctx.builder.export(&variant.name, value_id, loc));
        }
        Ok(ctx.builder.module(&enum_decl.name, variants, loc))
    }

    fn translate_field<A: chomsky_uir::Analysis<IKun>>(
        &self,
        field: &FieldDeclaration,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let loc = Loc::unknown();
        let mut field_id = ctx.builder.symbol(&field.name, loc);

        // 处理修饰符
        for modifier in &field.modifiers {
            match modifier.as_str() {
                "static" => field_id = ctx.builder.extension("static", vec![field_id], loc),
                "readonly" => field_id = ctx.builder.extension("readonly", vec![field_id], loc),
                "public" | "private" | "protected" | "internal" => {
                    let modifier_id = ctx.builder.symbol(modifier, loc);
                    field_id = ctx.builder.extension("visibility", vec![modifier_id, field_id], loc);
                }
                _ => {}
            }
        }

        Ok(ctx.builder.export(&field.name, field_id, loc))
    }

    fn translate_property<A: chomsky_uir::Analysis<IKun>>(
        &self,
        prop: &PropertyDeclaration,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let loc = Loc::unknown();
        let mut accessors = Vec::new();
        if let Some(get) = &prop.get_accessor {
            let body_id = if let Some(body) = &get.body {
                self.translate_block(body, ctx)?
            } else {
                ctx.builder.seq(vec![], loc)
            };
            let get_id = ctx.builder.lambda(vec![], body_id, loc);
            accessors.push(ctx.builder.export(&format!("get_{}", prop.name), get_id, loc));
        }
        if let Some(set) = &prop.set_accessor {
            let body_id = if let Some(body) = &set.body {
                self.translate_block(body, ctx)?
            } else {
                ctx.builder.seq(vec![], loc)
            };
            let set_id = ctx.builder.lambda(vec!["value".to_string()], body_id, loc);
            accessors.push(ctx.builder.export(&format!("set_{}", prop.name), set_id, loc));
        }
        Ok(ctx.builder.module(&prop.name, accessors, loc))
    }

    fn translate_indexer<A: chomsky_uir::Analysis<IKun>>(
        &self,
        indexer: &IndexerDeclaration,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let loc = Loc::unknown();
        let mut accessors = Vec::new();
        let mut params = Vec::new();
        for p in &indexer.parameters {
            params.push(p.name.clone());
        }

        if let Some(get) = &indexer.get_accessor {
            let body_id = if let Some(body) = &get.body {
                self.translate_block(body, ctx)?
            } else {
                ctx.builder.seq(vec![], loc)
            };
            let get_id = ctx.builder.lambda(params.clone(), body_id, loc);
            accessors.push(ctx.builder.export("get_Item", get_id, loc));
        }
        if let Some(set) = &indexer.set_accessor {
            let body_id = if let Some(body) = &set.body {
                self.translate_block(body, ctx)?
            } else {
                ctx.builder.seq(vec![], loc)
            };
            let mut set_params = params.clone();
            set_params.push("value".to_string());
            let set_id = ctx.builder.lambda(set_params, body_id, loc);
            accessors.push(ctx.builder.export("set_Item", set_id, loc));
        }
        Ok(ctx.builder.module("Indexer", accessors, loc))
    }

    fn translate_event<A: chomsky_uir::Analysis<IKun>>(
        &self,
        event: &EventDeclaration,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let loc = Loc::unknown();
        let name_id = ctx.builder.symbol(&event.name, loc);
        Ok(ctx.builder.export(&event.name, name_id, loc))
    }

    fn translate_method<A: chomsky_uir::Analysis<IKun>>(
        &self,
        method: &MethodDeclaration,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let loc = Loc::unknown();
        let mut stmts = Vec::new();

        // 处理特性的翻译
        for attr in &method.attributes {
            stmts.push(self.translate_attribute(attr, ctx)?);
        }

        let body_id = if let Some(body) = &method.body {
            self.translate_block(body, ctx)?
        } else {
            ctx.builder.seq(vec![], loc)
        };

        let mut params = Vec::new();
        for param in &method.parameters {
            params.push(param.name.clone());
        }

        let mut lambda = ctx.builder.lambda(params, body_id, loc);

        // 处理泛型参数
        if !method.type_parameters.is_empty() {
            let mut type_params = Vec::new();
            for tp in &method.type_parameters {
                type_params.push(self.translate_type_parameter(tp, ctx)?);
            }
            lambda = ctx.builder.extension("generic_method", vec![lambda], loc);
        }

        // 处理修饰符
        for modifier in &method.modifiers {
            match modifier.as_str() {
                "async" => lambda = ctx.builder.extension("async", vec![lambda], loc),
                "static" => lambda = ctx.builder.extension("static", vec![lambda], loc),
                "public" | "private" | "protected" | "internal" => {
                    let modifier_id = ctx.builder.symbol(modifier, loc);
                    lambda = ctx.builder.extension("visibility", vec![modifier_id, lambda], loc);
                }
                _ => {}
            }
        }

        Ok(ctx.builder.export(&method.name, lambda, loc))
    }

    fn translate_block<A: chomsky_uir::Analysis<IKun>>(
        &self,
        statements: &[Statement],
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let mut stmts = Vec::new();
        for stmt in statements {
            stmts.push(self.translate_stmt(stmt, ctx)?);
        }
        Ok(ctx.builder.block(stmts, Loc::unknown()))
    }

    fn translate_stmt<A: chomsky_uir::Analysis<IKun>>(
        &self,
        stmt: &Statement,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let loc = Loc::unknown();
        match stmt {
            Statement::Expression(expr) => self.translate_expr(expr, ctx),
            Statement::Return(expr) => {
                let expr_id = if let Some(e) = expr {
                    self.translate_expr(e, ctx)?
                } else {
                    ctx.builder.constant(0, loc)
                };
                Ok(ctx.builder.extension("return", vec![expr_id], loc))
            }
            Statement::Block(stmts) => self.translate_block(stmts, ctx),
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond_id = self.translate_expr(condition, ctx)?;
                let then_id = self.translate_stmt(then_branch, ctx)?;
                let else_id = if let Some(eb) = else_branch {
                    self.translate_stmt(eb, ctx)?
                } else {
                    ctx.builder.seq(vec![], loc)
                };
                Ok(ctx.builder.extension("if", vec![cond_id, then_id, else_id], loc))
            }
            Statement::While { condition, body } => {
                let cond_id = self.translate_expr(condition, ctx)?;
                let body_id = self.translate_stmt(body, ctx)?;
                Ok(ctx.builder.extension("while", vec![cond_id, body_id], loc))
            }
            Statement::For {
                init,
                condition,
                update,
                body,
            } => {
                let mut stmts = Vec::new();
                if let Some(i) = init {
                    stmts.push(self.translate_stmt(i, ctx)?);
                }
                let cond_id = if let Some(c) = condition {
                    self.translate_expr(c, ctx)?
                } else {
                    ctx.builder.constant(1, loc) // true
                };
                let body_id = self.translate_stmt(body, ctx)?;
                let update_id = if let Some(u) = update {
                    self.translate_expr(u, ctx)?
                } else {
                    ctx.builder.seq(vec![], loc)
                };
                let loop_id = ctx.builder.extension("for", vec![cond_id, body_id, update_id], loc);
                stmts.push(loop_id);
                Ok(ctx.builder.seq(stmts, loc))
            }
            Statement::Foreach {
                item_type: _,
                item_name,
                iterable,
                body,
            } => {
                let iter_id = self.translate_expr(iterable, ctx)?;
                let item_id = ctx.builder.symbol(item_name, loc);
                let body_id = self.translate_stmt(body, ctx)?;
                Ok(ctx.builder.extension("foreach", vec![iter_id, item_id, body_id], loc))
            }
            Statement::LocalVariable {
                r#type: _,
                name,
                initializer,
            } => {
                let init_id = if let Some(init) = initializer {
                    self.translate_expr(init, ctx)?
                } else {
                    ctx.builder.constant(0, loc)
                };
                Ok(ctx.builder.export(name, init_id, loc))
            }
            Statement::Break => Ok(ctx.builder.extension("break", vec![], loc)),
            Statement::Continue => Ok(ctx.builder.extension("continue", vec![], loc)),
        }
    }

    fn translate_expr<A: chomsky_uir::Analysis<IKun>>(
        &self,
        expr: &Expression,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let loc = Loc::unknown();
        match expr {
            Expression::Literal(lit) => match lit {
                Literal::Integer(n) => Ok(ctx.builder.constant(*n, loc)),
                Literal::String(s) => Ok(ctx.builder.string(s, loc)),
                Literal::Boolean(b) => Ok(ctx.builder.bool(*b, loc)),
                Literal::Null => Ok(ctx.builder.constant(0, loc)),
            },
            Expression::Identifier(id) => Ok(ctx.builder.symbol(id, loc)),
            Expression::MethodCall(call) => {
                let mut args = Vec::new();
                for arg in &call.arguments {
                    args.push(self.translate_expr(arg, ctx)?);
                }
                if let Some(target) = &call.target {
                    if let Expression::Identifier(target_name) = target.as_ref() {
                        if let Some(builtin) = ctx.resolve_builtin(target_name, &call.name, args.clone(), loc) {
                            return Ok(builtin);
                        }
                    }
                    let target_id = self.translate_expr(target, ctx)?;
                    Ok(ctx.builder.call(target_id, args, loc))
                } else {
                    let name_id = ctx.builder.symbol(&call.name, loc);
                    Ok(ctx.builder.call(name_id, args, loc))
                }
            }
            Expression::MemberAccess(access) => {
                let target_id = self.translate_expr(&access.target, ctx)?;
                let name_id = ctx.builder.symbol(&access.name, loc);
                Ok(ctx.builder.extension("get_member", vec![target_id, name_id], loc))
            }
            Expression::ElementAccess(access) => {
                let target_id = self.translate_expr(&access.target, ctx)?;
                let mut args = Vec::new();
                for arg in &access.arguments {
                    args.push(self.translate_expr(arg, ctx)?);
                }
                Ok(ctx.builder.call(target_id, args, loc))
            }
            Expression::New(new_expr) => {
                let mut args = Vec::new();
                for arg in &new_expr.arguments {
                    args.push(self.translate_expr(arg, ctx)?);
                }
                Ok(ctx.builder.cross_lang_call("nyar", "new", &new_expr.r#type, args, loc))
            }
            Expression::This => Ok(ctx.builder.symbol("this", loc)),
            Expression::Base => Ok(ctx.builder.symbol("base", loc)),
            Expression::Binary { left, op, right } => {
                let left_id = self.translate_expr(left, ctx)?;
                let right_id = self.translate_expr(right, ctx)?;
                Ok(ctx.builder.binary_op(op, left_id, right_id, loc))
            }
            Expression::Unary { op, expression } => {
                let expr_id = self.translate_expr(expression, ctx)?;
                Ok(ctx.builder.extension(op, vec![expr_id], loc))
            }
            Expression::Assignment { left, op, right } => {
                let left_id = self.translate_expr(left, ctx)?;
                let right_id = self.translate_expr(right, ctx)?;
                Ok(ctx.builder.binary_op(op, left_id, right_id, loc))
            }
            Expression::Await(expr) => {
                let expr_id = self.translate_expr(expr, ctx)?;
                Ok(ctx.builder.extension("await", vec![expr_id], loc))
            }
            Expression::Query(query) => self.translate_query(query, ctx),
        }
    }

    fn translate_query<A: chomsky_uir::Analysis<IKun>>(
        &self,
        query: &QueryExpression,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let loc = Loc::unknown();
        // LINQ 翻译通常将其转换为方法链调用
        // 例如: from x in source where x > 0 select x
        // 翻译为: source.Where(x => x > 0).Select(x => x)

        let mut current_id = self.translate_expr(&query.from_clause.expression, ctx)?;
        let _var_name = &query.from_clause.identifier;

        // 处理查询主体
        for clause in &query.body.clauses {
            match clause {
                QueryClause::Where(expr) => {
                    let body_id = self.translate_expr(expr, ctx)?;
                    let lambda_id = ctx.builder.lambda(
                        vec![query.from_clause.identifier.clone()],
                        body_id,
                        loc,
                    );
                    let name_id = ctx.builder.symbol("Where", loc);
                    let member_id = ctx.builder.extension("get_member", vec![current_id, name_id], loc);
                    current_id = ctx.builder.call(
                        member_id,
                        vec![lambda_id],
                        loc,
                    );
                }
                _ => {
                    // TODO: 其他 LINQ 子句
                }
            }
        }

        // 处理最终的 select 或 group
        match &query.body.select_or_group {
            SelectOrGroupClause::Select(expr) => {
                let body_id = self.translate_expr(expr, ctx)?;
                let lambda_id = ctx.builder.lambda(
                    vec![query.from_clause.identifier.clone()],
                    body_id,
                    loc,
                );
                let name_id = ctx.builder.symbol("Select", loc);
                let member_id = ctx.builder.extension("get_member", vec![current_id, name_id], loc);
                current_id = ctx.builder.call(
                    member_id,
                    vec![lambda_id],
                    loc,
                );
            }
            SelectOrGroupClause::Group {
                expression: _,
                by_expression: _,
            } => {
                // TODO: GroupBy
            }
        }

        Ok(current_id)
    }
}
