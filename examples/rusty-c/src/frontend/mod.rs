pub mod preprocessor;

use nyar_types::{Id, NyarContext, NyarError, NyarFrontend, Vfs};
use oak_c::ast::CRoot;
use oak_c::builder::CBuilder;
use oak_c::language::CLanguage;
use oak_core::source::SourceText;
use oak_core::errors::OakErrorKind;
use std::collections::HashMap;
use std::ops::Range;
use chomsky_types::Loc;
use chomsky_uir::{Analysis, IKun, egraph::HasDebugInfo};

use std::path::PathBuf;

/// Rusty C 前端实现
#[derive(Default)]
pub struct RustyCFrontend {
    language: CLanguage,
    pub include_paths: Vec<PathBuf>,
}

impl RustyCFrontend {
    /// 创建一个新的 Rusty C 前端
    pub fn new() -> Self {
        Self {
            language: CLanguage::default(),
            include_paths: Vec::new(),
        }
    }

    /// 添加包含路径
    pub fn add_include_path(&mut self, path: PathBuf) {
        self.include_paths.push(path);
    }
}

impl<A: Analysis<IKun> + 'static> NyarFrontend<A, CRoot> for RustyCFrontend 
where A::Data: HasDebugInfo
{
    type Language = CLanguage;

    fn parse(&self, source: &str) -> Result<CRoot, NyarError> {
        let mut preprocessor = preprocessor::Preprocessor::new();
        for path in &self.include_paths {
            preprocessor.add_include_path(path);
        }
        
        // Use current directory as base for includes
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let preprocessed = preprocessor.process(source, &current_dir)
            .map_err(|e| NyarError::Compile(format!("Preprocessor error: {}", e)))?;

        use oak_core::Builder;
        let builder = CBuilder::new(&self.language);
        let mut session = oak_core::parser::ParseSession::<CLanguage>::default();
        let source_text = SourceText::new(preprocessed);
        let output = builder.build(&source_text, &[], &mut session);
        output.result.map_err(|e| match e.kind() {
            OakErrorKind::SyntaxError { message, .. } => NyarError::Parse(message.clone()),
            _ => NyarError::Compile(format!("{:?}", e)),
        })
    }

    fn lower_unified<V: Vfs>(&self, ast: &CRoot, ctx: &mut NyarContext<'_, V, A>) -> Id {
        let mut converter = UirConverter::new(ctx);
        converter.convert_root(ast)
    }
}

struct UirConverter<'a, 'b, V: Vfs, A: Analysis<IKun>> {
    ctx: &'a mut NyarContext<'b, V, A>,
    typedefs: HashMap<String, Vec<ast::DeclarationSpecifier>>,
    structs: HashMap<String, Vec<ast::StructDeclaration>>,
    enums: HashMap<String, Vec<ast::Enumerator>>,
}

impl<'a, 'b, V: Vfs, A: Analysis<IKun>> UirConverter<'a, 'b, V, A> {
    fn new(ctx: &'a mut NyarContext<'b, V, A>) -> Self {
        Self {
            ctx,
            typedefs: HashMap::new(),
            structs: HashMap::new(),
            enums: HashMap::new(),
        }
    }

    fn to_loc(&self, _range: Range<usize>) -> Loc {
        Loc::default() // TODO: Fix location mapping
    }

    fn convert_root(&mut self, root: &CRoot) -> Id {
        let mut items = Vec::new();
        for decl in &root.translation_unit.external_declarations {
            items.push(self.convert_external_declaration(decl));
        }
        self.ctx.builder().module("main", items, Loc::default())
    }

    fn convert_external_declaration(&mut self, decl: &ast::ExternalDeclaration) -> Id {
        match decl {
            ast::ExternalDeclaration::FunctionDefinition(func) => {
                self.convert_function_definition(func)
            }
            ast::ExternalDeclaration::Declaration(decl) => self.convert_declaration(decl),
        }
    }

    fn convert_function_definition(&mut self, func: &ast::FunctionDefinition) -> Id {
        let loc = self.to_loc(func.span.clone().into());
        let name = self.get_declarator_name(&func.declarator);

        let mut params = Vec::new();
        if let ast::DirectDeclarator::Function {
            parameter_list,
            ..
        } = &func.declarator.direct_declarator
        {
            for param in &parameter_list.parameter_declarations {
                if let Some(decl) = &param.declarator {
                    let original_name = self.get_declarator_name(decl);
                    params.push(original_name);
                }
            }
        }

        let mut body_ids = Vec::new();
        for item in &func.compound_statement.block_items {
            body_ids.push(self.convert_block_item(item));
        }

        let body_block = self.ctx.builder().block(body_ids, loc.clone());
        let lambda = self.ctx.builder().lambda(params, body_block, loc.clone());
        self.ctx.builder().export(&name, lambda, loc)
    }

    fn convert_declaration(&mut self, decl: &ast::Declaration) -> Id {
        let loc = self.to_loc(decl.span.clone().into());

        // Handle typedef
        let is_typedef = decl.declaration_specifiers.iter().any(|s| {
            matches!(
                s,
                ast::DeclarationSpecifier::StorageClassSpecifier(ast::StorageClassSpecifier::Typedef { .. })
            )
        });

        // Handle struct/union/enum definitions in specifiers
        for spec in &decl.declaration_specifiers {
            if let ast::DeclarationSpecifier::TypeSpecifier(ts) = spec {
                match ts {
                    ast::TypeSpecifier::StructOrUnion(s) => {
                        if let Some(name) = &s.identifier {
                            if !s.struct_declarations.is_empty() {
                                self.structs.insert(name.clone(), s.struct_declarations.clone());
                            }
                        }
                    }
                    ast::TypeSpecifier::Enum(e) => {
                        if let Some(name) = &e.identifier {
                            if !e.enumerators.is_empty() {
                                self.enums.insert(name.clone(), e.enumerators.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if is_typedef {
            for init in &decl.init_declarators {
                let name = self.get_declarator_name(&init.declarator);
                self.typedefs.insert(name, decl.declaration_specifiers.clone());
            }
            return self.ctx.builder().constant(0, loc);
        }

        let mut ids = Vec::new();
        for init in &decl.init_declarators {
            let original_name = self.get_declarator_name(&init.declarator);

            // Skip function prototypes
            if let ast::DirectDeclarator::Function { .. } = &init.declarator.direct_declarator {
                continue;
            }

            let value = if let Some(init_val) = &init.initializer {
                self.convert_initializer(init_val)
            } else {
                self.ctx.builder().constant(0, loc.clone())
            };
            ids.push(self.ctx.builder().assign(&original_name, value, loc.clone()));
        }
        if ids.is_empty() {
            self.ctx.builder().constant(0, loc)
        } else if ids.len() == 1 {
            ids[0]
        } else {
            self.ctx.builder().block(ids, loc)
        }
    }

    fn convert_block_item(&mut self, item: &ast::BlockItem) -> Id {
        match item {
            ast::BlockItem::Declaration(decl) => self.convert_declaration(decl),
            ast::BlockItem::Statement(stmt) => self.convert_statement(stmt),
        }
    }

    fn convert_statement(&mut self, stmt: &ast::Statement) -> Id {
        let loc = self.to_loc(stmt.span().into());
        match stmt {
            ast::Statement::Compound(comp) => {
                let mut ids = Vec::new();
                for item in &comp.block_items {
                    ids.push(self.convert_block_item(item));
                }
                self.ctx.builder().block(ids, loc)
            }
            ast::Statement::Expression(expr_stmt) => {
                if let Some(expr) = &expr_stmt.expression {
                    self.convert_expression(expr)
                } else {
                    let loc = self.to_loc(expr_stmt.span.clone().into());
                    self.ctx.builder().constant(0, loc)
                }
            }
            ast::Statement::Selection(sel) => match sel {
                ast::SelectionStatement::If {
                    condition,
                    then_statement,
                    else_statement,
                    span,
                } => {
                    let cond = self.convert_expression(condition);
                    let then_id = self.convert_statement(then_statement);
                    let else_id = if let Some(e) = else_statement {
                        self.convert_statement(e)
                    } else {
                        self.ctx.builder().constant(0, loc.clone())
                    };
                    let loc = self.to_loc(span.clone().into());
                    self.ctx.builder().branch(cond, then_id, else_id, loc)
                }
                ast::SelectionStatement::Switch {
                    expression,
                    statement,
                    span,
                } => {
                    let cond = self.convert_expression(expression);
                    let body = self.convert_statement(statement);
                    let loc = self.to_loc(span.clone().into());
                    self.ctx.builder().extension("switch", vec![cond, body], loc)
                }
            },
            ast::Statement::Iteration(iter) => match iter {
                ast::IterationStatement::While {
                    condition,
                    statement,
                    span,
                } => {
                    let cond = self.convert_expression(condition);
                    let body = self.convert_statement(statement);
                    let loc = self.to_loc(span.clone().into());
                    self.ctx.builder().while_loop(cond, body, loc)
                }
                ast::IterationStatement::DoWhile {
                    statement,
                    condition,
                    span,
                } => {
                    let body = self.convert_statement(statement);
                    let cond = self.convert_expression(condition);
                    let loc = self.to_loc(span.clone().into());
                    self.ctx.builder().extension("do_while", vec![body, cond], loc)
                }
                ast::IterationStatement::For {
                    init,
                    condition,
                    update,
                    statement,
                    span,
                } => {
                    let i = if let Some(e) = init {
                        self.convert_expression(e)
                    } else {
                        self.ctx.builder().constant(0, loc.clone())
                    };
                    let c = if let Some(e) = condition {
                        self.convert_expression(e)
                    } else {
                        self.ctx.builder().constant(1, loc.clone())
                    };
                    let u = if let Some(e) = update {
                        self.convert_expression(e)
                    } else {
                        self.ctx.builder().constant(0, loc.clone())
                    };
                    let body = self.convert_statement(statement);
                    let loc = self.to_loc(span.clone().into());
                    self.ctx.builder().extension("for", vec![i, c, u, body], loc)
                }
            },
            ast::Statement::Jump(jump) => match jump {
                ast::JumpStatement::Goto(identifier, span) => {
                    let loc = self.to_loc(span.clone().into());
                    let name = self.ctx.builder().string(identifier, loc.clone());
                    self.ctx.builder().extension("goto", vec![name], loc)
                }
                ast::JumpStatement::Continue(span) => {
                    let loc = self.to_loc(span.clone().into());
                    self.ctx.builder().extension("continue", vec![], loc)
                }
                ast::JumpStatement::Break(span) => {
                    let loc = self.to_loc(span.clone().into());
                    self.ctx.builder().break_(loc)
                }
                ast::JumpStatement::Return(expression, span) => {
                    let loc = self.to_loc(span.clone().into());
                    let val = if let Some(expr) = expression {
                        self.convert_expression(expr)
                    } else {
                        self.ctx.builder().constant(0, loc.clone())
                    };
                    self.ctx.builder().return_(val, loc)
                }
            },
            _ => self.ctx.builder().constant(0, loc),
        }
    }

    fn convert_expression(&mut self, expr: &ast::Expression) -> Id {
        let loc = self.to_loc(expr.span.clone().into());
        match &*expr.kind {
            ast::ExpressionKind::Constant(c, _) => match c {
                ast::Constant::Integer(val, _) => self.ctx.builder().constant(*val, loc),
                ast::Constant::Float(val, _) => self.ctx.builder().float(*val, loc),
                ast::Constant::Character(val, _) => self.ctx.builder().constant(*val as i64, loc),
            },
            ast::ExpressionKind::StringLiteral(val, _) => self.ctx.builder().string(val.as_str(), loc),
            ast::ExpressionKind::Identifier(name, _) => {
                self.ctx.builder().symbol(name, loc)
            }
            ast::ExpressionKind::ArraySubscript { array, index, .. } => {
                let a = self.convert_expression(array);
                let i = self.convert_expression(index);
                self.ctx.builder().extension("index", vec![a, i], loc)
            }
            ast::ExpressionKind::MemberAccess {
                object,
                member,
                is_pointer,
                ..
            } => {
                let obj = self.convert_expression(object);
                let op = if *is_pointer { "arrow" } else { "dot" };
                let member_id = self.ctx.builder().string(member.as_str(), loc.clone());
                self.ctx.builder().extension(op, vec![obj, member_id], loc)
            }
            ast::ExpressionKind::Binary {
                left,
                operator,
                right,
                ..
            } => {
                let l = self.convert_expression(left);
                let r = self.convert_expression(right);
                let op = match operator {
                    ast::BinaryOperator::Add => "add",
                    ast::BinaryOperator::Subtract => "sub",
                    ast::BinaryOperator::Multiply => "mul",
                    ast::BinaryOperator::Divide => "div",
                    ast::BinaryOperator::Modulo => "mod",
                    ast::BinaryOperator::BitAnd => "bit_and",
                    ast::BinaryOperator::BitOr => "bit_or",
                    ast::BinaryOperator::BitXor => "bit_xor",
                    ast::BinaryOperator::ShiftLeft => "shl",
                    ast::BinaryOperator::ShiftRight => "shr",
                    ast::BinaryOperator::Equal => "eq",
                    ast::BinaryOperator::NotEqual => "ne",
                    ast::BinaryOperator::Less => "lt",
                    ast::BinaryOperator::LessEqual => "le",
                    ast::BinaryOperator::Greater => "gt",
                    ast::BinaryOperator::GreaterEqual => "ge",
                    ast::BinaryOperator::LogicalAnd => "and",
                    ast::BinaryOperator::LogicalOr => "or",
                };
                self.ctx.builder().binary_op(op, l, r, loc)
            }
            ast::ExpressionKind::Unary {
                operator, operand, ..
            } => {
                let arg = self.convert_expression(operand);
                let op = match operator {
                    ast::UnaryOperator::AddressOf => "address_of",
                    ast::UnaryOperator::Indirection => "deref",
                    ast::UnaryOperator::Plus => "pos",
                    ast::UnaryOperator::Minus => "neg",
                    ast::UnaryOperator::BitNot => "bit_not",
                    ast::UnaryOperator::LogicalNot => "not",
                    ast::UnaryOperator::Sizeof => "sizeof",
                };
                self.ctx.builder().extension(op, vec![arg], loc)
            }
            ast::ExpressionKind::PostfixIncDec {
                operand,
                is_increment,
                ..
            } => {
                let arg = self.convert_expression(operand);
                let op = if *is_increment { "post_inc" } else { "post_dec" };
                self.ctx.builder().extension(op, vec![arg], loc)
            }
            ast::ExpressionKind::PrefixIncDec {
                operand,
                is_increment,
                ..
            } => {
                let arg = self.convert_expression(operand);
                let op = if *is_increment { "pre_inc" } else { "pre_dec" };
                self.ctx.builder().extension(op, vec![arg], loc)
            }
            ast::ExpressionKind::Cast {
                type_name,
                expression,
                ..
            } => {
                let val = self.convert_expression(expression);
                let ty = self.convert_type_name(type_name);
                let type_id = self.ctx.builder().string(&ty, loc.clone());
                self.ctx.builder().extension("cast", vec![val, type_id], loc)
            }
            ast::ExpressionKind::Conditional {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                let cond = self.convert_expression(condition);
                let t = self.convert_expression(then_expr);
                let e = self.convert_expression(else_expr);
                self.ctx.builder().branch(cond, t, e, loc)
            }
            ast::ExpressionKind::Assignment {
                left,
                operator,
                right,
                ..
            } => {
                let r = self.convert_expression(right);
                let l = self.convert_expression(left);
                match operator {
                    ast::AssignmentOperator::Assign => self.ctx.builder().assign_to_id(l, r, loc),
                    _ => {
                        let op = match operator {
                            ast::AssignmentOperator::AddAssign => "add",
                            ast::AssignmentOperator::SubAssign => "sub",
                            ast::AssignmentOperator::MulAssign => "mul",
                            ast::AssignmentOperator::DivAssign => "div",
                            ast::AssignmentOperator::ModAssign => "mod",
                            ast::AssignmentOperator::AndAssign => "bit_and",
                            ast::AssignmentOperator::OrAssign => "bit_or",
                            ast::AssignmentOperator::XorAssign => "bit_xor",
                            ast::AssignmentOperator::ShlAssign => "shl",
                            ast::AssignmentOperator::ShrAssign => "shr",
                            ast::AssignmentOperator::Assign => unreachable!(),
                        };
                        let value = self.ctx.builder().binary_op(op, l, r, loc.clone());
                        self.ctx.builder().assign_to_id(l, value, loc)
                    }
                }
            }
            ast::ExpressionKind::Comma { expressions, .. } => {
                let mut ids = Vec::new();
                for expr in expressions {
                    ids.push(self.convert_expression(expr));
                }
                self.ctx.builder().block(ids, loc)
            }
            ast::ExpressionKind::FunctionCall {
                function,
                arguments,
                ..
            } => {
                let mut args = Vec::new();
                for arg in arguments {
                    args.push(self.convert_expression(arg));
                }

                if let ast::ExpressionKind::Identifier(name, _) = &*function.kind {
                    match name.as_str() {
                        "printf" | "print" | "puts" => {
                            return self.ctx.builder().cross_lang_call("nyar", "io", "print", args, loc);
                        }
                        "println" => {
                            return self.ctx.builder().cross_lang_call("nyar", "io", "println", args, loc);
                        }
                        "exit" => {
                            return self.ctx.builder().cross_lang_call("nyar", "std", "exit", args, loc);
                        }
                        "panic" | "abort" => {
                            return self.ctx.builder().cross_lang_call("nyar", "std", "panic", args, loc);
                        }
                        "malloc" => {
                            return self.ctx.builder().cross_lang_call("nyar", "mem", "alloc", args, loc);
                        }
                        "free" => {
                            return self.ctx.builder().cross_lang_call("nyar", "mem", "free", args, loc);
                        }
                        "realloc" => {
                            return self.ctx.builder().cross_lang_call("nyar", "mem", "realloc", args, loc);
                        }
                        "memset" => {
                            return self.ctx.builder().cross_lang_call("nyar", "mem", "set", args, loc);
                        }
                        "memcpy" => {
                            return self.ctx.builder().cross_lang_call("nyar", "mem", "copy", args, loc);
                        }
                        "strlen" => {
                            return self.ctx.builder().cross_lang_call("nyar", "str", "len", args, loc);
                        }
                        "strcmp" => {
                            return self.ctx.builder().cross_lang_call("nyar", "str", "cmp", args, loc);
                        }
                        "sin" => {
                            return self.ctx.builder().cross_lang_call("nyar", "math", "sin", args, loc);
                        }
                        "cos" => {
                            return self.ctx.builder().cross_lang_call("nyar", "math", "cos", args, loc);
                        }
                        "tan" => {
                            return self.ctx.builder().cross_lang_call("nyar", "math", "tan", args, loc);
                        }
                        "sqrt" => {
                            return self.ctx.builder().cross_lang_call("nyar", "math", "sqrt", args, loc);
                        }
                        "abs" => {
                            return self.ctx.builder().cross_lang_call("nyar", "math", "abs", args, loc);
                        }
                        "rand" => {
                            return self.ctx.builder().cross_lang_call("nyar", "math", "rand", args, loc);
                        }
                        "time" => {
                            return self.ctx.builder().cross_lang_call("nyar", "time", "now", args, loc);
                        }
                        "sleep" => {
                            return self.ctx.builder().cross_lang_call("nyar", "time", "sleep", args, loc);
                        }
                        _ => {}
                    }
                }

                let func_id = self.convert_expression(function);
                self.ctx.builder().call(func_id, args, loc)
            }
        }
    }

    fn convert_type_name(&self, type_name: &ast::TypeName) -> String {
        let mut res = String::new();
        for spec in &type_name.specifier_qualifiers {
            match spec {
                ast::SpecifierQualifier::TypeSpecifier(ts) => {
                    res.push_str(&self.convert_type_specifier(ts));
                }
                ast::SpecifierQualifier::TypeQualifier(tq) => {
                    match tq {
                        ast::TypeQualifier::Const { .. } => res.push_str("const "),
                        ast::TypeQualifier::Restrict { .. } => res.push_str("restrict "),
                        ast::TypeQualifier::Volatile { .. } => res.push_str("volatile "),
                    }
                }
            }
        }
        if let Some(decl) = &type_name.abstract_declarator {
            res.push_str(&self.convert_abstract_declarator(decl));
        }
        res.trim().to_string()
    }

    fn convert_type_specifier(&self, ts: &ast::TypeSpecifier) -> String {
        match ts {
            ast::TypeSpecifier::Void { .. } => "void ".to_string(),
            ast::TypeSpecifier::Char { .. } => "char ".to_string(),
            ast::TypeSpecifier::Short { .. } => "short ".to_string(),
            ast::TypeSpecifier::Int { .. } => "int ".to_string(),
            ast::TypeSpecifier::Long { .. } => "long ".to_string(),
            ast::TypeSpecifier::Float { .. } => "float ".to_string(),
            ast::TypeSpecifier::Double { .. } => "double ".to_string(),
            ast::TypeSpecifier::Signed { .. } => "signed ".to_string(),
            ast::TypeSpecifier::Unsigned { .. } => "unsigned ".to_string(),
            ast::TypeSpecifier::Bool { .. } => "_Bool ".to_string(),
            ast::TypeSpecifier::Complex { .. } => "_Complex ".to_string(),
            ast::TypeSpecifier::Imaginary { .. } => "_Imaginary ".to_string(),
            ast::TypeSpecifier::TypedefName(name, _) => format!("{} ", name),
            ast::TypeSpecifier::StructOrUnion(s) => {
                let kind = match s.kind {
                    ast::StructOrUnion::Struct { .. } => "struct",
                    ast::StructOrUnion::Union { .. } => "union",
                };
                if let Some(id) = &s.identifier {
                    format!("{} {} ", kind, id)
                } else {
                    format!("{} <anonymous> ", kind)
                }
            }
            ast::TypeSpecifier::Enum(e) => {
                if let Some(id) = &e.identifier {
                    format!("enum {} ", id)
                } else {
                    "enum <anonymous> ".to_string()
                }
            }
        }
    }

    fn convert_abstract_declarator(&self, decl: &ast::AbstractDeclarator) -> String {
        let mut res = String::new();
        if let Some(p) = &decl.pointer {
            res.push_str(&self.convert_pointer(p));
        }
        if let Some(d) = &decl.direct_abstract_declarator {
            res.push_str(&self.convert_direct_abstract_declarator(d));
        }
        res
    }

    fn convert_pointer(&self, p: &ast::Pointer) -> String {
        let mut res = "*".to_string();
        for q in &p.type_qualifiers {
            match q {
                ast::TypeQualifier::Const { .. } => res.push_str("const "),
                ast::TypeQualifier::Restrict { .. } => res.push_str("restrict "),
                ast::TypeQualifier::Volatile { .. } => res.push_str("volatile "),
            }
        }
        if let Some(inner) = &p.pointer {
            res.push_str(&self.convert_pointer(inner));
        }
        res
    }

    fn convert_direct_abstract_declarator(&self, decl: &ast::DirectAbstractDeclarator) -> String {
        match decl {
            ast::DirectAbstractDeclarator::AbstractDeclarator(d) => {
                format!("({})", self.convert_abstract_declarator(d))
            }
            ast::DirectAbstractDeclarator::Array {
                declarator,
                assignment_expression,
                ..
            } => {
                let mut res = String::new();
                if let Some(d) = declarator {
                    res.push_str(&self.convert_direct_abstract_declarator(d));
                }
                res.push('[');
                if let Some(_expr) = assignment_expression {
                    // We don't have an easy way to stringify expression here without a lot of work
                    // Just put a placeholder or leave empty for now
                    res.push_str("...");
                }
                res.push(']');
                res
            }
            ast::DirectAbstractDeclarator::Function {
                declarator,
                parameter_list,
                ..
            } => {
                let mut res = String::new();
                if let Some(d) = declarator {
                    res.push_str(&self.convert_direct_abstract_declarator(d));
                }
                res.push('(');
                if let Some(_params) = parameter_list {
                    res.push_str("...");
                }
                res.push(')');
                res
            }
        }
    }

    fn convert_initializer(&mut self, init: &ast::Initializer) -> Id {
        match init {
            ast::Initializer::AssignmentExpression(expr) => self.convert_expression(expr),
            ast::Initializer::InitializerList(list, span) => {
                let loc = self.to_loc(span.clone().into());
                let mut ids = Vec::new();
                for item in list {
                    ids.push(self.convert_initializer(item));
                }
                self.ctx.builder().extension("array", ids, loc)
            }
        }
    }

    fn get_declarator_name(&self, decl: &ast::Declarator) -> String {
        self.get_direct_declarator_name(&decl.direct_declarator)
    }

    fn get_direct_declarator_name(&self, decl: &ast::DirectDeclarator) -> String {
        match decl {
            ast::DirectDeclarator::Identifier(name, _) => name.clone(),
            ast::DirectDeclarator::Declarator(decl, _) => self.get_declarator_name(decl),
            ast::DirectDeclarator::Array { direct_declarator, .. } => {
                self.get_direct_declarator_name(direct_declarator)
            }
            ast::DirectDeclarator::Function { direct_declarator, .. } => {
                self.get_direct_declarator_name(direct_declarator)
            }
        }
    }
}
