use std::{cell::RefCell, ops::Range, path::Path};

use crate::{
    frontend_contract::{
        planning::{FrontendNeutralPlan, hir_module_to_frontend_neutral_plan},
    },
    hir::{
        BuiltinTypeAliasScope, ModuleTypeAliasScope, hoist_anonymous_classes, lower_type_expression,
        overload::{resolve_hir_calls, validate_extractor_patterns},
        render_type_expression, validate_ast_root,
    },
    mir::{FlagsLayout, MirLowerer, MirSumDeclaration, MirSumVariant, SumTypeLayout},
    types::{
        Identifier, NamePath, SourceID, SourceSpan,
        hir::{
            GenericType, HirArgument, HirAssociatedConst, HirAssociatedConstImpl, HirAssociatedType, HirAssociatedTypeImpl, HirAttribute,
            HirBlock, HirCallArgument, HirCompileWarning, HirDependencySemanticExport, HirDocumentation, HirEnum, HirExpr, HirExprKind,
            HirField, HirFlagMember, HirFlags, HirFunction, HirIdentifier, HirImpl, HirImport, HirImportBinding, HirKind, HirLiteral,
            HirMatchArm, HirModule, HirParam, HirParameterBindingKind, HirParent, HirPattern, HirProperty, HirSingleton, HirStatement,
            HirStatementKind, HirStruct, HirTrait, HirTypeAlias, HirTypeFunction, HirVariadicKind, HirVariant, HirVisibility,
            HirWhereConstraint, HirWidget, HirWidgetLifecycle, ValkyrieType,
        },
    },
    validation::{ControlFlowScheduler, validate_semantic_module},
    valkyrie::{
        backend_contract::interop::validate_interop_surface,
        mir::{SINGLETON_CONSTRUCTOR_NAME, SINGLETON_FINALIZER_NAME},
    },
};
use nyar_types::NyarType;
use ordered_float::OrderedFloat;
use std_data::text::valkyrie::{
    AstParser, AttributeItem, BinaryOperator, ClassDeclaration, ClassLikeKind, DeclarationBody, FlagsDeclaration, FlagsMemberDeclaration,
    FunctionDeclKind, FunctionDeclaration, FunctionParameter, FunctionStatement, GenericParameterDeclaration, ImplyAssociatedConstBinding,
    ImplyAssociatedTypeBinding, ImplyDeclaration, InheritanceItem, LetStatement, LiteralExpression, MacroAssignDeclaration,
    NamePath as AstNamePath, NamespaceDeclaration, ObjectFieldDeclaration, ObjectMethodDeclaration, ParameterBindingKind,
    ParameterVariadicKind, ParseError, RootStatement, StringLiteral as AstStringLiteral, StringSegment as AstStringSegment, SumTypeKind,
    TermExpression, TestsDeclaration, TraitAssociatedConstDeclaration, TraitAssociatedTypeDeclaration, TraitDeclaration, TypeExpression,
    UnaryOperator, UniteDeclaration, UniteVariantDeclaration, UsingStatement, ValkyrieRoot,
    ast::{PatternExpression, SubscriptKind},
};

thread_local! {
    static COMPILE_WARNINGS: RefCell<Vec<HirCompileWarning>> = const { RefCell::new(Vec::new()) };
}

#[cfg(test)]
mod source_group_tests {
    use super::{CompilerSourceGroup, ValkyrieCompiler};

    #[test]
    fn compiler_owns_dependency_group_linking() {
        let groups = vec![
            CompilerSourceGroup {
                dependency_key: "core".into(),
                name: "core".into(),
                source: "micro answer() -> i32 { return 1 }".into(),
                direct_dependencies: Vec::new(),
            },
            CompilerSourceGroup {
                dependency_key: "app".into(),
                name: "app".into(),
                source: "micro main() -> i32 { return answer() }".into(),
                direct_dependencies: vec!["core".into()],
            },
        ];
        let output = ValkyrieCompiler::default().compile_source_groups(&groups).expect("compiler closes source groups");
        assert_eq!(output.hir_module().name.to_string(), "app");
        assert!(output.semantic_mir().functions.iter().any(|function| function.symbol == "core::answer"));
    }

    #[test]
    fn compiler_rejects_unknown_dependency_identity() {
        let groups = vec![CompilerSourceGroup {
            dependency_key: "app".into(),
            name: "app".into(),
            source: "micro main() { return }".into(),
            direct_dependencies: vec!["missing".into()],
        }];
        let error = ValkyrieCompiler::default().compile_source_groups(&groups).expect_err("unknown dependency must fail at Compiler boundary");
        assert!(error.to_string().contains("semantic dependency export `missing`"));
    }

    #[test]
    fn compiler_rejects_dependency_identity_collision() {
        let groups = vec![
            CompilerSourceGroup {
                dependency_key: "same".into(),
                name: "first".into(),
                source: "micro first() { return }".into(),
                direct_dependencies: Vec::new(),
            },
            CompilerSourceGroup {
                dependency_key: "same".into(),
                name: "second".into(),
                source: "micro second() { return }".into(),
                direct_dependencies: Vec::new(),
            },
        ];
        let error = ValkyrieCompiler::default().compile_source_groups(&groups).expect_err("duplicate dependency identity must fail");
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
        HirExprKind::Call { callee, args, resolved } => {
            if resolved.is_none() {
                return Err(ParseError::invalid(format!("SMIR003 unresolved call contract in `{function}` at {:?}", expr.span)));
            }
            validate_expr_call_contracts(callee, function)?;
            for arg in args {
                validate_expr_call_contracts(&arg.value, function)?;
            }
        }
        HirExprKind::Construct { args, resolved, .. } => {
            if resolved.is_none() {
                return Err(ParseError::invalid(format!("SMIR003 unresolved constructor contract in `{function}` at {:?}", expr.span)));
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
        | HirExprKind::TryPropagate(value)
        | HirExprKind::Raise(value)
        | HirExprKind::Resume(value) => validate_expr_call_contracts(value, function)?,
        HirExprKind::ArrayNew { length, .. } => validate_expr_call_contracts(length, function)?,
        HirExprKind::ArrayLiteral { items } => {
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
mod tgrammar;
mod vx;

pub use super::CaptureAnalyzer;
use expr_lowering::{extract_name_path, lower_block, lower_term_expression};
use macro_expand::expand_macros_in_root;
use tgrammar::expand_tgrammar_in_root;
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
    /// 只允许引用已完成链接的直接依赖身份。
    pub direct_dependencies: Vec<String>,
}

/// Stable frontend build output consumed by the application layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontendBuildOutput {
    hir_module: HirModule,
    neutral_plan: FrontendNeutralPlan,
    semantic_mir: crate::valkyrie::mir::MirModule,
    compiled_program: nyar_types::CompiledProgram,
}

impl FrontendBuildOutput {
    /// Build output from a lowered HIR module.
    pub fn from_hir_module(hir_module: HirModule) -> Result<Self, ParseError> {
        let neutral_plan = hir_module_to_frontend_neutral_plan(&hir_module);
        let semantic_mir = crate::valkyrie::mir::MirLowerer::lower_module_semantic(&hir_module);
        Self::from_hir_and_semantic_mir(hir_module, neutral_plan, semantic_mir)
    }

    fn from_hir_and_semantic_mir(
        hir_module: HirModule,
        neutral_plan: FrontendNeutralPlan,
        semantic_mir: crate::valkyrie::mir::MirModule,
    ) -> Result<Self, ParseError> {
        let compiled_program = crate::valkyrie::compile_pipeline::compile_linked_semantic_mir(&semantic_mir)
            .map_err(|error| ParseError::invalid(format!("Compiler 成功载荷生产失败: {error:?}")))?;
        Ok(Self { hir_module, neutral_plan, semantic_mir, compiled_program })
    }

    /// 返回 lowering 后的 HIR 模块。
    pub fn hir_module(&self) -> &HirModule {
        &self.hir_module
    }

    /// 返回中性的前端计划。
    pub fn neutral_plan(&self) -> &FrontendNeutralPlan {
        &self.neutral_plan
    }

    /// Return the semantic MIR lowered once during frontend compilation.
    pub fn semantic_mir(&self) -> &crate::valkyrie::mir::MirModule {
        &self.semantic_mir
    }

    /// 返回 Compiler 已验证的 CanonicalProgram；消费者不得重新生产。
    pub fn canonical_program(&self) -> &nyar_types::CanonicalProgram {
        self.compiled_program.canonical()
    }

    /// 返回 Compiler 生成的不可拆分成功载荷。
    pub fn compiled_program(&self) -> &nyar_types::CompiledProgram {
        &self.compiled_program
    }

    /// 返回 `HIR` 函数数量，供装配层做调试输出。
    pub fn hir_function_count(&self) -> usize {
        self.hir_module.functions.len()
    }
}

/// Collect sum-type and flags layouts from a lowered HIR module.
pub fn compute_nominal_layouts(module: &HirModule) -> (Vec<SumTypeLayout>, Vec<FlagsLayout>) {
    (collect_sum_declarations(module).iter().map(MirSumDeclaration::physical_layout).collect(), collect_flags_layouts(module))
}

pub(crate) fn compute_nominal_declarations(module: &HirModule) -> (Vec<MirSumDeclaration>, Vec<FlagsLayout>) {
    (collect_sum_declarations(module), collect_flags_layouts(module))
}

fn collect_sum_declarations(module: &HirModule) -> Vec<MirSumDeclaration> {
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
                    MirSumVariant {
                        name: variant.name.to_string(),
                        tag,
                        fields: variant.fields.iter().map(|field| crate::mir::MirField { name: field.name.to_string(), ty: field.ty.clone() }).collect(),
                        result_type: variant.result_type.clone(),
                    }
                })
                .collect();
            MirSumDeclaration { name: enum_def.name.to_string(), is_unite: enum_def.is_unity, generics: enum_def.generics.clone(), variants }
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
                    MirSumVariant {
                        name: variant.name.to_string(),
                        tag,
                        fields: variant.fields.iter().map(|field| crate::mir::MirField { name: field.name.to_string(), ty: field.ty.clone() }).collect(),
                        result_type: variant.result_type.clone(),
                    }
                })
                .collect();
            let declaration = MirSumDeclaration { name: enum_def.name.to_string(), is_unite: enum_def.is_unity, generics: enum_def.generics.clone(), variants };
            if !layouts.contains(&declaration) {
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
fn resolve_sum_variant_tag(
    owner: &crate::types::Identifier,
    variant: &HirVariant,
    next_implicit: &mut u32,
) -> Result<u32, ParseError> {
    let tag = if let Some(discriminator) = &variant.discriminator {
        integer_literal_u32(discriminator).ok_or_else(|| {
            ParseError::invalid(format!(
                "`{}` variant `{}` discriminator must be a non-negative integer literal",
                owner, variant.name
            ))
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
        let mut root = AstParser::parse_root(source)?;
        expand_tgrammar_in_root(&mut root);
        expand_macros_in_root(&mut root);
        let hir = self.lower_root_with_semantic_exports_and_name(&root, imported_semantic_exports, module_name)?;
        self.validate_hir_semantic_contract(&hir)?;
        Ok(hir)
    }

    /// Parses a source file and lowers it into a minimal HIR module.
    pub fn compile_path(&self, path: &Path) -> Result<HirModule, ParseError> {
        let root = AstParser::parse_path(&path.to_path_buf())?;
        let hir = self.lower_root(&root)?;
        self.validate_hir_semantic_contract(&hir)?;
        Ok(hir)
    }

    /// Parses `.vx` source (Valkyrie + X-Grammar) and lowers into HIR with `view` → `render` normalization.
    pub fn compile_vx_source(&self, source: &str) -> Result<HirModule, ParseError> {
        let mut root = AstParser::parse_vx_root(source)?;
        expand_tgrammar_in_root(&mut root);
        expand_macros_in_root(&mut root);
        let hir = enhance_vx_widgets(self.lower_root(&root)?);
        self.validate_hir_semantic_contract(&hir)?;
        Ok(hir)
    }

    /// Parses a `.vx` file and lowers it into HIR with `view` → `render` normalization.
    pub fn compile_vx_path(&self, path: &Path) -> Result<HirModule, ParseError> {
        let source = std::fs::read_to_string(path)?;
        self.compile_vx_source(&source)
    }

    /// Parses source text and lowers it into the stable frontend build bundle.
    pub fn compile_source_to_build_output(&self, source: &str) -> Result<FrontendBuildOutput, ParseError> {
        let hir_module = self.compile_source(source)?;
        FrontendBuildOutput::from_hir_module(hir_module)
    }

    /// Builds the stable frontend bundle with resolved nominal dependency
    /// exports available to call resolution and extractor validation.
    pub fn compile_source_to_build_output_with_semantic_exports(
        &self,
        source: &str,
        imported_semantic_exports: &[HirDependencySemanticExport],
    ) -> Result<FrontendBuildOutput, ParseError> {
        let hir_module = self.compile_source_with_semantic_exports(source, imported_semantic_exports)?;
        FrontendBuildOutput::from_hir_module(hir_module)
    }

    /// 从完整依赖顺序的源码快照构建一个语义闭包。
    ///
    /// Resolver 只提供源码和依赖身份；导出合同、依赖 MIR 与可达链接全部
    /// 在 Compiler 内完成，调用方不得自行拼接 HIR 或 MIR。
    pub fn compile_source_groups(&self, groups: &[CompilerSourceGroup]) -> Result<FrontendBuildOutput, ParseError> {
        let mut exports = std::collections::BTreeMap::<String, HirDependencySemanticExport>::new();
        let mut hir_groups = Vec::with_capacity(groups.len());
        for group in groups {
            let dependency_exports = group
                .direct_dependencies
                .iter()
                .map(|name| exports.get(name).cloned().ok_or_else(|| ParseError::invalid(format!("semantic dependency export `{name}` is unavailable for `{}`", group.name))))
                .collect::<Result<Vec<_>, _>>()?;
            let hir_module = self.compile_source_with_semantic_exports_and_name(
                &group.source,
                &dependency_exports,
                Some(NamePath::new(vec![Identifier::new(&group.name)])),
            )?;
            let export = HirDependencySemanticExport {
                module: NamePath::new(vec![Identifier::new(&group.name)]),
                functions: hir_module.functions.clone(),
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
        let final_hir = hir_groups.pop().ok_or_else(|| ParseError::invalid("semantic source group plan is empty"))?;
        let mir_groups = hir_groups
            .iter()
            .map(crate::valkyrie::mir::MirLowerer::lower_module_semantic)
            .collect::<Vec<_>>();
        let mut final_mir = crate::valkyrie::mir::MirLowerer::lower_module_semantic(&final_hir);
        if !mir_groups.is_empty() {
            crate::valkyrie::compile_pipeline::link_reachable_dependency_mir(&mut final_mir, &mir_groups)?;
        }
        let neutral_plan = hir_module_to_frontend_neutral_plan(&final_hir);
        FrontendBuildOutput::from_hir_and_semantic_mir(final_hir, neutral_plan, final_mir)
    }

    /// Parses a source file and lowers it into the stable frontend build bundle.
    pub fn compile_path_to_build_output(&self, path: &Path) -> Result<FrontendBuildOutput, ParseError> {
        let hir_module = self.compile_path(path)?;
        FrontendBuildOutput::from_hir_module(hir_module)
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
            arguments
                .iter()
                .map(|argument| lower_trait_associated_type_references(argument, associated_names, generic_names))
                .collect(),
        ),
        ValkyrieType::Array(element) => ValkyrieType::Array(Box::new(lower_trait_associated_type_references(element, associated_names, generic_names))),
        ValkyrieType::FixedArray { element, length } => ValkyrieType::FixedArray {
            element: Box::new(lower_trait_associated_type_references(element, associated_names, generic_names)),
            length: *length,
        },
        ValkyrieType::Nullable(inner) => ValkyrieType::Nullable(Box::new(lower_trait_associated_type_references(inner, associated_names, generic_names))),
        ValkyrieType::Tuple(items) => ValkyrieType::Tuple(
            items
                .iter()
                .map(|item| lower_trait_associated_type_references(item, associated_names, generic_names))
                .collect(),
        ),
        ValkyrieType::Union(items) => ValkyrieType::Union(
            items
                .iter()
                .map(|item| lower_trait_associated_type_references(item, associated_names, generic_names))
                .collect(),
        ),
        ValkyrieType::Intersection(items) => ValkyrieType::Intersection(
            items
                .iter()
                .map(|item| lower_trait_associated_type_references(item, associated_names, generic_names))
                .collect(),
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
        validate_ast_root(root)?;
        let _warning_scope = CompileWarningScope::enter();
        let _builtin_type_alias_scope = BuiltinTypeAliasScope::enter(root);
        let module_name = module_name_override.unwrap_or_else(|| {
            root.statements
                .iter()
                .find_map(|statement| match statement {
                    RootStatement::Namespace(NamespaceDeclaration { name, .. }) => Some(lower_name_path(name)),
                    _ => None,
                })
                .unwrap_or_else(default_module_name)
        });

        let imports = root
            .statements
            .iter()
            .filter_map(|statement| match statement {
                RootStatement::Using(using) => Some(lower_using(using)),
                _ => None,
            })
            .collect();

        let _module_type_alias_scope = ModuleTypeAliasScope::enter_empty();
        let mut type_aliases = Vec::new();
        for statement in &root.statements {
            if let RootStatement::TypeAlias(alias) = statement {
                let generics: Vec<Identifier> = alias.generic_parameters.iter().map(|parameter| parameter.name.name.clone()).collect();
                let params: Vec<String> = generics.iter().map(|name| name.as_str().to_string()).collect();
                let target = lower_type_expression(&alias.target);
                ModuleTypeAliasScope::register_alias(alias.name.name.as_str(), params, target.clone());
                type_aliases.push(HirTypeAlias {
                    name: alias.name.name.clone(),
                    generics,
                    target,
                    span: with_source(&alias.span, self.source_id),
                });
            }
        }

        let functions = root
            .statements
            .iter()
            .scan(NamePath::default(), |current_namespace, statement| {
                if let RootStatement::Namespace(NamespaceDeclaration { name, body: None, .. }) = statement {
                    *current_namespace = lower_name_path(name);
                }
                Some((current_namespace.clone(), statement))
            })
            .flat_map(|(namespace, statement)| match statement {
                RootStatement::Function(function) if function.kind == FunctionDeclKind::Micro => {
                    vec![self.lower_function(function, &namespace)]
                }
                RootStatement::Namespace(namespace) => namespace
                    .body
                    .as_ref()
                    .map(|body| {
                        let namespace_path = lower_name_path(&namespace.name);
                        body.statements
                            .iter()
                            .filter_map(|stmt| match stmt {
                                FunctionStatement::Function { function, .. } if function.kind == FunctionDeclKind::Micro => {
                                    Some(self.lower_function(function, &namespace_path))
                                }
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
                _ => Vec::new(),
            })
            .collect();

        let type_functions = root
            .statements
            .iter()
            .filter_map(|statement| match statement {
                RootStatement::Function(function) if matches!(function.kind, FunctionDeclKind::Mezzo | FunctionDeclKind::Macro) => {
                    Some(self.lower_type_function(function))
                }
                RootStatement::MacroAssign(macro_assign) => Some(self.lower_macro_assign(macro_assign)),
                _ => None,
            })
            .collect();

        let (structs, widgets, singletons) = root
            .statements
            .iter()
            .scan(Vec::<Identifier>::new(), |current_namespace, statement| {
                if let RootStatement::Namespace(NamespaceDeclaration { name, body: None, .. }) = statement {
                    *current_namespace = name.parts.iter().map(|p| Identifier::new(p.as_str())).collect();
                }
                Some((current_namespace.clone(), statement))
            })
            .fold((Vec::new(), Vec::new(), Vec::new()), |(mut structs, mut widgets, mut singletons), (namespace, statement)| {
                if let RootStatement::Class(class_decl) = statement {
                    match class_decl.kind {
                        ClassLikeKind::Widget => widgets.push(self.lower_widget(class_decl)),
                        ClassLikeKind::Singleton => singletons.push(self.lower_singleton(class_decl, &namespace)),
                        _ => structs.push(self.lower_class(class_decl, &namespace)),
                    }
                }
                (structs, widgets, singletons)
            });

        let traits = root
            .statements
            .iter()
            .filter_map(|statement| match statement {
                RootStatement::Trait(trait_decl) => Some(self.lower_trait(trait_decl)),
                _ => None,
            })
            .collect();

        let enums = root
            .statements
            .iter()
            .filter_map(|statement| match statement {
                RootStatement::Unite(unite_decl) => Some(self.lower_unite(unite_decl)),
                _ => None,
            })
            .collect();

        let flags = root
            .statements
            .iter()
            .filter_map(|statement| match statement {
                RootStatement::Flags(flags_decl) => Some(self.lower_flags(flags_decl)),
                _ => None,
            })
            .collect();

        let impls = root
            .statements
            .iter()
            .filter_map(|statement| match statement {
                RootStatement::Imply(imply_decl) => Some(self.lower_imply(imply_decl)),
                _ => None,
            })
            .collect();

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
        resolve_hir_calls(&mut hir);
        validate_extractor_patterns(&hir)?;
        let mut injector = crate::valkyrie::derive::DeriveInjector::new();
        let derive_result = injector.inject_derives(&mut hir);
        if derive_result.has_errors() {
            return Err(ParseError::invalid(derive_result.errors.into_iter().map(|error| error.to_string()).collect::<Vec<_>>().join("; ")));
        }
        hir.warnings = take_compile_warnings();
        Ok(hir)
    }

    fn lower_function(&self, function: &FunctionDeclaration, declaring_namespace: &NamePath) -> HirFunction {
        HirFunction {
            name: function.name.name.clone(),
            declaring_namespace: declaring_namespace.clone(),
            doc: lower_documentation(&function.annotations),
            annotations: function
                .annotations
                .attributes()
                .map(|attribute| lower_attribute(attribute, self.source_id, function.span.clone()))
                .collect(),
            generics: lower_generic_parameters(&function.generic_parameters),
            where_constraints: lower_ast_where_constraints(&function.where_constraints, self.source_id),
            params: function.params.iter().map(|param| lower_param(param, self.source_id, function.span.clone())).collect(),
            return_type: function.return_type.as_ref().map(lower_type_expression).unwrap_or(ValkyrieType::Unit),
            body: lower_block(function.body.as_ref(), self.source_id, function.span.clone()),
            span: with_source(&function.span, self.source_id),
            visibility: lower_visibility(&function.annotations),
            is_abstract: function.body.is_none() || has_modifier(&function.annotations, "abstract"),
            is_final: has_modifier(&function.annotations, "final"),
            is_virtual: false,
            is_override: false,
        }
    }

    fn lower_type_function(&self, function: &FunctionDeclaration) -> HirTypeFunction {
        HirTypeFunction {
            name: function.name.name.clone(),
            documents: lower_documentation(&function.annotations),
            generics: lower_generic_parameters(&function.generic_parameters),
            params: function.params.iter().map(|param| lower_param(param, self.source_id, function.span.clone())).collect(),
            return_type: function.return_type.as_ref().map(lower_type_expression).unwrap_or(ValkyrieType::Unit),
            body: lower_block(function.body.as_ref(), self.source_id, function.span.clone()),
        }
    }

    fn lower_macro_assign(&self, macro_assign: &MacroAssignDeclaration) -> HirTypeFunction {
        let expr = lower_term_expression(&macro_assign.value, self.source_id, macro_assign.span.clone());
        let body = HirBlock { statements: Vec::new(), expr: Some(Box::new(expr)), span: with_source(&macro_assign.span, self.source_id) };
        HirTypeFunction {
            name: macro_assign.name.name.clone(),
            documents: lower_documentation(&macro_assign.annotations),
            generics: lower_generic_parameters(&macro_assign.generic_parameters),
            params: Vec::new(),
            return_type: ValkyrieType::Unit,
            body,
        }
    }

    fn lower_class(&self, class_decl: &ClassDeclaration, namespace: &[Identifier]) -> HirStruct {
        HirStruct {
            name: class_decl.name.name.clone(),
            namespace: namespace.to_vec(),
            doc: lower_documentation(&class_decl.annotations),
            generics: lower_generic_parameters(&class_decl.generic_parameters),
            parents: class_decl.inheritance.iter().map(lower_parent).collect(),
            fields: class_decl.body.fields.iter().map(lower_field).collect(),
            methods: class_decl
                .body
                .methods
                .iter()
                .filter(|method| !is_property_accessor(method))
                .map(|method| self.lower_object_method(method))
                .collect(),
            properties: self.lower_object_properties(&class_decl.body.methods),
            visibility: lower_visibility(&class_decl.annotations),
            is_value_type: class_decl.is_value_type,
            is_abstract: has_modifier(&class_decl.annotations, "abstract"),
            is_sealed: has_modifier(&class_decl.annotations, "sealed"),
            is_final: has_modifier(&class_decl.annotations, "final"),
            is_open: has_modifier(&class_decl.annotations, "open"),
            abstract_methods: Vec::new(),
            abstract_properties: Vec::new(),
            derives: lower_derives(&class_decl.annotations),
        }
    }

    fn lower_widget(&self, class_decl: &ClassDeclaration) -> HirWidget {
        let methods: Vec<HirFunction> = class_decl
            .body
            .methods
            .iter()
            .filter(|method| !is_property_accessor(method))
            .map(|method| self.lower_object_method(method))
            .collect();
        let lifecycle = HirWidgetLifecycle {
            has_on_mount: methods.iter().any(|m| m.name.as_str() == "on_mount"),
            has_on_unmount: methods.iter().any(|m| m.name.as_str() == "on_unmount"),
            has_on_update: methods.iter().any(|m| m.name.as_str() == "on_update"),
            has_before_update: methods.iter().any(|m| m.name.as_str() == "before_update"),
            has_after_update: methods.iter().any(|m| m.name.as_str() == "after_update"),
        };
        HirWidget {
            name: class_decl.name.name.clone(),
            doc: lower_documentation(&class_decl.annotations),
            generics: lower_generic_parameters(&class_decl.generic_parameters),
            fields: class_decl.body.fields.iter().map(lower_field).collect(),
            methods,
            visibility: lower_visibility(&class_decl.annotations),
            state_fields: class_decl
                .body
                .fields
                .iter()
                .filter(|field| field.name.as_str().starts_with('_') || field.name.as_str().starts_with("state_"))
                .map(|field| field.name.name.clone())
                .collect(),
            initial_state: Vec::new(),
            lifecycle,
        }
    }

    fn lower_singleton(&self, class_decl: &ClassDeclaration, namespace: &[Identifier]) -> HirSingleton {
        let all_methods: Vec<HirFunction> = class_decl
            .body
            .methods
            .iter()
            .filter(|method| !is_property_accessor(method))
            .map(|method| self.lower_object_method(method))
            .collect();
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
            name: class_decl.name.name.clone(),
            namespace: namespace.to_vec(),
            doc: lower_documentation(&class_decl.annotations),
            generics: lower_generic_parameters(&class_decl.generic_parameters),
            parents: class_decl.inheritance.iter().map(lower_parent).collect(),
            fields: class_decl.body.fields.iter().map(lower_field).collect(),
            methods: ordinary_methods,
            properties: self.lower_object_properties(&class_decl.body.methods),
            visibility: lower_visibility(&class_decl.annotations),
            derives: lower_derives(&class_decl.annotations),
            is_lazy: has_modifier(&class_decl.annotations, "lazy"),
            instance_name: Identifier::new(crate::valkyrie::mir::SINGLETON_INSTANCE_FIELD),
            constructor,
            finalizer,
        }
    }

    fn lower_flags(&self, flags_decl: &FlagsDeclaration) -> HirFlags {
        HirFlags {
            name: flags_decl.name.name.clone(),
            doc: lower_documentation(&flags_decl.annotations),
            members: flags_decl.members.iter().map(|member| self.lower_flag_member(member)).collect(),
            visibility: lower_visibility(&flags_decl.annotations),
        }
    }

    fn lower_flag_member(&self, member: &FlagsMemberDeclaration) -> HirFlagMember {
        HirFlagMember {
            name: member.name.name.clone(),
            doc: lower_documentation(&member.annotations),
            value: member.value.as_ref().map(|expr| lower_term_expression(expr, self.source_id, member.span.clone())).unwrap_or_else(|| {
                HirExpr { kind: HirExprKind::Literal(HirLiteral::Integer64(0)), span: with_source(&member.span, self.source_id) }
            }),
            is_combined: false,
        }
    }

    fn lower_trait(&self, trait_decl: &TraitDeclaration) -> HirTrait {
        let associated_names = trait_decl
            .body
            .associated_types
            .iter()
            .map(|item| item.name.name.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let methods: Vec<HirFunction> = trait_decl
            .body
            .methods
            .iter()
            .filter(|method| !is_property_accessor(method) && method.body.is_none())
            .map(|method| lower_trait_method(self.lower_object_method(method), &associated_names))
            .collect();
        let default_methods: Vec<HirFunction> = trait_decl
            .body
            .methods
            .iter()
            .filter(|method| !is_property_accessor(method) && method.body.is_some())
            .map(|method| lower_trait_method(self.lower_object_method(method), &associated_names))
            .collect();

        HirTrait {
            name: trait_decl.name.name.clone(),
            doc: lower_documentation(&trait_decl.annotations),
            generics: Vec::new(),
            methods,
            associated_types: trait_decl.body.associated_types.iter().map(|item| lower_trait_associated_type(item, self.source_id)).collect(),
            associated_constants: trait_decl
                .body
                .associated_constants
                .iter()
                .map(|item| lower_trait_associated_const(item, self.source_id))
                .collect(),
            super_traits: if trait_decl.is_alias {
                trait_decl.alias_targets.iter().map(lower_named_type).collect()
            }
            else {
                trait_decl.inheritance.iter().map(lower_named_type).collect()
            },
            default_methods,
            visibility: lower_visibility(&trait_decl.annotations),
        }
    }

    fn lower_unite(&self, unite_decl: &UniteDeclaration) -> HirEnum {
        let mut enum_def = match unite_decl.kind {
            SumTypeKind::Unite => HirEnum::new_unity(unite_decl.name.name.clone()),
            SumTypeKind::Enum => HirEnum::new(unite_decl.name.name.clone()),
            SumTypeKind::Union => unreachable!("named union must be rejected before HirEnum lowering"),
        };
        enum_def.doc = lower_documentation(&unite_decl.annotations);
        enum_def.visibility = lower_visibility(&unite_decl.annotations);
        enum_def.generics = lower_generic_parameters(&unite_decl.generic_parameters);
        enum_def.variants = unite_decl.variants.iter().map(|variant| self.lower_unite_variant(variant, unite_decl.kind)).collect();
        enum_def.is_unity = unite_decl.kind == SumTypeKind::Unite;
        enum_def
    }

    fn lower_imply(&self, imply_decl: &ImplyDeclaration) -> HirImpl {
        HirImpl {
            generics: lower_imply_generics(imply_decl),
            where_constraints: lower_imply_where_constraints(imply_decl, self.source_id),
            target: lower_type_expression(&imply_decl.target_type),
            trait_path: imply_decl.trait_type.as_ref().map(lower_trait_path),
            methods: imply_decl.methods.iter().map(|method| self.lower_object_method(method)).collect(),
            associated_type_impls: imply_decl
                .associated_type_bindings
                .iter()
                .map(|binding| lower_imply_associated_type_binding(binding, self.source_id))
                .collect(),
            associated_const_impls: imply_decl
                .associated_const_bindings
                .iter()
                .map(|binding| lower_imply_associated_const_binding(binding, self.source_id))
                .collect(),
        }
    }

    fn lower_object_method(&self, method: &ObjectMethodDeclaration) -> HirFunction {
        HirFunction {
            name: method.name.name.clone(),
            declaring_namespace: NamePath::default(),
            doc: lower_documentation(&method.annotations),
            annotations: method
                .annotations
                .attributes()
                .map(|attribute| lower_attribute(attribute, self.source_id, method.span.clone()))
                .collect(),
            generics: lower_generic_parameters(&method.generic_parameters),
            where_constraints: lower_ast_where_constraints(&method.where_constraints, self.source_id),
            params: lower_method_params(method, self.source_id),
            return_type: method.return_type.as_ref().map(lower_type_expression).unwrap_or(ValkyrieType::Unit),
            body: lower_block(method.body.as_ref(), self.source_id, method.span.clone()),
            span: with_source(&method.span, self.source_id),
            visibility: lower_visibility(&method.annotations),
            is_abstract: method.body.is_none() || has_modifier(&method.annotations, "abstract"),
            is_final: has_modifier(&method.annotations, "final"),
            is_virtual: has_modifier(&method.annotations, "virtual"),
            is_override: has_modifier(&method.annotations, "override"),
        }
    }

    fn lower_object_properties(&self, methods: &[ObjectMethodDeclaration]) -> Vec<HirProperty> {
        let mut lowered = Vec::new();

        for method in methods.iter().filter(|method| is_property_accessor(method)) {
            let Some(accessor_kind) = property_accessor_kind(method)
            else {
                continue;
            };
            let accessor = self.lower_property_accessor(method, accessor_kind);
            let ty = lower_property_type(method, accessor_kind);

            if let Some(existing) = lowered.iter_mut().find(|item: &&mut HirProperty| item.name == method.name.name) {
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
                name: method.name.name.clone(),
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

    fn lower_property_accessor(&self, method: &ObjectMethodDeclaration, accessor_kind: PropertyMethodKind) -> HirFunction {
        let accessor_name = match accessor_kind {
            PropertyMethodKind::Get => method.name.name.clone(),
            PropertyMethodKind::Set => Identifier::new(&format!("set_{}", method.name.as_str())),
        };

        HirFunction {
            name: accessor_name,
            declaring_namespace: NamePath::default(),
            doc: lower_documentation(&method.annotations),
            annotations: method
                .annotations
                .attributes()
                .map(|attribute| lower_attribute(attribute, self.source_id, method.span.clone()))
                .collect(),
            generics: lower_generic_parameters(&method.generic_parameters),
            where_constraints: lower_ast_where_constraints(&method.where_constraints, self.source_id),
            params: lower_property_params(method, self.source_id),
            return_type: method.return_type.as_ref().map(lower_type_expression).unwrap_or(ValkyrieType::Unit),
            body: lower_block(method.body.as_ref(), self.source_id, method.span.clone()),
            span: with_source(&method.span, self.source_id),
            visibility: lower_visibility(&method.annotations),
            is_abstract: method.body.is_none() || has_modifier(&method.annotations, "abstract"),
            is_final: has_modifier(&method.annotations, "final"),
            is_virtual: has_modifier(&method.annotations, "virtual"),
            is_override: has_modifier(&method.annotations, "override"),
        }
    }

    fn lower_unite_variant(&self, variant: &UniteVariantDeclaration, kind: SumTypeKind) -> HirVariant {
        let discriminator =
            variant.value.as_ref().map(|value| lower_term_expression(value, self.source_id, variant.span.clone())).or_else(|| {
                // `[tag(N)]` is unite-only; enums use `Variant = N` (`value`) or auto-increment.
                if kind == SumTypeKind::Enum {
                    None
                }
                else {
                    tag_attribute_discriminator(&variant.annotations, self.source_id, variant.span.clone())
                }
            });
        HirVariant {
            name: variant.name.name.clone(),
            doc: lower_documentation(&variant.annotations),
            fields: variant.fields.iter().map(lower_field).collect(),
            result_type: variant.result_type.as_ref().map(lower_type_expression),
            discriminator,
        }
    }
}

/// Extract `[tag(N)]` / `[tag(N, default)]` into a discriminator literal for `unite` layouts.
fn tag_attribute_discriminator(
    annotations: &std_data::text::valkyrie::Annotations,
    source_id: SourceID,
    span: Range<usize>,
) -> Option<HirExpr> {
    for attribute in annotations.attributes() {
        if !attribute.name.parts.last().is_some_and(|name| name == "tag") {
            continue;
        }
        let first = attribute.arguments.first()?;
        return Some(lower_term_expression(&first.value, source_id, span));
    }
    None
}

fn lower_trait_associated_type(item: &TraitAssociatedTypeDeclaration, source_id: SourceID) -> HirAssociatedType {
    HirAssociatedType {
        name: item.name.name.clone(),
        doc: lower_documentation(&item.annotations),
        type_params: Vec::new(),
        bounds: item.bounds.iter().map(lower_type_expression).collect(),
        default: item.default_type.as_ref().map(lower_type_expression),
        span: with_source(&item.span, source_id),
    }
}

fn lower_trait_associated_const(item: &TraitAssociatedConstDeclaration, source_id: SourceID) -> HirAssociatedConst {
    HirAssociatedConst {
        name: item.name.name.clone(),
        doc: lower_documentation(&item.annotations),
        const_type: lower_type_expression(&item.const_type),
        default_value: item.default_value.as_ref().map(|value| lower_term_expression(value, source_id, item.span.clone())),
        span: with_source(&item.span, source_id),
    }
}

fn lower_imply_associated_type_binding(item: &ImplyAssociatedTypeBinding, source_id: SourceID) -> HirAssociatedTypeImpl {
    HirAssociatedTypeImpl {
        name: item.name.name.clone(),
        concrete_type: lower_type_expression(&item.concrete_type),
        type_args: Vec::new(),
        span: with_source(&item.span, source_id),
    }
}

fn lower_imply_associated_const_binding(item: &ImplyAssociatedConstBinding, source_id: SourceID) -> HirAssociatedConstImpl {
    HirAssociatedConstImpl {
        name: item.name.name.clone(),
        const_type: item.const_type.as_ref().map(lower_type_expression),
        value: lower_term_expression(&item.value, source_id, item.span.clone()),
        span: with_source(&item.span, source_id),
    }
}

fn lower_attribute(attribute: &AttributeItem, source_id: SourceID, fallback_span: Range<usize>) -> HirAttribute {
    let arguments = attribute
        .arguments
        .iter()
        .map(|argument| HirArgument {
            key: argument.key.as_deref().map(Identifier::new),
            value: Box::new(lower_attribute_argument_expression(&argument.value, source_id, fallback_span.clone())),
        })
        .collect();
    HirAttribute::with_arguments(lower_name_path(&attribute.name), arguments)
}

fn lower_attribute_argument_expression(expr: &TermExpression, source_id: SourceID, fallback_span: Range<usize>) -> HirExpr {
    match expr {
        TermExpression::Name { path, span } => HirExpr { kind: HirExprKind::Path(lower_name_path(path)), span: with_source(span, source_id) },
        _ => lower_term_expression(expr, source_id, fallback_span),
    }
}

fn lower_documentation(annotations: &std_data::text::valkyrie::Annotations) -> HirDocumentation {
    HirDocumentation::from_lines(annotations.documents.clone())
}

fn lower_visibility(annotations: &std_data::text::valkyrie::Annotations) -> HirVisibility {
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

fn has_modifier(annotations: &std_data::text::valkyrie::Annotations, name: &str) -> bool {
    annotations.modifiers.iter().any(|modifier| modifier.as_str() == name)
}

fn lower_derives(annotations: &std_data::text::valkyrie::Annotations) -> Vec<NamePath> {
    annotations
        .attributes()
        .find(|attribute| attribute.name.parts.last().is_some_and(|name| name == "derive"))
        .map(|attribute| attribute.arguments.iter().filter_map(|argument| extract_name_path(&argument.value)).collect())
        .unwrap_or_default()
}

pub(super) fn lower_parent(item: &InheritanceItem) -> HirParent {
    match &item.base_type {
        TypeExpression::Path(path) => HirParent::full(
            lower_name_path(&path.name),
            item.alias.as_deref().map(Identifier::new),
            path.arguments.iter().map(lower_type_expression).collect(),
        ),
        other => HirParent::full(
            NamePath::new(vec![Identifier::new(&render_type_expression(other))]),
            item.alias.as_deref().map(Identifier::new),
            Vec::new(),
        ),
    }
}

fn lower_field(field: &ObjectFieldDeclaration) -> HirField {
    HirField {
        name: field.name.name.clone(),
        doc: lower_documentation(&field.annotations),
        ty: lower_type_expression(&field.field_type),
        visibility: lower_visibility(&field.annotations),
        is_mutable: has_modifier(&field.annotations, "mut"),
    }
}

fn lower_named_type(item: &InheritanceItem) -> ValkyrieType {
    lower_type_expression(&item.base_type)
}

fn lower_trait_path(ty: &TypeExpression) -> NamePath {
    match ty {
        TypeExpression::Path(path) => lower_name_path(&path.name),
        other => NamePath::new(vec![Identifier::new(&render_type_expression(other))]),
    }
}

fn lower_generic_parameters(parameters: &[GenericParameterDeclaration]) -> Vec<GenericType> {
    parameters.iter().map(lower_generic_parameter).collect()
}

fn lower_imply_generics(imply_decl: &ImplyDeclaration) -> Vec<GenericType> {
    imply_decl.generic_parameters.iter().map(lower_generic_parameter).collect()
}

fn lower_generic_parameter(parameter: &GenericParameterDeclaration) -> GenericType {
    GenericType {
        name: parameter.name.name.clone(),
        kind: HirKind::Type,
        bounds: parameter.bounds.iter().map(lower_bound_identifier).collect(),
    }
}

fn lower_bound_identifier(bound: &TypeExpression) -> Identifier {
    Identifier::new(&render_type_expression(bound))
}

fn lower_imply_where_constraints(imply_decl: &ImplyDeclaration, source_id: SourceID) -> Vec<HirWhereConstraint> {
    lower_ast_where_constraints(&imply_decl.where_constraints, source_id)
}

fn lower_ast_where_constraints(
    constraints: &[std_data::text::valkyrie::WhereConstraintDeclaration],
    source_id: SourceID,
) -> Vec<HirWhereConstraint> {
    constraints.iter().map(|constraint| HirWhereConstraint {
        target: lower_type_expression(&constraint.target_type),
        bounds: constraint.bounds.iter().map(lower_trait_bound).collect(),
        span: with_source(&constraint.span, source_id),
    }).collect()
}

fn lower_trait_bound(bound: &TypeExpression) -> crate::valkyrie::types::hir::HirTraitBound {
    let mut lowered = crate::valkyrie::types::hir::HirTraitBound {
        trait_path: lower_trait_path(bound),
        type_arguments: Vec::new(),
        associated_types: Vec::new(),
    };
    if let TypeExpression::Path(path) = bound {
        for argument in &path.arguments {
            match argument {
                TypeExpression::Associated { name, ty, .. } => {
                    lowered.associated_types.push(crate::valkyrie::types::hir::HirAssociatedTypeBinding {
                        name: name.name.clone(),
                        ty: lower_type_expression(ty),
                    });
                }
                argument => lowered.type_arguments.push(lower_type_expression(argument)),
            }
        }
    }
    lowered
}

fn lower_param(param: &FunctionParameter, source_id: SourceID, fallback_span: Range<usize>) -> HirParam {
    let span = if param.span.is_empty() { fallback_span } else { param.span.clone() };
    HirParam {
        name: HirIdentifier { name: param.name.name.clone(), shadow_index: 0, span: with_source(&span, source_id) },
        ty: param.parameter_type.as_ref().map(lower_type_expression).unwrap_or(ValkyrieType::AutoType),
        binding_kind: match param.binding_kind {
            ParameterBindingKind::PositionalOnly => HirParameterBindingKind::PositionalOnly,
            ParameterBindingKind::PositionalOrKeyword => HirParameterBindingKind::PositionalOrKeyword,
            ParameterBindingKind::KeywordOnly => HirParameterBindingKind::KeywordOnly,
        },
        is_mutable: param.is_mutable,
        default: param.default_value.as_ref().map(|expr| expr_lowering::lower_term_expression(expr, source_id, span.clone())),
        variadic: match param.variadic {
            ParameterVariadicKind::None => HirVariadicKind::None,
            ParameterVariadicKind::PositionalRest => HirVariadicKind::PositionalRest,
            ParameterVariadicKind::KeywordRest => HirVariadicKind::KeywordRest,
        },
    }
}

fn lower_method_params(method: &ObjectMethodDeclaration, source_id: SourceID) -> Vec<HirParam> {
    method.params.iter().map(|param| lower_param(param, source_id, method.span.clone())).collect()
}

fn lower_property_params(method: &ObjectMethodDeclaration, source_id: SourceID) -> Vec<HirParam> {
    method.params.iter().map(|param| lower_param(param, source_id, method.span.clone())).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PropertyMethodKind {
    Get,
    Set,
}

fn property_accessor_kind(method: &ObjectMethodDeclaration) -> Option<PropertyMethodKind> {
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

fn is_property_accessor(method: &ObjectMethodDeclaration) -> bool {
    property_accessor_kind(method).is_some()
}

fn lower_property_type(method: &ObjectMethodDeclaration, accessor_kind: PropertyMethodKind) -> ValkyrieType {
    match accessor_kind {
        PropertyMethodKind::Get => method.return_type.as_ref().map(lower_type_expression).unwrap_or(ValkyrieType::Unit),
        PropertyMethodKind::Set => {
            method.params.last().and_then(|param| param.parameter_type.as_ref().map(lower_type_expression)).unwrap_or(ValkyrieType::Unit)
        }
    }
}

fn property_is_abstract(method: &ObjectMethodDeclaration) -> bool {
    method.body.is_none() || has_modifier(&method.annotations, "abstract")
}

fn property_is_final(method: &ObjectMethodDeclaration) -> bool {
    has_modifier(&method.annotations, "final")
}

fn property_is_static(method: &ObjectMethodDeclaration) -> bool {
    has_modifier(&method.annotations, "static")
}

fn property_is_virtual(method: &ObjectMethodDeclaration) -> bool {
    has_modifier(&method.annotations, "virtual")
}

fn property_is_override(method: &ObjectMethodDeclaration) -> bool {
    has_modifier(&method.annotations, "override")
}

fn property_is_lazy(method: &ObjectMethodDeclaration) -> bool {
    has_modifier(&method.annotations, "lazy")
}

fn lower_using(using: &UsingStatement) -> HirImport {
    HirImport {
        path: lower_name_path(&using.path),
        alias: using.alias.as_deref().map(Identifier::new),
        bindings: using
            .selective_imports
            .iter()
            .map(|item| HirImportBinding { name: Identifier::new(&item.name), alias: item.alias.as_deref().map(Identifier::new) })
            .collect(),
        glob: using.glob_import,
    }
}

fn lower_name_path(path: &AstNamePath) -> NamePath {
    NamePath::new(path.parts.iter().map(|part| Identifier::new(part)).collect())
}

fn default_module_name() -> NamePath {
    NamePath::new(vec![Identifier::new("main")])
}

fn with_source(span: &Range<usize>, source_id: SourceID) -> SourceSpan {
    SourceSpan::new(source_id, span.start as u32, span.end as u32)
}

#[cfg(test)]
mod sum_discriminator_tests {
    use super::{compute_nominal_layouts, validate_enum_discriminators, *};
    use crate::{
        SourceID, ValkyrieCompiler,
        types::{Identifier, NamePath, SourceSpan},
        valkyrie::types::hir::{HirDependencySemanticExport, HirEnum, HirExpr, HirExprKind, HirLiteral, HirVariant},
    };

    fn test_span() -> SourceSpan {
        SourceSpan::new(SourceID::default(), 0, 0)
    }

    #[test]
    fn sum_layouts_are_not_synthesized_without_source_declarations() {
        let output = ValkyrieCompiler::default()
            .compile_source_to_build_output("[main] micro entry() -> i32 { return 23 }")
            .expect("普通源码必须完成正式 Compiler 成功边界");
        assert!(output.semantic_mir().sum_types.is_empty());
        assert!(output.compiled_program().canonical().linked.variants.is_empty());
    }

    #[test]
    fn familiar_sum_names_preserve_the_declared_variants() {
        let module = ValkyrieCompiler::default()
            .compile_source("enums Option { Declared = 7 } enums Result { Actual = 11 }")
            .expect("sum 声明必须来自当前源码");
        let (sums, _) = compute_nominal_layouts(&module);
        assert_eq!(sums.len(), 2);
        for (name, variant, tag) in [("Option", "Declared", 7), ("Result", "Actual", 11)] {
            let sum = sums.iter().find(|sum| sum.name == name).expect("声明 owner");
            assert_eq!(sum.variants.len(), 1);
            assert_eq!(sum.variants[0].name, variant);
            assert_eq!(sum.variants[0].tag, tag);
            assert!(sum.variants[0].payload_type.is_none());
        }
    }

    #[test]
    fn unresolved_call_contract_is_rejected_before_mir() {
        let expression = HirExpr {
            kind: HirExprKind::Call {
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
        let actual = compiler.compile_source("structure Holder { items: [i32] }")
            .expect_err("frontend must propagate copy error");
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
        let HirStatementKind::Expr(statement) = &function.body.statements[0].kind else { panic!("expected return statement") };
        let HirExprKind::Return(Some(expression)) = &statement.kind else { panic!("expected return value") };
        let HirExprKind::Call { resolved: Some(contract), .. } = &expression.kind else { panic!("expected resolved callback") };
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
    fn named_union_does_not_lower_as_numeric_enums() {
        let compiler = ValkyrieCompiler::new(SourceID::default());
        let error = compiler
            .compile_source("union Limb { Small { value: i64 }, Words { value: i64 } }")
            .expect_err("named union must be rejected");
        assert!(error.to_string().contains("cannot be lowered as numeric enums"), "{error}");
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
        let (sum_types, _) = compute_nominal_layouts(&module);
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
        let (sum_types, _) = compute_nominal_layouts(&module);
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
        let compiler = ValkyrieCompiler::new(SourceID::default());
        let dependency = compiler
            .compile_source(
                r#"
enums Status {
    Active = 2
    Inactive
}
"#,
            )
            .expect("dependency enums");
        let consumer = compiler
            .compile_source_with_semantic_exports(
                "micro main() { return }",
                &[HirDependencySemanticExport {
                    module: NamePath::new(vec![Identifier::new("dep")]),
                    functions: Vec::new(),
                    structs: Vec::new(),
                    enums: dependency.enums,
                    traits: Vec::new(),
                    type_aliases: Vec::new(),
                    impls: Vec::new(),
                }],
            )
            .expect("consumer with imported enums");
        let (sum_types, _) = compute_nominal_layouts(&consumer);
        let status = sum_types.iter().find(|layout| layout.name == "Status").expect("Status layout");
        assert_eq!(status.variants[0].tag, 2);
        assert_eq!(status.variants[1].tag, 3);
    }

    #[test]
    fn rejects_duplicate_discriminators_in_imported_semantic_export_enums() {
        let compiler = ValkyrieCompiler::new(SourceID::default());
        let duplicate_tag = |value: i64| {
            HirExpr { kind: HirExprKind::Literal(HirLiteral::Integer64(value)), span: test_span() }
        };
        let bad_enum = HirEnum {
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
                },
                HirVariant {
                    name: Identifier::new("Paused"),
                    doc: Default::default(),
                    fields: Vec::new(),
                    result_type: None,
                    discriminator: Some(duplicate_tag(0)),
                },
            ],
            visibility: Default::default(),
            is_unity: false,
        };
        let error = compiler
            .compile_source_with_semantic_exports(
                "micro main() { return }",
                &[HirDependencySemanticExport {
                    module: NamePath::new(vec![Identifier::new("dep")]),
                    functions: Vec::new(),
                    structs: Vec::new(),
                    enums: vec![bad_enum],
                    traits: Vec::new(),
                    type_aliases: Vec::new(),
                    impls: Vec::new(),
                }],
            )
            .expect_err("imported duplicate discriminator");
        assert!(error.to_string().contains("duplicate discriminator"), "{error}");
    }

    #[test]
    fn imported_semantic_export_unite_contributes_sum_layout_tags() {
        let compiler = ValkyrieCompiler::new(SourceID::default());
        let dependency = compiler
            .compile_source(
                r#"
unite Choice {
    [tag(2)]
    A { x: i64 }
    B { y: i64 }
}
"#,
            )
            .expect("dependency unite");
        let consumer = compiler
            .compile_source_with_semantic_exports(
                "micro main() { return }",
                &[HirDependencySemanticExport {
                    module: NamePath::new(vec![Identifier::new("dep")]),
                    functions: Vec::new(),
                    structs: Vec::new(),
                    enums: dependency.enums,
                    traits: Vec::new(),
                    type_aliases: Vec::new(),
                    impls: Vec::new(),
                }],
            )
            .expect("consumer with imported unite");
        let (sum_types, _) = compute_nominal_layouts(&consumer);
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
