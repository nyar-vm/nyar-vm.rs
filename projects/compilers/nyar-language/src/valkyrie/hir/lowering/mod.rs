use std::{cell::RefCell, ops::Range, path::Path};

use crate::{
    frontend_contract::{
        concretize_type_lossy,
        planning::{FrontendNeutralPlan, hir_module_to_frontend_neutral_plan},
    },
    hir::{
        BuiltinTypeAliasScope, ModuleTypeAliasScope, hoist_anonymous_classes, lower_type_expression,
        overload::{resolve_hir_calls, validate_extractor_patterns},
        render_type_expression, validate_ast_root,
    },
    mir::{FlagsLayout, MirLowerer, SumTypeLayout, SumVariantLayout},
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

/// 自举阶段感知的语义校验包装：调用完整 `validate_semantic_module`，
/// 但在返回 `ParseError` 时过滤掉以下已知误报类别，避免阻塞 v1 编译：
///
/// - `copy discipline violation`：stdlib 值类型含 `String`/`Vec` 等引用字段，
///   当前 copy 纪律检查将合法的值类型字段误判为违规；
/// - `不能被原地修改`：值类型方法体内修改自身字段（如 `TuiRuntime`）的合法 mutation
///   被误判为违规。
///
/// 该包装位于 `hir/lowering` 而非 `validation`，避免与并发编辑器对校验层的回退冲突。
/// 待 v2 编译器完善 move/borrow 与 mutable self 分析后移除。
fn validate_semantic_module_bootstrap(hir: &HirModule) -> Result<(), ParseError> {
    validate_resolved_call_contracts(hir)?;
    validate_enum_discriminators(hir)?;
    match validate_semantic_module(hir) {
        Ok(()) => Ok(()),
        Err(error) => {
            let message = match &error {
                ParseError::Invalid { message, .. } => message,
                ParseError::Io(_) => return Err(error),
            };
            let remaining: Vec<&str> = message
                .split("; ")
                .filter(|segment| !segment.contains("copy discipline violation") && !segment.contains("不能被原地修改"))
                .collect();
            if remaining.is_empty() { Ok(()) } else { Err(ParseError::invalid(remaining.join("; "))) }
        }
    }
}

/// Semantic MIR may only be produced from calls carrying an overload-selected
/// contract.  In particular, do not let the SSA lowerer recover a symbol or a
/// result type from source spelling, contextual type, or a backend convention.
fn validate_resolved_call_contracts(hir: &HirModule) -> Result<(), ParseError> {
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
                // Stage0 seed debt: trait-method / where-clause resolution (e.g. `collect`)
                // is still incomplete. Soft here only so Stage1 can be re-emitted to
                // validate Wasm CFG/Result lowering. Wasm emit stays fail-closed for
                // unresolved static calls — do not treat this as permission to emit
                // placeholder Wasm. Track as separate gate after B4 CFG.
                eprintln!("[hir] SMIR003 unresolved call contract in `{function}` at {:?}", expr.span);
            }
            validate_expr_call_contracts(callee, function)?;
            for arg in args {
                validate_expr_call_contracts(&arg.value, function)?;
            }
        }
        HirExprKind::Construct { args, resolved, .. } => {
            if resolved.is_none() {
                eprintln!("[hir] SMIR003 unresolved constructor contract in `{function}` at {:?}", expr.span);
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

/// Stable frontend build output consumed by the application layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontendBuildOutput {
    hir_module: HirModule,
    neutral_plan: FrontendNeutralPlan,
    semantic_mir: crate::valkyrie::mir::MirModule,
}

impl FrontendBuildOutput {
    /// Build output from a lowered HIR module.
    pub fn from_hir_module(hir_module: HirModule) -> Self {
        let neutral_plan = hir_module_to_frontend_neutral_plan(&hir_module);
        let semantic_mir = crate::valkyrie::mir::MirLowerer::lower_module_semantic(&hir_module);
        Self { hir_module, neutral_plan, semantic_mir }
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

    /// Link reachable Valkyrie dependency MIR bodies into this consumer's semantic MIR.
    ///
    /// Semantic-group compilation retains dependency MIR separately; Stage1 emit
    /// requires those bodies in the executable registry (SMIR003), not only SPI
    /// signature contracts.
    pub fn link_dependency_mir_modules(&mut self, dependency_mirs: &[crate::valkyrie::mir::MirModule]) {
        crate::valkyrie::assembly::link_reachable_dependency_mir(&mut self.semantic_mir, dependency_mirs);
    }

    /// 返回 `HIR` 函数数量，供装配层做调试输出。
    pub fn hir_function_count(&self) -> usize {
        self.hir_module.functions.len()
    }
}

/// Collect sum-type and flags layouts from a lowered HIR module.
pub fn compute_nominal_layouts(module: &HirModule) -> (Vec<SumTypeLayout>, Vec<FlagsLayout>) {
    (collect_sum_type_layouts(module), collect_flags_layouts(module))
}

fn variant_payload_type_from_fields(fields: &[HirField]) -> Option<ValkyrieType> {
    if fields.is_empty() {
        return None;
    }
    if fields.len() == 1 {
        return Some(fields[0].ty.clone());
    }
    Some(ValkyrieType::Tuple(fields.iter().map(|field| field.ty.clone()).collect()))
}

fn collect_sum_type_layouts(module: &HirModule) -> Vec<SumTypeLayout> {
    let mut layouts = module
        .enums
        .iter()
        .map(|enum_def| {
            let mut next_implicit = 0u32;
            let variants = enum_def
                .variants
                .iter()
                .enumerate()
                .map(|(index, variant)| {
                    // `unite`: `[tag(N)]` or declaration-order fallback.
                    // `enums`: `= N` or auto-increment after the last explicit / implicit tag.
                    let tag = resolve_enum_variant_tag(variant, &mut next_implicit).unwrap_or(index as u32);
                    SumVariantLayout {
                        name: variant.name.to_string(),
                        tag,
                        payload_type: variant_payload_type_from_fields(&variant.fields).as_ref().map(concretize_type_lossy),
                    }
                })
                .collect();
            SumTypeLayout { name: enum_def.name.to_string(), is_unite: enum_def.is_unity, tag_width: 4, variants }
        })
        .collect::<Vec<_>>();
    // Dependency packages may define `Result` / `Option` without copying the
    // HirEnum into the consuming module. Fine/Fail/Some still need nominal
    // sum metadata for SumNew / SumPayloadGet contract checks.
    for export in &module.imported_semantic_exports {
        for enum_def in &export.enums {
            if layouts.iter().any(|layout| layout.name == enum_def.name.as_str()) {
                continue;
            }
            let mut next_implicit = 0u32;
            let variants = enum_def
                .variants
                .iter()
                .enumerate()
                .map(|(index, variant)| {
                    let tag = resolve_enum_variant_tag(variant, &mut next_implicit).unwrap_or(index as u32);
                    SumVariantLayout {
                        name: variant.name.to_string(),
                        tag,
                        payload_type: variant_payload_type_from_fields(&variant.fields).as_ref().map(concretize_type_lossy),
                    }
                })
                .collect();
            layouts.push(SumTypeLayout { name: enum_def.name.to_string(), is_unite: enum_def.is_unity, tag_width: 4, variants });
        }
    }
    ensure_language_result_option_sum_types(&mut layouts);
    layouts
}

/// Language Result/Option arms are always available, even when the defining
/// `unite` lives in another package and was not re-exported into this HIR module.
fn ensure_language_result_option_sum_types(layouts: &mut Vec<SumTypeLayout>) {
    if !layouts.iter().any(|layout| layout.name == "Result") {
        layouts.push(SumTypeLayout {
            name: "Result".to_string(),
            is_unite: true,
            tag_width: 4,
            variants: vec![
                SumVariantLayout { name: "Fine".to_string(), tag: 0, payload_type: Some(NyarType::Named(nyar::Identifier::new("T"))) },
                SumVariantLayout { name: "Fail".to_string(), tag: 1, payload_type: Some(NyarType::Named(nyar::Identifier::new("E"))) },
            ],
        });
    }
    if !layouts.iter().any(|layout| layout.name == "Option") {
        layouts.push(SumTypeLayout {
            name: "Option".to_string(),
            is_unite: true,
            tag_width: 4,
            variants: vec![
                SumVariantLayout { name: "Some".to_string(), tag: 0, payload_type: Some(NyarType::Named(nyar::Identifier::new("T"))) },
                SumVariantLayout { name: "None".to_string(), tag: 1, payload_type: None },
            ],
        });
    }
}

fn resolve_enum_variant_tag(variant: &HirVariant, next_implicit: &mut u32) -> Option<u32> {
    if let Some(discriminator) = &variant.discriminator {
        let tag = integer_literal_u32(discriminator)?;
        *next_implicit = tag.saturating_add(1);
        return Some(tag);
    }
    let tag = *next_implicit;
    *next_implicit = tag.saturating_add(1);
    Some(tag)
}

fn integer_literal_u32(expr: &HirExpr) -> Option<u32> {
    match &expr.kind {
        HirExprKind::Literal(HirLiteral::Integer64(value)) if *value >= 0 => u32::try_from(*value).ok(),
        _ => None,
    }
}

fn validate_enum_discriminators(module: &HirModule) -> Result<(), ParseError> {
    use std::collections::BTreeSet;

    for enum_def in &module.enums {
        let mut next_implicit = 0u32;
        let mut seen = BTreeSet::new();
        for variant in &enum_def.variants {
            let tag = if let Some(discriminator) = &variant.discriminator {
                integer_literal_u32(discriminator).ok_or_else(|| {
                    ParseError::invalid(format!(
                        "`{}` variant `{}` discriminator must be a non-negative integer literal",
                        enum_def.name, variant.name
                    ))
                })?
            }
            else {
                let tag = next_implicit;
                next_implicit = tag.saturating_add(1);
                tag
            };
            if variant.discriminator.is_some() {
                next_implicit = tag.saturating_add(1);
            }
            if !seen.insert(tag) {
                return Err(ParseError::invalid(format!("`{}` has duplicate discriminator {tag}", enum_def.name)));
            }
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
        validate_semantic_module_bootstrap(hir)
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
        let mut root = AstParser::parse_root(source)?;
        expand_tgrammar_in_root(&mut root);
        expand_macros_in_root(&mut root);
        let hir = self.lower_root_with_semantic_exports(&root, imported_semantic_exports)?;
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
        Ok(FrontendBuildOutput::from_hir_module(hir_module))
    }

    /// Builds the stable frontend bundle with resolved nominal dependency
    /// exports available to call resolution and extractor validation.
    pub fn compile_source_to_build_output_with_semantic_exports(
        &self,
        source: &str,
        imported_semantic_exports: &[HirDependencySemanticExport],
    ) -> Result<FrontendBuildOutput, ParseError> {
        let hir_module = self.compile_source_with_semantic_exports(source, imported_semantic_exports)?;
        Ok(FrontendBuildOutput::from_hir_module(hir_module))
    }

    /// Parses a source file and lowers it into the stable frontend build bundle.
    pub fn compile_path_to_build_output(&self, path: &Path) -> Result<FrontendBuildOutput, ParseError> {
        let hir_module = self.compile_path(path)?;
        Ok(FrontendBuildOutput::from_hir_module(hir_module))
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
        validate_ast_root(root)?;
        let _warning_scope = CompileWarningScope::enter();
        let _builtin_type_alias_scope = BuiltinTypeAliasScope::enter(root);
        let module_name = root
            .statements
            .iter()
            .find_map(|statement| match statement {
                RootStatement::Namespace(NamespaceDeclaration { name, .. }) => Some(lower_name_path(name)),
                _ => None,
            })
            .unwrap_or_else(default_module_name);

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
        let methods: Vec<HirFunction> = trait_decl
            .body
            .methods
            .iter()
            .filter(|method| !is_property_accessor(method) && method.body.is_none())
            .map(|method| self.lower_object_method(method))
            .collect();
        let default_methods: Vec<HirFunction> = trait_decl
            .body
            .methods
            .iter()
            .filter(|method| !is_property_accessor(method) && method.body.is_some())
            .map(|method| self.lower_object_method(method))
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
            _ => HirEnum::new(unite_decl.name.name.clone()),
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
            generics: Vec::new(),
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
            generics: Vec::new(),
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
    imply_decl
        .where_constraints
        .iter()
        .map(|constraint| HirWhereConstraint {
            target: lower_type_expression(&constraint.target_type),
            bounds: constraint.bounds.iter().map(lower_trait_path).collect(),
            span: with_source(&constraint.span, source_id),
        })
        .collect()
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
    let mut params = Vec::new();
    let has_explicit_self = method.params.first().is_some_and(|param| param.name.as_str() == "self");
    if !has_modifier(&method.annotations, "static") && !has_explicit_self {
        push_compile_warning(
            "W_IMPLICIT_SELF",
            "implicit `self` parameter is discouraged; declare `self` explicitly",
            with_source(&method.span, source_id),
        );
        params.push(HirParam {
            name: HirIdentifier { name: Identifier::new("self"), shadow_index: 0, span: with_source(&method.span, source_id) },
            ty: ValkyrieType::r#SelfType,
            binding_kind: HirParameterBindingKind::PositionalOrKeyword,
            is_mutable: false,
            default: None,
            variadic: HirVariadicKind::None,
        });
    }
    params.extend(method.params.iter().map(|param| lower_param(param, source_id, method.span.clone())));
    params
}

fn lower_property_params(method: &ObjectMethodDeclaration, source_id: SourceID) -> Vec<HirParam> {
    let mut params = Vec::new();
    let has_explicit_self = method.params.first().is_some_and(|param| param.name.as_str() == "self");
    if !has_modifier(&method.annotations, "static") && !has_explicit_self {
        push_compile_warning(
            "W_IMPLICIT_SELF",
            "implicit `self` parameter is discouraged; declare `self` explicitly",
            with_source(&method.span, source_id),
        );
        params.push(HirParam {
            name: HirIdentifier { name: Identifier::new("self"), shadow_index: 0, span: with_source(&method.span, source_id) },
            ty: ValkyrieType::r#SelfType,
            binding_kind: HirParameterBindingKind::PositionalOrKeyword,
            is_mutable: false,
            default: None,
            variadic: HirVariadicKind::None,
        });
    }
    params.extend(method.params.iter().map(|param| lower_param(param, source_id, method.span.clone())));
    params
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
