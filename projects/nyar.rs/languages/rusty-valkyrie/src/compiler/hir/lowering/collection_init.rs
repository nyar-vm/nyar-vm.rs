//! 集合字面量的 post-binding 转换 pass。
//!
//! `let a: ArrayList<u32> = [0, 1, 2]` 这类「nominal 声明 + 数组字面量初始值」的绑定
//! 由声明期望类型驱动构造：合同识别只认限定路径 `std::collection::LiteralSequence`，
//! 并要求 `imply` 声明身份与之匹配；槽位按签名形状分类（不按名字特判）。
//! 构造结果冻结为 `HirExprKind::CollectionInit` 的两槽调用合同——
//! 先 `with_length(n)` 预留容量，再逐项 `append`。
//! 两槽实例身份留空，由既有单态化固定点（`instantiate_generic_callables`）自然铸造；
//! 本 pass 必须跑在调用解析与单态化之前。
//! 无合同事实匹配的 nominal 声明 + 字面量在构造点结构化拒绝
//! （`HirError::CollectionLiteralContractMissing`），禁止先构造 `Array` 再隐式转换。

use std::collections::{BTreeMap, BTreeSet};

use nyar_types::{DiagnosticSource, DiagnosticSpan, ItemId};

use crate::compiler::{
    hir::{
        control_flow_validation::{boolean_declarations_in_module, display_type, numeric_declarations_in_module},
        lowering::{
            errors::HirError,
            generic_binders::{combined_impl_generic_context, propagate_declared_generic_binders},
            generic_instantiation::{substitute_type, unify_instantiation},
            trait_binding::trait_declaration_index,
        },
        overload::{has_unresolved_generic_arguments, owner_type_generic_binders},
        type_relation::is_numeric_type_name_of,
    },
    types::{
        Identifier, NamePath, SourceSpan,
        hir::{
            GenericBinderId, HirBlock, HirCallableDomain, HirExpr, HirExprKind, HirFunction, HirImpl, HirLiteral, HirModule, HirResolvedCall,
            HirStatementKind, ValkyrieType,
        },
    },
};

/// `GenericBinderId` 到具体类型的代入；与单态化固定点同一形状，透明复用。
type Substitution = BTreeMap<GenericBinderId, ValkyrieType>;

/// 合同识别的唯一限定路径：`std::collection::LiteralSequence`。
///
/// 短名 `LiteralSequence` 不参与识别；声明面必须落在该完整路径上。
fn contract_path() -> NamePath {
    NamePath::new(vec![Identifier::new("std"), Identifier::new("collection"), Identifier::new("LiteralSequence")])
}

/// 一个目标类型上识别出的 `LiteralSequence` 合同事实。
///
/// 两成员按签名形状分类冻结：`with_length` 是无 `self`、返回 `Self` 的构造槽，
/// `append` 是以 `self` 开头、恰好一个值参数、返回 `unit` 的元素槽。
/// 成员克隆自 impl，避免后续体遍历与事实收集借用冲突。
struct LiteralSequenceFacts {
    /// impl 目标类型（可含 owner binder，如 `Nominal{ArrayList, [Generic(T)]}`）。
    target: ValkyrieType,
    /// trait 实参（合同 `LiteralSequence<T>` 的有序类型参数）。
    trait_arguments: Vec<ValkyrieType>,
    /// `with_length(n)` 构造成员。
    with_length: HirFunction,
    /// `append` 元素成员。
    append: HirFunction,
}

/// 声明签名是否以 `self` 参数开头；与调用合同同一规范口径。
fn has_self_receiver(function: &HirFunction) -> bool {
    function.params.first().is_some_and(|param| param.name.name.as_str() == "self")
}

/// 按签名形状收集一个 impl 的合同成员；形状不齐（缺槽或同形状多候选）即弃该 impl。
fn contract_members(implementation: &HirImpl) -> Option<(HirFunction, HirFunction)> {
    let mut constructor: Option<HirFunction> = None;
    let mut append: Option<HirFunction> = None;
    for method in &implementation.methods {
        if !has_self_receiver(method) {
            // 构造槽：无 `self`，返回 `Self`。
            if method.return_type == ValkyrieType::SelfType {
                if constructor.is_some() {
                    return None;
                }
                constructor = Some(method.clone());
            }
        }
        else if method.params.len() == 2 && method.return_type == ValkyrieType::Unit {
            // 元素槽：以 `self` 开头、恰好一个值参数、返回 `unit`。
            if append.is_some() {
                return None;
            }
            append = Some(method.clone());
        }
    }
    Some((constructor?, append?))
}

/// 收集模块（含依赖导出与子模块）内绑定到合同 trait 的全部事实。
///
/// 口径与 `trait_declaration_index` 的声明面一致：本组 impl + 依赖导出 impl + 子模块递归；
/// inherent impl（`trait_declaration == None`）与别的目标 impl 自然被身份过滤。
fn collect_contract_facts(module: &HirModule, contract: ItemId) -> Vec<LiteralSequenceFacts> {
    fn facts_for(root: &HirModule, implementation: &HirImpl, contract: ItemId) -> Option<LiteralSequenceFacts> {
        if implementation.trait_declaration != Some(contract) {
            return None;
        }
        let (with_length, append) = contract_members(implementation)?;
        // `freeze_impl_generic_binders` 只向 impl target 传播 binder，从不处理 `trait_arguments`；
        // 这里用与 owner 泛型同一口径的 combined context 补齐传播，
        // 否则元素类型残留未绑定泛型（`T[None]`），字面量项与代入结果对不上。
        let context = combined_impl_generic_context(root, implementation);
        let mut trait_arguments = implementation.trait_arguments.clone();
        for argument in &mut trait_arguments {
            propagate_declared_generic_binders(argument, &context);
        }
        Some(LiteralSequenceFacts { target: implementation.target.clone(), trait_arguments, with_length, append })
    }
    fn collect(root: &HirModule, module: &HirModule, contract: ItemId, out: &mut Vec<LiteralSequenceFacts>) {
        for implementation in &module.impls {
            if let Some(fact) = facts_for(root, implementation, contract) {
                out.push(fact);
            }
        }
        for export in &module.imported_semantic_exports {
            for implementation in &export.impls {
                if let Some(fact) = facts_for(root, implementation, contract) {
                    out.push(fact);
                }
            }
        }
        for child in &module.submodules {
            collect(root, child, contract, out);
        }
    }
    let mut out = Vec::new();
    collect(module, module, contract, &mut out);
    out
}

/// 按声明类型匹配合同事实：名义声明相等、目标代入闭合、trait 实参无未闭合泛型。
///
/// 多个事实按收集序取首个——同一目标类型上重复合同 impl 属声明面歧义，
/// 由既有 impl 一致性门禁负责拒绝，本 pass 不重做仲裁。
fn match_container<'a>(facts: &'a [LiteralSequenceFacts], expected: &ValkyrieType) -> Option<(&'a LiteralSequenceFacts, Substitution)> {
    let expected_declaration = expected.nominal_declaration()?;
    for fact in facts {
        let Some(target_declaration) = fact.target.nominal_declaration()
        else {
            continue;
        };
        if target_declaration != expected_declaration {
            continue;
        }
        let mut substitution = Substitution::new();
        if !unify_instantiation(&fact.target, expected, &mut substitution) {
            continue;
        }
        if owner_type_generic_binders(&fact.target).iter().any(|binder| !substitution.contains_key(binder)) {
            continue;
        }
        if substitution.values().any(has_unresolved_generic_arguments) {
            continue;
        }
        return Some((fact, substitution));
    }
    None
}

/// impl 成员的限定调用符号：声明作用域 + 成员名。
fn callable_symbol(function: &HirFunction) -> NamePath {
    let mut parts = function.declaring_namespace.parts().to_vec();
    parts.push(function.name.clone());
    NamePath::new(parts)
}

/// 合同缺失的结构化拒绝点。
fn contract_missing(expected: &ValkyrieType, span: &SourceSpan, source: &DiagnosticSource) -> HirError {
    HirError::CollectionLiteralContractMissing {
        type_label: display_type(expected),
        source: source.clone(),
        span: DiagnosticSpan { start: span.get_start() as usize, end: span.get_end() as usize },
    }
}

/// 字面量项与期望元素类型的口径不符时拒绝；标签与 `let` 初始化类型诊断同源。
fn literal_item_mismatch(literal: &HirLiteral, element_type: &ValkyrieType, span: &SourceSpan, source: &DiagnosticSource) -> HirError {
    let actual = match literal {
        HirLiteral::Integer64(_) => display_type(&ValkyrieType::Integer64 { signed: true }),
        HirLiteral::Float64(_) => display_type(&ValkyrieType::Float64),
        HirLiteral::Bool(_) => display_type(&ValkyrieType::Boolean),
        other => display_type(&infer_literal_type(other)),
    };
    HirError::LetInitializerTypeMismatch {
        actual,
        expected: display_type(element_type),
        source: source.clone(),
        span: DiagnosticSpan { start: span.get_start() as usize, end: span.get_end() as usize },
    }
}

/// 字面量的静态形态标签，仅用于诊断文本。
fn infer_literal_type(literal: &HirLiteral) -> ValkyrieType {
    match literal {
        HirLiteral::String(_) => ValkyrieType::Utf8,
        HirLiteral::Null | HirLiteral::Unit => ValkyrieType::Unit,
        _ => ValkyrieType::AutoType,
    }
}

/// 数值口径：`Nominal` 查模块数值声明集，其余走 `type_relation` 名称判定。
///
/// 与 `integer_literal_binds_to` 完全同源，禁止本 pass 另立名单。
fn numeric_element_accepts(element_type: &ValkyrieType, numeric: &BTreeSet<ItemId>) -> bool {
    match element_type {
        ValkyrieType::Nominal { declaration, .. } => numeric.contains(declaration),
        other => is_numeric_type_name_of(other),
    }
}

/// 布尔口径：`Nominal` 查模块 `bool` 声明集，其余只认 `Boolean` 与表面名 `bool`。
fn boolean_element_accepts(element_type: &ValkyrieType, boolean: &BTreeSet<ItemId>) -> bool {
    match element_type {
        ValkyrieType::Nominal { declaration, .. } => boolean.contains(declaration),
        ValkyrieType::Boolean => true,
        ValkyrieType::Named(name) => name.as_str() == "bool",
        _ => false,
    }
}

/// 遍历上下文：合同事实、字面量口径集、诊断源码身份，以及当前函数的声明返回类型。
///
/// `return_type` 用于在 `return [...]` 与函数体尾表达式处按声明期望类型驱动构造；
/// 嵌套 lambda 不继承外层返回类型，其 `return` 不使用该期望。
struct TransformContext<'a> {
    facts: &'a [LiteralSequenceFacts],
    numeric: &'a BTreeSet<ItemId>,
    boolean: &'a BTreeSet<ItemId>,
    source: DiagnosticSource,
    return_type: Option<ValkyrieType>,
}

/// 把一个 `ArrayLiteral` 的项按期望容器类型构造为 `CollectionInit`。
///
/// 两槽只冻结声明身份与调用合同（`instance` 留空，交单态化固定点铸造）；
/// 嵌套字面量按元素类型递归构造，非字面量项原样保留并继续下钻。
fn build_collection_init(
    expected: &ValkyrieType,
    items: Vec<HirExpr>,
    fact: &LiteralSequenceFacts,
    substitution: &Substitution,
    span: &SourceSpan,
    context: &TransformContext<'_>,
) -> Result<HirExpr, HirError> {
    if fact.trait_arguments.len() != 1 {
        return Err(contract_missing(expected, span, &context.source));
    }
    let element_type = substitute_type(&fact.trait_arguments[0], substitution);
    let with_length = HirResolvedCall {
        declaration: fact.with_length.declaration,
        instance: None,
        intrinsic: None,
        trait_dispatch: None,
        sum_owner: None,
        sum_variant: None,
        symbol: callable_symbol(&fact.with_length),
        domain: HirCallableDomain::Function,
        return_type: expected.clone(),
        parameter_types: fact.with_length.params.iter().map(|param| substitute_type(&param.ty, substitution)).collect(),
        has_receiver: has_self_receiver(&fact.with_length),
        extractor_payload_type: None,
    };
    let append = HirResolvedCall {
        declaration: fact.append.declaration,
        instance: None,
        intrinsic: None,
        trait_dispatch: None,
        sum_owner: None,
        sum_variant: None,
        symbol: callable_symbol(&fact.append),
        domain: HirCallableDomain::Function,
        return_type: ValkyrieType::Unit,
        parameter_types: vec![expected.clone(), element_type.clone()],
        has_receiver: true,
        extractor_payload_type: None,
    };
    let mut built_items = Vec::with_capacity(items.len());
    for item in items {
        let item_span = item.span.clone();
        match item.kind {
            HirExprKind::ArrayLiteral { items: nested } => {
                let (nested_fact, nested_substitution) = match_container(context.facts, &element_type)
                    .ok_or_else(|| contract_missing(&element_type, &item_span, &context.source))?;
                built_items.push(build_collection_init(&element_type, nested, nested_fact, &nested_substitution, &item_span, context)?);
            }
            kind @ (HirExprKind::Literal(HirLiteral::Integer64(_)) | HirExprKind::Literal(HirLiteral::Float64(_))) => {
                if !numeric_element_accepts(&element_type, context.numeric) {
                    let HirExprKind::Literal(literal) = &kind
                    else {
                        unreachable!()
                    };
                    return Err(literal_item_mismatch(literal, &element_type, &item_span, &context.source));
                }
                built_items.push(HirExpr { kind, span: item_span });
            }
            kind @ HirExprKind::Literal(HirLiteral::Bool(_)) => {
                if !boolean_element_accepts(&element_type, context.boolean) {
                    let HirExprKind::Literal(literal) = &kind
                    else {
                        unreachable!()
                    };
                    return Err(literal_item_mismatch(literal, &element_type, &item_span, &context.source));
                }
                built_items.push(HirExpr { kind, span: item_span });
            }
            // `String`/`Null`/`Unit` 与非字面量项原样保留，继续下钻嵌套绑定。
            other => {
                let mut expr = HirExpr { kind: other, span: item_span };
                transform_expr(&mut expr, context)?;
                built_items.push(expr);
            }
        }
    }
    Ok(HirExpr {
        kind: HirExprKind::CollectionInit {
            container_type: expected.clone(),
            element_type,
            with_length: Box::new(with_length),
            append: Box::new(append),
            items: built_items,
        },
        span: span.clone(),
    })
}

/// 把「期望名义容器类型 + 数组字面量」在构造点按 `LiteralSequence` 合同冻结为 `CollectionInit`。
///
/// 无合同事实时结构化拒绝，禁止退化为 `Array` 隐式转换。调用方负责确认期望类型是名义类型。
fn transform_nominal_literal(expr: &mut HirExpr, expected: &ValkyrieType, context: &TransformContext<'_>) -> Result<(), HirError> {
    let span = expr.span.clone();
    let items = match std::mem::replace(&mut expr.kind, HirExprKind::ArrayLiteral { items: Vec::new() }) {
        HirExprKind::ArrayLiteral { items } => items,
        other => {
            expr.kind = other;
            return transform_expr(expr, context);
        }
    };
    let Some((fact, substitution)) = match_container(context.facts, expected)
    else {
        // 无合同事实的 nominal 声明 + 字面量：在构造点结构化拒绝，禁止退化为 Array 隐式转换。
        return Err(contract_missing(expected, &span, &context.source));
    };
    expr.kind = build_collection_init(expected, items, fact, &substitution, &span, context)?.kind;
    Ok(())
}

/// 按上下文期望类型处理表达式：期望名义容器 + 数组字面量走合同构造，其余原样下钻。
fn transform_expected(expr: &mut HirExpr, expected: Option<&ValkyrieType>, context: &TransformContext<'_>) -> Result<(), HirError> {
    let Some(expected) = expected
    else {
        return transform_expr(expr, context);
    };
    if !matches!(expr.kind, HirExprKind::ArrayLiteral { .. }) || expected.nominal_declaration().is_none() {
        return transform_expr(expr, context);
    }
    transform_nominal_literal(expr, expected, context)
}

/// 转换一个 `let` 初始值：nominal 声明 + `ArrayLiteral` 即合同驱动构造点。
fn transform_initializer(initializer: &mut Box<HirExpr>, declared: &ValkyrieType, context: &TransformContext<'_>) -> Result<(), HirError> {
    let HirExprKind::ArrayLiteral { .. } = initializer.kind
    else {
        return transform_expr(initializer, context);
    };
    if declared.nominal_declaration().is_none() {
        return transform_expr(initializer, context);
    }
    transform_nominal_literal(initializer, declared, context)
}

fn transform_block(block: &mut HirBlock, tail_expected: Option<&ValkyrieType>, context: &TransformContext<'_>) -> Result<(), HirError> {
    for statement in &mut block.statements {
        match &mut statement.kind {
            HirStatementKind::Let { ty, initializer, .. } => {
                if let (Some(declared), Some(initializer)) = (ty.as_ref(), initializer.as_mut()) {
                    transform_initializer(initializer, declared, context)?;
                }
                else if let Some(initializer) = initializer.as_mut() {
                    transform_expr(initializer, context)?;
                }
            }
            HirStatementKind::Expr(expr) => transform_expr(expr, context)?,
        }
    }
    if let Some(expr) = &mut block.expr {
        transform_expected(expr, tail_expected, context)?;
    }
    Ok(())
}

fn transform_function(function: &mut HirFunction, context: &TransformContext<'_>) -> Result<(), HirError> {
    // 函数体尾表达式与 `return` 共享同一声明返回类型；嵌套 lambda 另行清空期望。
    let context = TransformContext {
        facts: context.facts,
        numeric: context.numeric,
        boolean: context.boolean,
        source: context.source.clone(),
        return_type: Some(function.return_type.clone()),
    };
    transform_block(&mut function.body, context.return_type.as_ref(), &context)
}

/// 表达式复合结构下钻；分支面与单态化固定点的 `visit_expr` 镜像。
fn transform_expr(expr: &mut HirExpr, context: &TransformContext<'_>) -> Result<(), HirError> {
    match &mut expr.kind {
        HirExprKind::Call { callee, args, .. } => {
            transform_expr(callee, context)?;
            for arg in args {
                transform_expr(&mut arg.value, context)?;
            }
        }
        HirExprKind::Construct { args, .. } => {
            for arg in args {
                transform_expr(arg, context)?;
            }
        }
        HirExprKind::Cast { expr: inner, .. }
        | HirExprKind::FieldAccess { object: inner, .. }
        | HirExprKind::FieldInit { value: inner, .. }
        | HirExprKind::Await(inner)
        | HirExprKind::Awake(inner)
        | HirExprKind::BlockOn(inner)
        | HirExprKind::YieldFrom(inner)
        | HirExprKind::Raise(inner)
        | HirExprKind::Resume(inner)
        | HirExprKind::Assign { value: inner, .. }
        | HirExprKind::Yield(Some(inner))
        | HirExprKind::Break { expr: Some(inner), .. } => transform_expr(inner, context)?,
        // `return [...]` 按声明返回类型驱动构造；与函数体尾表达式同一口径。
        HirExprKind::Return(Some(inner)) => transform_expected(inner, context.return_type.as_ref(), context)?,
        HirExprKind::TryPropagate { inner, .. } => transform_expr(inner, context)?,
        HirExprKind::StoreField { object, value, .. } => {
            transform_expr(object, context)?;
            transform_expr(value, context)?;
        }
        HirExprKind::GenericApply { callee, .. } => transform_expr(callee, context)?,
        HirExprKind::ArrayNew { length, .. } => transform_expr(length, context)?,
        HirExprKind::ArrayLiteral { items } | HirExprKind::TupleLiteral { items } => {
            for item in items {
                transform_expr(item, context)?;
            }
        }
        // 本 pass 产物：两槽声明身份已冻结，实例待固定点铸造；项已在构造时下钻。
        HirExprKind::CollectionInit { .. } => {}
        HirExprKind::Block(block) | HirExprKind::TryScope { body: block, .. } => transform_block(block, None, context)?,
        // lambda 的 `return` 不属于外层函数返回类型：清空期望，避免把外层容器类型套进闭包。
        HirExprKind::Lambda { body, .. } => {
            let inner = TransformContext {
                facts: context.facts,
                numeric: context.numeric,
                boolean: context.boolean,
                source: context.source.clone(),
                return_type: None,
            };
            transform_block(body, None, &inner)?;
        }
        HirExprKind::If { condition, then_branch, else_branch, .. } => {
            transform_expr(condition, context)?;
            transform_block(then_branch, None, context)?;
            if let Some(branch) = else_branch {
                transform_block(branch, None, context)?;
            }
        }
        HirExprKind::IfLet { scrutinee, then_branch, else_branch, .. } => {
            transform_expr(scrutinee, context)?;
            transform_block(then_branch, None, context)?;
            if let Some(branch) = else_branch {
                transform_block(branch, None, context)?;
            }
        }
        HirExprKind::Match { scrutinee, arms, .. } | HirExprKind::Case { scrutinee, arms, .. } => {
            transform_expr(scrutinee, context)?;
            for arm in arms {
                if let Some(guard) = &mut arm.guard {
                    transform_expr(guard, context)?;
                }
                transform_expr(&mut arm.body, context)?;
            }
        }
        HirExprKind::Loop { iterator, condition, body, .. } => {
            if let Some(iterator) = iterator {
                transform_expr(iterator, context)?;
            }
            if let Some(condition) = condition {
                transform_expr(condition, context)?;
            }
            transform_block(body, None, context)?;
        }
        HirExprKind::Catch { expr: inner, arms } => {
            transform_expr(inner, context)?;
            for arm in arms {
                if let Some(guard) = &mut arm.guard {
                    transform_expr(guard, context)?;
                }
                transform_expr(&mut arm.body, context)?;
            }
        }
        HirExprKind::With { base, updates } => {
            transform_expr(base, context)?;
            for (_, update) in updates {
                transform_expr(update, context)?;
            }
        }
        HirExprKind::SuperCall { args, .. } => {
            for arg in args {
                transform_expr(arg, context)?;
            }
        }
        HirExprKind::AnonymousClass { fields, methods, .. } => {
            for (_, value) in fields {
                transform_expr(value, context)?;
            }
            for method in methods {
                transform_function(method, context)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// 遍历模块（含子模块）全部 callable 体内的字面量绑定；面与 `visit_module_calls` 镜像。
fn transform_module(module: &mut HirModule, context: &TransformContext<'_>) -> Result<(), HirError> {
    for function in &mut module.functions {
        transform_function(function, context)?;
    }
    for structure in &mut module.structs {
        for method in &mut structure.methods {
            transform_function(method, context)?;
        }
        for property in &mut structure.properties {
            for accessor in property.getter.iter_mut().chain(property.setter.iter_mut()) {
                transform_function(accessor, context)?;
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
            transform_function(method, context)?;
        }
    }
    for trait_definition in &mut module.traits {
        for method in trait_definition.methods.iter_mut().chain(trait_definition.default_methods.iter_mut()) {
            transform_function(method, context)?;
        }
    }
    for implementation in &mut module.impls {
        for method in &mut implementation.methods {
            transform_function(method, context)?;
        }
    }
    for widget in &mut module.widgets {
        for method in &mut widget.methods {
            transform_function(method, context)?;
        }
    }
    for submodule in &mut module.submodules {
        transform_module(submodule, context)?;
    }
    Ok(())
}

/// 集合字面量 post-binding 转换入口。
///
/// 合同识别只认限定路径 `std::collection::LiteralSequence` 且声明身份唯一；
/// 路径缺失或歧义时合同事实为空集——此时 nominal 声明 + 字面量仍在构造点
/// 结构化拒绝，其余字面量（`Array`/`[T]` 声明、`auto` 推断）保持 `ArrayLiteral` 原样。
pub(crate) fn transform_collection_literals(module: &mut HirModule) -> Result<(), HirError> {
    let index = trait_declaration_index(module)?;
    let facts = match index.get(&contract_path()) {
        Some(entries) if entries.len() == 1 => collect_contract_facts(module, entries[0].0),
        _ => Vec::new(),
    };
    let numeric = numeric_declarations_in_module(module);
    let boolean = boolean_declarations_in_module(module);
    let source = if module.source_path.trim().is_empty() {
        DiagnosticSource { id: "<hir_module>".into(), revision: None }
    }
    else {
        DiagnosticSource { id: module.source_path.clone(), revision: None }
    };
    let context = TransformContext { facts: &facts, numeric: &numeric, boolean: &boolean, source, return_type: None };
    transform_module(module, &context)
}
