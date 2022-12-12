//! Rusty TypeScript 语言前端
//!
//! 这个库提供了 Rusty TypeScript 语言的解析和 Nyar 翻译功能。
//! 遵循 Project Chomsky Whitebook 规范。

#![feature(new_range_api)]

pub mod codegen;
pub mod errors;
pub mod project;
pub mod type_system;
pub mod runtime;

pub use crate::runtime::RustyTypeScriptRuntime;

use chomsky_cost;
use chomsky_emit::GaiaEmitter;
use chomsky_extract::{Backend, IKunExtractor};
use chomsky_types::Loc;
use chomsky_uir::{ConstraintAnalysis, EGraph, IKun, Id, IntentBuilder, IKunTree, Analysis};
use nyar_aot::{NyarContext, NyarFrontend};
use nyar_types::NyarError;
use nyar_vm::bytecode::format::NyarcModule;
use oak_core::{ParseSession, SourceText};
use oak_typescript::{ast, TypeScriptBuilder, TypeScriptLanguage, TypeScriptRoot};
use oak_vfs::Vfs;

/// Rusty TypeScript 前端
pub struct RustyTypescriptFrontend {
    language: TypeScriptLanguage,
    source_id: u32,
}

impl Default for RustyTypescriptFrontend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_arch = "wasm32")]
wit_bindgen::generate!({
    world: "compiler",
    path: "wit",
});

#[cfg(target_arch = "wasm32")]
struct Compiler;

#[cfg(target_arch = "wasm32")]
impl Guest for Compiler {
    fn compile(source: String) -> Result<Vec<u8>, String> {
        let frontend = RustyTypescriptFrontend::new();
        let artifacts = frontend.compile_to_wasm(&source)?;
        
        // Return main.wasm or the first artifact
        if let Some(wasm) = artifacts.get("main.wasm") {
            Ok(wasm.clone())
        } else if let Some((_, bytes)) = artifacts.iter().next() {
            Ok(bytes.clone())
        } else {
            Err("No output files generated".to_string())
        }
    }
}

#[cfg(target_arch = "wasm32")]
export!(Compiler);

impl RustyTypescriptFrontend {
    /// 创建新的前端实例
    pub fn new() -> Self {
        let mut language = TypeScriptLanguage::standard();
        language.decorators = true;
        language.jsx = true;
        Self {
            language,
            source_id: 1, // 默认 source_id
        }
    }

    /// 设置当前处理的源码 ID
    pub fn set_source_id(&mut self, id: u32) {
        self.source_id = id;
    }

    /// 编译源码为 WASM (AOT)
    pub fn compile_to_wasm(&self, source: &str) -> Result<std::collections::HashMap<String, Vec<u8>>, String> {
        let tree = self.lower_to_tree(source)?;

        let emitter = GaiaEmitter::new("wasm32-wasi").standalone();

        // 使用 AOT 编译器进行优化和生成
        // 由于 GaiaEmitter 实现了 Backend 接口，可以直接调用 generate
        let artifact = emitter.generate(&tree)
            .map_err(|e| format!("AOT error: {:?}", e))?;

        match artifact {
            chomsky_extract::BackendArtifact::Binary(bytes) => {
                let mut files = std::collections::HashMap::new();
                files.insert("main.wasm".to_string(), bytes);
                Ok(files)
            }
            chomsky_extract::BackendArtifact::Collection(files) => Ok(files),
            _ => Err("Expected binary artifact from AOT compiler".to_string()),
        }
    }

    fn lower_to_tree(&self, source: &str) -> Result<IKunTree, String> {
        let ast = self
            .parse(source)
            .map_err(|e| format!("Parse error: {:?}", e))?;
        let vfs = oak_vfs::MemoryVfs::new();
        self.lower(&ast, &vfs)
            .map_err(|e| format!("Lowering error: {:?}", e))
    }

    /// 编译源码为 Nyar 模块
    pub fn compile_to_nyar(&self, source: &str) -> Result<NyarcModule, String> {
        let tree = self.lower_to_tree(source)?;

        let mut egraph = EGraph::<IKun, ConstraintAnalysis>::new();
        let root_id = tree.to_egraph(&mut egraph);

        let mut translator = codegen::NyarTranslator::new();
        translator
            .generate(&egraph, root_id)
            .map_err(|e| format!("Codegen error: {:?}", e))
    }
}

impl NyarFrontend<()> for RustyTypescriptFrontend {
    type Language = TypeScriptLanguage;

    fn parse(&self, source: &str) -> Result<TypeScriptRoot, NyarError> {
        let builder = TypeScriptBuilder::new(&self.language);
        let mut session = ParseSession::<TypeScriptLanguage>::default();
        let source_text = SourceText::new(source);
        let output = oak_core::Builder::build(&builder, &source_text, &[], &mut session);

        output
            .result
            .map_err(|e| NyarError::Compile(format!("{:?}", e)))
    }

    fn lower_unified<V: Vfs>(&self, ast: &TypeScriptRoot, ctx: &mut NyarContext<V, ()>) -> Id {
        let source_id = self.source_id;
        let mut builder = ctx.builder();
        let mut converter = UirConverter::new(&mut builder, source_id);
        converter.convert_root(ast.clone())
    }

    fn lower<V: oak_vfs::Vfs>(&self, ast: &TypeScriptRoot, _vfs: &V) -> Result<IKunTree, NyarError> {
        let mut egraph = EGraph::<IKun, ConstraintAnalysis>::new();
        let mut builder = IntentBuilder::new(&mut egraph);
        let mut converter = UirConverter::new(&mut builder, self.source_id);

        let root_id = converter.convert_root(ast.clone());
        let extractor = IKunExtractor::new(&egraph, chomsky_cost::DEFAULT_COST_MODEL.clone());
        let tree = extractor.extract(root_id);
        Ok(tree)
    }
}

struct UirConverter<'a, A: Analysis<IKun> = ()> {
    builder: &'a mut IntentBuilder<'a, A>,
    source_id: u32,
}

impl<'a, A: Analysis<IKun>> UirConverter<'a, A> {
    fn new(builder: &'a mut IntentBuilder<'a, A>, source_id: u32) -> Self {
        Self { builder, source_id }
    }

    fn to_loc(&self, range: impl Into<std::ops::Range<usize>>) -> Loc {
        let range = range.into();
        Loc::new(self.source_id, range.start as u32, range.end as u32)
    }

    fn convert_root(&mut self, root: TypeScriptRoot) -> Id {
        let mut items = Vec::new();
        for stmt in root.statements {
            items.push(self.convert_statement(stmt));
        }
        self.builder.module("main", items, Loc::default())
    }

    fn convert_statement(&mut self, stmt: ast::Statement) -> Id {
        match stmt {
            ast::Statement::VariableDeclaration(var) => {
                if var.is_declare {
                    return self.builder.constant(0, self.to_loc(var.span));
                }
                let loc = self.to_loc(var.span);
                let value = if let Some(expr) = var.value {
                    self.convert_expression(expr)
                } else {
                    self.builder.constant(0, loc.clone())
                };
                let ty = var.ty.map(|t| self.convert_type_annotation(t, loc.clone()));
                let mut args = vec![self.builder.symbol(&var.name, loc.clone()), value];
                if let Some(ty_id) = ty {
                    args.push(ty_id);
                }
                args.push(self.builder.bool(var.is_declare, loc.clone()));
                self.builder.extension("assign", args, loc)
            }
            ast::Statement::FunctionDeclaration(func) => {
                if func.is_declare {
                    return self.builder.constant(0, self.to_loc(func.span.clone()));
                }
                let loc = self.to_loc(func.span.clone());
                let mut body_ids = Vec::new();
                for s in func.body {
                    body_ids.push(self.convert_statement(s));
                }

                let type_param_ids = func.type_params
                    .into_iter()
                    .map(|tp| self.convert_type_parameter(tp, loc.clone()))
                    .collect::<Vec<_>>();
                let type_params_seq = self.builder.seq(type_param_ids, loc.clone());

                let param_ids: Vec<_> = func
                    .params
                    .into_iter()
                    .map(|p| {
                        let p_loc = self.to_loc(p.span);
                        let mut p_args = vec![self.builder.symbol(&p.name, p_loc.clone())];
                        if let Some(ty) = p.ty {
                            p_args.push(self.convert_type_annotation(ty, p_loc.clone()));
                        } else {
                            p_args.push(self.builder.constant(0, p_loc.clone()));
                        }
                        p_args.push(self.builder.bool(p.optional, p_loc.clone()));
                        let mut param_id = self.builder.extension("param", p_args, p_loc.clone());

                        // Handle parameter decorators
                        for dec in p.decorators {
                            let dec_expr = self.convert_expression(dec.expression);
                            param_id = self.builder.call(dec_expr, vec![param_id], p_loc.clone());
                        }
                        param_id
                    })
                    .collect();
                let params_seq = self.builder.seq(param_ids, loc.clone());

                let mut lambda_args = vec![
                    self.builder.string(&func.name, loc.clone()),
                    type_params_seq,
                    params_seq,
                    self.builder.block(body_ids, loc.clone()),
                ];

                if let Some(ret) = func.return_type {
                    lambda_args.push(self.convert_type_annotation(ret, loc.clone()));
                }

                let mut lambda = self.builder.extension("function", lambda_args, loc.clone());

                // Handle function decorators
                for dec in func.decorators {
                    let dec_expr = self.convert_expression(dec.expression);
                    lambda = self.builder.call(dec_expr, vec![lambda], loc.clone());
                }

                let mut assign_args = vec![self.builder.symbol(&func.name, loc.clone()), lambda];
                assign_args.push(self.builder.bool(func.is_declare, loc.clone()));
                self.builder.extension("assign", assign_args, loc)
            }
            ast::Statement::ExpressionStatement(stmt) => self.convert_expression(stmt.expression),
            ast::Statement::ImportDeclaration(import) => {
                let loc = self.to_loc(import.span);
                let mut args = vec![self.builder.string(&import.module_specifier, loc.clone())];
                for s in import.specifiers {
                    match s {
                        ast::ImportSpecifier::Default(local) => {
                            args.push(self.builder.symbol(&local, loc.clone()));
                        }
                        ast::ImportSpecifier::Namespace(local) => {
                            args.push(self.builder.symbol(&local, loc.clone()));
                        }
                        ast::ImportSpecifier::Named { local, .. } => {
                            args.push(self.builder.symbol(&local, loc.clone()));
                        }
                    }
                }
                self.builder.extension("import", args, loc)
            }
            ast::Statement::ExportDeclaration(export) => {
                let loc = self.to_loc(export.span);
                if let Some(decl) = export.declaration {
                    let inner = self.convert_statement(*decl);
                    self.builder.extension("export", vec![inner], loc)
                } else {
                    self.builder.constant(0, loc)
                }
            }
            ast::Statement::ReturnStatement(stmt) => {
                let loc = self.to_loc(stmt.span);
                let val = if let Some(expr) = stmt.argument {
                    self.convert_expression(expr)
                } else {
                    self.builder.constant(0, loc.clone())
                };
                self.builder.return_(val, loc)
            }
            ast::Statement::ClassDeclaration(class) => {
                if class.is_declare {
                    return self.builder.constant(0, self.to_loc(class.span));
                }
                let loc = self.to_loc(class.span);
                let mut args = vec![self.builder.symbol(&class.name, loc.clone())];

                // Type parameters
                let type_param_ids: Vec<_> = class
                    .type_params
                    .into_iter()
                    .map(|tp| self.convert_type_parameter(tp, loc.clone()))
                    .collect();
                args.push(self.builder.seq(type_param_ids, loc.clone()));

                if let Some(ext) = class.extends {
                    args.push(self.convert_type_annotation(ext, loc.clone()));
                } else {
                    args.push(self.builder.constant(0, loc.clone())); // No base class
                }

                // Add abstract and implements
                args.push(self.builder.bool(class.is_abstract, loc.clone()));
                let implements_ids: Vec<_> = class
                    .implements
                    .into_iter()
                    .map(|imp| self.convert_type_annotation(imp, loc.clone()))
                    .collect();
                args.push(self.builder.seq(implements_ids, loc.clone()));

                for member in class.body {
                    args.push(self.convert_class_member(member, loc.clone()));
                }
                args.push(self.builder.bool(class.is_declare, loc.clone()));
                let mut class_id = self.builder.extension("gc.struct", args, loc.clone());

                // Handle class decorators
                for dec in class.decorators {
                    let dec_expr = self.convert_expression(dec.expression);
                    class_id = self.builder.call(dec_expr, vec![class_id], loc.clone());
                }
                class_id
            }
            ast::Statement::Namespace(ns) => {
                if ns.is_declare {
                    return self.builder.constant(0, self.to_loc(ns.span));
                }
                let loc = self.to_loc(ns.span);
                let mut items = Vec::new();
                for s in ns.body {
                    items.push(self.convert_statement(s));
                }
                let mut args = vec![self.builder.string(&ns.name, loc.clone())];
                args.push(self.builder.seq(items, loc.clone()));
                args.push(self.builder.bool(ns.is_declare, loc.clone()));
                self.builder.extension("module", args, loc)
            }
            ast::Statement::Interface(interface) => {
                if interface.is_declare {
                    return self.builder.constant(0, self.to_loc(interface.span));
                }
                let loc = self.to_loc(interface.span);
                let mut args = vec![self.builder.symbol(&interface.name, loc.clone())];

                // Type parameters
                let mut type_param_ids = Vec::new();
                for tp in interface.type_params {
                    type_param_ids.push(self.convert_type_parameter(tp, loc.clone()));
                }
                args.push(self.builder.seq(type_param_ids, loc.clone()));

                let mut extends_ids = Vec::new();
                for ext in interface.extends {
                    extends_ids.push(self.convert_type_annotation(ext, loc.clone()));
                }
                args.push(self.builder.seq(extends_ids, loc.clone()));

                for member in interface.body {
                    args.push(self.convert_class_member(member, loc.clone()));
                }
                args.push(self.builder.bool(interface.is_declare, loc.clone()));
                self.builder.extension("interface", args, loc)
            }
            ast::Statement::TypeAlias(alias) => {
                if alias.is_declare {
                    return self.builder.constant(0, self.to_loc(alias.span));
                }
                let loc = self.to_loc(alias.span);
                let name = self.builder.symbol(&alias.name, loc.clone());

                // Type parameters
                let mut type_param_ids = Vec::new();
                for tp in alias.type_params {
                    type_param_ids.push(self.convert_type_parameter(tp, loc.clone()));
                }
                let type_params = self.builder.seq(type_param_ids, loc.clone());

                let ty = self.convert_type_annotation(alias.ty, loc.clone());
                let is_declare = self.builder.bool(alias.is_declare, loc.clone());
                self.builder.extension(
                    "type_alias",
                    vec![
                        name,
                        type_params,
                        ty,
                        is_declare,
                    ],
                    loc,
                )
            }
            ast::Statement::Enum(enum_decl) => {
                if enum_decl.is_declare {
                    return self.builder.constant(0, self.to_loc(enum_decl.span));
                }
                let loc = self.to_loc(enum_decl.span);
                let mut args = vec![self.builder.symbol(&enum_decl.name, loc.clone())];
                for member in enum_decl.members {
                    let mut m_args = vec![self.builder.symbol(&member.name, loc.clone())];
                    if let Some(init) = member.initializer {
                        m_args.push(self.convert_expression(init));
                    }
                    args.push(self.builder.extension("enum_member", m_args, loc.clone()));
                }
                args.push(self.builder.bool(enum_decl.is_declare, loc.clone()));
                self.builder.extension("enum", args, loc)
            }
            ast::Statement::IfStatement(stmt) => {
                let loc = self.to_loc(stmt.span);
                let cond = self.convert_expression(stmt.test);
                let then_branch = self.convert_statement(*stmt.consequent);
                let else_branch = if let Some(alt) = stmt.alternate {
                    self.convert_statement(*alt)
                } else {
                    self.builder.constant(0, loc.clone())
                };
                self.builder.branch(cond, then_branch, else_branch, loc)
            }
            ast::Statement::WhileStatement(stmt) => {
                let loc = self.to_loc(stmt.span);
                let cond = self.convert_expression(stmt.test);
                let body = self.convert_statement(*stmt.body);
                self.builder.while_loop(cond, body, loc)
            }
            ast::Statement::BlockStatement(stmt) => {
                let loc = self.to_loc(stmt.span);
                let mut stmts = Vec::new();
                for s in stmt.statements {
                    stmts.push(self.convert_statement(s));
                }
                self.builder.block(stmts, loc)
            }
            ast::Statement::BreakStatement(stmt) => {
                let loc = self.to_loc(stmt.span);
                self.builder.extension("break", vec![], loc)
            }
            ast::Statement::ContinueStatement(stmt) => {
                let loc = self.to_loc(stmt.span);
                self.builder.extension("continue", vec![], loc)
            }
            ast::Statement::ThrowStatement(stmt) => {
                let loc = self.to_loc(stmt.span);
                let val = self.convert_expression(stmt.argument);
                self.builder.extension("throw", vec![val], loc)
            }
            ast::Statement::DoWhileStatement(stmt) => {
                let loc = self.to_loc(stmt.span);
                let cond = self.convert_expression(stmt.test);
                let body = self.convert_statement(*stmt.body);
                self.builder.extension("do_while", vec![cond, body], loc)
            }
            ast::Statement::ForStatement(stmt) => {
                let loc = self.to_loc(stmt.span);
                let init = if let Some(init) = stmt.initializer {
                    self.convert_statement(*init)
                } else {
                    self.builder.constant(0, loc.clone())
                };
                let test = if let Some(test) = stmt.test {
                    self.convert_expression(test)
                } else {
                    self.builder.bool(true, loc.clone())
                };
                let update = if let Some(update) = stmt.incrementor {
                    self.convert_expression(update)
                } else {
                    self.builder.constant(0, loc.clone())
                };
                let body = self.convert_statement(*stmt.body);
                self.builder
                    .extension("for_loop", vec![init, test, update, body], loc)
            }
            ast::Statement::ForInStatement(stmt) => {
                let loc = self.to_loc(stmt.span);
                let left = self.convert_statement(*stmt.left);
                let right = self.convert_expression(stmt.right);
                let body = self.convert_statement(*stmt.body);
                self.builder.extension("for_in", vec![left, right, body], loc)
            }
            ast::Statement::ForOfStatement(stmt) => {
                let loc = self.to_loc(stmt.span);
                let left = self.convert_statement(*stmt.left);
                let right = self.convert_expression(stmt.right);
                let body = self.convert_statement(*stmt.body);
                self.builder.extension("for_of", vec![left, right, body], loc)
            }
            ast::Statement::SwitchStatement(stmt) => {
                let loc = self.to_loc(stmt.span);
                let discriminant = self.convert_expression(stmt.discriminant);
                let mut cases = Vec::new();
                for case in stmt.cases {
                    let case_loc = self.to_loc(case.span);
                    let test = if let Some(test) = case.test {
                        self.convert_expression(test)
                    } else {
                        self.builder.symbol("default", case_loc.clone())
                    };
                    let mut body = Vec::new();
                    for s in case.consequent {
                        body.push(self.convert_statement(s));
                    }
                    let body_id = self.builder.block(body, case_loc.clone());
                    cases.push(self.builder.extension("case", vec![test, body_id], case_loc));
                }
                let mut args = vec![discriminant];
                args.extend(cases);
                self.builder.extension("switch", args, loc)
            }
            ast::Statement::TryStatement(stmt) => {
                let loc = self.to_loc(stmt.span.clone());
                let block = self.convert_statement(ast::Statement::BlockStatement(ast::BlockStatement {
                    decorators: Vec::new(),
                    is_declare: false,
                    statements: stmt.block,
                    span: stmt.span.clone(),
                }));
                let mut args = vec![block];

                if let Some(handler) = stmt.handler {
                    let handler_loc = self.to_loc(handler.span);
                    let param_name = handler.param.unwrap_or_else(|| "error".to_string());
                    let param = self.builder.symbol(&param_name, handler_loc.clone());
                    let body = self.convert_statement(ast::Statement::BlockStatement(ast::BlockStatement {
                        decorators: Vec::new(),
                        is_declare: false,
                        statements: handler.body,
                        span: handler.span.clone(),
                    }));
                    args.push(self.builder.extension("catch", vec![param, body], handler_loc));
                }

                if let Some(finalizer) = stmt.finalizer {
                    let finalizer_loc = self.to_loc(stmt.span.clone()); // Use stmt span for finalizer if not available
                    let body = self.convert_statement(ast::Statement::BlockStatement(ast::BlockStatement {
                        decorators: Vec::new(),
                        is_declare: false,
                        statements: finalizer,
                        span: stmt.span.clone(),
                    }));
                    args.push(self.builder.extension("finally", vec![body], finalizer_loc));
                }

                self.builder.extension("try", args, loc)
            }
        }
    }

    fn convert_expression(&mut self, expr: ast::Expression) -> Id {
        let loc = self.to_loc(expr.span.clone());
        match *expr.kind {
            ast::ExpressionKind::Identifier(name) => self.builder.symbol(&name, loc),
            ast::ExpressionKind::NumericLiteral(val) => self.builder.constant(val as i64, loc),
            ast::ExpressionKind::StringLiteral(val) => self.builder.string(&val, loc),
            ast::ExpressionKind::BooleanLiteral(val) => self.builder.bool(val, loc),
            ast::ExpressionKind::NullLiteral => self.builder.constant(0, loc),
            ast::ExpressionKind::BigIntLiteral(val) => {
                let s = self.builder.string(&val, loc.clone());
                self.builder.extension("bigint", vec![s], loc)
            }
            ast::ExpressionKind::RegexLiteral(val) => {
                let s = self.builder.string(&val, loc.clone());
                self.builder.extension("regex", vec![s], loc)
            }
            ast::ExpressionKind::TemplateString(val) => {
                let s = self.builder.string(&val, loc.clone());
                self.builder.extension("template", vec![s], loc)
            }
            ast::ExpressionKind::BinaryExpression {
                left,
                operator,
                right,
            } => {
                let l = self.convert_expression(*left);
                let r = self.convert_expression(*right);
                self.builder.binary_op(&operator, l, r, loc)
            }
            ast::ExpressionKind::CallExpression { func, args } => {
                let func_loc = self.to_loc(func.span.clone());

                // Detect console.log and map to std::io::println
                if let ast::ExpressionKind::MemberExpression {
                    object,
                    property,
                    computed,
                    ..
                } = &*func.kind
                {
                    if !*computed {
                        if let (
                            ast::ExpressionKind::Identifier(obj_name),
                            ast::ExpressionKind::Identifier(prop_name),
                        ) = (&*object.kind, &*property.kind)
                        {
                            if obj_name == "console" {
                                let mut arg_ids = Vec::new();
                                for arg in &args {
                                    arg_ids.push(self.convert_expression(arg.clone()));
                                }
                                if prop_name == "log" || prop_name == "println" {
                                    return self.builder.cross_lang_call(
                                        "nyar",
                                        "std::io",
                                        "println",
                                        arg_ids,
                                        func_loc,
                                    );
                                } else if prop_name == "print" {
                                    return self.builder.cross_lang_call(
                                        "nyar",
                                        "std::io",
                                        "print",
                                        arg_ids,
                                        func_loc,
                                    );
                                }
                            }
                        }
                    }
                }

                let f = self.convert_expression(*func);
                let mut arg_ids = Vec::new();
                for arg in args {
                    arg_ids.push(self.convert_expression(arg));
                }
                self.builder.call(f, arg_ids, loc)
            }
            ast::ExpressionKind::UnaryExpression { operator, argument } => {
                let arg = self.convert_expression(*argument);
                self.builder.extension(&operator, vec![arg], loc)
            }
            ast::ExpressionKind::MemberExpression {
                object,
                property,
                computed,
                optional,
            } => {
                let obj = self.convert_expression(*object);
                let prop = self.convert_expression(*property);
                let mut args = vec![obj, prop];
                args.push(self.builder.bool(optional, loc.clone()));
                if computed {
                    self.builder.extension("index", args, loc)
                } else {
                    self.builder.extension("gc.get_field", args, loc)
                }
            }
            ast::ExpressionKind::ConditionalExpression {
                test,
                consequent,
                alternate,
            } => {
                let t = self.convert_expression(*test);
                let c = self.convert_expression(*consequent);
                let a = self.convert_expression(*alternate);
                self.builder.branch(t, c, a, loc)
            }
            ast::ExpressionKind::NewExpression { func, args } => {
                let f = self.convert_expression(*func);
                let mut arg_ids = vec![f];
                for arg in args {
                    arg_ids.push(self.convert_expression(arg));
                }
                self.builder.extension("gc.new", arg_ids, loc)
            }
            ast::ExpressionKind::AssignmentExpression {
                left,
                operator,
                right,
            } => {
                let r = self.convert_expression(*right);
                if operator == "=" {
                    match &*left.kind {
                        ast::ExpressionKind::MemberExpression {
                            object, property, ..
                        } => {
                            let obj = self.convert_expression(*object.clone());
                            let prop = self.convert_expression(*property.clone());
                            self.builder.extension(
                                "gc.set_field",
                                vec![obj, prop, r],
                                loc,
                            )
                        }
                        _ => {
                            let l = self.convert_expression(*left);
                            self.builder.assign_to_id(l, r, loc)
                        }
                    }
                } else {
                    let l = self.convert_expression(*left.clone());
                    // Compound assignment like +=
                    let op = operator.trim_end_matches('=');
                    let value = self.builder.binary_op(op, l, r, loc.clone());
                    let target = self.convert_expression(*left);
                    self.builder.assign_to_id(target, value, loc)
                }
            }
            ast::ExpressionKind::AsExpression {
                expression,
                type_annotation,
            } => {
                let expr = self.convert_expression(*expression);
                let ty = self.convert_type_annotation(type_annotation, loc.clone());
                self.builder.extension("as", vec![expr, ty], loc)
            }
            ast::ExpressionKind::ImportExpression {
                module_specifier,
            } => {
                let spec = self.convert_expression(*module_specifier);
                self.builder.extension("import", vec![spec], loc)
            }
            ast::ExpressionKind::ArrowFunction {
                type_params,
                params,
                return_type,
                body,
                async_,
            } => {
                let body_id = self.convert_statement(*body);
                let mut type_param_ids = Vec::new();
                for tp in type_params {
                    type_param_ids.push(self.convert_type_parameter(tp, loc.clone()));
                }
                let type_params_seq = self.builder.seq(type_param_ids, loc.clone());

                let mut param_ids = Vec::new();
                for p in params {
                    let p_loc = self.to_loc(p.span);
                    let mut p_args = vec![self.builder.symbol(&p.name, p_loc.clone())];
                    if let Some(ty) = p.ty {
                        p_args.push(self.convert_type_annotation(ty, p_loc.clone()));
                    }
                    p_args.push(self.builder.bool(p.optional, p_loc.clone()));
                    param_ids.push(self.builder.extension("param", p_args, p_loc));
                }
                let params_seq = self.builder.seq(param_ids, loc.clone());

                let mut args = vec![type_params_seq, params_seq, body_id];
                if let Some(ret) = return_type {
                    args.push(self.convert_type_annotation(ret, loc.clone()));
                }
                if async_ {
                    self.builder.extension("async_lambda", args, loc)
                } else {
                    self.builder.extension("lambda", args, loc)
                }
            }
            ast::ExpressionKind::ObjectLiteral { properties } => {
                let mut args = Vec::new();
                for prop in properties {
                    match prop {
                        ast::ObjectProperty::Property { name, value, shorthand, span } => {
                            let p_loc = self.to_loc(span);
                            let key = self.builder.symbol(&name, p_loc.clone());
                            let value = self.convert_expression(value);
                            let shorthand_id = self.builder.bool(shorthand, p_loc.clone());
                            args.push(self.builder.extension("prop", vec![key, value, shorthand_id], p_loc));
                        }
                        ast::ObjectProperty::Spread(expr) => {
                            let p_loc = self.to_loc(expr.span.clone());
                            let inner = self.convert_expression(expr);
                            args.push(self.builder.extension("spread_prop", vec![inner], p_loc));
                        }
                    }
                }
                self.builder.extension("object", args, loc)
            }
            ast::ExpressionKind::ArrayLiteral { elements } => {
                let mut args = Vec::new();
                for elem in elements {
                    args.push(self.convert_expression(elem));
                }
                self.builder.extension("array", args, loc)
            }
            ast::ExpressionKind::SpreadElement(expr) => {
                let inner = self.convert_expression(*expr);
                self.builder.extension("spread", vec![inner], loc)
            }
            ast::ExpressionKind::AwaitExpression(expr) => {
                let inner = self.convert_expression(*expr);
                self.builder.extension("await", vec![inner], loc)
            }
            ast::ExpressionKind::YieldExpression(expr) => {
                let mut args = Vec::new();
                if let Some(e) = expr {
                    args.push(self.convert_expression(*e));
                }
                self.builder.extension("yield", args, loc)
            }
            ast::ExpressionKind::JsxElement(elem) => self.convert_jsx_element(*elem, loc),
            ast::ExpressionKind::JsxFragment(fragment) => self.convert_jsx_fragment(*fragment, loc),
            ast::ExpressionKind::JsxSelfClosingElement(element) => {
                self.convert_jsx_self_closing_element(*element, loc)
            }
            _ => self.builder.constant(0, loc),
        }
    }

    fn convert_jsx_element(&mut self, elem: ast::JsxElement, loc: Loc) -> Id {
        let mut args = vec![self.convert_jsx_tag_name(elem.opening_element.name, loc.clone())];

        let mut attr_ids = Vec::new();
        for attr_or_spread in elem.opening_element.attributes {
            match attr_or_spread {
                ast::JsxAttributeOrSpread::Attribute(attr) => {
                    let attr_loc = self.to_loc(attr.span);
                    let name = self.builder.string(&attr.name, attr_loc.clone());
                    let value = if let Some(val) = attr.value {
                        self.convert_jsx_attribute_value(val)
                    } else {
                        self.builder.bool(true, attr_loc.clone())
                    };
                    attr_ids.push(self.builder.extension(
                        "jsx_attr",
                        vec![name, value],
                        attr_loc,
                    ));
                }
                ast::JsxAttributeOrSpread::Spread(expr) => {
                    let spread_id = self.convert_expression(expr);
                    attr_ids.push(self.builder.extension(
                        "jsx_spread_attr",
                        vec![spread_id],
                        Loc::default(),
                    ));
                }
            }
        }
        args.push(self.builder.seq(attr_ids, loc.clone()));

        let mut child_ids = Vec::new();
        for child in elem.children {
            child_ids.push(self.convert_jsx_child(child));
        }
        args.push(self.builder.seq(child_ids, loc.clone()));

        self.builder.extension("jsx_element", args, loc)
    }

    fn convert_jsx_fragment(&mut self, frag: ast::JsxFragment, _loc: Loc) -> Id {
        let mut child_ids = Vec::new();
        for child in frag.children {
            child_ids.push(self.convert_jsx_child(child));
        }
        let seq = self.builder.seq(child_ids, Loc::default());
        self.builder.extension("jsx_fragment", vec![seq], Loc::default())
    }

    fn convert_jsx_self_closing_element(
        &mut self,
        elem: ast::JsxSelfClosingElement,
        loc: Loc,
    ) -> Id {
        let mut args = vec![self.convert_jsx_tag_name(elem.name, loc.clone())];

        let mut attr_ids = Vec::new();
        for attr_or_spread in elem.attributes {
            match attr_or_spread {
                ast::JsxAttributeOrSpread::Attribute(attr) => {
                    let attr_loc = self.to_loc(attr.span);
                    let name = self.builder.string(&attr.name, attr_loc.clone());
                    let value = if let Some(val) = attr.value {
                        self.convert_jsx_attribute_value(val)
                    } else {
                        self.builder.bool(true, attr_loc.clone())
                    };
                    attr_ids.push(self.builder.extension(
                        "jsx_attr",
                        vec![name, value],
                        attr_loc,
                    ));
                }
                ast::JsxAttributeOrSpread::Spread(expr) => {
                    let spread_id = self.convert_expression(expr);
                    attr_ids.push(self.builder.extension(
                        "jsx_spread_attr",
                        vec![spread_id],
                        Loc::default(),
                    ));
                }
            }
        }
        args.push(self.builder.seq(attr_ids, loc.clone()));
        args.push(self.builder.seq(vec![], loc.clone())); // No children

        self.builder.extension("jsx_element", args, loc)
    }

    fn convert_jsx_tag_name(&mut self, name: ast::JsxTagName, loc: Loc) -> Id {
        match name {
            ast::JsxTagName::Identifier(s) => self.builder.string(&s, loc),
            ast::JsxTagName::MemberExpression { object, property } => {
                let obj = self.convert_jsx_tag_name(*object, loc.clone());
                let prop = self.builder.string(&property, loc.clone());
                self.builder.extension("jsx_tag_member", vec![obj, prop], loc)
            }
        }
    }

    fn convert_jsx_attribute_value(&mut self, value: ast::JsxAttributeValue) -> Id {
        match value {
            ast::JsxAttributeValue::StringLiteral(s) => self.builder.string(&s, Loc::default()),
            ast::JsxAttributeValue::ExpressionContainer(expr) => {
                if let Some(e) = expr {
                    self.convert_expression(e)
                } else {
                    self.builder.constant(0, Loc::default())
                }
            }
            ast::JsxAttributeValue::Element(elem) => {
                let loc = self.to_loc(elem.span.clone());
                self.convert_jsx_element(*elem, loc)
            }
            ast::JsxAttributeValue::Fragment(frag) => {
                let loc = self.to_loc(frag.span.clone());
                self.convert_jsx_fragment(*frag, loc)
            }
        }
    }

    fn convert_jsx_child(&mut self, child: ast::JsxChild) -> Id {
        match child {
            ast::JsxChild::JsxText(text) => self.builder.string(&text, Loc::default()),
            ast::JsxChild::JsxExpressionContainer(expr) => {
                if let Some(e) = expr {
                    self.convert_expression(e)
                } else {
                    self.builder.constant(0, Loc::default())
                }
            }
            ast::JsxChild::JsxElement(elem) => {
                let loc = self.to_loc(elem.span.clone());
                self.convert_jsx_element(*elem, loc)
            }
            ast::JsxChild::JsxSelfClosingElement(elem) => {
                let loc = self.to_loc(elem.span.clone());
                self.convert_jsx_self_closing_element(*elem, loc)
            }
            ast::JsxChild::JsxFragment(frag) => {
                let loc = self.to_loc(frag.span.clone());
                self.convert_jsx_fragment(*frag, loc)
            }
        }
    }

    fn convert_type_annotation(&mut self, ty: ast::TypeAnnotation, loc: Loc) -> Id {
        match ty {
            ast::TypeAnnotation::Identifier(name) => self.builder.symbol(&name, loc),
            ast::TypeAnnotation::Predefined(name) => self.builder.symbol(&name, loc),
            ast::TypeAnnotation::Literal(lit) => {
                let lit_id = match lit {
                    ast::LiteralType::String(s) => self.builder.string(&s, loc.clone()),
                    ast::LiteralType::Number(n) => self.builder.constant(n as i64, loc.clone()),
                    ast::LiteralType::Boolean(b) => self.builder.bool(b, loc.clone()),
                    ast::LiteralType::BigInt(s) => {
                        let s_str = s.clone();
                        let s_id = self.builder.string(&s_str, loc.clone());
                        self.builder.extension("bigint", vec![s_id], loc.clone())
                    }
                };
                self.builder.extension("literal_type", vec![lit_id], loc)
            }
            ast::TypeAnnotation::Array(inner) => {
                let inner_id = self.convert_type_annotation(*inner, loc.clone());
                self.builder.extension("array_type", vec![inner_id], loc)
            }
            ast::TypeAnnotation::Tuple(elements) => {
                let mut elem_ids = Vec::new();
                for e in elements {
                    elem_ids.push(self.convert_type_annotation(e, loc.clone()));
                }
                let seq = self.builder.seq(elem_ids, loc.clone());
                self.builder.extension("tuple_type", vec![seq], loc)
            }
            ast::TypeAnnotation::Union(types) => {
                let mut type_ids = Vec::new();
                for t in types {
                    type_ids.push(self.convert_type_annotation(t, loc.clone()));
                }
                let seq = self.builder.seq(type_ids, loc.clone());
                self.builder.extension("union_type", vec![seq], loc)
            }
            ast::TypeAnnotation::Intersection(types) => {
                let mut type_ids = Vec::new();
                for t in types {
                    type_ids.push(self.convert_type_annotation(t, loc.clone()));
                }
                let seq = self.builder.seq(type_ids, loc.clone());
                self.builder.extension("intersection_type", vec![seq], loc)
            }
            ast::TypeAnnotation::Reference { name, args } => {
                match name.as_str() {
                    "Partial" if args.len() == 1 => {
                        let inner = self.convert_type_annotation(args[0].clone(), loc.clone());
                        self.builder.extension("partial_type", vec![inner], loc)
                    }
                    "Required" if args.len() == 1 => {
                        let inner = self.convert_type_annotation(args[0].clone(), loc.clone());
                        self.builder.extension("required_type", vec![inner], loc)
                    }
                    "Readonly" if args.len() == 1 => {
                        let inner = self.convert_type_annotation(args[0].clone(), loc.clone());
                        self.builder.extension("readonly_type", vec![inner], loc)
                    }
                    "Pick" if args.len() == 2 => {
                        let inner = self.convert_type_annotation(args[0].clone(), loc.clone());
                        let keys = self.convert_type_annotation(args[1].clone(), loc.clone());
                        self.builder.extension("pick_type", vec![inner, keys], loc)
                    }
                    "Omit" if args.len() == 2 => {
                        let inner = self.convert_type_annotation(args[0].clone(), loc.clone());
                        let keys = self.convert_type_annotation(args[1].clone(), loc.clone());
                        self.builder.extension("omit_type", vec![inner, keys], loc)
                    }
                    "Record" if args.len() == 2 => {
                        let key = self.convert_type_annotation(args[0].clone(), loc.clone());
                        let value = self.convert_type_annotation(args[1].clone(), loc.clone());
                        self.builder.extension("record_type", vec![key, value], loc)
                    }
                    _ => {
                        let name_id = self.builder.symbol(&name, loc.clone());
                        let mut arg_ids = Vec::new();
                        for a in args {
                            arg_ids.push(self.convert_type_annotation(a, loc.clone()));
                        }
                        let seq = self.builder.seq(arg_ids, loc.clone());
                        self.builder.extension("type_ref", vec![name_id, seq], loc)
                    }
                }
            }
            ast::TypeAnnotation::Function {
                params: _,
                args,
                return_type,
            } => {
                let mut param_ids = Vec::new();
                for p in args {
                    let mut p_args = vec![self.builder.symbol(&p.name, loc.clone())];
                    if let Some(ty) = p.ty {
                        p_args.push(self.convert_type_annotation(ty, loc.clone()));
                    }
                    p_args.push(self.builder.bool(p.optional, loc.clone()));
                    param_ids.push(self.builder.extension("param", p_args, loc.clone()));
                }
                let params_seq = self.builder.seq(param_ids, loc.clone());
                let ret_id = self.convert_type_annotation(*return_type, loc.clone());
                self.builder.extension("function_type", vec![params_seq, ret_id], loc)
            }
            ast::TypeAnnotation::Object(members) => {
                let mut member_ids = Vec::new();
                for m in members {
                    member_ids.push(self.convert_class_member(m, loc.clone()));
                }
                let seq = self.builder.seq(member_ids, loc.clone());
                self.builder.extension("object_type", vec![seq], loc)
            }
            ast::TypeAnnotation::Query(name) => {
                let name_id = self.builder.symbol(&name, loc.clone());
                self.builder.extension("typeof", vec![name_id], loc)
            }
            ast::TypeAnnotation::KeyOf(inner) => {
                let inner_id = self.convert_type_annotation(*inner, loc.clone());
                self.builder.extension("keyof", vec![inner_id], loc)
            }
            ast::TypeAnnotation::Conditional {
                check_type,
                extends_type,
                true_type,
                false_type,
            } => {
                let check_id = self.convert_type_annotation(*check_type, loc.clone());
                let extends_id = self.convert_type_annotation(*extends_type, loc.clone());
                let true_id = self.convert_type_annotation(*true_type, loc.clone());
                let false_id = self.convert_type_annotation(*false_type, loc.clone());
                self.builder.extension(
                    "conditional_type",
                    vec![check_id, extends_id, true_id, false_id],
                    loc,
                )
            }
            ast::TypeAnnotation::Mapped {
                key_name,
                key_type,
                value_type,
                readonly,
                optional,
            } => {
                let name_id = self.builder.symbol(&key_name, loc.clone());
                let key_id = self.convert_type_annotation(*key_type, loc.clone());
                let val_id = self.convert_type_annotation(*value_type, loc.clone());
                let mut args = vec![name_id, key_id, val_id];
                args.push(self.builder.constant(
                    match readonly {
                        Some(true) => 1,
                        Some(false) => -1,
                        None => 0,
                    },
                    loc.clone(),
                ));
                args.push(self.builder.constant(
                    match optional {
                        Some(true) => 1,
                        Some(false) => -1,
                        None => 0,
                    },
                    loc.clone(),
                ));
                self.builder.extension("mapped_type", args, loc)
            }
            ast::TypeAnnotation::TemplateLiteral(elements) => {
                let mut element_ids = Vec::new();
                for el in elements {
                    match el {
                        ast::TemplateElement::String(s) => {
                            element_ids.push(self.builder.string(&s, loc.clone()));
                        }
                        ast::TemplateElement::Type(ty) => {
                            element_ids.push(self.convert_type_annotation(*ty, loc.clone()));
                        }
                    }
                }
                let seq = self.builder.seq(element_ids, loc.clone());
                self.builder.extension("template_literal_type", vec![seq], loc)
            }
            ast::TypeAnnotation::Infer(name) => {
                let name_id = self.builder.symbol(&name, loc.clone());
                self.builder.extension("infer_type", vec![name_id], loc)
            }
        }
    }

    fn convert_type_parameter(&mut self, tp: ast::TypeParameter, loc: Loc) -> Id {
        let mut args = vec![self.builder.symbol(&tp.name, loc.clone())];
        if let Some(constraint) = tp.constraint {
            args.push(self.convert_type_annotation(constraint, loc.clone()));
        } else {
            args.push(self.builder.constant(0, loc.clone()));
        }
        if let Some(default) = tp.default {
            args.push(self.convert_type_annotation(default, loc.clone()));
        } else {
            args.push(self.builder.constant(0, loc.clone()));
        }
        self.builder.extension("type_param", args, loc)
    }

    fn convert_class_member(&mut self, member: ast::ClassMember, loc: Loc) -> Id {
        match member {
            ast::ClassMember::Property {
                decorators,
                name,
                ty,
                initializer,
                visibility,
                is_static,
                is_readonly,
                is_abstract,
                is_optional,
                span: _,
            } => {
                let mut args = vec![self.builder.symbol(&name, loc.clone())];
                if let Some(ty) = ty {
                    args.push(self.convert_type_annotation(ty, loc.clone()));
                } else {
                    args.push(self.builder.constant(0, loc.clone()));
                }
                if let Some(val) = initializer {
                    args.push(self.convert_expression(val));
                } else {
                    args.push(self.builder.constant(0, loc.clone()));
                }
                args.push(self.builder.bool(is_static, loc.clone()));
                args.push(self.builder.bool(is_readonly, loc.clone()));
                args.push(self.builder.bool(is_abstract, loc.clone()));
                args.push(self.builder.string(&format!("{:?}", visibility), loc.clone()));
                args.push(self.builder.bool(is_optional, loc.clone()));
                let mut prop_id = self.builder.extension("property", args, loc.clone());

                // Handle property decorators
                for dec in decorators {
                    let dec_expr = self.convert_expression(dec.expression);
                    prop_id = self.builder.call(dec_expr, vec![prop_id], loc.clone());
                }
                prop_id
            }
            ast::ClassMember::Method {
                decorators,
                name,
                type_params,
                params,
                return_type,
                body,
                visibility,
                is_static,
                is_abstract,
                is_getter,
                is_setter,
                is_optional,
                span: _,
            } => {
                let mut args = vec![self.builder.symbol(&name, loc.clone())];
                let mut type_param_ids = Vec::new();
                for tp in type_params {
                    type_param_ids.push(self.convert_type_parameter(tp, loc.clone()));
                }
                args.push(self.builder.seq(type_param_ids, loc.clone()));

                let mut param_ids = Vec::new();
                for p in params {
                    let p_loc = self.to_loc(p.span);
                    let mut p_args = vec![self.builder.symbol(&p.name, p_loc.clone())];
                    if let Some(ty) = p.ty {
                        p_args.push(self.convert_type_annotation(ty, p_loc.clone()));
                    } else {
                        p_args.push(self.builder.constant(0, p_loc.clone()));
                    }
                    p_args.push(self.builder.bool(p.optional, p_loc.clone()));
                    let mut param_id = self.builder.extension("param", p_args, p_loc.clone());

                    // Handle method parameter decorators
                    for dec in p.decorators {
                        let dec_expr = self.convert_expression(dec.expression);
                        param_id = self.builder.call(dec_expr, vec![param_id], p_loc.clone());
                    }
                    param_ids.push(param_id);
                }
                args.push(self.builder.seq(param_ids, loc.clone()));

                let mut body_ids = Vec::new();
                for s in body {
                    body_ids.push(self.convert_statement(s));
                }
                args.push(self.builder.block(body_ids, loc.clone()));

                if let Some(ret) = return_type {
                    args.push(self.convert_type_annotation(ret, loc.clone()));
                } else {
                    args.push(self.builder.constant(0, loc.clone()));
                }

                args.push(self.builder.string(&format!("{:?}", visibility), loc.clone()));
                args.push(self.builder.bool(is_static, loc.clone()));
                args.push(self.builder.bool(is_abstract, loc.clone()));
                args.push(self.builder.bool(is_getter, loc.clone()));
                args.push(self.builder.bool(is_setter, loc.clone()));
                args.push(self.builder.bool(is_optional, loc.clone()));
                let mut method_id = self.builder.extension("method", args, loc.clone());

                // Handle method decorators
                for dec in decorators {
                    let dec_expr = self.convert_expression(dec.expression);
                    method_id = self.builder.call(dec_expr, vec![method_id], loc.clone());
                }
                method_id
            }
        }
    }
}
