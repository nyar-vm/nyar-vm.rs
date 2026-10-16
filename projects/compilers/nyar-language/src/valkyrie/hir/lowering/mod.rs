use std::{cell::RefCell, ops::Range, path::Path};

use crate::{
    hir::{
        BuiltinTypeAliasScope, ModuleTypeAliasScope, hoist_anonymous_classes, lower_type_expression,
        overload::{resolve_hir_calls, validate_extractor_patterns},
        render_type_expression, validate_ast_root,
    },
    mir::{FlagsLayout, MirLowerer, MirSumDeclaration, MirSumVariant},
    types::{
        Identifier, NamePath, SourceID, SourceSpan,
        hir::{
            GenericType, HirArgument, HirAssociatedConst, HirAssociatedConstImpl, HirAssociatedType, HirAssociatedTypeBinding,
            HirAssociatedTypeImpl, HirAttribute, HirBlock, HirCallArgument, HirCallKind, HirCompileWarning, HirDependencySemanticExport,
            HirDocumentation, HirEnum, HirExpr, HirExprKind, HirField, HirFlagMember, HirFlags, HirFunction, HirIdentifier, HirImpl, HirImport,
            HirImportBinding, HirKind, HirLiteral, HirMatchArm, HirModule, HirParam, HirParameterBindingKind, HirParent, HirPattern,
            HirProperty, HirSingleton, HirStatement, HirStatementKind, HirStruct, HirTrait, HirTraitBound, HirTypeAlias, HirTypeFunction,
            HirVariadicKind, HirVariant, HirVisibility, HirWhereConstraint, HirWidget, HirWidgetLifecycle, ValkyrieType,
        },
    },
    validation::{ControlFlowScheduler, validate_semantic_module},
    valkyrie::{
        backend_contract::interop::{function_host_provider_target, validate_interop_surface},
        frontend::{
            self, ParseError, ValkyrieRoot,
            ast::{
                AssociatedType, Attribute, ClassDeclaration, EnumVariant, Enums, EnumsKind, FieldDeclaration, Flags, GenericParam,
                ImplyDeclaration, MethodDeclaration, MicroDeclaration, NamePath as AstNamePath, Param, Parent, SingletonDeclaration,
                StatementNode, StructureDeclaration, TermExpression, Trait, TypeExpression, TypeFunction, UnionDeclaration, UsingDeclaration,
                Variant, WidgetDeclaration,
            },
        },
        mir::{SINGLETON_CONSTRUCTOR_NAME, SINGLETON_FINALIZER_NAME},
    },
};
mod trait_binding;
use nyar_types::NyarType;
use ordered_float::OrderedFloat;

thread_local! {
    static COMPILE_WARNINGS: RefCell<Vec<HirCompileWarning>> = const { RefCell::new(Vec::new()) };
}

#[cfg(test)]
mod source_group_tests {
    use super::{CompilerSourceGroup, ValkyrieCompiler, template_expand::DEFAULT_COMPILE_ARCH};

    #[test]
    fn source_parse_error_identifies_file_and_preserves_local_offset() {
        let source = "micro broken() { value = }";
        let direct = crate::valkyrie::frontend::parse_source(source).expect_err("语法必须失败");
        let super::ParseError::Invalid { span: expected_span, .. } = direct
        else {
            panic!("缺少语法诊断")
        };
        let groups = vec![CompilerSourceGroup {
            dependency_key: "broken.package".into(),
            name: "broken.package".into(),
            source: String::new(),
            sources: vec![super::CompilerSourceFile { path: "source/broken.v".into(), source: source.into() }],
            direct_dependencies: vec![],
        }];
        let error = ValkyrieCompiler::default().resolve_source_groups(&groups, DEFAULT_COMPILE_ARCH).expect_err("不能生成残缺 HIR");
        let super::ParseError::Invalid { message, span } = error
        else {
            panic!("缺少语法诊断")
        };
        assert_eq!(span, expected_span);
        assert!(message.contains("broken.package") && message.contains("source/broken.v"), "{message}");
    }

    fn first_call(function: &super::HirFunction) -> &crate::types::hir::HirResolvedCall {
        let super::HirStatementKind::Let { initializer: Some(expression), .. } = &function.body.statements[0].kind
        else {
            panic!("缺少源码调用绑定");
        };
        let super::HirExprKind::Call { resolved: Some(call), .. } = &expression.kind
        else {
            panic!("调用没有完成 HIR 解析");
        };
        call
    }

    #[test]
    fn declaration_identity_survives_dependency_export_and_call_selection() {
        let groups = vec![
            CompilerSourceGroup {
                dependency_key: "library".into(),
                name: "library".into(),
                source: "micro answer() -> i32 { return 23 }".into(),
                sources: Vec::new(),
                direct_dependencies: vec![],
            },
            CompilerSourceGroup {
                dependency_key: "app".into(),
                name: "app".into(),
                source: "micro run() -> i32 { let value: i32 = answer(); return value }".into(),
                sources: Vec::new(),
                direct_dependencies: vec!["library".into()],
            },
        ];
        let modules = ValkyrieCompiler::default().resolve_source_groups(&groups, DEFAULT_COMPILE_ARCH).expect("完整源码闭包的 HIR");
        let declaration = modules[0].functions[0].declaration.expect("依赖声明身份");
        assert_eq!(modules[1].imported_semantic_exports[0].functions[0].declaration, Some(declaration));
        assert_eq!(first_call(&modules[1].functions[0]).declaration, Some(declaration));
        assert_ne!(modules[1].functions[0].declaration, Some(declaration));
    }

    #[test]
    #[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
    fn declaration_identity_survives_generic_signature_instantiation() {
        let hir = ValkyrieCompiler::default().compile_source(
            "micro identity<T>(value: T) -> T { return value } micro run(value: i32) -> i32 { let result: i32 = identity(value); return result }",
        ).expect("泛型源码调用");
        let declaration = hir.functions[0].declaration.expect("泛型声明身份");
        let call = first_call(&hir.functions[1]);
        assert_eq!(call.declaration, Some(declaration));
        assert_eq!(call.parameter_types, vec![super::ValkyrieType::Integer32 { signed: true }]);
        assert_ne!(call.parameter_types[0], hir.functions[0].params[0].ty);
    }

    #[test]
    fn declaration_identity_separates_same_named_static_methods() {
        let hir = ValkyrieCompiler::default().compile_source(
            "structure Alpha {} structure Beta {} imply Alpha { micro answer(value: i32) -> i32 { return value } } imply Beta { micro answer(value: i32) -> i32 { return value } } micro run(value: i32) -> i32 { let result: i32 = Alpha.answer(value); return result }",
        ).expect("同名不同 owner 的静态调用");
        let alpha = hir.impls[0].methods[0].declaration.expect("Alpha 声明身份");
        let beta = hir.impls[1].methods[0].declaration.expect("Beta 声明身份");
        assert_ne!(alpha, beta);
        let call = first_call(&hir.functions[0]);
        assert_eq!(call.declaration, Some(alpha));
        assert!(!call.has_receiver);
        assert_eq!(call.parameter_types.len(), 1);
    }

    #[test]
    fn materialized_hir_requires_call_resolution_before_semantic_success() {
        let compiler = ValkyrieCompiler::default();
        let source = "micro answer() -> i32 { return 1 } micro main() -> i32 { return answer() }";
        let hir = compiler.parse_source_group_without_call_resolution(source, &[], None, DEFAULT_COMPILE_ARCH).expect("物化阶段不绑定调用");
        let error = compiler.validate_hir_semantic_contract(&hir).expect_err("未绑定 HIR 不能越过语义边界");
        assert!(error.to_string().contains("SMIR003"), "{error}");
        compiler.compile_source(source).expect("分析入口必须执行解析和验证");
    }

    #[test]
    #[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
    fn source_closure_materialization_precedes_call_validation() {
        let groups = vec![
            CompilerSourceGroup {
                dependency_key: "library".into(),
                name: "library".into(),
                source: "micro helper() -> i32 { return missing() }".into(),
                sources: Vec::new(),
                direct_dependencies: Vec::new(),
            },
            CompilerSourceGroup {
                dependency_key: "app".into(),
                name: "app".into(),
                source: "micro main(".into(),
                sources: Vec::new(),
                direct_dependencies: vec!["library".into()],
            },
        ];
        let error = ValkyrieCompiler::default().compile_source_groups_to_program(&groups).expect_err("完整源码闭包必须先完成物化");
        assert!(!error.to_string().contains("SMIR003"), "调用验证不得先于后续源码解析: {error}");
    }

    #[test]
    fn materialized_group_does_not_grant_undeclared_dependency_visibility() {
        let groups = vec![
            CompilerSourceGroup {
                dependency_key: "library".into(),
                name: "library".into(),
                source: "micro answer() -> i32 { return 1 }".into(),
                sources: Vec::new(),
                direct_dependencies: Vec::new(),
            },
            CompilerSourceGroup {
                dependency_key: "app".into(),
                name: "app".into(),
                source: "micro main() -> i32 { return answer() }".into(),
                sources: Vec::new(),
                direct_dependencies: Vec::new(),
            },
        ];
        let error = ValkyrieCompiler::default().compile_source_groups_to_program(&groups).expect_err("完整闭包不授予未声明依赖可见性");
        assert!(error.to_string().contains("SMIR003"), "{error}");
    }

    #[test]
    fn compiler_owns_dependency_group_linking() {
        let groups = vec![
            CompilerSourceGroup {
                dependency_key: "core".into(),
                name: "core".into(),
                source: "micro answer() -> i32 { return 1 }".into(),
                sources: Vec::new(),
                direct_dependencies: Vec::new(),
            },
            CompilerSourceGroup {
                dependency_key: "app".into(),
                name: "app".into(),
                source: "micro main() -> i32 { return answer() }".into(),
                sources: Vec::new(),
                direct_dependencies: vec!["core".into()],
            },
        ];
        let output = ValkyrieCompiler::default().compile_source_groups_to_program(&groups).expect("compiler closes source groups");
        assert_eq!(output.canonical().linked.module_name, "app");
        assert!(output.canonical().linked.callable_names.values().any(|name| name.to_string() == "core::answer"));
    }

    #[test]
    fn compiler_links_transitive_calls_by_instance_and_excludes_unused_bodies() {
        let groups = vec![
            CompilerSourceGroup {
                dependency_key: "base".into(),
                name: "base".into(),
                source: "micro answer() -> i32 { return 23 } micro unused() -> bool { return true }".into(),
                sources: Vec::new(),
                direct_dependencies: vec![],
            },
            CompilerSourceGroup {
                dependency_key: "library".into(),
                name: "library".into(),
                source: "micro relay() -> i32 { return answer() }".into(),
                sources: Vec::new(),
                direct_dependencies: vec!["base".into()],
            },
            CompilerSourceGroup {
                dependency_key: "app".into(),
                name: "app".into(),
                source: "micro run() -> i32 { return relay() }".into(),
                sources: Vec::new(),
                direct_dependencies: vec!["library".into()],
            },
        ];
        let output = ValkyrieCompiler::default().compile_source_groups_to_program(&groups).expect("实例闭包必须贯穿 Canonical 校验及表示规划");
        let program = output.canonical();
        assert_eq!(program.mir.functions.len(), 3);
        assert!(!program.linked.callable_names.values().any(|name| name.to_string() == "base::unused"));
        let calls = program
            .mir
            .functions
            .values()
            .flat_map(|function| function.blocks.values())
            .flat_map(|block| &block.instructions)
            .filter_map(|instruction| {
                if let nyar_types::CanonicalOperation::Invoke { callee: nyar_types::CanonicalCallee::Item(instance), .. } =
                    instruction.operation
                {
                    Some(instance)
                }
                else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 2);
        assert!(calls.iter().all(|instance| program.mir.functions.contains_key(instance)));
    }

    #[test]
    #[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
    fn generic_top_level_constructor_seeds_receive_callable_instances() {
        let compiler = ValkyrieCompiler::default();
        let hir = compiler
            .compile_source(
                "unite Option<T> { Some { value: T }, None } micro Some<T>(value: T) -> Option<T> { return Some(value) } micro option_none<T>() -> Option<T> { return None }",
            )
            .expect("泛型构造种子必须注册 callable instance");
        let some = hir.functions.iter().find(|function| function.name.as_str() == "Some").expect("Some constructor");
        let option_none = hir.functions.iter().find(|function| function.name.as_str() == "option_none").expect("option_none constructor");
        assert!(some.instance.is_some(), "generic `Some<T>` must receive seed callable instance");
        assert!(option_none.instance.is_some(), "generic `option_none<T>` must receive seed callable instance");
    }

    #[test]
    fn trait_semicolon_signatures_do_not_materialize_callable_instances() {
        let hir = ValkyrieCompiler::default()
            .compile_source("trait As<T> { micro as_type(self, target_type: type<T>) -> T?; }")
            .expect("trait signature must compile");
        let as_trait = hir.traits.iter().find(|trait_def| trait_def.name.as_str() == "As").expect("As trait");
        assert!(as_trait.methods.iter().any(|method| method.name.as_str() == "as_type" && method.is_abstract));
        assert!(as_trait.default_methods.is_empty(), "semicolon-only trait members must not become default bodies");
        let mir = crate::valkyrie::mir::MirLowerer::lower_module_semantic(&hir);
        assert!(
            !mir.functions.iter().any(|function| function.symbol.contains("as_type")),
            "abstract trait signatures must not lower to MIR callables"
        );
    }

    #[test]
    fn monomorphic_trait_default_methods_receive_callable_instances() {
        let compiler = ValkyrieCompiler::default();
        let hir = compiler
            .compile_source("trait Counter { micro value(self) -> i32 micro bump(self) -> i32 { return 1 } }")
            .expect("trait default method must compile");
        let counter = hir.traits.iter().find(|trait_def| trait_def.name.as_str() == "Counter").expect("Counter trait");
        let bump = counter.default_methods.iter().find(|method| method.name.as_str() == "bump").expect("bump default");
        assert!(bump.instance.is_some(), "materialized trait default must receive callable instance");
    }

    #[test]
    fn constructor_identities_are_registered_before_overload_selection() {
        let compiler = ValkyrieCompiler::default();
        let hir = compiler.compile_source(
            "structure Packet { value: i32 } unite State { Ready { value: i32 }, Empty } micro run(value: i32) -> Packet { let packet = Packet { value: value }; return packet }",
        ).expect("源码构造声明");
        let structure = &hir.structs[0];
        let variant = &hir.enums[0].variants[0];
        let structure_declaration = structure.constructor_declaration.expect("结构构造声明 identity");
        assert!(structure.constructor_instance.is_some());
        assert!(variant.instance.is_some());
        assert_ne!(Some(structure_declaration), variant.declaration);
        let super::HirStatementKind::Let { initializer: Some(expression), .. } = &hir.functions[0].body.statements[0].kind
        else {
            panic!("缺少构造表达式");
        };
        let super::HirExprKind::Construct { resolved: Some(call), .. } = &expression.kind
        else {
            panic!("构造调用未绑定");
        };
        assert_eq!(call.declaration, Some(structure_declaration));
        assert_eq!(call.instance, structure.constructor_instance);
    }

    #[test]
    fn compiler_carries_fragment_contracts_into_canonical_program() {
        let output = ValkyrieCompiler::default()
            .compile_source_to_program("[export(name: \"answer\")] [main] micro answer() -> i32 { return 23 }")
            .expect("源码必须形成完整 Canonical 成功载荷");
        let linked = &output.canonical().linked;
        let fragment = linked.fragments.values().next().expect("Compiler 必须绑定至少一个语义片段");
        assert!(!fragment.exported_operations.is_empty(), "片段不得丢失 callable identity");
        assert!(fragment.entry_operation.is_some(), "片段不得丢失入口 identity");
        assert!(fragment.exported_operations.iter().all(|instance| linked.callable_names.contains_key(instance)));
    }

    #[test]
    fn compiler_rejects_unknown_dependency_identity() {
        let groups = vec![CompilerSourceGroup {
            dependency_key: "app".into(),
            name: "app".into(),
            source: "micro main() { return }".into(),
            sources: Vec::new(),
            direct_dependencies: vec!["missing".into()],
        }];
        let error = ValkyrieCompiler::default()
            .compile_source_groups_to_program(&groups)
            .expect_err("unknown dependency must fail at Compiler boundary");
        assert!(error.to_string().contains("semantic dependency export `missing`"));
    }

    #[test]
    fn compiler_rejects_dependency_identity_collision() {
        let groups = vec![
            CompilerSourceGroup {
                dependency_key: "same".into(),
                name: "first".into(),
                source: "micro first() { return }".into(),
                sources: Vec::new(),
                direct_dependencies: Vec::new(),
            },
            CompilerSourceGroup {
                dependency_key: "same".into(),
                name: "second".into(),
                source: "micro second() { return }".into(),
                sources: Vec::new(),
                direct_dependencies: Vec::new(),
            },
        ];
        let error = ValkyrieCompiler::default().compile_source_groups_to_program(&groups).expect_err("duplicate dependency identity must fail");
        assert!(error.to_string().contains("identity collision"));
    }
}

struct CompileWarningScope;

impl CompileWarningScope {
    fn enter() -> Self {
        COMPILE_WARNINGS.with(|warnings| warnings.borrow_mut().clear());
        Self
    }
}

fn push_compile_warning(code: &'static str, message: impl Into<String>, span: SourceSpan) {
    COMPILE_WARNINGS.with(|warnings| warnings.borrow_mut().push(HirCompileWarning { code: code.to_string(), message: message.into(), span }));
}

fn take_compile_warnings() -> Vec<HirCompileWarning> {
    COMPILE_WARNINGS.with(|warnings| std::mem::take(&mut *warnings.borrow_mut()))
}

/// 前端统一验证调用身份、枚举判别与完整语义合同，不按错误文本放行。
fn validate_hir_contract(hir: &HirModule) -> Result<(), ParseError> {
    validate_resolved_call_contracts(hir)?;
    validate_enum_discriminators(hir)?;
    validate_semantic_module(hir)
}

/// Semantic MIR may only be produced from calls carrying an overload-selected
/// contract.  In particular, do not let the SSA lowerer recover a symbol or a
/// result type from source spelling, contextual type, or a backend convention.
fn validate_resolved_call_contracts(hir: &HirModule) -> Result<(), ParseError> {
    for submodule in &hir.submodules {
        validate_resolved_call_contracts(submodule)?;
    }
    for singleton in &hir.singletons {
        for function in &singleton.methods {
            validate_function_call_contracts(function)?;
        }
    }
    for function in &hir.functions {
        validate_function_call_contracts(function)?;
    }
    for structure in &hir.structs {
        for function in &structure.methods {
            validate_function_call_contracts(function)?;
        }
        for property in &structure.properties {
            if let Some(function) = &property.getter {
                validate_function_call_contracts(function)?;
            }
            if let Some(function) = &property.setter {
                validate_function_call_contracts(function)?;
            }
        }
    }
    for trait_def in &hir.traits {
        for function in trait_def.methods.iter().chain(&trait_def.default_methods) {
            validate_function_call_contracts(function)?;
        }
    }
    for implementation in &hir.impls {
        for function in &implementation.methods {
            validate_function_call_contracts(function)?;
        }
    }
    Ok(())
}

fn validate_function_call_contracts(function: &HirFunction) -> Result<(), ParseError> {
    validate_block_call_contracts(&function.body, &function.name.to_string())
}

fn validate_block_call_contracts(block: &HirBlock, function: &str) -> Result<(), ParseError> {
    for statement in &block.statements {
        match &statement.kind {
            HirStatementKind::Let { initializer, .. } => {
                if let Some(initializer) = initializer {
                    validate_expr_call_contracts(initializer, function)?;
                }
            }
            HirStatementKind::Expr(expr) => validate_expr_call_contracts(expr, function)?,
        }
    }
    if let Some(expr) = &block.expr {
        validate_expr_call_contracts(expr, function)?;
    }
    Ok(())
}

fn validate_expr_call_contracts(expr: &HirExpr, function: &str) -> Result<(), ParseError> {
    match &expr.kind {
        HirExprKind::Call { callee, args, resolved, .. } => {
            if resolved.is_none() {
                let callee_name = match &callee.kind {
                    HirExprKind::Path(path) => path.to_string(),
                    _ => "<computed>".to_string(),
                };
                return Err(ParseError::invalid(format!(
                    "SMIR003 unresolved call contract `{callee_name}` in `{function}` at {:?}",
                    expr.span
                )));
            }
            validate_expr_call_contracts(callee, function)?;
            for arg in args {
                validate_expr_call_contracts(&arg.value, function)?;
            }
        }
        HirExprKind::Construct { name, path, args, resolved, .. } => {
            if resolved.is_none() {
                return Err(ParseError::invalid(format!(
                    "SMIR003 unresolved constructor contract `{}` / `{}` in `{function}` at {:?}",
                    name, path, expr.span
                )));
            }
            for arg in args {
                validate_expr_call_contracts(arg, function)?;
            }
        }
        HirExprKind::FieldInit { value, .. }
        | HirExprKind::Await(value)
        | HirExprKind::Awake(value)
        | HirExprKind::BlockOn(value)
        | HirExprKind::YieldFrom(value)
        | HirExprKind::TryPropagate { inner: value, .. }
        | HirExprKind::Raise(value)
        | HirExprKind::Resume(value) => validate_expr_call_contracts(value, function)?,
        HirExprKind::Cast { expr, .. } => validate_expr_call_contracts(expr, function)?,
        HirExprKind::ArrayNew { length, .. } => validate_expr_call_contracts(length, function)?,
        HirExprKind::ArrayLiteral { items } | HirExprKind::TupleLiteral { items } => {
            for item in items {
                validate_expr_call_contracts(item, function)?;
            }
        }
        HirExprKind::FieldAccess { object, .. } => validate_expr_call_contracts(object, function)?,
        HirExprKind::StoreField { object, value, .. } => {
            validate_expr_call_contracts(object, function)?;
            validate_expr_call_contracts(value, function)?;
        }
        HirExprKind::GenericApply { callee, .. } => validate_expr_call_contracts(callee, function)?,
        HirExprKind::Block(block) => validate_block_call_contracts(block, function)?,
        HirExprKind::Lambda { body, .. } => validate_block_call_contracts(body, function)?,
        HirExprKind::AnonymousClass { fields, methods, .. } => {
            for (_, value) in fields {
                validate_expr_call_contracts(value, function)?;
            }
            for method in methods {
                validate_function_call_contracts(method)?;
            }
        }
        HirExprKind::If { condition, then_branch, else_branch } => {
            validate_expr_call_contracts(condition, function)?;
            validate_block_call_contracts(then_branch, function)?;
            if let Some(block) = else_branch {
                validate_block_call_contracts(block, function)?;
            }
        }
        HirExprKind::IfLet { scrutinee, then_branch, else_branch, .. } => {
            validate_expr_call_contracts(scrutinee, function)?;
            validate_block_call_contracts(then_branch, function)?;
            if let Some(block) = else_branch {
                validate_block_call_contracts(block, function)?;
            }
        }
        HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } | HirExprKind::Catch { expr: scrutinee, arms } => {
            validate_expr_call_contracts(scrutinee, function)?;
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    validate_expr_call_contracts(guard, function)?;
                }
                validate_expr_call_contracts(&arm.body, function)?;
            }
        }
        HirExprKind::Loop { iterator, condition, body, .. } => {
            if let Some(iterator) = iterator {
                validate_expr_call_contracts(iterator, function)?;
            }
            if let Some(condition) = condition {
                validate_expr_call_contracts(condition, function)?;
            }
            validate_block_call_contracts(body, function)?;
        }
        HirExprKind::Return(value) | HirExprKind::Yield(value) => {
            if let Some(value) = value {
                validate_expr_call_contracts(value, function)?;
            }
        }
        HirExprKind::Assign { value, .. } => validate_expr_call_contracts(value, function)?,
        HirExprKind::Break { expr, .. } => {
            if let Some(value) = expr {
                validate_expr_call_contracts(value, function)?;
            }
        }
        HirExprKind::TryScope { body, .. } => validate_block_call_contracts(body, function)?,
        HirExprKind::With { base, updates } => {
            validate_expr_call_contracts(base, function)?;
            for (_, value) in updates {
                validate_expr_call_contracts(value, function)?;
            }
        }
        HirExprKind::SuperCall { args, .. } => {
            for arg in args {
                validate_expr_call_contracts(arg, function)?;
            }
        }
        HirExprKind::Literal(_) | HirExprKind::Variable(_) | HirExprKind::Path(_) | HirExprKind::Continue { .. } | HirExprKind::Fallthrough => {
        }
    }
    Ok(())
}

mod expr_lowering;
mod macro_expand;
mod template_const;
mod template_expand;
mod vx;

pub use super::CaptureAnalyzer;
use expr_lowering::{extract_name_path, lower_block, lower_term_expression};
use macro_expand::expand_macros_in_root;
use template_expand::{DEFAULT_COMPILE_ARCH, expand_templates_in_root};
use vx::enhance_vx_widgets;

/// Minimal compiler facade that lowers parser output into HIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValkyrieCompiler {
    /// Source id attached to synthesized spans during lowering.
    pub source_id: SourceID,
}

/// Resolver 交给 Compiler 的有序源码语义组。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilerSourceGroup {
    /// Resolver 分配的稳定依赖身份。
    pub dependency_key: String,
    /// 诊断和模块身份使用的名称。
    pub name: String,
    /// 已完成 staging 的源码内容。
    pub source: String,
    /// 生产源码闭包中的独立文件；每个文件必须单独经过 Oak parse。
    /// `source` 仅保留给旧测试夹具，生产 Resolver 不再把文件拼成一段文本。
    pub sources: Vec<CompilerSourceFile>,
    /// 只允许引用已完成链接的直接依赖身份。
    pub direct_dependencies: Vec<String>,
}

/// 一个具有独立 source span 边界的 Valkyrie 源文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilerSourceFile {
    /// 仓相对或逻辑文件身份，仅用于诊断与证据。
    pub path: String,
    /// 文件原文。
    pub source: String,
}

/// 单元测试中从 HIR 构造阶段夹具，不作为生产源码闭包入口。
#[cfg(test)]
pub(crate) fn compiled_program_from_hir_module(hir_module: HirModule) -> Result<nyar_types::CompiledProgram, ParseError> {
    let semantic_mir = crate::valkyrie::mir::MirLowerer::lower_module_semantic(&hir_module);
    let compiled_program = crate::valkyrie::compile_pipeline::compile_linked_semantic_mir(&semantic_mir)
        .map_err(|error| ParseError::invalid(format!("Compiler 成功载荷生产失败: {error:?}")))?;
    Ok(compiled_program)
}

pub(crate) fn compute_nominal_declarations(module: &HirModule, nominal_offset: u32) -> (Vec<MirSumDeclaration>, Vec<FlagsLayout>) {
    (collect_sum_declarations(module, nominal_offset), collect_flags_layouts(module))
}

fn collect_sum_declarations(module: &HirModule, nominal_offset: u32) -> Vec<MirSumDeclaration> {
    let mut next_field = 0usize;
    let mut next_nominal = nominal_offset;
    let mut next_variant = 0u32;
    let mut layouts = module
        .enums
        .iter()
        .map(|enum_def| {
            let mut next_implicit = 0u32;
            let variants = enum_def
                .variants
                .iter()
                .map(|variant| {
                    // `unite`: `[tag(N)]` or declaration-order fallback.
                    // `enums`: `= N` or auto-increment after the last explicit / implicit tag.
                    let tag = resolve_sum_variant_tag(&enum_def.name, variant, &mut next_implicit)
                        .expect("enum discriminators must be validated before sum layout collection");
                    let id = nyar_types::VariantId::from_index(next_variant).expect("variant identity overflow");
                    next_variant += 1;
                    MirSumVariant {
                        id,
                        declaration: variant.declaration,
                        name: variant.name.to_string(),
                        tag,
                        fields: variant
                            .fields
                            .iter()
                            .map(|field| {
                                let id = nyar_types::FieldId::from_index(next_field as u32).expect("field identity overflow");
                                next_field += 1;
                                crate::mir::MirField { id, name: field.name.to_string(), ty: field.ty.clone() }
                            })
                            .collect(),
                        result_type: variant.result_type.clone(),
                    }
                })
                .collect();
            let nominal = nyar_types::NominalInstanceId::from_index(next_nominal).expect("nominal identity overflow");
            next_nominal += 1;
            MirSumDeclaration {
                nominal,
                declaration: enum_def.declaration,
                name: enum_def.name.to_string(),
                is_unite: enum_def.is_unity,
                generics: enum_def.generics.clone(),
                variants,
            }
        })
        .collect::<Vec<_>>();
    // Dependency packages may define `Result` / `Option` without copying the
    // HirEnum into the consuming module. Fine/Fail/Some still need nominal
    // sum metadata for SumNew / SumPayloadGet contract checks.
    for export in &module.imported_semantic_exports {
        for enum_def in &export.enums {
            let mut next_implicit = 0u32;
            let variants = enum_def
                .variants
                .iter()
                .map(|variant| {
                    let tag = resolve_sum_variant_tag(&enum_def.name, variant, &mut next_implicit)
                        .expect("enum discriminators must be validated before sum layout collection");
                    let id = nyar_types::VariantId::from_index(next_variant).expect("variant identity overflow");
                    next_variant += 1;
                    MirSumVariant {
                        id,
                        declaration: variant.declaration,
                        name: variant.name.to_string(),
                        tag,
                        fields: variant
                            .fields
                            .iter()
                            .map(|field| {
                                let id = nyar_types::FieldId::from_index(next_field as u32).expect("field identity overflow");
                                next_field += 1;
                                crate::mir::MirField { id, name: field.name.to_string(), ty: field.ty.clone() }
                            })
                            .collect(),
                        result_type: variant.result_type.clone(),
                    }
                })
                .collect();
            let nominal = nyar_types::NominalInstanceId::from_index(next_nominal).expect("nominal identity overflow");
            next_nominal += 1;
            let declaration = MirSumDeclaration {
                nominal,
                declaration: enum_def.declaration,
                name: enum_def.name.to_string(),
                is_unite: enum_def.is_unity,
                generics: enum_def.generics.clone(),
                variants,
            };
            if !layouts.iter().any(|prior| prior.name == declaration.name) {
                layouts.push(declaration);
            }
        }
    }
    layouts
}

fn integer_literal_u32(expr: &HirExpr) -> Option<u32> {
    match &expr.kind {
        HirExprKind::Literal(HirLiteral::Integer64(value)) if *value >= 0 => u32::try_from(*value).ok(),
        _ => None,
    }
}

/// Resolve the next variant tag for `unite` (`[tag(N)]` or declaration order) and
/// `enums` (`Variant = N` or auto-increment after the last assigned tag).
fn resolve_sum_variant_tag(owner: &crate::types::Identifier, variant: &HirVariant, next_implicit: &mut u32) -> Result<u32, ParseError> {
    let tag = if let Some(discriminator) = &variant.discriminator {
        integer_literal_u32(discriminator).ok_or_else(|| {
            ParseError::invalid(format!("`{}` variant `{}` discriminator must be a non-negative integer literal", owner, variant.name))
        })?
    }
    else {
        let tag = *next_implicit;
        *next_implicit = tag.saturating_add(1);
        tag
    };
    if variant.discriminator.is_some() {
        *next_implicit = tag.saturating_add(1);
    }
    Ok(tag)
}

fn validate_sum_type_discriminators(owner: &crate::types::Identifier, variants: &[HirVariant]) -> Result<(), ParseError> {
    use std::collections::BTreeSet;

    let mut next_implicit = 0u32;
    let mut seen = BTreeSet::new();
    for variant in variants {
        let tag = resolve_sum_variant_tag(owner, variant, &mut next_implicit)?;
        if !seen.insert(tag) {
            return Err(ParseError::invalid(format!("`{}` has duplicate discriminator {tag}", owner)));
        }
    }
    Ok(())
}

fn validate_enum_discriminators(module: &HirModule) -> Result<(), ParseError> {
    for enum_def in &module.enums {
        validate_sum_type_discriminators(&enum_def.name, &enum_def.variants)?;
    }
    for export in &module.imported_semantic_exports {
        for enum_def in &export.enums {
            validate_sum_type_discriminators(&enum_def.name, &enum_def.variants)?;
        }
    }
    Ok(())
}

fn collect_flags_layouts(module: &HirModule) -> Vec<FlagsLayout> {
    module.flags.iter().map(|flags| FlagsLayout { name: flags.name.to_string() }).collect()
}

impl Default for ValkyrieCompiler {
    fn default() -> Self {
        Self::new(SourceID::default())
    }
}

impl ValkyrieCompiler {
    /// Creates a compiler facade bound to a source id.
    pub fn new(source_id: SourceID) -> Self {
        Self { source_id }
    }

    /// Validates an already materialized HIR module before it crosses into
    /// Semantic MIR. Cache consumers must use this instead of trusting the
    /// schema version of serialized HIR as a semantic guarantee.
    pub fn validate_hir_semantic_contract(&self, hir: &HirModule) -> Result<(), ParseError> {
        validate_interop_surface(hir)?;
        validate_hir_contract(hir)
    }

    /// Parses source text and lowers it into a minimal HIR module.
    pub fn compile_source(&self, source: &str) -> Result<HirModule, ParseError> {
        self.compile_source_with_semantic_exports(source, &[])
    }

    /// Parses source text with nominal metadata exported by resolved
    /// dependencies. This metadata is supplied by the workspace resolver,
    /// never reconstructed from dependency source text in this consumer.
    pub fn compile_source_with_semantic_exports(
        &self,
        source: &str,
        imported_semantic_exports: &[HirDependencySemanticExport],
    ) -> Result<HirModule, ParseError> {
        self.compile_source_with_semantic_exports_and_name(source, imported_semantic_exports, None)
    }

    /// 在解析调用前固定 Resolver 分配的模块身份。
    pub fn compile_source_with_semantic_exports_and_name(
        &self,
        source: &str,
        imported_semantic_exports: &[HirDependencySemanticExport],
        module_name: Option<NamePath>,
    ) -> Result<HirModule, ParseError> {
        let mut root = frontend::parse_source(source)?;
        expand_macros_in_root(&mut root);
        expand_templates_in_root(&mut root, source, DEFAULT_COMPILE_ARCH)?;
        let hir = self.lower_root_with_semantic_exports_and_name(&root, imported_semantic_exports, module_name)?;
        self.validate_hir_semantic_contract(&hir)?;
        Ok(hir)
    }

    /// 正式源码组的第一阶段：只物化 HIR，不解析调用。
    ///
    /// 调用解析必须等完整源码闭包进入 Compiler 后统一进行；该入口不产生
    /// 可执行产物，也不允许被装配层直接使用。
    pub(crate) fn parse_source_group_without_call_resolution(
        &self,
        source: &str,
        imported_semantic_exports: &[HirDependencySemanticExport],
        module_name: Option<NamePath>,
        arch: &str,
    ) -> Result<HirModule, ParseError> {
        let mut root = frontend::parse_source(source)?;
        expand_macros_in_root(&mut root);
        expand_templates_in_root(&mut root, source, arch)?;
        let hir = AstToHir::new(self.source_id).lower_root_without_call_resolution(&root, imported_semantic_exports, module_name)?;
        validate_interop_surface(&hir)?;
        Ok(hir)
    }

    /// Parses a source file and lowers it into a minimal HIR module.
    pub fn compile_path(&self, path: &Path) -> Result<HirModule, ParseError> {
        let source = std::fs::read_to_string(path)?;
        self.compile_source(&source)
    }

    /// Parses `.vx` source (Valkyrie + X-Grammar) and lowers into HIR with `view` → `render` normalization.
    pub fn compile_vx_source(&self, source: &str) -> Result<HirModule, ParseError> {
        Err(ParseError::invalid("Oak frontend does not support `.vx` source yet"))
    }

    /// Parses a `.vx` file and lowers it into HIR with `view` → `render` normalization.
    pub fn compile_vx_path(&self, path: &Path) -> Result<HirModule, ParseError> {
        let source = std::fs::read_to_string(path)?;
        self.compile_vx_source(&source)
    }

    /// 测试夹具：将单一源码降低为成功载荷。
    #[cfg(test)]
    pub(crate) fn compile_source_to_program(&self, source: &str) -> Result<nyar_types::CompiledProgram, ParseError> {
        let hir_module = self.compile_source(source)?;
        compiled_program_from_hir_module(hir_module)
    }

    /// 从完整依赖顺序的源码快照构建一个语义闭包。
    ///
    /// Resolver 只提供源码和依赖身份；导出合同、依赖 MIR 与可达链接全部
    /// 在 Compiler 内完成，调用方不得自行拼接 HIR 或 MIR。
    pub(crate) fn compile_source_groups_to_program(&self, groups: &[CompilerSourceGroup]) -> Result<nyar_types::CompiledProgram, ParseError> {
        self.compile_source_groups_to_program_with_host_bindings(groups, &[], DEFAULT_COMPILE_ARCH)
    }

    /// 从完整依赖顺序的源码快照构建语义闭包，并应用 Resolver 选定的 host provider 绑定。
    pub(crate) fn compile_source_groups_to_program_with_host_bindings(
        &self,
        groups: &[CompilerSourceGroup],
        host_bindings: &[crate::valkyrie::compile_pipeline::CompilerHostProviderBinding],
        arch: &str,
    ) -> Result<nyar_types::CompiledProgram, ParseError> {
        let mut hir_groups = self.resolve_source_groups(groups, arch)?;
        let final_hir = hir_groups.pop().ok_or_else(|| ParseError::invalid("semantic source group plan is empty"))?;
        // monomorph 必须接在整份闭包 HIR 实例号之后，避免与后包声明实例撞号。
        let mut mono_counter = Some(
            std::iter::once(&final_hir)
                .chain(hir_groups.iter())
                .filter_map(max_hir_callable_instance_index)
                .max()
                .map(|index| index.saturating_add(1))
                .unwrap_or(0),
        );
        let mut mir_groups = hir_groups
            .iter()
            .map(|hir| crate::valkyrie::mir::MirLowerer::lower_module_semantic_with_mono_counter(hir, &mut mono_counter))
            .collect::<Vec<_>>();
        let mut final_mir = crate::valkyrie::mir::MirLowerer::lower_module_semantic_with_mono_counter(&final_hir, &mut mono_counter);
        let modules = std::iter::once(&final_mir).chain(mir_groups.iter()).collect::<Vec<_>>();
        let functions = modules.iter().flat_map(|module| module.functions.iter().cloned()).collect::<Vec<_>>();
        let imports = modules.iter().flat_map(|module| module.external_calls.iter().cloned()).collect::<Vec<_>>();
        let structures = modules.iter().flat_map(|module| module.structs.iter().cloned()).collect::<Vec<_>>();
        let type_identities = crate::valkyrie::mir::ssa::type_identity_table(&functions, &imports, &structures, &final_mir.sum_types);
        final_mir.type_identities = type_identities.clone();
        for module in &mut mir_groups {
            module.type_identities = type_identities.clone();
        }
        if !mir_groups.is_empty() {
            crate::valkyrie::compile_pipeline::link_reachable_dependency_mir(&mut final_mir, &mir_groups)?;
        }
        crate::valkyrie::compile_pipeline::apply_host_provider_bindings(&mut final_mir, &mir_groups, host_bindings)?;
        crate::valkyrie::compile_pipeline::compile_linked_semantic_mir(&final_mir)
            .map_err(|error| ParseError::invalid(format!("Compiler 成功载荷生产失败: {error:?}")))
    }

    fn resolve_source_groups(&self, groups: &[CompilerSourceGroup], arch: &str) -> Result<Vec<HirModule>, ParseError> {
        let mut exports = std::collections::BTreeMap::<String, HirDependencySemanticExport>::new();
        let mut hir_groups = Vec::with_capacity(groups.len());
        let mut next_declaration = 0u32;
        let mut next_instance = 0u32;
        for group in groups {
            let dependency_exports = group
                .direct_dependencies
                .iter()
                .map(|name| {
                    exports
                        .get(name)
                        .cloned()
                        .ok_or_else(|| ParseError::invalid(format!("semantic dependency export `{name}` is unavailable for `{}`", group.name)))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut hir_module = self.parse_source_files_without_call_resolution(
                group,
                &dependency_exports,
                Some(NamePath::new(vec![Identifier::new(&group.name)])),
                arch,
            )?;
            register_function_declarations(&mut hir_module, &mut next_declaration, &mut next_instance)?;
            bind_declared_nominal_types(&mut hir_module)?;
            trait_binding::bind_trait_declarations(&mut hir_module)?;
            let export = HirDependencySemanticExport {
                module: NamePath::new(vec![Identifier::new(&group.name)]),
                functions: exportable_dependency_functions(&hir_module.functions),
                structs: hir_module.structs.clone(),
                enums: hir_module.enums.clone(),
                traits: hir_module.traits.clone(),
                type_aliases: hir_module.type_aliases.clone(),
                impls: hir_module.impls.clone(),
            };
            if exports.insert(group.dependency_key.clone(), export).is_some() {
                return Err(ParseError::invalid(format!("semantic dependency export identity collision for `{}`", group.dependency_key)));
            }
            hir_groups.push(hir_module);
        }
        for hir in &mut hir_groups {
            resolve_hir_calls(hir);
            validate_extractor_patterns(hir)?;
            self.validate_hir_semantic_contract(hir)?;
        }
        Ok(hir_groups)
    }

    fn parse_source_files_without_call_resolution(
        &self,
        group: &CompilerSourceGroup,
        imported_semantic_exports: &[HirDependencySemanticExport],
        module_name: Option<NamePath>,
        arch: &str,
    ) -> Result<HirModule, ParseError> {
        if group.sources.is_empty() {
            return self.parse_source_group_without_call_resolution(&group.source, imported_semantic_exports, module_name, arch);
        }
        let mut merged: Option<HirModule> = None;
        for file in &group.sources {
            let mut root = frontend::parse_source(&file.source).map_err(|error| match error {
                ParseError::Invalid { message, span } => {
                    ParseError::Invalid { message: format!("源码组 `{}` 文件 `{}`: {message}", group.dependency_key, file.path), span }
                }
                other => other,
            })?;
            expand_macros_in_root(&mut root);
            expand_templates_in_root(&mut root, &file.source, arch)?;
            let module = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                AstToHir::new(self.source_id).lower_root_without_call_resolution(&root, imported_semantic_exports, module_name.clone())
            }))
            .map_err(|_| ParseError::invalid(format!("source file `{}` contains an unexpanded syntax node", file.path)))??;
            if let Some(target) = &mut merged {
                target.imports.extend(module.imports);
                target.warnings.extend(module.warnings);
                target.submodules.extend(module.submodules);
                target.functions.extend(module.functions);
                target.structs.extend(module.structs);
                target.enums.extend(module.enums);
                target.imported_enums.extend(module.imported_enums);
                target.flags.extend(module.flags);
                target.traits.extend(module.traits);
                target.impls.extend(module.impls);
                target.type_functions.extend(module.type_functions);
                target.type_families.extend(module.type_families);
                target.widgets.extend(module.widgets);
                target.singletons.extend(module.singletons);
                target.statements.extend(module.statements);
                target.type_aliases.extend(module.type_aliases);
            }
            else {
                merged = Some(module);
            }
        }
        let hir = merged.ok_or_else(|| ParseError::invalid(format!("semantic source group `{}` has no source files", group.name)))?;
        validate_interop_surface(&hir)?;
        Ok(hir)
    }

    /// Lowers parser output into a HIR module.
    pub fn lower_root(&self, root: &ValkyrieRoot) -> Result<HirModule, ParseError> {
        self.lower_root_with_semantic_exports(root, &[])
    }

    /// Lowers parser output with resolved nominal dependency exports in scope.
    pub fn lower_root_with_semantic_exports(
        &self,
        root: &ValkyrieRoot,
        imported_semantic_exports: &[HirDependencySemanticExport],
    ) -> Result<HirModule, ParseError> {
        AstToHir::new(self.source_id).lower_root_with_semantic_exports(root, imported_semantic_exports)
    }

    /// 在 HIR 调用解析前覆盖模块身份。
    pub fn lower_root_with_semantic_exports_and_name(
        &self,
        root: &ValkyrieRoot,
        imported_semantic_exports: &[HirDependencySemanticExport],
        module_name: Option<NamePath>,
    ) -> Result<HirModule, ParseError> {
        AstToHir::new(self.source_id).lower_root_with_semantic_exports_and_name(root, imported_semantic_exports, module_name)
    }
}

/// 在结构体/枚举已分配 declaration ItemId 之后，把唯一可解析的 `Named`/`Apply(Named, …)`
/// 绑定为 [`ValkyrieType::Nominal`]。这是 Resolver→HIR 的声明身份冻结，不是短名改写或后置补丁。
fn bind_declared_nominal_types(module: &mut HirModule) -> Result<(), ParseError> {
    fn qualified_struct(item: &HirStruct) -> Identifier {
        if item.namespace.is_empty() {
            return item.name.clone();
        }
        let mut parts = item.namespace.iter().map(|part| part.as_str()).collect::<Vec<_>>();
        parts.push(item.name.as_str());
        Identifier::new(&parts.join("."))
    }
    fn collect_structs(module: &HirModule, out: &mut Vec<HirStruct>) {
        out.extend(module.structs.iter().cloned());
        for export in &module.imported_semantic_exports {
            out.extend(export.structs.iter().cloned());
        }
        for submodule in &module.submodules {
            collect_structs(submodule, out);
        }
    }
    fn collect_enums(module: &HirModule, out: &mut Vec<HirEnum>) {
        out.extend(module.enums.iter().cloned());
        for export in &module.imported_semantic_exports {
            out.extend(export.enums.iter().cloned());
        }
        for submodule in &module.submodules {
            collect_enums(submodule, out);
        }
    }
    fn unique_struct<'a>(name: &Identifier, namespace: &NamePath, structs: &'a [HirStruct]) -> Result<Option<&'a HirStruct>, ParseError> {
        let exact: Vec<&HirStruct> = structs
            .iter()
            .filter(|item| qualified_struct(item) == *name || (item.name == *name && NamePath::new(item.namespace.clone()) == *namespace))
            .collect();
        if exact.len() > 1 {
            return Err(ParseError::invalid(format!("名义类型 `{name}` 在作用域内歧义，无法冻结 declaration identity")));
        }
        if exact.len() == 1 {
            return Ok(Some(exact[0]));
        }
        let short: Vec<&HirStruct> = structs.iter().filter(|item| &item.name == name).collect();
        if short.len() > 1 {
            return Err(ParseError::invalid(format!("名义类型短名 `{name}` 对应多个声明，禁止按短名猜测 identity")));
        }
        Ok(short.into_iter().next())
    }
    fn unique_enum<'a>(name: &Identifier, enums: &'a [HirEnum]) -> Result<Option<&'a HirEnum>, ParseError> {
        let matches: Vec<&HirEnum> = enums.iter().filter(|item| &item.name == name).collect();
        if matches.len() > 1 {
            return Err(ParseError::invalid(format!("sum/unite `{name}` 对应多个声明，禁止按短名猜测 identity")));
        }
        Ok(matches.into_iter().next())
    }
    fn resolve_declaration(
        name: &Identifier,
        namespace: &NamePath,
        structs: &[HirStruct],
        enums: &[HirEnum],
    ) -> Result<Option<nyar_types::ItemId>, ParseError> {
        if let Some(item) = unique_struct(name, namespace, structs)? {
            return Ok(Some(item.constructor_declaration.ok_or_else(|| {
                ParseError::invalid(format!("结构 `{}` 缺少 constructor_declaration，无法绑定名义类型", qualified_struct(item)))
            })?));
        }
        if let Some(item) = unique_enum(name, enums)? {
            return Ok(Some(
                item.declaration.ok_or_else(|| ParseError::invalid(format!("sum/unite `{}` 缺少 declaration，无法绑定名义类型", item.name)))?,
            ));
        }
        Ok(None)
    }
    fn bind_type(ty: &mut ValkyrieType, namespace: &NamePath, structs: &[HirStruct], enums: &[HirEnum]) -> Result<(), ParseError> {
        match ty {
            ValkyrieType::Nominal { arguments, .. } => {
                for argument in arguments {
                    bind_type(argument, namespace, structs, enums)?;
                }
            }
            ValkyrieType::Named(name) => {
                if let Some(declaration) = resolve_declaration(name, namespace, structs, enums)? {
                    *ty = ValkyrieType::Nominal { declaration, arguments: Vec::new() };
                }
            }
            ValkyrieType::Apply(base, args) => {
                for arg in args.iter_mut() {
                    bind_type(arg, namespace, structs, enums)?;
                }
                let named_base = match base.as_ref() {
                    ValkyrieType::Named(name) => Some(name.clone()),
                    _ => None,
                };
                if let Some(name) = named_base {
                    if let Some(declaration) = resolve_declaration(&name, namespace, structs, enums)? {
                        let arguments = std::mem::take(args);
                        *ty = ValkyrieType::Nominal { declaration, arguments };
                        return Ok(());
                    }
                }
                bind_type(base, namespace, structs, enums)?;
            }
            ValkyrieType::Array(inner) | ValkyrieType::Nullable(inner) => bind_type(inner, namespace, structs, enums)?,
            ValkyrieType::FixedArray { element, .. } => bind_type(element, namespace, structs, enums)?,
            ValkyrieType::Tuple(items) | ValkyrieType::Union(items) | ValkyrieType::Intersection(items) => {
                for item in items {
                    bind_type(item, namespace, structs, enums)?;
                }
            }
            ValkyrieType::Function(function) => {
                for param in &mut function.params {
                    bind_type(param, namespace, structs, enums)?;
                }
                bind_type(&mut function.return_type, namespace, structs, enums)?;
            }
            ValkyrieType::Associated(associated) => {
                bind_type(&mut associated.base, namespace, structs, enums)?;
                for argument in &mut associated.type_arguments {
                    bind_type(argument, namespace, structs, enums)?;
                }
            }
            ValkyrieType::TraitObject(object) => {
                for argument in &mut object.type_arguments {
                    bind_type(argument, namespace, structs, enums)?;
                }
            }
            ValkyrieType::Row(row) => {
                for method in &mut row.methods {
                    for param in &mut method.params {
                        bind_type(param, namespace, structs, enums)?;
                    }
                    bind_type(&mut method.return_type, namespace, structs, enums)?;
                }
            }
            ValkyrieType::TypeLambda(lambda) => {
                bind_type(&mut lambda.body, namespace, structs, enums)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn bind_expr(expr: &mut HirExpr, namespace: &NamePath, structs: &[HirStruct], enums: &[HirEnum]) -> Result<(), ParseError> {
        match &mut expr.kind {
            HirExprKind::Call { callee, args, resolved, .. } => {
                bind_expr(callee, namespace, structs, enums)?;
                for arg in args {
                    bind_expr(&mut arg.value, namespace, structs, enums)?;
                }
                if let Some(contract) = resolved {
                    for param in &mut contract.parameter_types {
                        bind_type(param, namespace, structs, enums)?;
                    }
                    bind_type(&mut contract.return_type, namespace, structs, enums)?;
                    if let Some(payload) = &mut contract.extractor_payload_type {
                        bind_type(payload, namespace, structs, enums)?;
                    }
                }
            }
            HirExprKind::Construct { args, resolved, .. } => {
                for arg in args {
                    bind_expr(arg, namespace, structs, enums)?;
                }
                if let Some(contract) = resolved {
                    for param in &mut contract.parameter_types {
                        bind_type(param, namespace, structs, enums)?;
                    }
                    bind_type(&mut contract.return_type, namespace, structs, enums)?;
                }
            }
            HirExprKind::Cast { expr: inner, ty, .. } => {
                bind_expr(inner, namespace, structs, enums)?;
                bind_type(ty, namespace, structs, enums)?;
            }
            HirExprKind::Lambda { params, return_type, body, .. } => {
                for param in params {
                    bind_type(&mut param.ty, namespace, structs, enums)?;
                }
                bind_type(return_type, namespace, structs, enums)?;
                bind_block(body, namespace, structs, enums)?;
            }
            HirExprKind::TryScope { result_type, body, .. } => {
                if let Some(ty) = result_type {
                    bind_type(ty, namespace, structs, enums)?;
                }
                bind_block(body, namespace, structs, enums)?;
            }
            HirExprKind::FieldInit { value, .. }
            | HirExprKind::Await(value)
            | HirExprKind::Awake(value)
            | HirExprKind::BlockOn(value)
            | HirExprKind::YieldFrom(value)
            | HirExprKind::TryPropagate { inner: value, .. }
            | HirExprKind::Raise(value)
            | HirExprKind::Resume(value)
            | HirExprKind::Return(Some(value))
            | HirExprKind::Yield(Some(value))
            | HirExprKind::Break { expr: Some(value), .. }
            | HirExprKind::Assign { value, .. }
            | HirExprKind::FieldAccess { object: value, .. }
            | HirExprKind::GenericApply { callee: value, .. } => bind_expr(value, namespace, structs, enums)?,
            HirExprKind::StoreField { object, value, .. } => {
                bind_expr(object, namespace, structs, enums)?;
                bind_expr(value, namespace, structs, enums)?;
            }
            HirExprKind::ArrayNew { length, .. } => bind_expr(length, namespace, structs, enums)?,
            HirExprKind::ArrayLiteral { items } | HirExprKind::TupleLiteral { items } => {
                for item in items {
                    bind_expr(item, namespace, structs, enums)?;
                }
            }
            HirExprKind::Block(block) => bind_block(block, namespace, structs, enums)?,
            HirExprKind::If { condition, then_branch, else_branch, .. } => {
                bind_expr(condition, namespace, structs, enums)?;
                bind_block(then_branch, namespace, structs, enums)?;
                if let Some(else_branch) = else_branch {
                    bind_block(else_branch, namespace, structs, enums)?;
                }
            }
            HirExprKind::IfLet { pattern: _, scrutinee, then_branch, else_branch, .. } => {
                bind_expr(scrutinee, namespace, structs, enums)?;
                bind_block(then_branch, namespace, structs, enums)?;
                if let Some(else_branch) = else_branch {
                    bind_block(else_branch, namespace, structs, enums)?;
                }
            }
            HirExprKind::Match { scrutinee, arms, .. } | HirExprKind::Case { scrutinee, arms, .. } => {
                bind_expr(scrutinee, namespace, structs, enums)?;
                for arm in arms {
                    bind_expr(&mut arm.body, namespace, structs, enums)?;
                }
            }
            HirExprKind::AnonymousClass { fields, methods, .. } => {
                for (_, value) in fields {
                    bind_expr(value, namespace, structs, enums)?;
                }
                for method in methods {
                    bind_function(method, structs, enums)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn bind_block(block: &mut HirBlock, namespace: &NamePath, structs: &[HirStruct], enums: &[HirEnum]) -> Result<(), ParseError> {
        for statement in &mut block.statements {
            match &mut statement.kind {
                HirStatementKind::Let { ty, initializer, .. } => {
                    if let Some(annotation) = ty {
                        bind_type(annotation, namespace, structs, enums)?;
                    }
                    if let Some(initializer) = initializer {
                        bind_expr(initializer, namespace, structs, enums)?;
                    }
                }
                HirStatementKind::Expr(expr) => bind_expr(expr, namespace, structs, enums)?,
            }
        }
        if let Some(expr) = &mut block.expr {
            bind_expr(expr, namespace, structs, enums)?;
        }
        Ok(())
    }
    fn bind_function(function: &mut HirFunction, structs: &[HirStruct], enums: &[HirEnum]) -> Result<(), ParseError> {
        for param in &mut function.params {
            bind_type(&mut param.ty, &function.declaring_namespace, structs, enums)?;
        }
        bind_type(&mut function.return_type, &function.declaring_namespace, structs, enums)?;
        for constraint in &mut function.where_constraints {
            bind_type(&mut constraint.target, &function.declaring_namespace, structs, enums)?;
            for bound in &mut constraint.bounds {
                for argument in &mut bound.type_arguments {
                    bind_type(argument, &function.declaring_namespace, structs, enums)?;
                }
                for equation in &mut bound.associated_types {
                    bind_type(&mut equation.ty, &function.declaring_namespace, structs, enums)?;
                }
            }
        }
        // 函数体注解（let/cast/lambda 等）必须同步冻结；否则 return 校验会把残留 Named 与已绑定 Nominal 当成不兼容。
        bind_block(&mut function.body, &function.declaring_namespace, structs, enums)
    }
    let mut structs = Vec::new();
    let mut enums = Vec::new();
    collect_structs(module, &mut structs);
    collect_enums(module, &mut enums);
    for item in &mut module.structs {
        let namespace = NamePath::new(item.namespace.clone());
        for field in &mut item.fields {
            bind_type(&mut field.ty, &namespace, &structs, &enums)?;
        }
        for function in &mut item.methods {
            bind_function(function, &structs, &enums)?;
        }
    }
    for enum_def in &mut module.enums {
        for variant in &mut enum_def.variants {
            for field in &mut variant.fields {
                bind_type(&mut field.ty, &NamePath::default(), &structs, &enums)?;
            }
            if let Some(result) = &mut variant.result_type {
                bind_type(result, &NamePath::default(), &structs, &enums)?;
            }
        }
    }
    for function in &mut module.functions {
        bind_function(function, &structs, &enums)?;
    }
    for singleton in &mut module.singletons {
        let namespace = NamePath::new(singleton.namespace.clone());
        for field in &mut singleton.fields {
            bind_type(&mut field.ty, &namespace, &structs, &enums)?;
        }
        for function in &mut singleton.methods {
            bind_function(function, &structs, &enums)?;
        }
        if let Some(function) = &mut singleton.constructor {
            bind_function(function, &structs, &enums)?;
        }
        if let Some(function) = &mut singleton.finalizer {
            bind_function(function, &structs, &enums)?;
        }
    }
    for impl_block in &mut module.impls {
        bind_type(&mut impl_block.target, &NamePath::default(), &structs, &enums)?;
        for function in &mut impl_block.methods {
            bind_function(function, &structs, &enums)?;
        }
    }
    for trait_def in &mut module.traits {
        for function in trait_def.methods.iter_mut().chain(trait_def.default_methods.iter_mut()) {
            bind_function(function, &structs, &enums)?;
        }
    }
    for submodule in &mut module.submodules {
        bind_declared_nominal_types(submodule)?;
    }
    Ok(())
}

/// 整份 HIR 模块（含子模块）已冻结 callable `ItemInstanceId` 的最大下标。
fn max_hir_callable_instance_index(module: &HirModule) -> Option<u32> {
    fn consider(instance: Option<nyar_types::ItemInstanceId>, max: &mut Option<u32>) {
        if let Some(instance) = instance {
            let index = instance.index();
            *max = Some(max.map_or(index, |current| current.max(index)));
        }
    }
    fn walk(module: &HirModule, max: &mut Option<u32>) {
        for function in &module.functions {
            consider(function.instance, max);
        }
        for structure in &module.structs {
            consider(structure.constructor_instance, max);
            for method in &structure.methods {
                consider(method.instance, max);
            }
            for property in &structure.properties {
                for accessor in property.getter.iter().chain(property.setter.iter()) {
                    consider(accessor.instance, max);
                }
            }
        }
        for enum_definition in &module.enums {
            for variant in &enum_definition.variants {
                consider(variant.instance, max);
            }
        }
        for singleton in &module.singletons {
            for method in
                singleton.methods.iter().chain(singleton.constructor.iter().map(Box::as_ref)).chain(singleton.finalizer.iter().map(Box::as_ref))
            {
                consider(method.instance, max);
            }
        }
        for trait_definition in &module.traits {
            for method in trait_definition.methods.iter().chain(trait_definition.default_methods.iter()) {
                consider(method.instance, max);
            }
        }
        for implementation in &module.impls {
            for method in &implementation.methods {
                consider(method.instance, max);
            }
        }
        for widget in &module.widgets {
            for method in &widget.methods {
                consider(method.instance, max);
            }
        }
        for submodule in &module.submodules {
            walk(submodule, max);
        }
    }
    let mut max = None;
    walk(module, &mut max);
    max
}

fn register_function_declarations(module: &mut HirModule, next: &mut u32, next_instance: &mut u32) -> Result<(), ParseError> {
    fn register(function: &mut HirFunction, next: &mut u32, next_instance: &mut u32, owner_monomorphic: bool) -> Result<(), ParseError> {
        if function.declaration.is_some() || function.instance.is_some() {
            return Err(ParseError::invalid("源码声明不能重复分配 ItemId"));
        }
        let following = next.checked_add(1).ok_or_else(|| ParseError::invalid("ItemId 声明空间耗尽"))?;
        function.declaration = Some(nyar_types::ItemId::from_index(*next).ok_or_else(|| ParseError::invalid("无效 ItemId"))?);
        *next = following;
        // 顶层与单例成员在 MIR 中总会物化；泛型种子（如 `Some<T>` / `option_none<T>`）同样需要 callable instance。
        if owner_monomorphic {
            let following = next_instance.checked_add(1).ok_or_else(|| ParseError::invalid("ItemInstanceId 实例空间耗尽"))?;
            function.instance =
                Some(nyar_types::ItemInstanceId::from_index(*next_instance).ok_or_else(|| ParseError::invalid("无效 ItemInstanceId"))?);
            *next_instance = following;
        }
        Ok(())
    }
    for function in &mut module.functions {
        register(function, next, next_instance, true)?;
    }
    for structure in &mut module.structs {
        if structure.constructor_declaration.is_some() || structure.constructor_instance.is_some() {
            return Err(ParseError::invalid("结构构造声明不能重复注册"));
        }
        structure.constructor_declaration = Some(nyar_types::ItemId::from_index(*next).ok_or_else(|| ParseError::invalid("无效 ItemId"))?);
        *next = next.checked_add(1).ok_or_else(|| ParseError::invalid("ItemId 声明空间耗尽"))?;
        if structure.generics.is_empty() {
            structure.constructor_instance =
                Some(nyar_types::ItemInstanceId::from_index(*next_instance).ok_or_else(|| ParseError::invalid("无效 ItemInstanceId"))?);
            *next_instance = next_instance.checked_add(1).ok_or_else(|| ParseError::invalid("ItemInstanceId 实例空间耗尽"))?;
        }
        for method in &mut structure.methods {
            register(method, next, next_instance, structure.generics.is_empty())?;
        }
        for property in &mut structure.properties {
            for accessor in property.getter.iter_mut().chain(property.setter.iter_mut()) {
                register(accessor, next, next_instance, structure.generics.is_empty())?;
            }
        }
    }
    for enum_definition in &mut module.enums {
        if enum_definition.declaration.is_some() {
            return Err(ParseError::invalid("枚举声明不能重复注册"));
        }
        enum_definition.declaration = Some(nyar_types::ItemId::from_index(*next).ok_or_else(|| ParseError::invalid("无效 ItemId"))?);
        *next = next.checked_add(1).ok_or_else(|| ParseError::invalid("ItemId 声明空间耗尽"))?;
        for variant in &mut enum_definition.variants {
            if variant.declaration.is_some() || variant.instance.is_some() {
                return Err(ParseError::invalid("变体构造声明不能重复注册"));
            }
            variant.declaration = Some(nyar_types::ItemId::from_index(*next).ok_or_else(|| ParseError::invalid("无效 ItemId"))?);
            *next = next.checked_add(1).ok_or_else(|| ParseError::invalid("ItemId 声明空间耗尽"))?;
            if enum_definition.generics.is_empty() {
                variant.instance =
                    Some(nyar_types::ItemInstanceId::from_index(*next_instance).ok_or_else(|| ParseError::invalid("无效 ItemInstanceId"))?);
                *next_instance = next_instance.checked_add(1).ok_or_else(|| ParseError::invalid("ItemInstanceId 实例空间耗尽"))?;
            }
        }
    }
    for singleton in &mut module.singletons {
        for method in singleton
            .methods
            .iter_mut()
            .chain(singleton.constructor.iter_mut().map(Box::as_mut))
            .chain(singleton.finalizer.iter_mut().map(Box::as_mut))
        {
            register(method, next, next_instance, true)?;
        }
    }
    for trait_definition in &mut module.traits {
        if trait_definition.declaration.is_some() {
            return Err(ParseError::invalid("trait 声明不能重复注册"));
        }
        trait_definition.declaration = Some(nyar_types::ItemId::from_index(*next).ok_or_else(|| ParseError::invalid("无效 ItemId"))?);
        *next = next.checked_add(1).ok_or_else(|| ParseError::invalid("ItemId 声明空间耗尽"))?;
        for method in trait_definition.methods.iter_mut() {
            register(method, next, next_instance, false)?;
        }
        for method in trait_definition.default_methods.iter_mut() {
            // Trait 默认实现会物化到 MIR（`Trait.method`），必须有 callable instance。
            // 泛型 trait（如 `As<T>`）的默认体同样进入 Canonical，不能只在单态 trait 上分配 instance。
            let materialized_default = !method.is_abstract;
            register(method, next, next_instance, materialized_default)?;
        }
    }
    for implementation in &mut module.impls {
        for method in &mut implementation.methods {
            register(method, next, next_instance, implementation.generics.is_empty())?;
        }
    }
    for widget in &mut module.widgets {
        for method in &mut widget.methods {
            register(method, next, next_instance, widget.generics.is_empty())?;
        }
    }
    for submodule in &mut module.submodules {
        register_function_declarations(submodule, next, next_instance)?;
    }
    Ok(())
}

/// Trait 成员以 `;` 结束时 parser 常给出空块而非 `body: None`，需回收为纯签名。
fn finalize_trait_method_signature(mut method: HirFunction) -> HirFunction {
    if method.body.statements.is_empty() && method.body.expr.is_none() {
        method.is_abstract = true;
    }
    method
}

fn lower_trait_method(mut method: HirFunction, associated_names: &std::collections::BTreeSet<Identifier>) -> HirFunction {
    let generic_names = method.generics.iter().map(|generic| generic.name.clone()).collect::<std::collections::BTreeSet<_>>();
    for param in &mut method.params {
        param.ty = lower_trait_associated_type_references(&param.ty, associated_names, &generic_names);
    }
    method.return_type = lower_trait_associated_type_references(&method.return_type, associated_names, &generic_names);
    for constraint in &mut method.where_constraints {
        constraint.target = lower_trait_associated_type_references(&constraint.target, associated_names, &generic_names);
        for bound in &mut constraint.bounds {
            for argument in &mut bound.type_arguments {
                *argument = lower_trait_associated_type_references(argument, associated_names, &generic_names);
            }
            for equation in &mut bound.associated_types {
                equation.ty = lower_trait_associated_type_references(&equation.ty, associated_names, &generic_names);
            }
        }
    }
    method
}

fn lower_trait_associated_type_references(
    ty: &ValkyrieType,
    associated_names: &std::collections::BTreeSet<Identifier>,
    generic_names: &std::collections::BTreeSet<Identifier>,
) -> ValkyrieType {
    match ty {
        ValkyrieType::Named(name) if associated_names.contains(name) && !generic_names.contains(name) => {
            ValkyrieType::Associated(Box::new(crate::types::hir::AssociatedType {
                base: ValkyrieType::SelfType,
                name: name.clone(),
                type_arguments: Vec::new(),
            }))
        }
        ValkyrieType::Apply(base, arguments) => ValkyrieType::Apply(
            Box::new(lower_trait_associated_type_references(base, associated_names, generic_names)),
            arguments.iter().map(|argument| lower_trait_associated_type_references(argument, associated_names, generic_names)).collect(),
        ),
        ValkyrieType::Array(element) => {
            ValkyrieType::Array(Box::new(lower_trait_associated_type_references(element, associated_names, generic_names)))
        }
        ValkyrieType::FixedArray { element, length } => ValkyrieType::FixedArray {
            element: Box::new(lower_trait_associated_type_references(element, associated_names, generic_names)),
            length: *length,
        },
        ValkyrieType::Nullable(inner) => {
            ValkyrieType::Nullable(Box::new(lower_trait_associated_type_references(inner, associated_names, generic_names)))
        }
        ValkyrieType::Tuple(items) => ValkyrieType::Tuple(
            items.iter().map(|item| lower_trait_associated_type_references(item, associated_names, generic_names)).collect(),
        ),
        ValkyrieType::Union(items) => ValkyrieType::Union(
            items.iter().map(|item| lower_trait_associated_type_references(item, associated_names, generic_names)).collect(),
        ),
        ValkyrieType::Intersection(items) => ValkyrieType::Intersection(
            items.iter().map(|item| lower_trait_associated_type_references(item, associated_names, generic_names)).collect(),
        ),
        ValkyrieType::Function(function) => ValkyrieType::Function(Box::new(crate::types::hir::FunctionType {
            params: function
                .params
                .iter()
                .map(|param| lower_trait_associated_type_references(param, associated_names, generic_names))
                .collect(),
            return_type: lower_trait_associated_type_references(&function.return_type, associated_names, generic_names),
        })),
        ValkyrieType::Associated(associated) => ValkyrieType::Associated(Box::new(crate::types::hir::AssociatedType {
            base: lower_trait_associated_type_references(&associated.base, associated_names, generic_names),
            name: associated.name.clone(),
            type_arguments: associated
                .type_arguments
                .iter()
                .map(|argument| lower_trait_associated_type_references(argument, associated_names, generic_names))
                .collect(),
        })),
        other => other.clone(),
    }
}

/// Lowers `ValkyrieRoot` into `HirModule`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AstToHir {
    /// Source id attached to lowered items.
    pub source_id: SourceID,
}

impl AstToHir {
    /// Creates a new lowerer bound to a source id.
    pub fn new(source_id: SourceID) -> Self {
        Self { source_id }
    }

    /// Lowers a parser root into a module-shaped HIR view.
    pub fn lower_root(&self, root: &ValkyrieRoot) -> Result<HirModule, ParseError> {
        self.lower_root_with_semantic_exports(root, &[])
    }

    /// Lowers a parser root with resolved nominal dependency exports in scope.
    pub fn lower_root_with_semantic_exports(
        &self,
        root: &ValkyrieRoot,
        imported_semantic_exports: &[HirDependencySemanticExport],
    ) -> Result<HirModule, ParseError> {
        self.lower_root_with_semantic_exports_and_name(root, imported_semantic_exports, None)
    }

    /// 在 HIR 调用解析前覆盖模块身份；仅供完整 source closure 编译使用。
    pub fn lower_root_with_semantic_exports_and_name(
        &self,
        root: &ValkyrieRoot,
        imported_semantic_exports: &[HirDependencySemanticExport],
        module_name_override: Option<NamePath>,
    ) -> Result<HirModule, ParseError> {
        let mut hir = self.lower_root_without_call_resolution(root, imported_semantic_exports, module_name_override)?;
        if !imported_semantic_exports.is_empty() {
            return Err(ParseError::invalid("依赖声明注册必须由完整 Compiler 源码闭包拥有"));
        }
        register_function_declarations(&mut hir, &mut 0, &mut 0)?;
        bind_declared_nominal_types(&mut hir)?;
        trait_binding::bind_trait_declarations(&mut hir)?;
        resolve_hir_calls(&mut hir);
        validate_extractor_patterns(&hir)?;
        Ok(hir)
    }

    /// 仅物化 HIR；调用身份必须由完整源码组阶段统一解析。
    pub(crate) fn lower_root_without_call_resolution(
        &self,
        root: &ValkyrieRoot,
        imported_semantic_exports: &[HirDependencySemanticExport],
        module_name_override: Option<NamePath>,
    ) -> Result<HirModule, ParseError> {
        validate_ast_root(root)?;
        let _warning_scope = CompileWarningScope::enter();
        let _builtin_type_alias_scope = BuiltinTypeAliasScope::enter(root);
        let module_name = module_name_override.unwrap_or_else(|| {
            root.items
                .iter()
                .find_map(|item| match item {
                    StatementNode::Namespace(namespace) if namespace.items.is_empty() => Some(lower_name_path(&namespace.name)),
                    _ => None,
                })
                .unwrap_or_else(default_module_name)
        });

        let imports = root
            .items
            .iter()
            .filter_map(|item| match item {
                StatementNode::Using(using) => Some(lower_using(using)),
                _ => None,
            })
            .collect();

        let _module_type_alias_scope = ModuleTypeAliasScope::enter_empty();
        let type_aliases = Vec::new();

        let mut functions = Vec::new();
        let mut type_functions = Vec::new();
        let mut structs = Vec::new();
        let mut widgets = Vec::new();
        let mut singletons = Vec::new();
        let mut traits = Vec::new();
        let mut enums = Vec::new();
        let mut flags = Vec::new();
        let mut impls = Vec::new();

        let mut current_namespace = NamePath::default();
        for item in &root.items {
            match item {
                StatementNode::Namespace(namespace) => {
                    if namespace.items.is_empty() {
                        current_namespace = lower_name_path(&namespace.name);
                    }
                    else {
                        let namespace_path = lower_name_path(&namespace.name);
                        self.lower_items(
                            &namespace.items,
                            &namespace_path,
                            &mut functions,
                            &mut type_functions,
                            &mut structs,
                            &mut widgets,
                            &mut singletons,
                            &mut traits,
                            &mut enums,
                            &mut flags,
                            &mut impls,
                        )?;
                    }
                }
                other => self.lower_statement_node(
                    other,
                    &current_namespace,
                    &mut functions,
                    &mut type_functions,
                    &mut structs,
                    &mut widgets,
                    &mut singletons,
                    &mut traits,
                    &mut enums,
                    &mut flags,
                    &mut impls,
                )?,
            }
        }

        let mut hir = HirModule {
            name: module_name,
            doc: HirDocumentation::default(),
            imports,
            warnings: Vec::new(),
            submodules: Vec::new(),
            functions,
            structs,
            enums,
            imported_enums: Vec::new(),
            imported_semantic_exports: imported_semantic_exports.to_vec(),
            flags,
            traits,
            impls,
            type_functions,
            type_families: Vec::new(),
            widgets,
            singletons,
            statements: Vec::new(),
            type_aliases,
        };
        hoist_anonymous_classes(&mut hir);
        let mut injector = crate::valkyrie::derive::DeriveInjector::new();
        let derive_result = injector.inject_derives(&mut hir);
        if derive_result.has_errors() {
            return Err(ParseError::invalid(derive_result.errors.into_iter().map(|error| error.to_string()).collect::<Vec<_>>().join("; ")));
        }
        hir.warnings = take_compile_warnings();
        Ok(hir)
    }

    fn lower_items(
        &self,
        items: &[StatementNode],
        namespace: &NamePath,
        functions: &mut Vec<HirFunction>,
        type_functions: &mut Vec<HirTypeFunction>,
        structs: &mut Vec<HirStruct>,
        widgets: &mut Vec<HirWidget>,
        singletons: &mut Vec<HirSingleton>,
        traits: &mut Vec<HirTrait>,
        enums: &mut Vec<HirEnum>,
        flags: &mut Vec<HirFlags>,
        impls: &mut Vec<HirImpl>,
    ) -> Result<(), ParseError> {
        for item in items {
            self.lower_statement_node(item, namespace, functions, type_functions, structs, widgets, singletons, traits, enums, flags, impls)?;
        }
        Ok(())
    }

    fn lower_statement_node(
        &self,
        item: &StatementNode,
        namespace: &NamePath,
        functions: &mut Vec<HirFunction>,
        type_functions: &mut Vec<HirTypeFunction>,
        structs: &mut Vec<HirStruct>,
        widgets: &mut Vec<HirWidget>,
        singletons: &mut Vec<HirSingleton>,
        traits: &mut Vec<HirTrait>,
        enums: &mut Vec<HirEnum>,
        flags: &mut Vec<HirFlags>,
        impls: &mut Vec<HirImpl>,
    ) -> Result<(), ParseError> {
        match item {
            StatementNode::Micro(micro) => {
                functions.push(self.lower_micro(micro, namespace));
                Ok(())
            }
            StatementNode::TypeFunction(type_function) => {
                type_functions.push(self.lower_type_function(type_function));
                Ok(())
            }
            StatementNode::Class(class_decl) => {
                structs.push(self.lower_class(class_decl, namespace));
                Ok(())
            }
            StatementNode::Structure(structure) => {
                structs.push(self.lower_structure(structure, namespace));
                Ok(())
            }
            StatementNode::Singleton(singleton) => {
                singletons.push(self.lower_singleton(singleton, namespace));
                Ok(())
            }
            StatementNode::Widget(widget) => {
                widgets.push(self.lower_widget(widget));
                Ok(())
            }
            StatementNode::Trait(trait_decl) => {
                let mut lowered = self.lower_trait(trait_decl);
                lowered.declaring_namespace = namespace.clone();
                traits.push(lowered);
                Ok(())
            }
            StatementNode::Enums(enum_decl) => {
                enums.push(self.lower_enums(enum_decl));
                Ok(())
            }
            StatementNode::Variant(variant_decl) => {
                enums.push(self.lower_variant_decl(variant_decl));
                Ok(())
            }
            StatementNode::Flags(flags_decl) => {
                flags.push(self.lower_flags(flags_decl));
                Ok(())
            }
            StatementNode::Imply(imply) => {
                impls.push(self.lower_imply(imply, namespace));
                Ok(())
            }
            StatementNode::Union(union_decl) => Err(ParseError::invalid(format!("named union `{}` is not yet lowered", union_decl.name.name))),
            StatementNode::Using(_)
            | StatementNode::Namespace(_)
            | StatementNode::Let(_)
            | StatementNode::ExprStmt(_)
            | StatementNode::Statement(_) => Ok(()),
            StatementNode::Template(_) => Err(ParseError::invalid("未展开的 TGrammar 模板节点进入 HIR lowering")),
            unsupported => Err(ParseError::invalid(format!("unsupported Oak root item is not yet lowered: {unsupported:?}"))),
        }
    }

    fn lower_micro(&self, function: &MicroDeclaration, declaring_namespace: &NamePath) -> HirFunction {
        let span = frontend::std_range(&function.span);
        HirFunction {
            declaration: None,
            instance: None,
            name: Identifier::new(&function.name.name),
            declaring_namespace: declaring_namespace.clone(),
            doc: lower_documentation(&function.annotations),
            annotations: function.annotations.iter().map(|attribute| lower_attribute(attribute, self.source_id, span.clone())).collect(),
            generics: lower_generic_parameters(&function.generics),
            where_constraints: lower_where_constraints(&function.where_constraints, self.source_id),
            params: function.params.iter().map(|param| lower_param(param, self.source_id, span.clone())).collect(),
            return_type: function.return_type.as_ref().map(lower_type_expression).unwrap_or(ValkyrieType::Unit),
            body: lower_block(&function.body, self.source_id, span.clone()),
            span: with_source(&span, self.source_id),
            visibility: lower_visibility(&function.annotations),
            is_abstract: function.is_abstract
                || has_modifier(&function.annotations, "abstract")
                || has_modifier(&function.annotations, "host_contract"),
            is_final: has_modifier(&function.annotations, "final"),
            is_virtual: false,
            is_override: false,
        }
    }

    fn lower_type_function(&self, function: &TypeFunction) -> HirTypeFunction {
        let span = frontend::std_range(&function.span);
        HirTypeFunction {
            name: Identifier::new(&function.name.name),
            documents: lower_documentation(&function.annotations),
            generics: lower_generic_parameters(&function.generics),
            where_constraints: lower_where_constraints(&function.where_constraints, self.source_id),
            params: function.params.iter().map(|param| lower_param(param, self.source_id, span.clone())).collect(),
            return_type: function.return_type.as_ref().map(lower_type_expression).unwrap_or(ValkyrieType::Unit),
            body: lower_block(&function.body, self.source_id, span),
        }
    }

    fn lower_class(&self, class_decl: &ClassDeclaration, namespace: &NamePath) -> HirStruct {
        self.lower_class_like(
            Identifier::new(&class_decl.name.name),
            namespace.parts(),
            &class_decl.annotations,
            &class_decl.generics,
            &class_decl.parents,
            &class_decl.body.fields,
            &class_decl.body.methods,
            false,
        )
    }

    fn lower_structure(&self, structure: &StructureDeclaration, namespace: &NamePath) -> HirStruct {
        self.lower_class_like(
            Identifier::new(&structure.name.name),
            namespace.parts(),
            &structure.annotations,
            &structure.generics,
            &structure.parents,
            &structure.body.fields,
            &structure.body.methods,
            true,
        )
    }

    fn lower_class_like(
        &self,
        name: Identifier,
        namespace: &[Identifier],
        annotations: &[Attribute],
        generics: &[GenericParam],
        parents: &[Parent],
        fields: &[FieldDeclaration],
        methods: &[MethodDeclaration],
        is_value_type: bool,
    ) -> HirStruct {
        HirStruct {
            constructor_declaration: None,
            constructor_instance: None,
            name,
            namespace: namespace.to_vec(),
            doc: lower_documentation(annotations),
            generics: lower_generic_parameters(generics),
            parents: parents.iter().map(lower_parent).collect(),
            fields: fields.iter().map(lower_field).collect(),
            methods: methods.iter().filter(|method| !is_property_accessor(method)).map(|method| self.lower_method(method)).collect(),
            properties: self.lower_object_properties(methods),
            visibility: lower_visibility(annotations),
            is_value_type,
            is_abstract: has_modifier(annotations, "abstract"),
            is_sealed: has_modifier(annotations, "sealed"),
            is_final: has_modifier(annotations, "final"),
            is_open: has_modifier(annotations, "open"),
            abstract_methods: Vec::new(),
            abstract_properties: Vec::new(),
            derives: lower_derives(annotations),
        }
    }

    fn lower_widget(&self, widget: &WidgetDeclaration) -> HirWidget {
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        for item in &widget.items {
            match item {
                StatementNode::Micro(micro) => methods.push(self.lower_micro(micro, &NamePath::default())),
                _ => {}
            }
        }
        let lifecycle = HirWidgetLifecycle {
            has_on_mount: methods.iter().any(|m| m.name.as_str() == "on_mount"),
            has_on_unmount: methods.iter().any(|m| m.name.as_str() == "on_unmount"),
            has_on_update: methods.iter().any(|m| m.name.as_str() == "on_update"),
            has_before_update: methods.iter().any(|m| m.name.as_str() == "before_update"),
            has_after_update: methods.iter().any(|m| m.name.as_str() == "after_update"),
        };
        HirWidget {
            name: oak_identifier(&widget.name),
            doc: lower_documentation(&widget.annotations),
            generics: lower_generic_parameters(&widget.generics),
            fields,
            methods,
            visibility: lower_visibility(&widget.annotations),
            state_fields: Vec::new(),
            initial_state: Vec::new(),
            lifecycle,
        }
    }

    fn lower_singleton(&self, singleton: &SingletonDeclaration, namespace: &NamePath) -> HirSingleton {
        let all_methods: Vec<HirFunction> =
            singleton.body.methods.iter().filter(|method| !is_property_accessor(method)).map(|method| self.lower_method(method)).collect();
        let mut constructor: Option<Box<HirFunction>> = None;
        let mut finalizer: Option<Box<HirFunction>> = None;
        let mut ordinary_methods: Vec<HirFunction> = Vec::with_capacity(all_methods.len());
        for method in all_methods {
            match method.name.as_str() {
                SINGLETON_CONSTRUCTOR_NAME if constructor.is_none() => {
                    constructor = Some(Box::new(method));
                }
                SINGLETON_FINALIZER_NAME if finalizer.is_none() => {
                    finalizer = Some(Box::new(method));
                }
                // 已存在 constructor/finalizer 的重复定义走默认分支归入 ordinary_methods，
                // 不再使用 `SINGLETON_CONSTRUCTOR_NAME | SINGLETON_FINALIZER_NAME` 或模式——
                // 对常量标识符使用 or 模式会被 Rust 解析为变量绑定而非常量匹配，触发 E0408/E0384。
                _ => {
                    ordinary_methods.push(method);
                }
            }
        }
        HirSingleton {
            name: Identifier::new(&singleton.name.name),
            namespace: namespace.parts().to_vec(),
            doc: lower_documentation(&singleton.annotations),
            generics: lower_generic_parameters(&singleton.generics),
            parents: singleton.parents.iter().map(lower_parent).collect(),
            fields: singleton.body.fields.iter().map(lower_field).collect(),
            methods: ordinary_methods,
            properties: self.lower_object_properties(&singleton.body.methods),
            visibility: lower_visibility(&singleton.annotations),
            derives: lower_derives(&singleton.annotations),
            is_lazy: has_modifier(&singleton.annotations, "lazy"),
            instance_name: Identifier::new(crate::valkyrie::mir::SINGLETON_INSTANCE_FIELD),
            constructor,
            finalizer,
        }
    }

    fn lower_flags(&self, flags_decl: &Flags) -> HirFlags {
        HirFlags {
            name: oak_identifier(&flags_decl.name),
            doc: lower_documentation(&flags_decl.annotations),
            members: flags_decl.variants.iter().map(|member| self.lower_flag_member(member)).collect(),
            visibility: lower_visibility(&flags_decl.annotations),
        }
    }

    fn lower_flag_member(&self, member: &EnumVariant) -> HirFlagMember {
        let span = frontend::std_range(&member.span);
        HirFlagMember {
            name: oak_identifier(&member.name),
            doc: lower_documentation(&member.annotations),
            value: member
                .value
                .as_ref()
                .map(|expr| lower_term_expression(expr, self.source_id, span.clone()))
                .unwrap_or_else(|| HirExpr { kind: HirExprKind::Literal(HirLiteral::Integer64(0)), span: with_source(&span, self.source_id) }),
            is_combined: false,
        }
    }

    fn lower_trait(&self, trait_decl: &Trait) -> HirTrait {
        let associated_names =
            trait_decl.body.associated_types.iter().map(|item| oak_identifier(&item.name)).collect::<std::collections::BTreeSet<_>>();
        let mut methods = Vec::new();
        let mut default_methods = Vec::new();
        for method in trait_decl.body.methods.iter().filter(|method| !is_property_accessor(method)) {
            let lowered = finalize_trait_method_signature(lower_trait_method(self.lower_method(method), &associated_names));
            if lowered.is_abstract {
                methods.push(lowered);
            }
            else {
                default_methods.push(lowered);
            }
        }

        HirTrait {
            declaration: None,
            declaring_namespace: Default::default(),
            name: oak_identifier(&trait_decl.name),
            doc: lower_documentation(&trait_decl.annotations),
            generics: lower_generic_parameters(&trait_decl.generics),
            methods,
            associated_types: trait_decl.body.associated_types.iter().map(|item| lower_trait_associated_type(item, self.source_id)).collect(),
            associated_constants: Vec::new(),
            super_traits: Vec::new(),
            default_methods,
            visibility: lower_visibility(&trait_decl.annotations),
        }
    }

    fn lower_enums(&self, enum_decl: &Enums) -> HirEnum {
        let mut enum_def = match enum_decl.kind {
            EnumsKind::Unity => HirEnum::new_unity(oak_identifier(&enum_decl.name)),
            EnumsKind::Enums | EnumsKind::Enum => HirEnum::new(oak_identifier(&enum_decl.name)),
        };
        enum_def.doc = lower_documentation(&enum_decl.annotations);
        enum_def.visibility = lower_visibility(&enum_decl.annotations);
        enum_def.generics = lower_generic_parameters(&enum_decl.generics);
        enum_def.variants = enum_decl.variants.iter().map(|variant| self.lower_enum_variant(variant, enum_decl.kind)).collect();
        enum_def.is_unity = enum_decl.kind == EnumsKind::Unity;
        enum_def
    }

    fn lower_variant_decl(&self, variant_decl: &Variant) -> HirEnum {
        let mut enum_def = HirEnum::new_unity(oak_identifier(&variant_decl.name));
        enum_def.doc = lower_documentation(&variant_decl.annotations);
        enum_def.visibility = lower_visibility(&variant_decl.annotations);
        enum_def.generics = lower_generic_parameters(&variant_decl.generics);
        enum_def.variants = variant_decl
            .cases
            .iter()
            .enumerate()
            .map(|(index, case)| {
                let span = frontend::std_range(&case.span);
                HirVariant {
                    declaration: None,
                    instance: None,
                    name: Identifier::new(&format!("case_{index}")),
                    doc: HirDocumentation::default(),
                    fields: Vec::new(),
                    result_type: None,
                    discriminator: Some(lower_term_expression(&case.body, self.source_id, span)),
                }
            })
            .collect();
        enum_def.is_unity = true;
        enum_def
    }

    fn lower_imply(&self, imply: &ImplyDeclaration, declaring_namespace: &NamePath) -> HirImpl {
        let trait_arguments = imply
            .trait_type
            .as_ref()
            .and_then(|ty| match ty {
                TypeExpression::Apply(node) => Some(node.arguments.iter().map(lower_type_expression).collect()),
                _ => None,
            })
            .unwrap_or_default();
        HirImpl {
            trait_declaration: None,
            declaring_namespace: declaring_namespace.clone(),
            trait_arguments,
            generics: lower_generic_parameters(&imply.generics),
            where_constraints: Vec::new(),
            target: lower_type_expression(&imply.target_type),
            trait_path: imply.trait_type.as_ref().map(lower_trait_path_from_type),
            methods: imply.methods.iter().map(|method| self.lower_method(method)).collect(),
            associated_type_impls: Vec::new(),
            associated_const_impls: Vec::new(),
        }
    }

    fn lower_method(&self, method: &MethodDeclaration) -> HirFunction {
        let span = frontend::std_range(&method.span);
        HirFunction {
            declaration: None,
            instance: None,
            name: oak_identifier(&method.name),
            declaring_namespace: NamePath::default(),
            doc: lower_documentation(&method.annotations),
            annotations: method.annotations.iter().map(|attribute| lower_attribute(attribute, self.source_id, span.clone())).collect(),
            generics: lower_generic_parameters(&method.generics),
            where_constraints: lower_where_constraints(&method.where_constraints, self.source_id),
            params: method.parameters.iter().map(|param| lower_param(param, self.source_id, span.clone())).collect(),
            return_type: method.return_type.as_ref().map(lower_type_expression).unwrap_or(ValkyrieType::Unit),
            body: method.body.as_ref().map(|block| lower_block(block, self.source_id, span.clone())).unwrap_or_else(|| HirBlock {
                statements: Vec::new(),
                expr: None,
                span: with_source(&span, self.source_id),
            }),
            span: with_source(&span, self.source_id),
            visibility: lower_visibility(&method.annotations),
            is_abstract: method.body.is_none() || has_modifier(&method.annotations, "abstract"),
            is_final: has_modifier(&method.annotations, "final"),
            is_virtual: has_modifier(&method.annotations, "virtual"),
            is_override: has_modifier(&method.annotations, "override"),
        }
    }

    fn lower_object_properties(&self, methods: &[MethodDeclaration]) -> Vec<HirProperty> {
        let mut lowered = Vec::new();

        for method in methods.iter().filter(|method| is_property_accessor(method)) {
            let Some(accessor_kind) = property_accessor_kind(method)
            else {
                continue;
            };
            let accessor = self.lower_property_accessor(method, accessor_kind);
            let ty = lower_property_type(method, accessor_kind);

            if let Some(existing) = lowered.iter_mut().find(|item: &&mut HirProperty| item.name == oak_identifier(&method.name)) {
                existing.ty = ty;
                existing.doc = lower_documentation(&method.annotations);
                existing.visibility = lower_visibility(&method.annotations);
                existing.is_abstract = existing.is_abstract || property_is_abstract(method);
                existing.is_final = existing.is_final || property_is_final(method);
                existing.is_static = existing.is_static || property_is_static(method);
                existing.is_virtual = existing.is_virtual || property_is_virtual(method);
                existing.is_override = existing.is_override || property_is_override(method);
                existing.is_lazy = existing.is_lazy || property_is_lazy(method);
                match accessor_kind {
                    PropertyMethodKind::Get => {
                        existing.getter = Some(accessor);
                    }
                    PropertyMethodKind::Set => {
                        existing.setter = Some(accessor);
                        existing.is_readonly = false;
                    }
                }
                continue;
            }

            let mut hir_property = HirProperty {
                name: oak_identifier(&method.name),
                doc: lower_documentation(&method.annotations),
                ty,
                getter: None,
                setter: None,
                is_readonly: accessor_kind == PropertyMethodKind::Get,
                visibility: lower_visibility(&method.annotations),
                is_abstract: property_is_abstract(method),
                is_final: property_is_final(method),
                is_static: property_is_static(method),
                is_virtual: property_is_virtual(method),
                is_override: property_is_override(method),
                is_lazy: property_is_lazy(method),
                lazy_backing_field: None,
            };

            match accessor_kind {
                PropertyMethodKind::Get => {
                    hir_property.getter = Some(accessor);
                }
                PropertyMethodKind::Set => {
                    hir_property.setter = Some(accessor);
                    hir_property.is_readonly = false;
                }
            }

            lowered.push(hir_property);
        }

        lowered
    }

    fn lower_enum_variant(&self, variant: &EnumVariant, kind: EnumsKind) -> HirVariant {
        let span = frontend::std_range(&variant.span);
        let discriminator = variant.value.as_ref().map(|value| lower_term_expression(value, self.source_id, span.clone())).or_else(|| {
            if kind == EnumsKind::Unity { tag_attribute_discriminator(&variant.annotations, self.source_id, span.clone()) } else { None }
        });
        HirVariant {
            declaration: None,
            instance: None,
            name: oak_identifier(&variant.name),
            doc: lower_documentation(&variant.annotations),
            fields: variant.fields.iter().map(lower_field).collect(),
            result_type: None,
            discriminator,
        }
    }

    fn lower_property_accessor(&self, method: &MethodDeclaration, accessor_kind: PropertyMethodKind) -> HirFunction {
        let accessor_name = match accessor_kind {
            PropertyMethodKind::Get => oak_identifier(&method.name),
            PropertyMethodKind::Set => Identifier::new(&format!("set_{}", method.name.name)),
        };

        HirFunction {
            declaration: None,
            instance: None,
            name: accessor_name,
            declaring_namespace: NamePath::default(),
            doc: lower_documentation(&method.annotations),
            annotations: method
                .annotations
                .iter()
                .map(|attribute| lower_attribute(attribute, self.source_id, frontend::std_range(&method.span)))
                .collect(),
            generics: lower_generic_parameters(&method.generics),
            where_constraints: Vec::new(),
            params: method.parameters.iter().map(|param| lower_param(param, self.source_id, frontend::std_range(&method.span))).collect(),
            return_type: method.return_type.as_ref().map(lower_type_expression).unwrap_or(ValkyrieType::Unit),
            body: method.body.as_ref().map(|block| lower_block(block, self.source_id, frontend::std_range(&method.span))).unwrap_or_else(
                || HirBlock { statements: Vec::new(), expr: None, span: with_source(&frontend::std_range(&method.span), self.source_id) },
            ),
            span: with_source(&frontend::std_range(&method.span), self.source_id),
            visibility: lower_visibility(&method.annotations),
            is_abstract: method.body.is_none() || has_modifier(&method.annotations, "abstract"),
            is_final: has_modifier(&method.annotations, "final"),
            is_virtual: has_modifier(&method.annotations, "virtual"),
            is_override: has_modifier(&method.annotations, "override"),
        }
    }
}

/// Extract `[tag(N)]` / `[tag(N, default)]` into a discriminator literal for `unite` layouts.
fn tag_attribute_discriminator(annotations: &[Attribute], source_id: SourceID, span: Range<usize>) -> Option<HirExpr> {
    for attribute in annotations {
        if attribute.name.name != "tag" {
            continue;
        }
        let first = attribute.args.first()?;
        return Some(lower_term_expression(&first.value, source_id, span));
    }
    None
}

fn lower_trait_associated_type(item: &AssociatedType, source_id: SourceID) -> HirAssociatedType {
    HirAssociatedType {
        name: Identifier::new(&item.name.name),
        doc: lower_documentation(&item.annotations),
        type_params: Vec::new(),
        bounds: item.bounds.iter().map(lower_type_expression).collect(),
        default: item.default.as_ref().map(lower_type_expression),
        span: with_source(&frontend::std_range(&item.span), source_id),
    }
}

fn lower_attribute(attribute: &Attribute, source_id: SourceID, fallback_span: Range<usize>) -> HirAttribute {
    let arguments = attribute
        .args
        .iter()
        .map(|argument| HirArgument {
            key: argument.key.as_ref().map(|key| Identifier::new(&key.name)),
            value: Box::new(lower_attribute_argument_expression(&argument.value, source_id, fallback_span.clone())),
        })
        .collect();
    HirAttribute::with_arguments(NamePath::new(vec![oak_identifier(&attribute.name)]), arguments)
}

fn lower_attribute_argument_expression(expr: &TermExpression, source_id: SourceID, fallback_span: Range<usize>) -> HirExpr {
    match expr {
        TermExpression::NamePath(path) => {
            HirExpr { kind: HirExprKind::Path(lower_name_path(path)), span: with_source(&frontend::std_range(&path.span), source_id) }
        }
        _ => lower_term_expression(expr, source_id, fallback_span),
    }
}

fn lower_documentation(_annotations: &[Attribute]) -> HirDocumentation {
    HirDocumentation::default()
}

fn lower_visibility(annotations: &[Attribute]) -> HirVisibility {
    if has_modifier(annotations, "private") {
        HirVisibility::private()
    }
    else if has_modifier(annotations, "protected") {
        HirVisibility::protected()
    }
    else if has_modifier(annotations, "internal") {
        HirVisibility::internal()
    }
    else {
        HirVisibility::public()
    }
}

/// `[host_provider(...)]` 实现只通过 Resolver 绑定进入 consumer，不作为依赖导出 API。
fn exportable_dependency_functions(functions: &[HirFunction]) -> Vec<HirFunction> {
    functions.iter().filter(|function| function_host_provider_target(function).is_none()).cloned().collect()
}

fn has_modifier(annotations: &[Attribute], name: &str) -> bool {
    annotations.iter().any(|attribute| attribute.name.name == name)
}

fn lower_derives(annotations: &[Attribute]) -> Vec<NamePath> {
    annotations
        .iter()
        .find(|attribute| attribute.name.name == "derive")
        .map(|attribute| attribute.args.iter().filter_map(|argument| extract_name_path(&argument.value)).collect())
        .unwrap_or_default()
}

pub(super) fn lower_parent(parent: &Parent) -> HirParent {
    HirParent::full(lower_name_path(&parent.name), parent.alias.as_ref().map(|alias| Identifier::new(&alias.name)), Vec::new())
}

fn lower_field(field: &FieldDeclaration) -> HirField {
    HirField {
        name: Identifier::new(&field.name.name),
        doc: lower_documentation(&field.annotations),
        ty: lower_type_expression(&field.typing),
        visibility: lower_visibility(&field.annotations),
        is_mutable: has_modifier(&field.annotations, "mut"),
    }
}

fn lower_generic_parameters(parameters: &[GenericParam]) -> Vec<GenericType> {
    parameters.iter().map(lower_generic_parameter).collect()
}

fn lower_generic_parameter(parameter: &GenericParam) -> GenericType {
    GenericType {
        // AST→HIR 时尚无 owner ItemId；后续 bind 再冻结 GenericBinderId。
        binder: None,
        name: Identifier::new(&parameter.name.name),
        kind: HirKind::Type,
        bounds: parameter.constraints.iter().map(lower_bound_identifier).collect(),
    }
}

fn lower_bound_identifier(bound: &frontend::ast::TypeExpression) -> Identifier {
    Identifier::new(&render_type_expression(bound))
}

fn lower_param(param: &Param, source_id: SourceID, fallback_span: Range<usize>) -> HirParam {
    let span_range = if param.span.is_empty() { fallback_span } else { frontend::std_range(&param.span) };
    HirParam {
        name: HirIdentifier { name: Identifier::new(&param.name.name), shadow_index: 0, span: with_source(&span_range, source_id) },
        ty: param.ty.as_ref().map(lower_type_expression).unwrap_or(ValkyrieType::AutoType),
        binding_kind: HirParameterBindingKind::PositionalOrKeyword,
        is_mutable: has_modifier(&param.annotations, "mut"),
        default: param.default.as_ref().map(|expr| lower_term_expression(expr, source_id, span_range.clone())),
        variadic: HirVariadicKind::None,
    }
}

fn lower_method_params(method: &MethodDeclaration, source_id: SourceID) -> Vec<HirParam> {
    let span = frontend::std_range(&method.span);
    method.parameters.iter().map(|param| lower_param(param, source_id, span.clone())).collect()
}

fn lower_property_params(method: &MethodDeclaration, source_id: SourceID) -> Vec<HirParam> {
    lower_method_params(method, source_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PropertyMethodKind {
    Get,
    Set,
}

fn property_accessor_kind(method: &MethodDeclaration) -> Option<PropertyMethodKind> {
    if has_modifier(&method.annotations, "get") {
        Some(PropertyMethodKind::Get)
    }
    else if has_modifier(&method.annotations, "set") {
        Some(PropertyMethodKind::Set)
    }
    else {
        None
    }
}

fn is_property_accessor(method: &MethodDeclaration) -> bool {
    property_accessor_kind(method).is_some()
}

fn lower_property_type(method: &MethodDeclaration, accessor_kind: PropertyMethodKind) -> ValkyrieType {
    match accessor_kind {
        PropertyMethodKind::Get => method.return_type.as_ref().map(lower_type_expression).unwrap_or(ValkyrieType::Unit),
        PropertyMethodKind::Set => {
            method.parameters.last().and_then(|param| param.ty.as_ref().map(lower_type_expression)).unwrap_or(ValkyrieType::Unit)
        }
    }
}

fn property_is_abstract(method: &MethodDeclaration) -> bool {
    method.body.is_none() || has_modifier(&method.annotations, "abstract")
}

fn property_is_final(method: &MethodDeclaration) -> bool {
    has_modifier(&method.annotations, "final")
}

fn property_is_static(method: &MethodDeclaration) -> bool {
    has_modifier(&method.annotations, "static")
}

fn property_is_virtual(method: &MethodDeclaration) -> bool {
    has_modifier(&method.annotations, "virtual")
}

fn property_is_override(method: &MethodDeclaration) -> bool {
    has_modifier(&method.annotations, "override")
}

fn property_is_lazy(method: &MethodDeclaration) -> bool {
    has_modifier(&method.annotations, "lazy")
}

fn lower_using(using: &UsingDeclaration) -> HirImport {
    HirImport {
        path: lower_name_path(&using.path),
        alias: using.alias.as_ref().map(|alias| Identifier::new(&alias.name)),
        bindings: using.imports.iter().map(|item| HirImportBinding { name: Identifier::new(&item.name), alias: None }).collect(),
        glob: false,
    }
}

fn lower_trait_path_from_type(ty: &TypeExpression) -> NamePath {
    match ty {
        TypeExpression::Namepath(path) => lower_name_path(path),
        TypeExpression::Apply(node) => lower_trait_path_from_type(&node.base),
        other => NamePath::new(vec![Identifier::new(&render_type_expression(other))]),
    }
}

fn lower_name_path(path: &AstNamePath) -> NamePath {
    NamePath::new(path.parts.iter().map(|part| Identifier::new(&part.name)).collect())
}

fn default_module_name() -> NamePath {
    NamePath::new(vec![Identifier::new("main")])
}

fn with_source(span: &Range<usize>, source_id: SourceID) -> SourceSpan {
    SourceSpan::new(source_id, span.start as u32, span.end as u32)
}

fn oak_identifier(id: &frontend::ast::Identifier) -> Identifier {
    Identifier::new(&id.name)
}

#[cfg(test)]
mod sum_discriminator_tests {
    use super::{validate_enum_discriminators, *};
    use crate::{
        SourceID, ValkyrieCompiler,
        types::{Identifier, NamePath, SourceSpan},
        valkyrie::{
            hir::lowering::template_expand::DEFAULT_COMPILE_ARCH,
            types::hir::{HirDependencySemanticExport, HirEnum, HirExpr, HirExprKind, HirLiteral, HirVariant},
        },
    };

    fn test_span() -> SourceSpan {
        SourceSpan::new(SourceID::default(), 0, 0)
    }

    #[test]
    fn sum_layouts_are_not_synthesized_without_source_declarations() {
        let output = ValkyrieCompiler::default()
            .compile_source_to_program("[main] micro entry() -> i32 { return 23 }")
            .expect("普通源码必须完成正式 Compiler 成功边界");
        let semantic_mir = crate::valkyrie::mir::MirLowerer::lower_module_semantic(
            &ValkyrieCompiler::default().compile_source("[main] micro entry() -> i32 { return 23 }").expect("test source"),
        );
        assert!(semantic_mir.sum_types.is_empty());
        assert!(output.canonical().linked.variants.is_empty());
    }

    #[test]
    fn familiar_sum_names_preserve_the_declared_variants() {
        let module = ValkyrieCompiler::default()
            .compile_source("enums Option { Declared = 7 } enums Result { Actual = 11 }")
            .expect("sum 声明必须来自当前源码");
        let sums = MirLowerer::lower_module_semantic(&module).sum_types;
        assert_eq!(sums.len(), 2);
        for (name, variant, tag) in [("Option", "Declared", 7), ("Result", "Actual", 11)] {
            let sum = sums.iter().find(|sum| sum.name == name).expect("声明 owner");
            assert_eq!(sum.variants.len(), 1);
            assert_eq!(sum.variants[0].name, variant);
            assert_eq!(sum.variants[0].tag, tag);
            assert!(sum.variants[0].payload_type().is_none());
        }
    }

    #[test]
    fn unresolved_call_contract_is_rejected_before_mir() {
        let expression = HirExpr {
            kind: HirExprKind::Call {
                call_kind: super::HirCallKind::Function,
                callee: Box::new(HirExpr { kind: HirExprKind::Path(NamePath::new(vec![Identifier::new("missing")])), span: test_span() }),
                args: Vec::new(),
                resolved: None,
            },
            span: test_span(),
        };
        let error = validate_expr_call_contracts(&expression, "contract_test").expect_err("unresolved call must fail");
        assert!(error.to_string().contains("SMIR003"), "{error}");
    }

    #[test]
    fn frontend_does_not_filter_copy_contract_errors() {
        let compiler = ValkyrieCompiler::new(SourceID::default());
        let actual = compiler.compile_source("structure Holder { items: [i32] }").expect_err("frontend must propagate copy error");
        assert!(actual.to_string().contains("copy discipline violation"));
    }

    #[test]
    fn singleton_unresolved_call_is_rejected_before_mir_from_source() {
        let compiler = ValkyrieCompiler::new(SourceID::default());
        let error = compiler
            .compile_source("singleton Counter { micro invalid(self) -> i32 { return missing() } }")
            .expect_err("singleton methods must obey the same resolved call contract");
        assert!(error.to_string().contains("SMIR003"), "{error}");
    }

    #[test]
    fn non_callable_local_is_rejected_from_source() {
        let compiler = ValkyrieCompiler::new(SourceID::default());
        let error = compiler
            .compile_source("micro invalid(value: i32) -> i32 { return value() }")
            .expect_err("scalar local must not acquire an inferred callable signature");
        assert!(error.to_string().contains("SMIR003"), "{error}");
    }

    #[test]
    fn unique_callee_with_wrong_argument_type_is_rejected_from_source() {
        let compiler = ValkyrieCompiler::new(SourceID::default());
        let error = compiler
            .compile_source("micro take(value: i32) -> i32 { return value } micro invalid(value: utf8) -> i32 { return take(value) }")
            .expect_err("unique name and arity must not override signature matching");
        assert!(error.to_string().contains("SMIR003"), "{error}");
    }

    #[test]
    fn callable_local_preserves_declared_signature_from_source() {
        let compiler = ValkyrieCompiler::new(SourceID::default());
        let module = compiler
            .compile_source("micro invoke(callback: micro(i32) -> i32, value: i32) -> i32 { return callback(value) }")
            .expect("declared function value must resolve");
        let function = module.functions.iter().find(|function| function.name.as_str() == "invoke").expect("invoke");
        let return_expr = match function.body.statements.first().map(|statement| &statement.kind) {
            Some(HirStatementKind::Expr(statement)) => statement.as_ref(),
            _ => function.body.expr.as_deref().expect("expected return in statements or block tail"),
        };
        let HirExprKind::Return(Some(expression)) = &return_expr.kind
        else {
            panic!("expected return value")
        };
        let HirExprKind::Call { resolved: Some(contract), .. } = &expression.kind
        else {
            panic!("expected resolved callback")
        };
        assert_eq!(contract.parameter_types, vec![function.params[1].ty.clone()]);
        assert_eq!(contract.return_type, function.return_type);
    }

    #[test]
    fn array_push_missing_element_type_is_rejected_from_source() {
        let compiler = ValkyrieCompiler::new(SourceID::default());
        let error = compiler
            .compile_source("micro invalid(values: [i32]) -> [i32] { push(values, missing) return values }")
            .expect_err("missing element facts must not become AutoType intrinsic arguments");
        assert!(error.to_string().contains("SMIR003"), "{error}");
    }

    #[test]
    fn array_push_nominal_receiver_mismatch_is_rejected_from_source() {
        let compiler = ValkyrieCompiler::new(SourceID::default());
        let error = compiler
            .compile_source("class Owner {} imply Owner { micro invalid(self, values: [utf8]) -> [utf8] { push(values, self) return values } }")
            .expect_err("nominal self must not be treated as an unknown utf8 element");
        assert!(error.to_string().contains("SMIR003"), "{error}");
    }

    #[test]
    fn unite_rejects_duplicate_explicit_tags() {
        let compiler = ValkyrieCompiler::new(SourceID::default());
        let error = compiler
            .compile_source(
                r#"
unite Choice {
    [tag(0)]
    A { x: i64 }
    [tag(0)]
    B { y: i64 }
}
"#,
            )
            .expect_err("duplicate unite tag");
        assert!(error.to_string().contains("duplicate discriminator"), "{error}");
    }

    #[test]
    fn unite_rejects_implicit_tag_collision_with_explicit_tag() {
        let compiler = ValkyrieCompiler::new(SourceID::default());
        let error = compiler
            .compile_source(
                r#"
unite Choice {
    A { x: i64 }
    [tag(0)]
    B { y: i64 }
}
"#,
            )
            .expect_err("implicit tag collision");
        assert!(error.to_string().contains("duplicate discriminator"), "{error}");
    }

    #[test]
    fn named_union_is_not_yet_lowered() {
        let compiler = ValkyrieCompiler::new(SourceID::default());
        let error = compiler
            .compile_source(
                r#"union Limb {
    small: i64
    words: i64
}"#,
            )
            .expect_err("named union must be rejected");
        assert!(error.to_string().contains("not yet lowered"), "{error}");
    }

    #[test]
    fn unite_rejects_non_integer_tag_literal() {
        let compiler = ValkyrieCompiler::new(SourceID::default());
        let error = compiler
            .compile_source(
                r#"
unite Choice {
    [tag(foo)]
    A { x: i64 }
}
"#,
            )
            .expect_err("invalid unite tag literal");
        assert!(error.to_string().contains("non-negative integer literal"), "{error}");
    }

    #[test]
    fn unite_default_tags_follow_declaration_order() {
        let compiler = ValkyrieCompiler::new(SourceID::default());
        let module = compiler
            .compile_source(
                r#"
unite Choice {
    A { x: i64 }
    B { y: i64 }
}
"#,
            )
            .expect("compile unite with default tags");
        let sum_types = MirLowerer::lower_module_semantic(&module).sum_types;
        let choice = sum_types.iter().find(|layout| layout.name == "Choice").expect("Choice layout");
        assert_eq!(choice.variants[0].tag, 0);
        assert_eq!(choice.variants[1].tag, 1);
    }

    #[test]
    fn enums_rejects_duplicate_explicit_discriminators() {
        let compiler = ValkyrieCompiler::new(SourceID::default());
        let error = compiler
            .compile_source(
                r#"
enums Status {
    Active = 0
    Paused = 0
}
"#,
            )
            .expect_err("duplicate enums discriminator");
        assert!(error.to_string().contains("duplicate discriminator"), "{error}");
    }

    #[test]
    fn enums_auto_increment_follows_last_explicit_discriminator() {
        let compiler = ValkyrieCompiler::new(SourceID::default());
        let module = compiler
            .compile_source(
                r#"
enums Status {
    Active = 2
    Inactive
}
"#,
            )
            .expect("compile enums with gap auto-increment");
        let sum_types = MirLowerer::lower_module_semantic(&module).sum_types;
        let status = sum_types.iter().find(|layout| layout.name == "Status").expect("Status layout");
        assert_eq!(status.variants[0].tag, 2);
        assert_eq!(status.variants[1].tag, 3);
    }

    #[test]
    fn enums_rejects_negative_discriminator_literal() {
        let compiler = ValkyrieCompiler::new(SourceID::default());
        let error = compiler
            .compile_source(
                r#"
enums Status {
    Active = -1
}
"#,
            )
            .expect_err("negative enums discriminator");
        assert!(error.to_string().contains("non-negative integer literal"), "{error}");
    }

    #[test]
    fn imported_semantic_export_enums_contribute_sum_layout_tags() {
        let groups = vec![
            CompilerSourceGroup {
                dependency_key: "dep".into(),
                name: "dep".into(),
                source: r#"
enums Status {
    Active = 2
    Inactive
}
"#
                .into(),
                sources: Vec::new(),
                direct_dependencies: Vec::new(),
            },
            CompilerSourceGroup {
                dependency_key: "app".into(),
                name: "app".into(),
                source: "micro main() { return }".into(),
                sources: Vec::new(),
                direct_dependencies: vec!["dep".into()],
            },
        ];
        let modules = ValkyrieCompiler::default().resolve_source_groups(&groups, DEFAULT_COMPILE_ARCH).expect("consumer with imported enums");
        let sum_types = MirLowerer::lower_module_semantic(&modules[1]).sum_types;
        let status = sum_types.iter().find(|layout| layout.name == "Status").expect("Status layout");
        assert_eq!(status.variants[0].tag, 2);
        assert_eq!(status.variants[1].tag, 3);
    }

    #[test]
    fn rejects_duplicate_discriminators_in_imported_semantic_export_enums() {
        let compiler = ValkyrieCompiler::new(SourceID::default());
        let duplicate_tag = |value: i64| HirExpr { kind: HirExprKind::Literal(HirLiteral::Integer64(value)), span: test_span() };
        let bad_enum = HirEnum {
            declaration: None,
            name: Identifier::new("Status"),
            doc: Default::default(),
            generics: Vec::new(),
            variants: vec![
                HirVariant {
                    name: Identifier::new("Active"),
                    doc: Default::default(),
                    fields: Vec::new(),
                    result_type: None,
                    discriminator: Some(duplicate_tag(0)),
                    ..Default::default()
                },
                HirVariant {
                    name: Identifier::new("Paused"),
                    doc: Default::default(),
                    fields: Vec::new(),
                    result_type: None,
                    discriminator: Some(duplicate_tag(0)),
                    ..Default::default()
                },
            ],
            visibility: Default::default(),
            is_unity: false,
        };
        let mut module = compiler.compile_source("micro main() { return }").expect("consumer shell");
        module.imported_semantic_exports.push(HirDependencySemanticExport {
            module: NamePath::new(vec![Identifier::new("dep")]),
            functions: Vec::new(),
            structs: Vec::new(),
            enums: vec![bad_enum],
            traits: Vec::new(),
            type_aliases: Vec::new(),
            impls: Vec::new(),
        });
        let error = compiler.validate_hir_semantic_contract(&module).expect_err("imported duplicate discriminator");
        assert!(error.to_string().contains("duplicate discriminator"), "{error}");
    }

    #[test]
    fn imported_semantic_export_unite_contributes_sum_layout_tags() {
        let groups = vec![
            CompilerSourceGroup {
                dependency_key: "dep".into(),
                name: "dep".into(),
                source: r#"
unite Choice {
    [tag(2)]
    A { x: i64 }
    B { y: i64 }
}
"#
                .into(),
                sources: Vec::new(),
                direct_dependencies: Vec::new(),
            },
            CompilerSourceGroup {
                dependency_key: "app".into(),
                name: "app".into(),
                source: "micro main() { return }".into(),
                sources: Vec::new(),
                direct_dependencies: vec!["dep".into()],
            },
        ];
        let modules = ValkyrieCompiler::default().resolve_source_groups(&groups, DEFAULT_COMPILE_ARCH).expect("consumer with imported unite");
        let sum_types = MirLowerer::lower_module_semantic(&modules[1]).sum_types;
        let choice = sum_types.iter().find(|layout| layout.name == "Choice").expect("Choice layout");
        assert!(choice.is_unite);
        assert_eq!(choice.variants[0].tag, 2);
        assert_eq!(choice.variants[1].tag, 3);
    }

    #[test]
    fn rejects_duplicate_unite_and_enums_names() {
        let compiler = ValkyrieCompiler::new(SourceID::default());
        let error = compiler
            .compile_source(
                r#"
unite Kind {
    A { x: i64 }
}

enums Kind {
    A
    B
}
"#,
            )
            .expect_err("duplicate unite and enums name");
        let message = error.to_string();
        assert!(message.contains("duplicate definition"), "{error}");
        assert!(message.contains("unite") && message.contains("enums"), "{error}");
    }

    #[test]
    fn rejects_duplicate_unite_and_union_names() {
        let compiler = ValkyrieCompiler::new(SourceID::default());
        let error = compiler
            .compile_source(
                r#"
unite Limb {
    Small { value: i64 }
    Large { value: i64 }
}

union Limb {
    small: i64
    words: i64
}
"#,
            )
            .expect_err("duplicate sum type name");
        let message = error.to_string();
        assert!(message.contains("duplicate definition"), "{error}");
        assert!(message.contains("unite") && message.contains("union"), "{error}");
    }
}

#[cfg(test)]
mod range_call_repro {
    //! 对齐 `valkyrie.v` core：`Range`/`TextSpan` 字段为 `usize`，跨 namespace 构造须冻结 Nominal。
    //! 禁止用 `i32` 规避或只 `eprintln` 打印 HIR 冒充通过。
    use super::*;
    use crate::types::{
        SourceID,
        hir::{HirExprKind, HirStatementKind},
    };

    fn compile(source: &str) -> Result<HirModule, ParseError> {
        ValkyrieCompiler::new(SourceID::default()).compile_source(source)
    }

    fn find_struct<'a>(hir: &'a HirModule, name: &str) -> &'a HirStruct {
        hir.structs
            .iter()
            .find(|item| item.name.as_str() == name)
            .or_else(|| hir.submodules.iter().flat_map(|module| &module.structs).find(|item| item.name.as_str() == name))
            .unwrap_or_else(|| panic!("缺少结构 `{name}`"))
    }

    fn assert_nominal_return(hir: &HirModule, function_name: &str, owner_name: &str) {
        let owner = find_struct(hir, owner_name);
        let declaration = owner.constructor_declaration.expect("结构须已分配 constructor_declaration");
        let function = hir
            .functions
            .iter()
            .chain(hir.impls.iter().flat_map(|item| item.methods.iter()))
            .find(|item| item.name.as_str() == function_name)
            .unwrap_or_else(|| panic!("缺少函数 `{function_name}`"));
        assert_eq!(
            function.return_type,
            ValkyrieType::Nominal { declaration, arguments: Vec::new() },
            "`{function_name}` 返回类型须是 Nominal{{ItemId}}，不得残留 Named/限定名字符串"
        );
    }

    fn assert_resolved_range_construct(hir: &HirModule, function_name: &str) {
        let function = hir
            .functions
            .iter()
            .chain(hir.impls.iter().flat_map(|item| item.methods.iter()))
            .find(|item| item.name.as_str() == function_name)
            .unwrap_or_else(|| panic!("缺少函数 `{function_name}`"));
        let return_expr = match function.body.statements.first().map(|statement| &statement.kind) {
            Some(HirStatementKind::Expr(statement)) => statement.as_ref(),
            _ => function.body.expr.as_deref().expect("函数体须有 return"),
        };
        let HirExprKind::Return(Some(expression)) = &return_expr.kind
        else {
            panic!("期望 return 表达式");
        };
        match &expression.kind {
            HirExprKind::Call { resolved: Some(contract), .. } | HirExprKind::Construct { resolved: Some(contract), .. } => {
                assert!(contract.symbol.to_string().contains("Range"), "构造合同符号须指向 Range，got {}", contract.symbol);
                assert!(
                    matches!(contract.return_type, ValkyrieType::Nominal { .. }),
                    "Range 构造返回须是 Nominal，got {:?}",
                    contract.return_type
                );
            }
            other => panic!("期望已解析的 Range 构造/调用，got {other:?}"),
        }
    }

    #[test]
    fn range_usize_literals_bind_nominal_and_resolve() {
        let source = r#"
structure Range {
    start: usize,
    end: usize,
}
micro make() -> Range {
    return Range(1, 2)
}
"#;
        let hir = compile(source).unwrap_or_else(|error| panic!("usize Range 构造必须通过 Compiler contract: {error}"));
        assert_nominal_return(&hir, "make", "Range");
        assert_resolved_range_construct(&hir, "make");
    }

    #[test]
    fn textspan_as_range_same_namespace_binds_nominal() {
        // 先验收字段投影入参（无 `+`）；加法是独立 overload 合同。
        let source = r#"
structure Range {
    start: usize,
    end: usize,
}
structure TextSpan {
    offset: usize,
    length: usize
}
imply TextSpan {
    micro as_range(self) -> Range {
        return Range(self.offset, self.length)
    }
}
"#;
        let hir = compile(source).unwrap_or_else(|error| panic!("同 namespace TextSpan.as_range 必须通过: {error}"));
        assert_nominal_return(&hir, "as_range", "Range");
        assert_resolved_range_construct(&hir, "as_range");
    }

    #[test]
    fn textspan_as_range_with_usize_add_binds_nominal() {
        // 对齐生产 core：`structure usize` + `infix +` + `Range(self.offset, self.offset + self.length)`。
        // 禁止省略 usize 运算符声明后仍期望 `+` 魔法通过。
        let source = r#"
[primitive("core::primitive::usize")]
structure usize { }

imply usize {
    infix `+`(self, rhs: Self): Self {
        return self
    }
}

structure Range {
    start: usize,
    end: usize,
}
structure TextSpan {
    offset: usize,
    length: usize
}
imply TextSpan {
    micro as_range(self) -> Range {
        return Range(self.offset, self.offset + self.length)
    }
}
"#;
        let hir = compile(source).unwrap_or_else(|error| panic!("usize 加法 + Range 构造必须通过: {error}"));
        assert_nominal_return(&hir, "as_range", "Range");
        assert_resolved_range_construct(&hir, "as_range");
        let usize_decl = find_struct(&hir, "usize").constructor_declaration.expect("usize declaration");
        let add = hir.impls.iter().flat_map(|item| item.methods.iter()).find(|method| method.name.as_str() == "+").expect("infix +");
        assert!(
            matches!(add.params[0].ty, ValkyrieType::Nominal { declaration, .. } if declaration == usize_decl)
                || matches!(
                    &hir.impls
                        .iter()
                        .find(|item| matches!(&item.target, ValkyrieType::Nominal { declaration, .. } if *declaration == usize_decl)),
                    Some(_)
                ),
            "usize imply / Self 须冻结为 Nominal{{ItemId}}，不得残留 Named(\"usize\")"
        );
    }

    #[test]
    fn textspan_as_range_cross_namespace_binds_nominal() {
        let source = r#"
[primitive("core::primitive::usize")]
structure usize { }

imply usize {
    infix `+`(self, rhs: Self): Self {
        return self
    }
}

namespace core.types
structure Range {
    start: usize,
    end: usize,
}
namespace core.text
structure TextSpan {
    offset: usize,
    length: usize
}
imply TextSpan {
    micro as_range(self) -> Range {
        return Range(self.offset, self.offset + self.length)
    }
}
"#;
        let hir = compile(source).unwrap_or_else(|error| panic!("跨 namespace TextSpan.as_range 必须通过: {error}"));
        assert_nominal_return(&hir, "as_range", "Range");
        assert_resolved_range_construct(&hir, "as_range");
    }

    #[test]
    fn option_or_else_some_construct_binds_nominal_without_short_name() {
        // 对齐生产 `core/source/types/Option.v`：`unite` 变体 + 自由函数 `Some` 同名并存时，
        // `or_else` 臂内 `Some(value)` 必须冻结 Constructor 合同，不得 SMIR003。
        let source = r#"
namespace core::types;
unite Option<T> {
    Some { value: T },
    None,
}
micro Some<T>(value: T) -> Option<T> {
    let wrapped: Option<T> = Some(value)
    return wrapped
}
imply Option<T> {
    micro or_else(self, f: micro() -> Option<T>): Option<T> {
        match self {
            case Some(value):
                Some(value)
            case None:
                f()
        }
    }
}
"#;
        let hir = compile(source).unwrap_or_else(|error| panic!("Option.or_else 中 Some(value) 必须通过 Compiler contract: {error}"));
        let option = hir
            .enums
            .iter()
            .find(|item| item.name.as_str() == "Option")
            .or_else(|| hir.submodules.iter().flat_map(|module| &module.enums).find(|item| item.name.as_str() == "Option"))
            .expect("Option");
        let declaration = option.declaration.expect("Option declaration");
        let or_else = hir.impls.iter().flat_map(|item| item.methods.iter()).find(|item| item.name.as_str() == "or_else").expect("or_else");
        assert!(
            matches!(&or_else.return_type, ValkyrieType::Nominal { declaration: id, .. } if *id == declaration),
            "or_else 返回须是 Nominal{{Option ItemId}}，got {:?}",
            or_else.return_type
        );
        let mut saw_resolved_some = false;
        fn walk_expr(expr: &crate::types::hir::HirExpr, saw: &mut bool) {
            match &expr.kind {
                HirExprKind::Call { resolved, args, .. } => {
                    if let Some(contract) = resolved {
                        if contract.symbol.to_string().contains("Some") {
                            *saw = true;
                            assert!(
                                matches!(contract.return_type, ValkyrieType::Nominal { .. }),
                                "Some 构造返回须是 Nominal，got {:?}",
                                contract.return_type
                            );
                            assert!(contract.sum_owner.is_some() && contract.sum_variant.is_some(), "Some 须携带 sum_owner/sum_variant");
                        }
                    }
                    for arg in args {
                        walk_expr(&arg.value, saw);
                    }
                }
                HirExprKind::Construct { resolved, args, .. } => {
                    if let Some(contract) = resolved {
                        if contract.symbol.to_string().contains("Some") {
                            *saw = true;
                            assert!(
                                matches!(contract.return_type, ValkyrieType::Nominal { .. }),
                                "Some 构造返回须是 Nominal，got {:?}",
                                contract.return_type
                            );
                            assert!(contract.sum_owner.is_some() && contract.sum_variant.is_some(), "Some 须携带 sum_owner/sum_variant");
                        }
                    }
                    for arg in args {
                        walk_expr(arg, saw);
                    }
                }
                HirExprKind::Match { scrutinee, arms, .. } => {
                    walk_expr(scrutinee, saw);
                    for arm in arms {
                        walk_expr(&arm.body, saw);
                    }
                }
                HirExprKind::Return(Some(inner)) => walk_expr(inner, saw),
                HirExprKind::Block(block) => {
                    for statement in &block.statements {
                        if let HirStatementKind::Expr(inner) = &statement.kind {
                            walk_expr(inner, saw);
                        }
                    }
                    if let Some(inner) = &block.expr {
                        walk_expr(inner, saw);
                    }
                }
                _ => {}
            }
        }
        for statement in &or_else.body.statements {
            if let HirStatementKind::Expr(expr) = &statement.kind {
                walk_expr(expr, &mut saw_resolved_some);
            }
        }
        if let Some(expr) = &or_else.body.expr {
            walk_expr(expr, &mut saw_resolved_some);
        }
        assert!(saw_resolved_some, "or_else 臂内须有已解析的 Some 构造合同");
    }

    #[test]
    fn associated_type_named_base_binds_to_nominal_declaration() {
        // `Carrier::Payload` 降低为 Associated{base: Named}；bind 须冻结 base 为 Nominal{ItemId}。
        // 不返回该 Associated，避免控制流把 Associated 与结构 Nominal 认亲。
        let source = r#"
structure Payload {
    value: i64,
}
structure Carrier {
}
micro inspect(value: Carrier::Payload): unit {
}
"#;
        let hir = compile(source).unwrap_or_else(|error| panic!("Associated 基类型须能完成 bind: {error}"));
        let carrier = find_struct(&hir, "Carrier");
        let carrier_decl = carrier.constructor_declaration.expect("Carrier constructor_declaration");
        let inspect = hir.functions.iter().find(|item| item.name.as_str() == "inspect").expect("inspect");
        let param_ty = &inspect.params[0].ty;
        match param_ty {
            ValkyrieType::Associated(associated) => {
                assert_eq!(associated.name.as_str(), "Payload");
                assert_eq!(
                    associated.base,
                    ValkyrieType::Nominal { declaration: carrier_decl, arguments: Vec::new() },
                    "Associated.base 须是 Nominal{{Carrier}}，不得残留 Named，got {:?}",
                    associated.base
                );
            }
            other => panic!("参数类型须是 Associated，got {other:?}"),
        }
    }

    #[test]
    fn same_module_usize_eq_literal_resolves_operator_contract() {
        // 字面量 `0` 默认推断为 i32；必须在 unify 前按 formal `usize` 特化。
        let source = r#"
[primitive("core::primitive::usize")]
structure usize { }
imply usize {
    infix `==`(self, rhs: Self): bool { return true }
}
micro check(ordinal: usize): bool {
    return ordinal == 0
}
"#;
        ValkyrieCompiler::default().compile_source(source).unwrap_or_else(|error| panic!("同模块 usize==0 必须解析: {error}"));
    }

    #[test]
    #[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
    fn cross_package_array_host_get_usize_ops_resolve() {
        // 对齐 std.adaptor.wasm `array_host_get`：`ordinal == 0` / `ordinal > item_count`。
        let groups = vec![
            CompilerSourceGroup {
                dependency_key: "core".into(),
                name: "core".into(),
                source: r#"
namespace core::primitive;
[primitive("core::primitive::usize")]
structure usize { }
imply usize {
    infix `==`(self, rhs: Self): bool { return true }
    infix `>`(self, rhs: Self): bool { return true }
    infix `-`(self, rhs: Self): Self { return self }
}
namespace core::types;
unite Option<T> { Some { value: T }, None }
micro Some<T>(value: T) -> Option<T> {
    let wrapped: Option<T> = Some(value)
    return wrapped
}
"#
                .into(),
                sources: Vec::new(),
                direct_dependencies: Vec::new(),
            },
            CompilerSourceGroup {
                dependency_key: "std.adaptor.wasm".into(),
                name: "std.adaptor.wasm".into(),
                source: r#"
namespace std.adaptor.wasm.collection;
micro array_host_get<T>(array: [T], ordinal: usize): Option<T> {
    let item_count: usize = 1
    if ordinal == 0 || ordinal > item_count {
        return None
    }
    return Some(array::[ordinal - 1])
}
"#
                .into(),
                sources: Vec::new(),
                direct_dependencies: vec!["core".into()],
            },
        ];
        ValkyrieCompiler::default()
            .resolve_source_groups(&groups, DEFAULT_COMPILE_ARCH)
            .unwrap_or_else(|error| panic!("跨包 array_host_get Some/下标必须解析: {error}"));
    }

    #[test]
    #[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
    fn cross_package_some_with_local_value_resolves() {
        let groups = vec![
            CompilerSourceGroup {
                dependency_key: "core".into(),
                name: "core".into(),
                source: r#"
namespace core::types;
unite Option<T> { Some { value: T }, None }
micro Some<T>(value: T) -> Option<T> {
    let wrapped: Option<T> = Some(value)
    return wrapped
}
"#
                .into(),
                sources: Vec::new(),
                direct_dependencies: Vec::new(),
            },
            CompilerSourceGroup {
                dependency_key: "app".into(),
                name: "app".into(),
                source: r#"
micro wrap<T>(value: T): Option<T> {
    return Some(value)
}
"#
                .into(),
                sources: Vec::new(),
                direct_dependencies: vec!["core".into()],
            },
        ];
        ValkyrieCompiler::default()
            .resolve_source_groups(&groups, DEFAULT_COMPILE_ARCH)
            .unwrap_or_else(|error| panic!("跨包 Some(value) 必须解析: {error}"));
    }

    #[test]
    #[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
    fn cross_package_array_host_get_links_without_orphan_callable() {
        // 对齐 std.adaptor.wasm Array.v：`array.length` / `array::[i]` / Some/None 不得留下无合同 Call。
        let groups = vec![
            CompilerSourceGroup {
                dependency_key: "core".into(),
                name: "core".into(),
                source: r#"
namespace core::primitive;
[primitive("core::primitive::usize")]
structure usize { }
imply usize {
    infix `==`(self, rhs: Self): bool { return true }
    infix `>`(self, rhs: Self): bool { return true }
    infix `-`(self, rhs: Self): Self { return self }
}
namespace core::types;
unite Option<T> { Some { value: T }, None }
micro Some<T>(value: T) -> Option<T> {
    let wrapped: Option<T> = Some(value)
    return wrapped
}
"#
                .into(),
                sources: Vec::new(),
                direct_dependencies: Vec::new(),
            },
            CompilerSourceGroup {
                dependency_key: "std.adaptor.wasm".into(),
                name: "std.adaptor.wasm".into(),
                source: r#"
namespace std.adaptor.wasm.collection;
micro array_host_get<T>(array: [T], ordinal: usize): Option<T> {
    let item_count: usize = array.length
    if ordinal == 0 || ordinal > item_count {
        return None
    }
    return Some(array::[ordinal - 1])
}
micro array_host_set<T>(mut array: [T], ordinal: usize, value: T): unit {
    let item_count: usize = array.length
    if ordinal == 0 || ordinal > item_count {
        return
    }
    array::[ordinal - 1] = value
}
"#
                .into(),
                sources: Vec::new(),
                direct_dependencies: vec!["core".into()],
            },
        ];
        let compiler = ValkyrieCompiler::default();
        let mut hir_groups = compiler.resolve_source_groups(&groups, DEFAULT_COMPILE_ARCH).expect("HIR");
        let final_hir = hir_groups.pop().expect("adaptor");
        let mir_groups = hir_groups.iter().map(crate::valkyrie::mir::MirLowerer::lower_module_semantic).collect::<Vec<_>>();
        let mut final_mir = crate::valkyrie::mir::MirLowerer::lower_module_semantic(&final_hir);
        let mut defined = std::collections::BTreeSet::new();
        for module in std::iter::once(&final_mir).chain(mir_groups.iter()) {
            eprintln!("-- mir module functions --");
            for function in &module.functions {
                eprintln!("  {:?} decl={:?} inst={:?}", function.symbol, function.declaration, function.instance);
                if let Some(instance) = function.instance {
                    defined.insert(instance);
                }
            }
            for contract in &module.external_calls {
                eprintln!("  import {:?} inst={:?}", contract.symbol, contract.instance);
                if let Some(instance) = contract.instance {
                    defined.insert(instance);
                }
            }
        }
        eprintln!("defined instances: {defined:?}");
        for function in &final_mir.functions {
            for block in &function.blocks {
                for instruction in &block.instructions {
                    if let crate::valkyrie::mir::ssa::MirOperation::Call {
                        callee: crate::valkyrie::mir::ssa::MirOperand::Callable(callee),
                        ..
                    } = &instruction.kind
                    {
                        if !defined.contains(callee) {
                            panic!(
                                "orphan Callable {callee:?} in {}; ops={:?}",
                                function.symbol,
                                function
                                    .blocks
                                    .iter()
                                    .flat_map(|block| block.instructions.iter().map(|item| format!("{:?}", item.kind)))
                                    .collect::<Vec<_>>()
                            );
                        }
                    }
                }
            }
        }
        let modules = std::iter::once(&final_mir).chain(mir_groups.iter()).collect::<Vec<_>>();
        let functions = modules.iter().flat_map(|module| module.functions.iter().cloned()).collect::<Vec<_>>();
        let imports = modules.iter().flat_map(|module| module.external_calls.iter().cloned()).collect::<Vec<_>>();
        let structures = modules.iter().flat_map(|module| module.structs.iter().cloned()).collect::<Vec<_>>();
        let type_identities = crate::valkyrie::mir::ssa::type_identity_table(&functions, &imports, &structures, &final_mir.sum_types);
        final_mir.type_identities = type_identities.clone();
        let mut deps = mir_groups;
        for module in &mut deps {
            module.type_identities = type_identities.clone();
        }
        crate::valkyrie::compile_pipeline::link_reachable_dependency_mir(&mut final_mir, &deps)
            .unwrap_or_else(|error| panic!("array_host 跨包链接必须成功: {error}"));
        // 完整 Compiler 载荷路径：字面量须特化为 Nominal{usize}，不得 SMIR007 Integer32。
        ValkyrieCompiler::default()
            .compile_source_groups_to_program(&groups)
            .unwrap_or_else(|error| panic!("array_host_get 完整 Compiler 载荷不得留下孤儿实例: {error}"));
    }

    #[test]
    fn same_module_private_wasm_host_call_resolves() {
        let source = r#"
namespace demo.text;
micro host_length(value: utf16): isize {
    return __host_utf16_length(value)
}
[wasm("env", "utf16_length")]
private micro __host_utf16_length(s: utf16): isize
"#;
        ValkyrieCompiler::default().compile_source(source).unwrap_or_else(|error| panic!("同模块 private wasm host 调用必须解析: {error}"));
    }

    #[test]
    fn same_module_private_wasm_host_call_with_std_text_path_resolves() {
        // Oak parse_type 必须产出完整 NamePath（`.` / `::`）；禁止前端再按 span 切片重解析。
        let source = r#"
namespace demo.text;
micro host_length(value: std.text.utf16): isize {
    return __host_utf16_length(value)
}
[wasm("env", "utf16_length")]
private micro __host_utf16_length(s: utf16): isize
"#;
        ValkyrieCompiler::default().compile_source(source).unwrap_or_else(|error| panic!("Oak NamePath 须保留 std.text.utf16: {error}"));
    }

    #[test]
    fn numeric_type_name_call_lowers_as_cast() {
        // 对齐 std.adaptor.wasm `time_ms`：`i64(perf_now())` 不得走未解析函数调用。
        let source = r#"
micro time_ms(value: f64): i64 {
    return i64(value)
}
"#;
        let module = ValkyrieCompiler::default().compile_source(source).unwrap_or_else(|error| panic!("数值类型名调用必须降为 Cast: {error}"));
        let function = module.functions.iter().find(|item| item.name.as_str() == "time_ms").expect("time_ms");
        let Some(expr) = function.body.expr.as_ref()
        else {
            panic!("expected return expr");
        };
        let HirExprKind::Return(Some(inner)) = &expr.kind
        else {
            panic!("expected return: {:?}", expr.kind);
        };
        assert!(
            matches!(inner.kind, HirExprKind::Cast { ty: ValkyrieType::Integer64 { signed: true }, .. }),
            "i64(x) must lower to Cast, got {:?}",
            inner.kind
        );
    }

    #[test]
    #[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
    fn same_module_array_subscript_resolves() {
        let source = r#"
[primitive("core::primitive::usize")]
structure usize { }
imply usize {
    infix `-`(self, rhs: Self): Self { return self }
}
micro get<T>(array: [T], ordinal: usize): T {
    return array::[ordinal - 1]
}
"#;
        ValkyrieCompiler::default().compile_source(source).unwrap_or_else(|error| panic!("同模块 array::[i] 必须解析: {error}"));
    }

    #[test]
    #[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
    fn same_module_some_after_let_subscript_resolves() {
        let source = r#"
[primitive("core::primitive::usize")]
structure usize { }
imply usize {
    infix `-`(self, rhs: Self): Self { return self }
}
unite Option<T> { Some { value: T }, None }
micro Some<T>(value: T) -> Option<T> {
    let wrapped: Option<T> = Some(value)
    return wrapped
}
micro array_host_get<T>(array: [T], ordinal: usize): Option<T> {
    let item: T = array::[ordinal - 1]
    return Some(item)
}
"#;
        ValkyrieCompiler::default().compile_source(source).unwrap_or_else(|error| panic!("同模块 let + Some(item) 必须解析: {error}"));
    }
}

/// Oak 已区分返回类型与约束；这里只转换结构化语法事实。
fn lower_where_constraints(constraints: &[oak_valkyrie::ast::type_nodes::WhereConstraint], source_id: SourceID) -> Vec<HirWhereConstraint> {
    constraints
        .iter()
        .map(|constraint| HirWhereConstraint {
            target: lower_type_expression(&constraint.target),
            bounds: constraint
                .bounds
                .iter()
                .map(|bound| HirTraitBound {
                    declaration: None,
                    trait_path: NamePath::new(bound.trait_path.parts.iter().map(oak_identifier).collect()),
                    type_arguments: bound.type_arguments.iter().map(lower_type_expression).collect(),
                    associated_types: bound
                        .associated_types
                        .iter()
                        .map(|equation| HirAssociatedTypeBinding {
                            name: oak_identifier(&equation.name),
                            ty: lower_type_expression(&equation.ty),
                        })
                        .collect(),
                })
                .collect(),
            span: with_source(&frontend::std_range(&constraint.span), source_id),
        })
        .collect()
}

#[cfg(test)]
mod where_contract_tests;
