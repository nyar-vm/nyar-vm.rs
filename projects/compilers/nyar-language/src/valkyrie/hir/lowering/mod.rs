use std::{cell::RefCell, ops::Range, path::Path};

use crate::{
    hir::{
        ModuleTypeAliasScope, VisibleTypeAlias, hoist_anonymous_classes, lower_type_expression,
        overload::{resolve_hir_calls, validate_extractor_patterns},
        render_type_expression, validate_ast_root,
    },
    mir::{FlagsLayout, MirLowerer, MirSumDeclaration, MirSumVariant},
    types::{
        Identifier, NamePath, SourceID, SourceSpan,
        hir::{
            GenericBinderId, GenericType, HirArgument, HirAssociatedConst, HirAssociatedConstImpl, HirAssociatedType, HirAssociatedTypeBinding,
            HirAssociatedTypeImpl, HirAttribute, HirBlock, HirCallArgument, HirCompileWarning, HirDependencySemanticExport, HirDocumentation,
            HirEnum, HirExpr, HirExprKind, HirField, HirFlagMember, HirFlags, HirFunction, HirIdentifier, HirImpl, HirImport, HirImportBinding,
            HirKind, HirLiteral, HirMatchArm, HirModule, HirParam, HirParameterBindingKind, HirParent, HirPattern, HirProperty, HirSingleton,
            HirStatement, HirStatementKind, HirStruct, HirTrait, HirTraitBound, HirTypeAlias, HirTypeFunction, HirVariadicKind, HirVariant,
            HirVisibility, HirWhereConstraint, HirWidget, HirWidgetLifecycle, ValkyrieType,
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
                StatementNode, StructureDeclaration, TermExpression, Trait, TypeAliasDeclaration, TypeExpression, TypeFunction,
                UnionDeclaration, UsingDeclaration, Variant, WidgetDeclaration,
            },
        },
        mir::{SINGLETON_CONSTRUCTOR_NAME, SINGLETON_FINALIZER_NAME},
        operator_members,
        semantic_closure::{GroupSemanticClosure, retain_demanded_declarations, source_group_closure_with_host_bindings},
    },
};
mod trait_binding;
pub mod errors;
pub use errors::HirError;
use nyar_types::{DiagnosticSource, DiagnosticSpan, NyarType};
use ordered_float::OrderedFloat;

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

/// 前端统一验证调用身份、枚举判别与完整语义合同，不按错误文本放行。
fn validate_hir_contract(hir: &HirModule) -> Result<(), HirError> {
    validate_resolved_call_contracts(hir)?;
    validate_enum_discriminators(hir)?;
    validate_semantic_module(hir)
}

/// Semantic MIR may only be produced from calls carrying an overload-selected
/// contract.  In particular, do not let the SSA lowerer recover a symbol or a
/// result type from source spelling, contextual type, or a backend convention.
fn validate_resolved_call_contracts(hir: &HirModule) -> Result<(), HirError> {
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

fn validate_function_call_contracts(function: &HirFunction) -> Result<(), HirError> {
    validate_block_call_contracts(&function.body, &function.name.to_string())
}

fn validate_block_call_contracts(block: &HirBlock, function: &str) -> Result<(), HirError> {
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

#[doc(hidden)]
pub fn validate_expr_call_contracts(expr: &HirExpr, function: &str) -> Result<(), HirError> {
    let unknown_source = DiagnosticSource { id: "unknown".into(), revision: None };
    let unknown_span = DiagnosticSpan { start: 0, end: 0 };

    match &expr.kind {
        HirExprKind::Call { callee, args, resolved, .. } => {
            if resolved.is_none() {
                let callee_name = match &callee.kind {
                    HirExprKind::Path(path) => path.to_string(),
                    _ => "<computed>".to_string(),
                };
                return Err(HirError::UnresolvedCall {
                    selector: callee_name,
                    context: function.to_string(),
                    source: unknown_source.clone(),
                    span: unknown_span.clone(),
                }.into());
            }
            validate_expr_call_contracts(callee, function)?;
            for arg in args {
                validate_expr_call_contracts(&arg.value, function)?;
            }
        }
        HirExprKind::Construct { path, args, resolved } => {
            if resolved.is_none() {
                return Err(HirError::UnresolvedConstructor {
                    name: path.to_string(),
                    context: function.to_string(),
                    source: unknown_source.clone(),
                    span: unknown_span.clone(),
                }.into());
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
mod match_pattern_recovery;
mod source_context;
mod macro_expand;
mod template_const;
pub(crate) mod template_expand;
mod vx;

pub use super::CaptureAnalyzer;
use expr_lowering::{extract_name_path, lower_block};
use macro_expand::expand_macros_in_root;
#[doc(hidden)]
pub use template_expand::expand_templates_in_root;

#[doc(hidden)]
pub use expr_lowering::lower_term_expression;
#[doc(hidden)]
pub use template_const::{eval_const_condition, parse_const_loop_range, parse_const_range_text};
#[doc(hidden)]
pub use template_expand::DEFAULT_COMPILE_ARCH;
#[doc(hidden)]
pub use vx::enhance_vx_widgets;

/// Minimal compiler facade that lowers parser output into HIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValkyrieCompiler {
    /// Source id attached to synthesized spans during lowering.
    pub source_id: SourceID,
}

/// Resolver 交给 Compiler 的有序源码语义组。
// ---------------------------------------------------------------------------
// Compiler 输入数据结构（生产权在 Legion/Resolver，定义在 nyar_types::compiler_input）
// 本模块 re-export 数据结构，并补充只属于 nyar-language 的验证自由函数。
// ---------------------------------------------------------------------------

pub use nyar_types::compiler_input::{CompilerSourceFile, CompilerSourceGroup, CompilerSourceInput, GeneratedSourceGroup};

/// 建立生成源码组；producer 与入口身份都由生成器显式提供，不接受缺省推断。
pub fn build_generated_group(producer: impl Into<String>, group: CompilerSourceGroup) -> Result<GeneratedSourceGroup, HirError> {
    let producer = producer.into();
    if producer.trim().is_empty() {
        return Err(HirError::invalid("生成源码组缺少 producer 身份"));
    }
    if !group.is_entry {
        return Err(HirError::invalid(format!("生成源码组 `{}` 必须显式标记入口", group.dependency_key)));
    }
    Ok(GeneratedSourceGroup { producer, group })
}

/// 含生成源码的 Compiler 输入；构造时即验证 producer、入口身份、组身份与依赖顺序。
pub fn build_source_input_with_generated(
    groups: Vec<CompilerSourceGroup>,
    generated: Vec<GeneratedSourceGroup>,
) -> Result<CompilerSourceInput, HirError> {
    let input = CompilerSourceInput { groups, generated };
    compose_source_input_groups(&input)?;
    Ok(input)
}

/// 把生成组追加到应用组之后，得到唯一有序源码闭包。
///
/// 生成组必须显式入口、身份唯一，且只能依赖此前已提供的组；其余拓扑与
/// 文件身份校验由 Compiler 的源码快照校验统一承担。
pub fn compose_source_input_groups(input: &CompilerSourceInput) -> Result<Vec<CompilerSourceGroup>, HirError> {
    let mut composed = input.groups.clone();
    let mut provided = input.groups.iter().map(|group| group.dependency_key.clone()).collect::<std::collections::BTreeSet<_>>();
    for generated in &input.generated {
        if generated.producer.trim().is_empty() {
            return Err(HirError::invalid("生成源码组缺少 producer 身份"));
        }
        if !generated.group.is_entry {
            return Err(HirError::invalid(format!("生成源码组 `{}` 必须显式标记入口", generated.group.dependency_key)));
        }
        if generated.group.dependency_key.is_empty() || !provided.insert(generated.group.dependency_key.clone()) {
            return Err(HirError::invalid(format!("生成源码组身份 `{}` 为空或与已提供组冲突", generated.group.dependency_key)));
        }
        for dependency in &generated.group.direct_dependencies {
            if dependency == &generated.group.dependency_key || !provided.contains(dependency) {
                return Err(HirError::invalid(format!(
                    "生成源码组 `{}` 的依赖 `{dependency}` 未在此之前提供",
                    generated.group.dependency_key
                )));
            }
        }
        composed.push(generated.group.clone());
    }
    Ok(composed)
}
/// 同名时本组声明遮蔽依赖别名。不可见组的别名不在返回值里，因此既不会被展开，
/// 也不能遮蔽同名内建类型；本组声明此时只有名字可见（`target: None`）。
fn visible_type_aliases(roots: &[&ValkyrieRoot], imported_semantic_exports: &[HirDependencySemanticExport]) -> Vec<VisibleTypeAlias> {
    let mut visible = Vec::new();
    for export in imported_semantic_exports {
        for alias in &export.type_aliases {
            visible.push(VisibleTypeAlias {
                name: alias.name.to_string(),
                params: alias.generics.iter().map(ToString::to_string).collect(),
                target: Some(alias.target.clone()),
            });
        }
    }
    for root in roots {
        for alias in root_type_alias_declarations(root) {
            visible.push(VisibleTypeAlias { name: alias.name.name.clone(), params: Vec::new(), target: None });
        }
    }
    visible
}

/// 把本组（或本根）`type` 声明的目标填入已建立的作用域。
///
/// 全部可见名字在填目标之前已经注册，因此别名之间可以互相引用（含前向引用）；
/// 展开在引用点进行，环由 `expand_module_type_alias` 的 visiting 集合拒绝。
fn register_root_type_alias_targets(roots: &[&ValkyrieRoot]) {
    for root in roots {
        for alias in root_type_alias_declarations(root) {
            ModuleTypeAliasScope::register_alias(&alias.name.name, Vec::new(), lower_type_expression(&alias.target));
        }
    }
}

/// 收集一个源码根内全部 `type` 别名声明（含 namespace body 与包装语句）。
fn root_type_alias_declarations(root: &ValkyrieRoot) -> Vec<&TypeAliasDeclaration> {
    fn walk<'a>(items: &'a [StatementNode], out: &mut Vec<&'a TypeAliasDeclaration>) {
        for item in items {
            match item {
                StatementNode::TypeAlias(alias) => out.push(alias),
                StatementNode::Namespace(namespace) => {
                    if let Some(body) = &namespace.body {
                        walk(&body.items, out);
                    }
                }
                StatementNode::Statement(inner) => walk(std::slice::from_ref(inner.as_ref()), out),
                _ => {}
            }
        }
    }
    let mut declarations = Vec::new();
    walk(&root.items, &mut declarations);
    declarations
}

/// 在任何解析/裁剪前验证不可变输入身份，避免 map 覆盖制造第二份事实。
fn validate_source_groups(groups: &[CompilerSourceGroup]) -> Result<(), HirError> {
    let mut completed = std::collections::BTreeSet::new();
    if groups.is_empty() {
        return Err(HirError::invalid("Compiler 源码快照缺少入口组"));
    }
    if !groups.iter().any(|group| group.is_entry) {
        return Err(HirError::invalid("Compiler 源码快照缺少显式入口组"));
    }
    for group in groups {
        if group.dependency_key.is_empty() || completed.contains(&group.dependency_key) {
            return Err(HirError::invalid("Compiler 源码组身份为空或重复"));
        }
        for dependency in &group.direct_dependencies {
            if !completed.contains(dependency) {
                return Err(HirError::invalid(format!("依赖 `{dependency}` 未在 `{}` 之前提供", group.dependency_key)));
            }
        }
        let mut paths = std::collections::BTreeSet::new();
        for file in &group.sources {
            if file.path.is_empty() || !paths.insert(&file.path) {
                return Err(HirError::invalid(format!("源码组 `{}` 的文件身份为空或重复", group.dependency_key)));
            }
        }
        if group.is_entry && group.sources.is_empty() {
            return Err(HirError::invalid(format!("Compiler 入口组 `{}` 缺少源码文件", group.dependency_key)));
        }
        completed.insert(group.dependency_key.clone());
    }
    Ok(())
}

/// 集成测试夹具：从 HIR 构造阶段载荷，不作为生产源码闭包入口。
#[doc(hidden)]
pub fn compiled_program_from_hir_module(hir_module: HirModule) -> Result<nyar_types::CompiledProgram, HirError> {
    let semantic_mir = crate::valkyrie::mir::MirLowerer::lower_module_semantic(&hir_module);
    let compiled_program = crate::valkyrie::compile_pipeline::compile_linked_semantic_mir(&semantic_mir)
        .map_err(|error| HirError::invalid(format!("Compiler 成功载荷生产失败: {error:?}")))?;
    Ok(compiled_program)
}

pub(crate) fn compute_nominal_declarations(
    module: &HirModule,
    nominal_offset: u32,
    diagnostics: &mut Vec<crate::mir::MirDiagnostic>,
) -> (Vec<MirSumDeclaration>, Vec<FlagsLayout>) {
    (collect_sum_declarations(module, nominal_offset, diagnostics), collect_flags_layouts(module))
}

fn collect_sum_declarations(
    module: &HirModule,
    nominal_offset: u32,
    diagnostics: &mut Vec<crate::mir::MirDiagnostic>,
) -> Vec<MirSumDeclaration> {
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
                            .filter_map(|field| {
                                let Some(id) = field.id else {
                                    diagnostics.push(crate::mir::MirDiagnostic::UnresolvedFieldIdentity { field: field.name.to_string() });
                                    return None;
                                };
                                Some(crate::mir::MirField { id, name: field.name.to_string(), ty: field.ty.clone() })
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
                            .filter_map(|field| {
                                let Some(id) = field.id else {
                                    diagnostics.push(crate::mir::MirDiagnostic::UnresolvedFieldIdentity { field: field.name.to_string() });
                                    return None;
                                };
                                Some(crate::mir::MirField { id, name: field.name.to_string(), ty: field.ty.clone() })
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
fn resolve_sum_variant_tag(owner: &crate::types::Identifier, variant: &HirVariant, next_implicit: &mut u32) -> Result<u32, HirError> {
    let tag = if let Some(discriminator) = &variant.discriminator {
        integer_literal_u32(discriminator).ok_or_else(|| {
            HirError::invalid(format!("`{}` variant `{}` discriminator must be a non-negative integer literal", owner, variant.name))
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

fn validate_sum_type_discriminators(owner: &crate::types::Identifier, variants: &[HirVariant]) -> Result<(), HirError> {
    use std::collections::BTreeSet;

    let mut next_implicit = 0u32;
    let mut seen = BTreeSet::new();
    for variant in variants {
        let tag = resolve_sum_variant_tag(owner, variant, &mut next_implicit)?;
        if !seen.insert(tag) {
            return Err(HirError::invalid(format!("`{}` has duplicate discriminator {tag}", owner)));
        }
    }
    Ok(())
}

#[doc(hidden)]
pub fn validate_enum_discriminators(module: &HirModule) -> Result<(), HirError> {
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
    pub fn validate_hir_semantic_contract(&self, hir: &HirModule) -> Result<(), HirError> {
        validate_interop_surface(hir)?;
        validate_hir_contract(hir)
    }

    /// Parses source text and lowers it into a minimal HIR module.
    pub fn compile_source(&self, source: &str) -> Result<HirModule, HirError> {
        self.compile_source_with_semantic_exports(source, &[])
    }

    /// Parses source text with nominal metadata exported by resolved
    /// dependencies. This metadata is supplied by the workspace resolver,
    /// never reconstructed from dependency source text in this consumer.
    pub fn compile_source_with_semantic_exports(
        &self,
        source: &str,
        imported_semantic_exports: &[HirDependencySemanticExport],
    ) -> Result<HirModule, HirError> {
        self.compile_source_with_semantic_exports_and_name(source, imported_semantic_exports, None)
    }

    /// 在解析调用前固定 Resolver 分配的模块身份。
    pub fn compile_source_with_semantic_exports_and_name(
        &self,
        source: &str,
        imported_semantic_exports: &[HirDependencySemanticExport],
        module_name: Option<NamePath>,
    ) -> Result<HirModule, HirError> {
        let mut root = frontend::parse_source(source)?;
        expand_macros_in_root(&mut root);
        expand_templates_in_root(&mut root, source, DEFAULT_COMPILE_ARCH)?;
        let hir = source_context::with_hir_lower_source(source, || {
            self.lower_root_with_semantic_exports_and_name(&root, imported_semantic_exports, module_name)
        })?;
        self.validate_hir_semantic_contract(&hir)?;
        Ok(hir)
    }

    /// 正式源码组的第一阶段：只物化 HIR，不解析调用。
    ///
    /// 调用解析必须等完整源码闭包进入 Compiler 后统一进行；该入口不产生
    /// 可执行产物，也不允许被装配层直接使用。
    #[doc(hidden)]
    pub fn parse_source_group_without_call_resolution(
        &self,
        source: &str,
        imported_semantic_exports: &[HirDependencySemanticExport],
        module_name: Option<NamePath>,
        arch: &str,
    ) -> Result<HirModule, HirError> {
        let mut root = frontend::parse_source(source)?;
        expand_macros_in_root(&mut root);
        expand_templates_in_root(&mut root, source, arch)?;
        let hir = source_context::with_hir_lower_source(source, || {
            AstToHir::new(self.source_id).lower_root_without_call_resolution(&root, imported_semantic_exports, module_name)
        })?;
        validate_interop_surface(&hir)?;
        Ok(hir)
    }

    /// Parses a source file and lowers it into a minimal HIR module.
    pub fn compile_path(&self, path: &Path) -> Result<HirModule, HirError> {
        let source = std::fs::read_to_string(path).map_err(|e| HirError::invalid(format!("读取源码失败: {e}")))?;
        self.compile_source(&source)
    }

    /// Parses `.vx` source (Valkyrie + X-Grammar) and lowers into HIR with `view` → `render` normalization.
    pub fn compile_vx_source(&self, source: &str) -> Result<HirModule, HirError> {
        Err(HirError::VxUnsupported)
    }

    /// Parses a `.vx` file and lowers it into HIR with `view` → `render` normalization.
    pub fn compile_vx_path(&self, path: &Path) -> Result<HirModule, HirError> {
        let source = std::fs::read_to_string(path).map_err(|e| HirError::invalid(format!("读取 .vx 源码失败: {e}")))?;
        self.compile_vx_source(&source)
    }

    /// 集成测试夹具：将单一源码降低为成功载荷。
    #[doc(hidden)]
    pub fn compile_source_to_program(&self, source: &str) -> Result<nyar_types::CompiledProgram, HirError> {
        let hir_module = self.compile_source(source)?;
        compiled_program_from_hir_module(hir_module)
    }

    /// 从完整依赖顺序的源码快照构建一个语义闭包。
    ///
    /// Resolver 只提供源码和依赖身份；导出合同、依赖 MIR 与可达链接全部
    /// 在 Compiler 内完成，调用方不得自行拼接 HIR 或 MIR。
    #[doc(hidden)]
    pub fn compile_source_groups_to_program(&self, groups: &[CompilerSourceGroup]) -> Result<nyar_types::CompiledProgram, HirError> {
        self.compile_source_groups_to_program_with_host_bindings(groups, &[], DEFAULT_COMPILE_ARCH)
    }

    /// 从「应用组 + 独立生成组」的完整输入构建程序。
    ///
    /// 生成组在这里合成到唯一源码闭包；不存在只编译应用组而静默丢弃生成源码的路径。
    #[doc(hidden)]
    pub fn compile_source_input_to_program_with_host_bindings(
        &self,
        input: &CompilerSourceInput,
        host_bindings: &[crate::valkyrie::compile_pipeline::CompilerHostProviderBinding],
        arch: &str,
    ) -> Result<nyar_types::CompiledProgram, HirError> {
        let groups = compose_source_input_groups(input)?;
        self.compile_source_groups_to_program_with_host_bindings(&groups, host_bindings, arch)
    }
    /// 从完整依赖顺序的源码快照构建语义闭包，并应用 Resolver 选定的 host provider 绑定。
    #[doc(hidden)]
    pub fn compile_source_groups_to_program_with_host_bindings(
        &self,
        groups: &[CompilerSourceGroup],
        host_bindings: &[crate::valkyrie::compile_pipeline::CompilerHostProviderBinding],
        arch: &str,
    ) -> Result<nyar_types::CompiledProgram, HirError> {
        let entry_indices = groups.iter().enumerate().filter(|(_, group)| group.is_entry).map(|(index, _)| index).collect::<Vec<_>>();
        let mut hir_groups = self.resolve_source_groups_with_host_bindings(groups, host_bindings, arch)?;
        let final_index = hir_groups.len().checked_sub(1).ok_or_else(|| HirError::invalid("semantic source group plan is empty"))?;
        let final_hir = hir_groups.pop().ok_or_else(|| HirError::invalid("semantic source group plan is empty"))?;
        let all_hirs = std::iter::once(&final_hir).chain(hir_groups.iter()).collect::<Vec<_>>();
        let host_runtime = crate::valkyrie::mir::ssa::contracts::allocate_program_host_runtime(&all_hirs);
        // monomorph 与 host runtime 均接在整份闭包 HIR 实例池之后，避免跨组撞号。
        let mut mono_counter = Some(
            host_runtime
                .by_name
                .values()
                .map(|instance| instance.index())
                .max()
                .map(|index| index.saturating_add(1))
                .unwrap_or(0),
        );
        let mut mir_groups = hir_groups
            .iter()
            .map(|hir| {
                crate::valkyrie::mir::MirLowerer::lower_module_semantic_with_mono_counter_and_host(hir, &mut mono_counter, Some(&host_runtime), false)
            })
            .collect::<Vec<_>>();
        let mut final_mir =
            crate::valkyrie::mir::MirLowerer::lower_module_semantic_with_mono_counter_and_host(&final_hir, &mut mono_counter, Some(&host_runtime), true);
        // 程序模块是全部入口组共同的链接根宿主：非末尾入口组的 `[export]` / `[main]`
        // 必须按已冻结合同身份并入，而不是只让最后一个组决定产物表面。
        for index in entry_indices {
            if index != final_index {
                crate::valkyrie::compile_pipeline::merge_entry_group_surface(&mut final_mir, &mir_groups[index])?;
            }
        }
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
        // host provider 绑定在 HIR 物化后映射为冻结实例身份；Semantic MIR 不保存声明索引 key。
        let resolved_host_bindings = crate::valkyrie::compile_pipeline::resolve_host_provider_bindings(&all_hirs, host_bindings)?;
        crate::valkyrie::compile_pipeline::apply_host_provider_bindings(&mut final_mir, &mir_groups, &resolved_host_bindings)?;
        crate::valkyrie::compile_pipeline::compile_linked_semantic_mir(&final_mir)
            .map_err(|error| HirError::invalid(format!("Compiler 成功载荷生产失败: {error:?}")))
    }

    #[doc(hidden)]
    pub fn resolve_source_groups(&self, groups: &[CompilerSourceGroup], arch: &str) -> Result<Vec<HirModule>, HirError> {
        self.resolve_source_groups_with_host_bindings(groups, &[], arch)
    }

    /// 从完整依赖顺序的源码快照构建语义闭包，并把 Resolver 选定的 host provider 声明纳入 body lowering。
    pub fn resolve_source_groups_with_host_bindings(
        &self,
        groups: &[CompilerSourceGroup],
        host_bindings: &[crate::valkyrie::compile_pipeline::CompilerHostProviderBinding],
        arch: &str,
    ) -> Result<Vec<HirModule>, HirError> {
        validate_source_groups(groups)?;
        let closure =
            source_group_closure_with_host_bindings(groups, host_bindings, arch).map_err(|error| HirError::invalid(error.to_string()))?;
        let mut exports = std::collections::BTreeMap::<String, HirDependencySemanticExport>::new();
        let mut hir_groups = Vec::with_capacity(groups.len());
        let mut next_declaration = 0u32;
        let mut next_instance = 0u32;
        let mut next_field = 0u32;
        for (group, selection) in groups.iter().zip(&closure.groups) {
            let dependency_exports = group
                .direct_dependencies
                .iter()
                .map(|name| {
                    exports
                        .get(name)
                        .cloned()
                        .ok_or_else(|| HirError::LoweringFailed {
                            scenario: "dependency_export_unavailable".into(),
                            details: vec![("export".into(), name.clone()), ("group".into(), group.name.clone())],
                            source: DiagnosticSource { id: "unknown".into(), revision: None },
                            span: DiagnosticSpan { start: 0, end: 0 },
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut hir_module = self.parse_source_files_without_call_resolution(
                group,
                selection,
                &dependency_exports,
                Some(NamePath::new(vec![Identifier::new(&group.name)])),
                arch,
            )?;
            hir_module.dependency_key = group.dependency_key.clone();
            hir_module.source_path = group.sources.first().map(|file| file.path.clone()).unwrap_or_else(|| format!("{}.v", group.name));
            register_function_declarations(&mut hir_module, &mut next_declaration, &mut next_instance)?;
            register_field_declarations(&mut hir_module, &mut next_field)?;
            freeze_module_generic_binders(&mut hir_module);
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
                return Err(HirError::LoweringFailed {
                scenario: "dependency_export_identity_collision".into(),
                details: vec![("dependency_key".into(), group.dependency_key.clone())],
                source: DiagnosticSource { id: "unknown".into(), revision: None },
                span: DiagnosticSpan { start: 0, end: 0 },
            }.into());
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
        selection: &GroupSemanticClosure,
        imported_semantic_exports: &[HirDependencySemanticExport],
        module_name: Option<NamePath>,
        arch: &str,
    ) -> Result<HirModule, HirError> {
        if selection.body_lowering_source_paths.is_empty() && selection.body_lowering_declarations.is_empty() {
            // 空需求是闭包的正常结果；空模块仍保留依赖导出和模块身份。
            return AstToHir::new(self.source_id).lower_root_without_call_resolution(
                &ValkyrieRoot { items: Vec::new() },
                imported_semantic_exports,
                module_name,
            );
        }
        // 宏/模板展开必须先于别名收集：展开会合成声明，别名作用域只登记最终进入 lowering 的声明。
        let mut prepared = Vec::new();
        for file in &group.sources {
            if !selection.body_lowering_source_paths.contains(&file.path) {
                continue;
            }
            // 计划内的文件属于入口语义闭包；语法或语义错误必须结构化失败，不得吞错后编译残缺程序。
            let root = self.prepare_source_file_root(group, selection, file, arch)?;
            prepared.push((file, root));
        }
        if prepared.is_empty() {
            return Err(HirError::NoBodySourceInGroup {
                dependency_key: group.dependency_key.clone(),
                source: DiagnosticSource { id: group.dependency_key.clone(), revision: None },
                span: DiagnosticSpan { start: 0, end: 0 },
            }.into());
        }
        // 类型别名作用域是组级事实：本组全部文件的 `type` 声明 + 可见依赖导出的别名。
        let roots = prepared.iter().map(|(_, root)| root).collect::<Vec<_>>();
        let _type_alias_scope = ModuleTypeAliasScope::enter(visible_type_aliases(&roots, imported_semantic_exports));
        register_root_type_alias_targets(&roots);
        let mut merged: Option<HirModule> = None;
        for (file, root) in &prepared {
            let module = self.lower_prepared_source_file(file, root, imported_semantic_exports, module_name.clone())?;
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
        let hir = merged.expect("prepared source files imply a merged HirModule");
        validate_interop_surface(&hir)?;
        Ok(hir)
    }

    /// 解析一个源文件并完成宏/模板展开；别名收集与 HIR lowering 都在展开结果上进行。
    fn prepare_source_file_root(
        &self,
        group: &CompilerSourceGroup,
        selection: &GroupSemanticClosure,
        file: &CompilerSourceFile,
        arch: &str,
    ) -> Result<ValkyrieRoot, HirError> {
        let mut root = frontend::parse_source(&file.source).map_err(|parse_err| {
            HirError::invalid(format!("源码组 `{}` 文件 `{}`: {}", group.dependency_key, file.path, parse_err))
        })?;
        // 声明级物化边界：进入 HIR 的声明身份必须与闭包规划一致。
        // 非被需求声明不得在此后参与调用解析、合同校验、MIR 与 layout。
        retain_demanded_declarations(&mut root.items, &group.dependency_key, &file.path, &selection.body_lowering_declarations);
        expand_macros_in_root(&mut root);
        expand_templates_in_root(&mut root, &file.source, arch)?;
        Ok(root)
    }

    /// 降低一个已解析并展开的源文件；类型别名作用域由调用方按组建立。
    fn lower_prepared_source_file(
        &self,
        file: &CompilerSourceFile,
        root: &ValkyrieRoot,
        imported_semantic_exports: &[HirDependencySemanticExport],
        module_name: Option<NamePath>,
    ) -> Result<HirModule, HirError> {
        source_context::with_hir_lower_source(&file.source, || {
            AstToHir::new(self.source_id)
                .lower_prepared_root(root, imported_semantic_exports, module_name)
                .map_err(|error| HirError::invalid(format!("source file `{}`: {error}", file.path)))
        })
    }

    /// Lowers parser output into a HIR module.
    pub fn lower_root(&self, root: &ValkyrieRoot) -> Result<HirModule, HirError> {
        self.lower_root_with_semantic_exports(root, &[])
    }

    /// Lowers parser output with resolved nominal dependency exports in scope.
    pub fn lower_root_with_semantic_exports(
        &self,
        root: &ValkyrieRoot,
        imported_semantic_exports: &[HirDependencySemanticExport],
    ) -> Result<HirModule, HirError> {
        AstToHir::new(self.source_id).lower_root_with_semantic_exports(root, imported_semantic_exports)
    }

    /// 在 HIR 调用解析前覆盖模块身份。
    pub fn lower_root_with_semantic_exports_and_name(
        &self,
        root: &ValkyrieRoot,
        imported_semantic_exports: &[HirDependencySemanticExport],
        module_name: Option<NamePath>,
    ) -> Result<HirModule, HirError> {
        AstToHir::new(self.source_id).lower_root_with_semantic_exports_and_name(root, imported_semantic_exports, module_name)
    }
}

/// `Self` 在其声明作用域内解析出的名义 owner。
///
/// `construct_path` 与 `overload::struct_constructor_path` 使用同一组成（namespace + name），
/// 因此 `Self { … }` 会精确命中 owner 自己的构造器候选；`name` 是候选 owner 使用的短名，
/// 供 `Self::member` 限定路径改写。两者都只是名称解析结果，声明身份仍由候选冻结。
struct SelfOwnerPath {
    construct_path: NamePath,
    name: Identifier,
}

impl SelfOwnerPath {
    fn from_struct(item: &HirStruct) -> Self {
        let mut parts = item.namespace.clone();
        parts.push(item.name.clone());
        Self { construct_path: NamePath::new(parts), name: item.name.clone() }
    }

    fn named(namespace: &[Identifier], name: &Identifier) -> Self {
        let mut parts = namespace.to_vec();
        parts.push(name.clone());
        Self { construct_path: NamePath::new(parts), name: name.clone() }
    }
}

/// 在结构体/枚举已分配 declaration ItemId 之后，把唯一可解析的 `Named`/`Apply(Named, …)`
/// 绑定为 [`ValkyrieType::Nominal`]。这是 Resolver→HIR 的声明身份冻结，不是短名改写或后置补丁。
fn bind_declared_nominal_types(module: &mut HirModule) -> Result<(), HirError> {
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
    fn unique_struct<'a>(name: &Identifier, namespace: &NamePath, structs: &'a [HirStruct]) -> Result<Option<&'a HirStruct>, HirError> {
        let unknown_source = DiagnosticSource { id: "unknown".into(), revision: None };
        let unknown_span = DiagnosticSpan { start: 0, end: 0 };
        let exact: Vec<&HirStruct> = structs
            .iter()
            .filter(|item| qualified_struct(item) == *name || (item.name == *name && NamePath::new(item.namespace.clone()) == *namespace))
            .collect();
        if exact.len() > 1 {
            return Err(HirError::NominalAmbiguous {
                name: name.to_string(),
                candidate_count: exact.len(),
                source: unknown_source.clone(),
                span: unknown_span.clone(),
            }.into());
        }
        if exact.len() == 1 {
            return Ok(Some(exact[0]));
        }
        let short: Vec<&HirStruct> = structs.iter().filter(|item| &item.name == name).collect();
        if short.len() > 1 {
            return Err(HirError::NominalAmbiguous {
                name: name.to_string(),
                candidate_count: short.len(),
                source: unknown_source.clone(),
                span: unknown_span.clone(),
            }.into());
        }
        Ok(short.into_iter().next())
    }
    fn unique_enum<'a>(name: &Identifier, enums: &'a [HirEnum]) -> Result<Option<&'a HirEnum>, HirError> {
        let unknown_source = DiagnosticSource { id: "unknown".into(), revision: None };
        let unknown_span = DiagnosticSpan { start: 0, end: 0 };
        let matches: Vec<&HirEnum> = enums.iter().filter(|item| &item.name == name).collect();
        if matches.len() > 1 {
            return Err(HirError::SumUniteAmbiguous {
                name: name.to_string(),
                candidate_count: matches.len(),
                source: unknown_source.clone(),
                span: unknown_span.clone(),
            }.into());
        }
        Ok(matches.into_iter().next())
    }
    fn resolve_declaration(
        name: &Identifier,
        namespace: &NamePath,
        structs: &[HirStruct],
        enums: &[HirEnum],
    ) -> Result<Option<nyar_types::ItemId>, HirError> {
        if let Some(item) = unique_struct(name, namespace, structs)? {
            return Ok(Some(item.constructor_declaration.ok_or_else(|| {
                HirError::invalid(format!("结构 `{}` 缺少 constructor_declaration，无法绑定名义类型", qualified_struct(item)))
            })?));
        }
        if let Some(item) = unique_enum(name, enums)? {
            return Ok(Some(
                item.declaration.ok_or_else(|| HirError::invalid(format!("sum/unite `{}` 缺少 declaration，无法绑定名义类型", item.name)))?,
            ));
        }
        Ok(None)
    }
    /// `imply` 目标已冻结为声明身份后，`Self` 在该块内指目标声明本身。
    ///
    /// 只对名义目标成立：结构化数组 `[T]` 的 `Self` 不是 `class Array<T>` 名义类型，
    /// 因此失败闭合，不借数组 owner 约定改写。
    fn impl_self_owner(target: &ValkyrieType, structs: &[HirStruct], enums: &[HirEnum]) -> Option<SelfOwnerPath> {
        let declaration = match target {
            ValkyrieType::Nominal { declaration, .. } => *declaration,
            _ => return None,
        };
        if let Some(item) = structs.iter().find(|item| item.constructor_declaration == Some(declaration)) {
            return Some(SelfOwnerPath::from_struct(item));
        }
        enums.iter().find(|item| item.declaration == Some(declaration)).map(|item| SelfOwnerPath::named(&[], &item.name))
    }
    /// 结构化数组 `[T]` 的声明 owner 是否是 `class Array<T>`。
    ///
    /// `ValkyrieType::Array` 不携带声明身份，只能由这条 owner 约定接回声明；该约定与
    /// `impl_nominal_type_name(Array(_)) == "Array"` 一致。这是绑定阶段唯一一次名称判定，
    /// 保证 `[T]` 与 `Array<T>` 得到同一个类型身份，而不是在下游按名互认。
    fn is_array_owner(declaration: nyar_types::ItemId, structs: &[HirStruct]) -> bool {
        array_owner_declarations(structs).contains(&declaration)
    }
    fn bind_type(ty: &mut ValkyrieType, namespace: &NamePath, structs: &[HirStruct], enums: &[HirEnum]) -> Result<(), HirError> {
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
            ValkyrieType::QualifiedPath { namespace: path_namespace, name } => {
                let path = NamePath::new(path_namespace.iter().map(|segment| Identifier::new(segment.as_str())).collect());
                let Some(declaration) = resolve_declaration(name, &path, structs, enums)? else {
                    return Err(HirError::invalid(format!(
                        "限定类型路径 `{}` 未绑定声明身份",
                        ValkyrieType::qualified_path_label(path_namespace, name)
                    )));
                };
                *ty = ValkyrieType::Nominal { declaration, arguments: Vec::new() };
            }
            ValkyrieType::Apply(base, args) => {
                for arg in args.iter_mut() {
                    bind_type(arg, namespace, structs, enums)?;
                }
                let resolved_declaration = match base.as_ref() {
                    ValkyrieType::Named(name) => resolve_declaration(name, namespace, structs, enums)?,
                    ValkyrieType::QualifiedPath { namespace: path_namespace, name } => {
                        let path_namespace = NamePath::new(path_namespace.iter().map(|segment| Identifier::new(segment.as_str())).collect());
                        resolve_declaration(name, &path_namespace, structs, enums)?
                    }
                    _ => None,
                };
                if let Some(declaration) = resolved_declaration {
                    // `class Array<T>` 是结构化数组 `[T]` 的声明 owner：`Array<T>` 与
                    // `[T]` 是同一个类型，数组值在所有阶段只有结构化 `Array` 一个身份。
                    // owner 归属沿用 `impl_nominal_type_name` 的既定约定，不另造名义类型。
                    if args.len() == 1 && is_array_owner(declaration, structs) {
                        let element = args.remove(0);
                        *ty = ValkyrieType::Array(Box::new(element));
                        return Ok(());
                    }
                    let arguments = std::mem::take(args);
                    *ty = ValkyrieType::Nominal { declaration, arguments };
                    return Ok(());
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
    fn bind_expr(
        expr: &mut HirExpr,
        namespace: &NamePath,
        structs: &[HirStruct],
        enums: &[HirEnum],
        self_owner: Option<&SelfOwnerPath>,
    ) -> Result<(), HirError> {
        match &mut expr.kind {
            HirExprKind::Call { callee, args, resolved, .. } => {
                bind_expr(callee, namespace, structs, enums, self_owner)?;
                for arg in args {
                    bind_expr(&mut arg.value, namespace, structs, enums, self_owner)?;
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
            HirExprKind::Construct { path, args, resolved } => {
                // `Self { … }` 在声明作用域内构造 owner 声明本身；这里的改写只是把
                // `Self` 解析到声明路径，候选匹配仍由已冻结的声明身份完成。
                if let Some(owner) = self_owner {
                    if path.parts().len() == 1 && path.parts()[0].as_str() == "Self" {
                        *path = owner.construct_path.clone();
                    }
                }
                for arg in args {
                    bind_expr(arg, namespace, structs, enums, self_owner)?;
                }
                if let Some(contract) = resolved {
                    for param in &mut contract.parameter_types {
                        bind_type(param, namespace, structs, enums)?;
                    }
                    bind_type(&mut contract.return_type, namespace, structs, enums)?;
                }
            }
            HirExprKind::Cast { expr: inner, ty, .. } => {
                bind_expr(inner, namespace, structs, enums, self_owner)?;
                bind_type(ty, namespace, structs, enums)?;
            }
            HirExprKind::Lambda { params, return_type, body, .. } => {
                for param in params {
                    bind_type(&mut param.ty, namespace, structs, enums)?;
                }
                bind_type(return_type, namespace, structs, enums)?;
                bind_block(body, namespace, structs, enums, self_owner)?;
            }
            HirExprKind::TryScope { result_type, body, .. } => {
                if let Some(ty) = result_type {
                    bind_type(ty, namespace, structs, enums)?;
                }
                bind_block(body, namespace, structs, enums, self_owner)?;
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
            | HirExprKind::FieldAccess { object: value, .. } => bind_expr(value, namespace, structs, enums, self_owner)?,
            HirExprKind::Path(path) => {
                // `Self::member` 的 `Self` 是声明作用域内的名义 owner；候选 owner 以短名索引。
                if let Some(owner) = self_owner {
                    if path.parts().len() > 1 && path.parts()[0].as_str() == "Self" {
                        let mut parts = path.parts().to_vec();
                        parts[0] = owner.name.clone();
                        *path = NamePath::new(parts);
                    }
                }
            }
            HirExprKind::GenericApply { callee, arguments } => {
                bind_expr(callee, namespace, structs, enums, self_owner)?;
                for argument in arguments {
                    bind_type(argument, namespace, structs, enums)?;
                }
            }
            HirExprKind::StoreField { object, value, .. } => {
                bind_expr(object, namespace, structs, enums, self_owner)?;
                bind_expr(value, namespace, structs, enums, self_owner)?;
            }
            HirExprKind::ArrayNew { element_type, length } => {
                bind_type(element_type, namespace, structs, enums)?;
                bind_expr(length, namespace, structs, enums, self_owner)?;
            }
            HirExprKind::ArrayLiteral { items } | HirExprKind::TupleLiteral { items } => {
                for item in items {
                    bind_expr(item, namespace, structs, enums, self_owner)?;
                }
            }
            HirExprKind::Block(block) => bind_block(block, namespace, structs, enums, self_owner)?,
            HirExprKind::If { condition, then_branch, else_branch, .. } => {
                bind_expr(condition, namespace, structs, enums, self_owner)?;
                bind_block(then_branch, namespace, structs, enums, self_owner)?;
                if let Some(else_branch) = else_branch {
                    bind_block(else_branch, namespace, structs, enums, self_owner)?;
                }
            }
            HirExprKind::IfLet { pattern: _, scrutinee, then_branch, else_branch, .. } => {
                bind_expr(scrutinee, namespace, structs, enums, self_owner)?;
                bind_block(then_branch, namespace, structs, enums, self_owner)?;
                if let Some(else_branch) = else_branch {
                    bind_block(else_branch, namespace, structs, enums, self_owner)?;
                }
            }
            // 循环、catch、with、super 调用都携带类型位与嵌套表达式：漏掉任何一个都会让该子树
            // 退回 `Named`，随后与已冻结的 `Nominal` 当成不兼容。
            HirExprKind::Loop { pattern: _, iterator, condition, body, .. } => {
                if let Some(iterator) = iterator {
                    bind_expr(iterator, namespace, structs, enums, self_owner)?;
                }
                if let Some(condition) = condition {
                    bind_expr(condition, namespace, structs, enums, self_owner)?;
                }
                bind_block(body, namespace, structs, enums, self_owner)?;
            }
            HirExprKind::Catch { expr: inner, arms } => {
                bind_expr(inner, namespace, structs, enums, self_owner)?;
                for arm in arms {
                    if let Some(guard) = &mut arm.guard {
                        bind_expr(guard, namespace, structs, enums, self_owner)?;
                    }
                    bind_expr(&mut arm.body, namespace, structs, enums, self_owner)?;
                }
            }
            HirExprKind::With { base, updates } => {
                bind_expr(base, namespace, structs, enums, self_owner)?;
                for (_, update) in updates {
                    bind_expr(update, namespace, structs, enums, self_owner)?;
                }
            }
            HirExprKind::SuperCall { args, .. } => {
                for arg in args {
                    bind_expr(arg, namespace, structs, enums, self_owner)?;
                }
            }
            HirExprKind::Match { scrutinee, arms, .. } | HirExprKind::Case { scrutinee, arms, .. } => {
                bind_expr(scrutinee, namespace, structs, enums, self_owner)?;
                for arm in arms {
                    if let Some(guard) = &mut arm.guard {
                        bind_expr(guard, namespace, structs, enums, self_owner)?;
                    }
                    bind_expr(&mut arm.body, namespace, structs, enums, self_owner)?;
                }
            }
            HirExprKind::AnonymousClass { fields, methods, .. } => {
                for (_, value) in fields {
                    bind_expr(value, namespace, structs, enums, self_owner)?;
                }
                // 匿名类内的 `Self` 指匿名类本身，不能继承外层 owner 身份。
                for method in methods {
                    bind_function(method, structs, enums, None)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn bind_block(
        block: &mut HirBlock,
        namespace: &NamePath,
        structs: &[HirStruct],
        enums: &[HirEnum],
        self_owner: Option<&SelfOwnerPath>,
    ) -> Result<(), HirError> {
        for statement in &mut block.statements {
            match &mut statement.kind {
                HirStatementKind::Let { ty, initializer, .. } => {
                    if let Some(annotation) = ty {
                        bind_type(annotation, namespace, structs, enums)?;
                    }
                    if let Some(initializer) = initializer {
                        bind_expr(initializer, namespace, structs, enums, self_owner)?;
                    }
                }
                HirStatementKind::Expr(expr) => bind_expr(expr, namespace, structs, enums, self_owner)?,
            }
        }
        if let Some(expr) = &mut block.expr {
            bind_expr(expr, namespace, structs, enums, self_owner)?;
        }
        Ok(())
    }
    fn bind_function(
        function: &mut HirFunction,
        structs: &[HirStruct],
        enums: &[HirEnum],
        self_owner: Option<&SelfOwnerPath>,
    ) -> Result<(), HirError> {
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
        bind_block(&mut function.body, &function.declaring_namespace, structs, enums, self_owner)
    }
    let mut structs = Vec::new();
    let mut enums = Vec::new();
    collect_structs(module, &mut structs);
    collect_enums(module, &mut enums);
    for item in &mut module.structs {
        let namespace = NamePath::new(item.namespace.clone());
        for field in &mut item.fields {
            bind_type(&mut field.ty, &namespace, &structs, &enums)?;
            propagate_declared_generic_binders(&mut field.ty, &item.generics);
        }
        let self_owner = SelfOwnerPath::from_struct(item);
        for function in &mut item.methods {
            bind_function(function, &structs, &enums, Some(&self_owner))?;
            propagate_declared_generic_binders(&mut function.return_type, &item.generics);
            for param in &mut function.params {
                propagate_declared_generic_binders(&mut param.ty, &item.generics);
            }
            propagate_generic_binders_in_block(&mut function.body, &item.generics);
            propagate_generic_binders_in_where_constraints(&mut function.where_constraints, &item.generics);
        }
    }
    for enum_def in &mut module.enums {
        for variant in &mut enum_def.variants {
            for field in &mut variant.fields {
                bind_type(&mut field.ty, &NamePath::default(), &structs, &enums)?;
                propagate_declared_generic_binders(&mut field.ty, &enum_def.generics);
            }
            if let Some(result) = &mut variant.result_type {
                bind_type(result, &NamePath::default(), &structs, &enums)?;
                propagate_declared_generic_binders(result, &enum_def.generics);
            }
        }
    }
    for function in &mut module.functions {
        bind_function(function, &structs, &enums, None)?;
        propagate_declared_generic_binders(&mut function.return_type, &function.generics);
        for param in &mut function.params {
            propagate_declared_generic_binders(&mut param.ty, &function.generics);
        }
        propagate_generic_binders_in_block(&mut function.body, &function.generics);
        propagate_generic_binders_in_where_constraints(&mut function.where_constraints, &function.generics);
    }
    for singleton in &mut module.singletons {
        let namespace = NamePath::new(singleton.namespace.clone());
        for field in &mut singleton.fields {
            bind_type(&mut field.ty, &namespace, &structs, &enums)?;
            propagate_declared_generic_binders(&mut field.ty, &singleton.generics);
        }
        let self_owner = SelfOwnerPath::named(&singleton.namespace, &singleton.name);
        for function in &mut singleton.methods {
            bind_function(function, &structs, &enums, Some(&self_owner))?;
            propagate_declared_generic_binders(&mut function.return_type, &singleton.generics);
            for param in &mut function.params {
                propagate_declared_generic_binders(&mut param.ty, &singleton.generics);
            }
            propagate_generic_binders_in_block(&mut function.body, &singleton.generics);
            propagate_generic_binders_in_where_constraints(&mut function.where_constraints, &singleton.generics);
        }
        if let Some(function) = &mut singleton.constructor {
            bind_function(function, &structs, &enums, Some(&self_owner))?;
            propagate_declared_generic_binders(&mut function.return_type, &singleton.generics);
            for param in &mut function.params {
                propagate_declared_generic_binders(&mut param.ty, &singleton.generics);
            }
            propagate_generic_binders_in_block(&mut function.body, &singleton.generics);
            propagate_generic_binders_in_where_constraints(&mut function.where_constraints, &singleton.generics);
        }
        if let Some(function) = &mut singleton.finalizer {
            bind_function(function, &structs, &enums, Some(&self_owner))?;
            propagate_declared_generic_binders(&mut function.return_type, &singleton.generics);
            for param in &mut function.params {
                propagate_declared_generic_binders(&mut param.ty, &singleton.generics);
            }
            propagate_generic_binders_in_block(&mut function.body, &singleton.generics);
            propagate_generic_binders_in_where_constraints(&mut function.where_constraints, &singleton.generics);
        }
    }
    for impl_block in &mut module.impls {
        bind_type(&mut impl_block.target, &NamePath::default(), &structs, &enums)?;
        let self_owner = impl_self_owner(&impl_block.target, &structs, &enums);
        for function in &mut impl_block.methods {
            bind_function(function, &structs, &enums, self_owner.as_ref())?;
        }
    }
    for trait_def in &mut module.traits {
        for function in trait_def.methods.iter_mut().chain(trait_def.default_methods.iter_mut()) {
            bind_function(function, &structs, &enums, None)?;
            propagate_declared_generic_binders(&mut function.return_type, &trait_def.generics);
            for param in &mut function.params {
                propagate_declared_generic_binders(&mut param.ty, &trait_def.generics);
            }
            propagate_generic_binders_in_block(&mut function.body, &trait_def.generics);
            propagate_generic_binders_in_where_constraints(&mut function.where_constraints, &trait_def.generics);
        }
    }
    for submodule in &mut module.submodules {
        bind_declared_nominal_types(submodule)?;
    }
    // `imply Option<T>` 等块头无显式泛型参数时，target 实参仍须回收为 owner 的 Generic binder。
    propagate_impl_owner_type_generics(module);
    Ok(())
}

/// 全部承担结构化数组 `[T]` owner 身份的声明：唯一约定是名为 `Array` 且带一个泛型的聚合。
fn array_owner_declarations(structs: &[HirStruct]) -> Vec<nyar_types::ItemId> {
    structs
        .iter()
        .filter(|item| item.name.as_str() == "Array" && item.generics.len() == 1)
        .filter_map(|item| item.constructor_declaration)
        .collect()
}

/// 模块（含可见依赖导出与子模块）内结构化数组的 owner 声明。
fn array_owner_declaration(module: &HirModule) -> Option<nyar_types::ItemId> {
    let local = array_owner_declarations(&module.structs);
    if let Some(declaration) = local.first() {
        return Some(*declaration);
    }
    for export in &module.imported_semantic_exports {
        if let Some(declaration) = array_owner_declarations(&export.structs).first() {
            return Some(*declaration);
        }
    }
    module.submodules.iter().find_map(array_owner_declaration)
}

fn lookup_owner_generics_by_declaration(module: &HirModule, declaration: nyar_types::ItemId) -> Vec<GenericType> {
    if let Some(item) = module.enums.iter().find(|item| item.declaration == Some(declaration)) {
        return item.generics.clone();
    }
    if let Some(item) = module.structs.iter().find(|item| item.constructor_declaration == Some(declaration)) {
        return item.generics.clone();
    }
    for export in &module.imported_semantic_exports {
        if let Some(item) = export.enums.iter().find(|item| item.declaration == Some(declaration)) {
            return item.generics.clone();
        }
        if let Some(item) = export.structs.iter().find(|item| item.constructor_declaration == Some(declaration)) {
            return item.generics.clone();
        }
    }
    for submodule in &module.submodules {
        let nested = lookup_owner_generics_by_declaration(submodule, declaration);
        if !nested.is_empty() {
            return nested;
        }
    }
    Vec::new()
}

fn combined_impl_generic_context(module: &HirModule, implementation: &HirImpl) -> Vec<GenericType> {
    let mut generics = implementation.generics.clone();
    let owner_generics = match &implementation.target {
        ValkyrieType::Nominal { declaration, .. } => lookup_owner_generics_by_declaration(module, *declaration),
        // `imply Array<T>`：结构化数组不携带声明身份，owner 由 `class Array<T>` 的
        // 唯一约定接回，与绑定阶段 `Array<T>` → 结构化 `Array` 的归一使用同一事实。
        ValkyrieType::Array(_) => {
            array_owner_declaration(module).map(|declaration| lookup_owner_generics_by_declaration(module, declaration)).unwrap_or_default()
        }
        _ => Vec::new(),
    };
    for owner in owner_generics {
        if !generics.iter().any(|item| item.name == owner.name) {
            generics.push(owner);
        }
    }
    generics
}

fn propagate_generic_binders_in_where_constraints(
    constraints: &mut [crate::valkyrie::types::hir::HirWhereConstraint],
    generics: &[GenericType],
) {
    for constraint in constraints {
        propagate_declared_generic_binders(&mut constraint.target, generics);
        for bound in &mut constraint.bounds {
            for argument in &mut bound.type_arguments {
                propagate_declared_generic_binders(argument, generics);
            }
            for equation in &mut bound.associated_types {
                propagate_declared_generic_binders(&mut equation.ty, generics);
            }
        }
    }
}

fn propagate_impl_owner_type_generics(module: &mut HirModule) {
    let bindings = module.impls.iter().map(|implementation| combined_impl_generic_context(module, implementation)).collect::<Vec<_>>();
    for (implementation, generics) in module.impls.iter_mut().zip(bindings) {
        propagate_declared_generic_binders(&mut implementation.target, &generics);
        for method in &mut implementation.methods {
            propagate_declared_generic_binders(&mut method.return_type, &generics);
            for param in &mut method.params {
                propagate_declared_generic_binders(&mut param.ty, &generics);
            }
            propagate_generic_binders_in_block(&mut method.body, &generics);
        }
    }
    for submodule in &mut module.submodules {
        propagate_impl_owner_type_generics(submodule);
    }
}

/// 整份 HIR 模块（含子模块）已冻结 `ItemId` 的最大下标。
pub(crate) fn max_hir_declaration_index(module: &HirModule) -> Option<u32> {
    fn consider(declaration: Option<nyar_types::ItemId>, max: &mut Option<u32>) {
        if let Some(declaration) = declaration {
            let index = declaration.index();
            *max = Some(max.map_or(index, |current| current.max(index)));
        }
    }
    fn walk(module: &HirModule, max: &mut Option<u32>) {
        for function in &module.functions {
            consider(function.declaration, max);
        }
        for structure in &module.structs {
            consider(structure.constructor_declaration, max);
            for method in &structure.methods {
                consider(method.declaration, max);
            }
            for property in &structure.properties {
                for accessor in property.getter.iter().chain(property.setter.iter()) {
                    consider(accessor.declaration, max);
                }
            }
        }
        for enum_definition in &module.enums {
            consider(enum_definition.declaration, max);
            for variant in &enum_definition.variants {
                consider(variant.declaration, max);
            }
        }
        for singleton in &module.singletons {
            for method in
                singleton.methods.iter().chain(singleton.constructor.iter().map(Box::as_ref)).chain(singleton.finalizer.iter().map(Box::as_ref))
            {
                consider(method.declaration, max);
            }
        }
        for trait_definition in &module.traits {
            consider(trait_definition.declaration, max);
            for method in trait_definition.methods.iter().chain(trait_definition.default_methods.iter()) {
                consider(method.declaration, max);
            }
        }
        for implementation in &module.impls {
            for method in &implementation.methods {
                consider(method.declaration, max);
            }
        }
        for widget in &module.widgets {
            for method in &widget.methods {
                consider(method.declaration, max);
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

/// 整份 HIR 模块（含子模块）已冻结 callable `ItemInstanceId` 的最大下标。
pub(crate) fn max_hir_callable_instance_index(module: &HirModule) -> Option<u32> {
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
            consider(singleton.accessor_instance, max);
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

fn register_function_declarations(module: &mut HirModule, next: &mut u32, next_instance: &mut u32) -> Result<(), HirError> {
    fn register(function: &mut HirFunction, next: &mut u32, next_instance: &mut u32, owner_monomorphic: bool) -> Result<(), HirError> {
        if function.declaration.is_some() || function.instance.is_some() {
            return Err(HirError::invalid("源码声明不能重复分配 ItemId"));
        }
        let following = next.checked_add(1).ok_or_else(|| HirError::invalid("ItemId 声明空间耗尽"))?;
        function.declaration = Some(nyar_types::ItemId::from_index(*next).ok_or_else(|| HirError::invalid("无效 ItemId"))?);
        *next = following;
        // 顶层与单例成员在 MIR 中总会物化；泛型种子（如 `Some<T>` / `option_none<T>`）同样需要 callable instance。
        if owner_monomorphic {
            let following = next_instance.checked_add(1).ok_or_else(|| HirError::invalid("ItemInstanceId 实例空间耗尽"))?;
            function.instance =
                Some(nyar_types::ItemInstanceId::from_index(*next_instance).ok_or_else(|| HirError::invalid("无效 ItemInstanceId"))?);
            *next_instance = following;
        }
        Ok(())
    }
    for function in &mut module.functions {
        register(function, next, next_instance, true)?;
    }
    for structure in &mut module.structs {
        if structure.constructor_declaration.is_some() || structure.constructor_instance.is_some() {
            return Err(HirError::invalid("结构构造声明不能重复注册"));
        }
        structure.constructor_declaration = Some(nyar_types::ItemId::from_index(*next).ok_or_else(|| HirError::invalid("无效 ItemId"))?);
        *next = next.checked_add(1).ok_or_else(|| HirError::invalid("ItemId 声明空间耗尽"))?;
        if structure.generics.is_empty() {
            structure.constructor_instance =
                Some(nyar_types::ItemInstanceId::from_index(*next_instance).ok_or_else(|| HirError::invalid("无效 ItemInstanceId"))?);
            *next_instance = next_instance.checked_add(1).ok_or_else(|| HirError::invalid("ItemInstanceId 实例空间耗尽"))?;
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
            return Err(HirError::invalid("枚举声明不能重复注册"));
        }
        enum_definition.declaration = Some(nyar_types::ItemId::from_index(*next).ok_or_else(|| HirError::invalid("无效 ItemId"))?);
        *next = next.checked_add(1).ok_or_else(|| HirError::invalid("ItemId 声明空间耗尽"))?;
        for variant in &mut enum_definition.variants {
            if variant.declaration.is_some() || variant.instance.is_some() {
                return Err(HirError::invalid("变体构造声明不能重复注册"));
            }
            variant.declaration = Some(nyar_types::ItemId::from_index(*next).ok_or_else(|| HirError::invalid("无效 ItemId"))?);
            *next = next.checked_add(1).ok_or_else(|| HirError::invalid("ItemId 声明空间耗尽"))?;
            if enum_definition.generics.is_empty() {
                variant.instance =
                    Some(nyar_types::ItemInstanceId::from_index(*next_instance).ok_or_else(|| HirError::invalid("无效 ItemInstanceId"))?);
                *next_instance = next_instance.checked_add(1).ok_or_else(|| HirError::invalid("ItemInstanceId 实例空间耗尽"))?;
            }
        }
    }
    for singleton in &mut module.singletons {
        if singleton.accessor_instance.is_some() {
            return Err(HirError::invalid("单例 accessor 实例不能重复注册"));
        }
        singleton.accessor_instance =
            Some(nyar_types::ItemInstanceId::from_index(*next_instance).ok_or_else(|| HirError::invalid("无效 ItemInstanceId"))?);
        *next_instance = next_instance.checked_add(1).ok_or_else(|| HirError::invalid("ItemInstanceId 实例空间耗尽"))?;
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
            return Err(HirError::invalid("trait 声明不能重复注册"));
        }
        trait_definition.declaration = Some(nyar_types::ItemId::from_index(*next).ok_or_else(|| HirError::invalid("无效 ItemId"))?);
        *next = next.checked_add(1).ok_or_else(|| HirError::invalid("ItemId 声明空间耗尽"))?;
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

/// 为结构体、枚举变体与 singleton 字段分配全局唯一 [`nyar_types::FieldId`]。
///
/// 字段身份属于 frontend 事实：MIR 仅消费 `HirField::id`，不得按字段名或布局
/// 重新编号。源码组闭包共享同一计数器，依赖导出在注册后克隆，因此消费方与
/// 生产方看到同一字段身份。
fn register_field_declarations(module: &mut HirModule, next: &mut u32) -> Result<(), HirError> {
    fn register_fields(fields: &mut [HirField], next: &mut u32) -> Result<(), HirError> {
        for field in fields {
            if field.id.is_some() {
                return Err(HirError::invalid(format!("字段 `{}` 身份不能重复分配", field.name)));
            }
            let id = nyar_types::FieldId::from_index(*next).ok_or_else(|| HirError::invalid("FieldId 字段空间耗尽"))?;
            field.id = Some(id);
            *next = next.checked_add(1).ok_or_else(|| HirError::invalid("FieldId 字段空间耗尽"))?;
        }
        Ok(())
    }
    for structure in &mut module.structs {
        register_fields(&mut structure.fields, next)?;
    }
    for enum_definition in &mut module.enums {
        for variant in &mut enum_definition.variants {
            register_fields(&mut variant.fields, next)?;
        }
    }
    for singleton in &mut module.singletons {
        register_fields(&mut singleton.fields, next)?;
    }
    for submodule in &mut module.submodules {
        register_field_declarations(submodule, next)?;
    }
    Ok(())
}

fn freeze_generic_binders(generics: &mut [GenericType], owner: nyar_types::ItemId) {
    for (ordinal, generic) in generics.iter_mut().enumerate() {
        if generic.binder.is_none() {
            generic.binder = Some(GenericBinderId { owner, ordinal: ordinal as u32 });
        }
    }
}

fn propagate_declared_generic_binders(ty: &mut ValkyrieType, generics: &[GenericType]) {
    match ty {
        // Oak 常把 unite/struct 字段里的泛型参数落成 `Named(T)`；必须回收为
        // 带 GenericBinderId 的 Generic，否则构造器参数无法与 `Option<i64>` 上下文统一。
        ValkyrieType::Named(name) => {
            if let Some(declared) = generics.iter().find(|item| item.name == *name) {
                *ty = ValkyrieType::Generic(declared.clone());
            }
        }
        ValkyrieType::Generic(generic) => {
            if generic.binder.is_none() {
                if let Some(declared) = generics.iter().find(|item| item.name == generic.name) {
                    generic.binder = declared.binder;
                    generic.bounds = declared.bounds.clone();
                }
            }
        }
        ValkyrieType::Nominal { arguments, .. } | ValkyrieType::Apply(_, arguments) | ValkyrieType::Tuple(arguments) => {
            for argument in arguments {
                propagate_declared_generic_binders(argument, generics);
            }
        }
        ValkyrieType::Array(inner) | ValkyrieType::Nullable(inner) | ValkyrieType::FixedArray { element: inner, .. } => {
            propagate_declared_generic_binders(inner, generics);
        }
        ValkyrieType::Union(arguments) | ValkyrieType::Intersection(arguments) => {
            for argument in arguments {
                propagate_declared_generic_binders(argument, generics);
            }
        }
        ValkyrieType::Function(function) => {
            for param in &mut function.params {
                propagate_declared_generic_binders(param, generics);
            }
            propagate_declared_generic_binders(&mut function.return_type, generics);
        }
        ValkyrieType::Associated(associated) => {
            propagate_declared_generic_binders(&mut associated.base, generics);
            for argument in &mut associated.type_arguments {
                propagate_declared_generic_binders(argument, generics);
            }
        }
        ValkyrieType::TraitObject(object) => {
            for argument in &mut object.type_arguments {
                propagate_declared_generic_binders(argument, generics);
            }
        }
        ValkyrieType::Row(row) => {
            for method in &mut row.methods {
                for param in &mut method.params {
                    propagate_declared_generic_binders(param, generics);
                }
                propagate_declared_generic_binders(&mut method.return_type, generics);
            }
        }
        ValkyrieType::TypeLambda(lambda) => propagate_declared_generic_binders(&mut lambda.body, generics),
        _ => {}
    }
}

fn propagate_generic_binders_in_block(block: &mut HirBlock, generics: &[GenericType]) {
    for statement in &mut block.statements {
        match &mut statement.kind {
            HirStatementKind::Let { ty, initializer, .. } => {
                if let Some(ty) = ty {
                    propagate_declared_generic_binders(ty, generics);
                }
                if let Some(initializer) = initializer {
                    propagate_generic_binders_in_expr(initializer, generics);
                }
            }
            HirStatementKind::Expr(expr) => propagate_generic_binders_in_expr(expr, generics),
        }
    }
    if let Some(expr) = &mut block.expr {
        propagate_generic_binders_in_expr(expr, generics);
    }
}

fn propagate_generic_binders_in_expr(expr: &mut HirExpr, generics: &[GenericType]) {
    match &mut expr.kind {
        HirExprKind::Cast { ty, expr: inner, .. } => {
            propagate_declared_generic_binders(ty, generics);
            propagate_generic_binders_in_expr(inner, generics);
        }
        HirExprKind::Lambda { params, return_type, body, .. } => {
            for param in params {
                propagate_declared_generic_binders(&mut param.ty, generics);
            }
            propagate_declared_generic_binders(return_type, generics);
            propagate_generic_binders_in_block(body, generics);
        }
        HirExprKind::TryScope { result_type, body, .. } => {
            if let Some(ty) = result_type {
                propagate_declared_generic_binders(ty, generics);
            }
            propagate_generic_binders_in_block(body, generics);
        }
        HirExprKind::Block(block) => propagate_generic_binders_in_block(block, generics),
        HirExprKind::If { condition, then_branch, else_branch, .. } => {
            propagate_generic_binders_in_expr(condition, generics);
            propagate_generic_binders_in_block(then_branch, generics);
            if let Some(branch) = else_branch {
                propagate_generic_binders_in_block(branch, generics);
            }
        }
        HirExprKind::Return(Some(value))
        | HirExprKind::Yield(Some(value))
        | HirExprKind::Await(value)
        | HirExprKind::Awake(value)
        | HirExprKind::BlockOn(value)
        | HirExprKind::YieldFrom(value)
        | HirExprKind::Raise(value)
        | HirExprKind::Resume(value) => propagate_generic_binders_in_expr(value, generics),
        HirExprKind::TryPropagate { inner, .. } => propagate_generic_binders_in_expr(inner, generics),
        HirExprKind::Call { callee, args, .. } => {
            propagate_generic_binders_in_expr(callee, generics);
            for arg in args {
                propagate_generic_binders_in_expr(&mut arg.value, generics);
            }
        }
        HirExprKind::Construct { args, .. } => {
            for arg in args {
                propagate_generic_binders_in_expr(arg, generics);
            }
        }
        HirExprKind::FieldAccess { object, .. } | HirExprKind::FieldInit { value: object, .. } => {
            propagate_generic_binders_in_expr(object, generics);
        }
        HirExprKind::StoreField { object, value, .. } => {
            propagate_generic_binders_in_expr(object, generics);
            propagate_generic_binders_in_expr(value, generics);
        }
        HirExprKind::GenericApply { callee, arguments, .. } => {
            propagate_generic_binders_in_expr(callee, generics);
            for argument in arguments {
                propagate_declared_generic_binders(argument, generics);
            }
        }
        HirExprKind::ArrayNew { length, .. } => propagate_generic_binders_in_expr(length, generics),
        HirExprKind::ArrayLiteral { items } | HirExprKind::TupleLiteral { items } => {
            for item in items {
                propagate_generic_binders_in_expr(item, generics);
            }
        }
        HirExprKind::IfLet { scrutinee, then_branch, else_branch, .. } => {
            propagate_generic_binders_in_expr(scrutinee, generics);
            propagate_generic_binders_in_block(then_branch, generics);
            if let Some(branch) = else_branch {
                propagate_generic_binders_in_block(branch, generics);
            }
        }
        HirExprKind::Match { scrutinee, arms, .. } | HirExprKind::Case { scrutinee, arms, .. } => {
            propagate_generic_binders_in_expr(scrutinee, generics);
            for arm in arms {
                if let Some(guard) = &mut arm.guard {
                    propagate_generic_binders_in_expr(guard, generics);
                }
                propagate_generic_binders_in_expr(&mut arm.body, generics);
            }
        }
        HirExprKind::Loop { iterator, body, condition, .. } => {
            if let Some(iterator) = iterator {
                propagate_generic_binders_in_expr(iterator, generics);
            }
            if let Some(condition) = condition {
                propagate_generic_binders_in_expr(condition, generics);
            }
            propagate_generic_binders_in_block(body, generics);
        }
        HirExprKind::Catch { expr, arms, .. } => {
            propagate_generic_binders_in_expr(expr, generics);
            for arm in arms {
                if let Some(guard) = &mut arm.guard {
                    propagate_generic_binders_in_expr(guard, generics);
                }
                propagate_generic_binders_in_expr(&mut arm.body, generics);
            }
        }
        HirExprKind::AnonymousClass { fields, .. } => {
            for (_, value) in fields {
                propagate_generic_binders_in_expr(value, generics);
            }
        }
        _ => {}
    }
}

fn freeze_function_generic_binders(function: &mut HirFunction) {
    let Some(owner) = function.declaration
    else {
        return;
    };
    freeze_generic_binders(&mut function.generics, owner);
    for param in &mut function.params {
        propagate_declared_generic_binders(&mut param.ty, &function.generics);
    }
    propagate_declared_generic_binders(&mut function.return_type, &function.generics);
    for constraint in &mut function.where_constraints {
        propagate_declared_generic_binders(&mut constraint.target, &function.generics);
        for bound in &mut constraint.bounds {
            for argument in &mut bound.type_arguments {
                propagate_declared_generic_binders(argument, &function.generics);
            }
            for equation in &mut bound.associated_types {
                propagate_declared_generic_binders(&mut equation.ty, &function.generics);
            }
        }
    }
    // `let empty: Option<T> = None` 等注解与返回路径必须共享同一 GenericBinderId。
    propagate_generic_binders_in_block(&mut function.body, &function.generics);
}

fn freeze_enum_generic_binders(enum_definition: &mut HirEnum) {
    let Some(owner) = enum_definition.declaration
    else {
        return;
    };
    freeze_generic_binders(&mut enum_definition.generics, owner);
    for variant in &mut enum_definition.variants {
        for field in &mut variant.fields {
            propagate_declared_generic_binders(&mut field.ty, &enum_definition.generics);
        }
        if let Some(result_type) = &mut variant.result_type {
            propagate_declared_generic_binders(result_type, &enum_definition.generics);
        }
    }
}

fn freeze_struct_generic_binders(structure: &mut HirStruct) {
    let Some(owner) = structure.constructor_declaration
    else {
        return;
    };
    freeze_generic_binders(&mut structure.generics, owner);
    for field in &mut structure.fields {
        propagate_declared_generic_binders(&mut field.ty, &structure.generics);
    }
    for method in &mut structure.methods {
        freeze_function_generic_binders(method);
    }
    for property in &mut structure.properties {
        if let Some(getter) = &mut property.getter {
            freeze_function_generic_binders(getter);
        }
        if let Some(setter) = &mut property.setter {
            freeze_function_generic_binders(setter);
        }
    }
}

fn freeze_trait_generic_binders(trait_definition: &mut HirTrait) {
    let Some(owner) = trait_definition.declaration
    else {
        return;
    };
    freeze_generic_binders(&mut trait_definition.generics, owner);
    for associated in &mut trait_definition.associated_types {
        freeze_generic_binders(&mut associated.type_params, owner);
        for bound in &mut associated.bounds {
            propagate_declared_generic_binders(bound, &trait_definition.generics);
        }
        if let Some(default) = &mut associated.default {
            propagate_declared_generic_binders(default, &trait_definition.generics);
        }
    }
    for method in trait_definition.methods.iter_mut().chain(trait_definition.default_methods.iter_mut()) {
        freeze_function_generic_binders(method);
    }
}

fn freeze_impl_generic_binders(implementation: &mut HirImpl) {
    propagate_declared_generic_binders(&mut implementation.target, &implementation.generics);
    for method in &mut implementation.methods {
        freeze_function_generic_binders(method);
    }
}

fn freeze_module_generic_binders(module: &mut HirModule) {
    for function in &mut module.functions {
        freeze_function_generic_binders(function);
    }
    for structure in &mut module.structs {
        freeze_struct_generic_binders(structure);
    }
    for enum_definition in &mut module.enums {
        freeze_enum_generic_binders(enum_definition);
    }
    for singleton in &mut module.singletons {
        for method in singleton
            .methods
            .iter_mut()
            .chain(singleton.constructor.iter_mut().map(Box::as_mut))
            .chain(singleton.finalizer.iter_mut().map(Box::as_mut))
        {
            freeze_function_generic_binders(method);
        }
    }
    for trait_definition in &mut module.traits {
        freeze_trait_generic_binders(trait_definition);
    }
    for implementation in &mut module.impls {
        freeze_impl_generic_binders(implementation);
    }
    for widget in &mut module.widgets {
        for method in &mut widget.methods {
            freeze_function_generic_binders(method);
        }
    }
    for submodule in &mut module.submodules {
        freeze_module_generic_binders(submodule);
    }
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
    pub fn lower_root(&self, root: &ValkyrieRoot) -> Result<HirModule, HirError> {
        self.lower_root_with_semantic_exports(root, &[])
    }

    /// Lowers a parser root with resolved nominal dependency exports in scope.
    pub fn lower_root_with_semantic_exports(
        &self,
        root: &ValkyrieRoot,
        imported_semantic_exports: &[HirDependencySemanticExport],
    ) -> Result<HirModule, HirError> {
        self.lower_root_with_semantic_exports_and_name(root, imported_semantic_exports, None)
    }

    /// 在 HIR 调用解析前覆盖模块身份；仅供完整 source closure 编译使用。
    pub fn lower_root_with_semantic_exports_and_name(
        &self,
        root: &ValkyrieRoot,
        imported_semantic_exports: &[HirDependencySemanticExport],
        module_name_override: Option<NamePath>,
    ) -> Result<HirModule, HirError> {
        let mut hir = self.lower_root_without_call_resolution(root, imported_semantic_exports, module_name_override)?;
        if !imported_semantic_exports.is_empty() {
            return Err(HirError::invalid("依赖声明注册必须由完整 Compiler 源码闭包拥有"));
        }
        register_function_declarations(&mut hir, &mut 0, &mut 0)?;
        register_field_declarations(&mut hir, &mut 0)?;
        freeze_module_generic_binders(&mut hir);
        bind_declared_nominal_types(&mut hir)?;
        trait_binding::bind_trait_declarations(&mut hir)?;
        resolve_hir_calls(&mut hir);
        validate_extractor_patterns(&hir)?;
        Ok(hir)
    }

    /// 仅物化 HIR；调用身份必须由完整源码组阶段统一解析。
    ///
    /// 单根入口自己拥有类型别名作用域（本根声明 + 可见依赖导出）。完整源码组编译走
    /// [`AstToHir::lower_prepared_root`]，作用域由那一层按组建立。
    pub(crate) fn lower_root_without_call_resolution(
        &self,
        root: &ValkyrieRoot,
        imported_semantic_exports: &[HirDependencySemanticExport],
        module_name_override: Option<NamePath>,
    ) -> Result<HirModule, HirError> {
        let _type_alias_scope = ModuleTypeAliasScope::enter(visible_type_aliases(std::slice::from_ref(&root), imported_semantic_exports));
        register_root_type_alias_targets(std::slice::from_ref(&root));
        self.lower_prepared_root(root, imported_semantic_exports, module_name_override)
    }

    /// 降低一个已完成宏/模板展开、且别名作用域已经建立的源码根。
    pub(crate) fn lower_prepared_root(
        &self,
        root: &ValkyrieRoot,
        imported_semantic_exports: &[HirDependencySemanticExport],
        module_name_override: Option<NamePath>,
    ) -> Result<HirModule, HirError> {
        validate_ast_root(root)?;
        let _warning_scope = CompileWarningScope::enter();
        let module_name = module_name_override.unwrap_or_else(|| {
            root.items
                .iter()
                .find_map(|item| match item {
                    StatementNode::Namespace(namespace) if !namespace.has_body() => Some(lower_name_path(&namespace.name)),
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

        let mut type_aliases = Vec::new();

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
                    if !namespace.has_body() {
                        current_namespace = lower_name_path(&namespace.name);
                    }
                    else if let Some(body) = &namespace.body {
                        let namespace_path = lower_name_path(&namespace.name);
                        self.lower_items(
                            &body.items,
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
                            &mut type_aliases,
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
                    &mut type_aliases,
                )?,
            }
        }

        let mut hir = HirModule {
            name: module_name,
            dependency_key: String::new(),
            source_path: String::new(),
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
            return Err(HirError::invalid(derive_result.errors.into_iter().map(|error| error.to_string()).collect::<Vec<_>>().join("; ")));
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
        type_aliases: &mut Vec<HirTypeAlias>,
    ) -> Result<(), HirError> {
        for item in items {
            self.lower_statement_node(
                item,
                namespace,
                functions,
                type_functions,
                structs,
                widgets,
                singletons,
                traits,
                enums,
                flags,
                impls,
                type_aliases,
            )?;
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
        type_aliases: &mut Vec<HirTypeAlias>,
    ) -> Result<(), HirError> {
        match item {
            StatementNode::Micro(micro) => {
                functions.push(self.lower_micro(micro, namespace));
                Ok(())
            }
            StatementNode::TypeFunction(type_function) => {
                type_functions.push(self.lower_type_function(type_function));
                Ok(())
            }
            StatementNode::TypeAlias(alias) => {
                type_aliases.push(self.lower_type_alias(alias));
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
            StatementNode::Union(union_decl) => Err(HirError::LoweringFailed {
                scenario: "named_union_not_lowered".into(),
                details: vec![("union_name".into(), union_decl.name.name.clone())],
                source: DiagnosticSource { id: "unknown".into(), revision: None },
                span: DiagnosticSpan { start: 0, end: 0 },
            }.into()),
            StatementNode::Using(_)
            | StatementNode::Namespace(_)
            | StatementNode::Let(_)
            | StatementNode::ExprStmt(_)
            | StatementNode::Statement(_) => Ok(()),
            StatementNode::Template(_) => Err(HirError::LoweringFailed {
                scenario: "unexpanded_tgrammar_template".into(),
                details: vec![],
                source: DiagnosticSource { id: "unknown".into(), revision: None },
                span: DiagnosticSpan { start: 0, end: 0 },
            }.into()),
            unsupported => Err(HirError::LoweringFailed {
                scenario: "unsupported_oak_root_item".into(),
                details: vec![("item".into(), format!("{unsupported:?}"))],
                source: DiagnosticSource { id: "unknown".into(), revision: None },
                span: DiagnosticSpan { start: 0, end: 0 },
            }.into()),
        }
    }

    fn lower_micro(&self, function: &MicroDeclaration, declaring_namespace: &NamePath) -> HirFunction {
        let span = frontend::std_range(&function.span);
        HirFunction {
            declaration: None,
            instance: None,
            name: operator_member_name(&function.name, &function.annotations),
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

    fn lower_type_alias(&self, alias: &TypeAliasDeclaration) -> HirTypeAlias {
        HirTypeAlias {
            name: Identifier::new(&alias.name.name),
            generics: Vec::new(),
            target: lower_type_expression(&alias.target),
            span: with_source(&frontend::std_range(&alias.span), self.source_id),
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
            accessor_instance: None,
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
            name: operator_member_name(&method.name, &method.annotations),
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
        id: None,
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
    // Oak 对 namespace library.console; 这类点分命名空间保留为单个语法片段，
    // 而调用点路径按段拆分。HIR 的 NamePath 以段为身份单位，因此在语法到语义
    // 边界统一按点号归一化，禁止让带分隔符的整串成为第二种名称形态。
    let parts = path
        .parts
        .iter()
        .flat_map(|part| part.name.split('.').filter(|segment| !segment.is_empty()))
        .map(Identifier::new)
        .collect();
    NamePath::new(parts)
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

/// 运算符声明（`infix \`+\`` / `prefix \`-\`` / `suffix \`[]\``）的 HIR 成员名。
///
/// 成员名固定为 `{fixity} {lexeme}`：`prefix \`-\`` 与 `infix \`-\`` 共享词素，
/// 只有组合名能让两者在普通成员解析与调用点匹配中区分开。HIR 只保留这个普通
/// 成员名，不保留 unary/binary/operator 语义类别、`OperatorId` 或 operator domain；
/// 调用点由 AST 运算符语法糖展开成同一名字的普通成员调用（见 `expr_lowering`）。
fn operator_member_name(declared: &frontend::ast::Identifier, annotations: &[Attribute]) -> Identifier {
    Identifier::new(&operator_members::declared_member_name(&declared.name, annotations))
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
