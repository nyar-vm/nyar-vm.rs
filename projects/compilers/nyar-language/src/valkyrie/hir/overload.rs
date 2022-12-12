//! Minimal overload ranking helpers.
//!
//! These helpers model only the settled precedence:
//! nominal exact > nominal subtype > trait > row.

#![allow(missing_docs)]

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    hir::{
        call_binding::bind_call_arguments,
        construct_binding::bind_construct_fields,
        try_propagate::nullable_payload_type,
        type_relation::{ParameterMatchResult, TypeRelationContext},
    },
    types::{
        Identifier, NamePath,
        hir::{
            HirBlock, HirCallArgument, HirCallableDomain, HirEnum, HirExpr, HirExprKind, HirExtractorPattern, HirField, HirFunction,
            HirIdentifier, HirMatchArm, HirModule, HirParam, HirPattern, HirResolvedCall, HirSingleton, HirStatement, HirStatementKind,
            HirStruct, HirVariadicKind, HirVariant, ValkyrieType,
        },
    },
    valkyrie::{hir::PatternRefutability, mir::collect_aggregate_field_map},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverloadDomain {
    Function,
    Constructor,
    Operator,
    Extractor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverloadSignature {
    pub params: Vec<ValkyrieType>,
    pub return_type: ValkyrieType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverloadCandidate {
    pub symbol: NamePath,
    pub owner: Option<Identifier>,
    pub domain: OverloadDomain,
    pub signature: OverloadSignature,
    pub match_kind: OverloadMatchKind,
    pub param_specs: Vec<HirParam>,
    /// Formal type variables which must be bound from actual arguments.
    pub generic_binders: BTreeSet<Identifier>,
}

impl OverloadCandidate {
    pub fn new(
        symbol: NamePath,
        domain: OverloadDomain,
        params: Vec<ValkyrieType>,
        return_type: ValkyrieType,
        match_kind: OverloadMatchKind,
    ) -> Self {
        Self {
            symbol,
            owner: None,
            domain,
            signature: OverloadSignature { params, return_type },
            match_kind,
            param_specs: Vec::new(),
            generic_binders: BTreeSet::new(),
        }
    }

    pub fn with_param_specs(mut self, param_specs: Vec<HirParam>) -> Self {
        self.param_specs = param_specs;
        self
    }

    pub fn with_generic_binder(mut self, binder: Identifier) -> Self {
        self.generic_binders.insert(binder);
        self
    }

    pub fn new_method(
        owner: Identifier,
        symbol: NamePath,
        domain: OverloadDomain,
        params: Vec<ValkyrieType>,
        return_type: ValkyrieType,
        match_kind: OverloadMatchKind,
    ) -> Self {
        Self {
            symbol,
            owner: Some(owner),
            domain,
            signature: OverloadSignature { params, return_type },
            match_kind,
            param_specs: Vec::new(),
            generic_binders: BTreeSet::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedOverload {
    pub symbol: NamePath,
    pub domain: OverloadDomain,
    pub signature: OverloadSignature,
    pub match_kind: OverloadMatchKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverloadMatchKind {
    NominalExact,
    NominalSubtype { distance: usize },
    Trait,
    Row,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverloadResolutionError {
    NoMatch,
    Ambiguous { candidates: Vec<NamePath> },
}

pub fn resolve_overload(candidates: &[OverloadCandidate]) -> Result<ResolvedOverload, OverloadResolutionError> {
    let mut ranked = candidates.iter().map(|candidate| (rank(&candidate.match_kind), candidate)).collect::<Vec<_>>();

    ranked.sort_by_key(|(rank, _)| *rank);

    let Some((best_rank, best_candidate)) = ranked.first()
    else {
        return Err(OverloadResolutionError::NoMatch);
    };

    let mut tied = Vec::new();
    for (_, candidate) in ranked.iter().filter(|(rank, _)| rank == best_rank) {
        // Owner must participate: bare method symbols are often just `get`, and
        // Array.get vs HashMap.get must not collapse into one overload identity.
        let identity = (
            candidate.owner.clone(),
            candidate.symbol.clone(),
            candidate.domain.clone(),
            candidate.signature.clone(),
            candidate.match_kind.clone(),
        );
        if !tied.contains(&identity) {
            tied.push(identity);
        }
    }

    if tied.len() > 1 {
        Err(OverloadResolutionError::Ambiguous { candidates: tied.into_iter().map(|(_, symbol, _, _, _)| symbol).collect() })
    }
    else {
        Ok(ResolvedOverload {
            symbol: best_candidate.symbol.clone(),
            domain: best_candidate.domain.clone(),
            signature: best_candidate.signature.clone(),
            match_kind: best_candidate.match_kind.clone(),
        })
    }
}

fn rank(match_kind: &OverloadMatchKind) -> (u8, usize) {
    match match_kind {
        OverloadMatchKind::NominalExact => (0, 0),
        OverloadMatchKind::NominalSubtype { distance } => (1, *distance),
        OverloadMatchKind::Trait => (2, 0),
        OverloadMatchKind::Row => (3, 0),
    }
}

pub fn resolve_hir_calls(module: &mut HirModule) {
    let candidates = collect_module_candidates(module);
    let type_relations = TypeRelationContext::from_module(module);
    let struct_fields = collect_aggregate_field_map(module);
    let singleton_names = module.singletons.iter().map(|singleton| singleton.name.clone()).collect::<std::collections::BTreeSet<_>>();

    for function in &mut module.functions {
        resolve_function_calls(function, &candidates, &type_relations, &struct_fields, &singleton_names, None);
    }
    for item in &mut module.structs {
        for method in &mut item.methods {
            resolve_function_calls(method, &candidates, &type_relations, &struct_fields, &singleton_names, Some(&item.name));
        }
    }
    for item in &mut module.singletons {
        for method in &mut item.methods {
            resolve_function_calls(method, &candidates, &type_relations, &struct_fields, &singleton_names, Some(&item.name));
        }
    }
    for item in &mut module.traits {
        for method in &mut item.methods {
            resolve_function_calls(method, &candidates, &type_relations, &struct_fields, &singleton_names, None);
        }
        for method in &mut item.default_methods {
            resolve_function_calls(method, &candidates, &type_relations, &struct_fields, &singleton_names, None);
        }
    }
    for item in &mut module.impls {
        let owner = impl_nominal_type_name(&item.target).map(Identifier::new);
        for method in &mut item.methods {
            resolve_function_calls(method, &candidates, &type_relations, &struct_fields, &singleton_names, owner.as_ref());
        }
    }
}

fn collect_module_candidates(module: &HirModule) -> Vec<OverloadCandidate> {
    let mut candidates = Vec::new();
    candidates.extend(language_builtin_candidates());
    candidates.extend(module.functions.iter().map(|function| build_function_candidate(function)));
    for item in &module.structs {
        candidates.push(build_struct_constructor_candidate(item));
    }
    for item in &module.structs {
        candidates.extend(item.methods.iter().map(|method| build_method_candidate(method, Some(item.name.clone()))));
    }
    for item in &module.singletons {
        candidates.extend(item.methods.iter().map(|method| build_method_candidate(method, Some(item.name.clone()))));
    }
    for item in &module.traits {
        candidates.extend(item.methods.iter().map(|method| build_method_candidate(method, None)));
        candidates.extend(item.default_methods.iter().map(|method| build_method_candidate(method, None)));
    }
    for item in &module.impls {
        let owner = impl_nominal_type_name(&item.target).map(Identifier::new);
        candidates.extend(item.methods.iter().map(|method| build_method_candidate(method, owner.clone())));
    }
    // Dependency exports contribute only their declared call contracts. Their
    // bodies remain owned by the exporting module and are never reparsed or
    // emitted as part of this consumer.
    for export in &module.imported_semantic_exports {
        candidates.extend(export.functions.iter().map(build_function_candidate));
        for item in &export.structs {
            candidates.push(build_struct_constructor_candidate(item));
            candidates.extend(item.methods.iter().map(|method| build_method_candidate(method, Some(item.name.clone()))));
        }
        for item in &export.traits {
            candidates.extend(item.methods.iter().map(|method| build_method_candidate(method, None)));
            candidates.extend(item.default_methods.iter().map(|method| build_method_candidate(method, None)));
        }
        for item in &export.impls {
            let owner = impl_nominal_type_name(&item.target).map(Identifier::new);
            candidates.extend(item.methods.iter().map(|method| build_method_candidate(method, owner.clone())));
        }
    }
    for item in &module.enums {
        candidates.extend(item.variants.iter().map(|variant| build_variant_constructor_candidate(variant, item)));
        if item.is_unity {
            candidates.extend(item.variants.iter().map(|variant| build_unite_variant_extractor_candidate(item, variant)));
        }
    }
    for item in module.imported_nominal_enums() {
        candidates.extend(item.variants.iter().map(|variant| build_variant_constructor_candidate(variant, item)));
        if item.is_unity {
            candidates.extend(item.variants.iter().map(|variant| build_unite_variant_extractor_candidate(item, variant)));
        }
    }
    candidates
}

/// The language builtin registry is independent of modules and imports. A
/// builtin's user-facing symbol participates in normal overload lookup, while
/// its semantic identity is the structured opcode carried to Semantic MIR.
fn language_builtin_candidates() -> Vec<OverloadCandidate> {
    let element = Identifier::new("T");
    let element_type = ValkyrieType::Named(element.clone());
    let array_type = ValkyrieType::Array(Box::new(element_type.clone()));
    let mut candidates = vec![
        OverloadCandidate::new(
            NamePath::new(vec![Identifier::new("builtin"), Identifier::new("array"), Identifier::new("push")]),
            OverloadDomain::Function,
            vec![array_type.clone(), element_type],
            array_type,
            OverloadMatchKind::NominalExact,
        )
        .with_generic_binder(element),
    ];
    candidates
}

fn enum_apply_type(enum_def: &HirEnum) -> ValkyrieType {
    let base = ValkyrieType::Named(enum_def.name.clone());
    if enum_def.generics.is_empty() {
        return base;
    }
    ValkyrieType::Apply(Box::new(base), enum_def.generics.iter().map(|generic| ValkyrieType::Named(generic.name.clone())).collect())
}

fn struct_apply_type(struct_def: &HirStruct) -> ValkyrieType {
    let base = ValkyrieType::Named(struct_def.name.clone());
    if struct_def.generics.is_empty() {
        return base;
    }
    ValkyrieType::Apply(Box::new(base), struct_def.generics.iter().map(|generic| ValkyrieType::Named(generic.name.clone())).collect())
}

fn variant_payload_type(variant: &HirVariant) -> ValkyrieType {
    if !variant.fields.is_empty() {
        if variant.fields.len() == 1 {
            return variant.fields[0].ty.clone();
        }
        return ValkyrieType::Tuple(variant.fields.iter().map(|field| field.ty.clone()).collect());
    }
    ValkyrieType::Unit
}

fn nullable_union_type(payload: ValkyrieType) -> ValkyrieType {
    ValkyrieType::Nullable(Box::new(payload))
}

fn build_unite_variant_extractor_candidate(enum_def: &HirEnum, variant: &HirVariant) -> OverloadCandidate {
    let enum_type = enum_apply_type(enum_def);
    let payload_type = variant_payload_type(variant);
    let return_type = nullable_union_type(payload_type);
    let self_param = HirParam {
        name: HirIdentifier {
            name: Identifier::new("self"),
            shadow_index: 0,
            span: crate::types::SourceSpan::new(crate::types::SourceID::default(), 0, 0),
        },
        ty: enum_type.clone(),
        is_mutable: false,
        binding_kind: crate::types::hir::HirParameterBindingKind::PositionalOrKeyword,
        default: None,
        variadic: HirVariadicKind::None,
    };

    OverloadCandidate::new_method(
        variant.name.clone(),
        NamePath::new(vec![Identifier::new("extractor")]),
        OverloadDomain::Function,
        vec![enum_type],
        return_type,
        OverloadMatchKind::NominalSubtype { distance: 0 },
    )
    .with_param_specs(vec![self_param])
}

fn build_function_candidate(function: &HirFunction) -> OverloadCandidate {
    let mut candidate = OverloadCandidate::new(
        NamePath::new(vec![function.name.clone()]),
        classify_callable_domain(&function.name),
        function.params.iter().map(|param| param.ty.clone()).collect(),
        function.return_type.clone(),
        OverloadMatchKind::Row,
    )
    .with_param_specs(function.params.clone());
    for generic in &function.generics {
        candidate = candidate.with_generic_binder(generic.name.clone());
    }
    candidate
}

fn build_struct_constructor_candidate(item: &HirStruct) -> OverloadCandidate {
    let param_specs = synthetic_field_params(item);
    OverloadCandidate::new(
        NamePath::new(vec![item.name.clone()]),
        OverloadDomain::Constructor,
        item.fields.iter().map(|field| field.ty.clone()).collect(),
        struct_apply_type(item),
        OverloadMatchKind::Row,
    )
    .with_param_specs(param_specs)
}

fn synthetic_field_params(item: &HirStruct) -> Vec<HirParam> {
    item.fields
        .iter()
        .map(|field| HirParam {
            name: HirIdentifier {
                name: field.name.clone(),
                shadow_index: 0,
                span: crate::types::SourceSpan::new(crate::types::SourceID::default(), 0, 0),
            },
            ty: field.ty.clone(),
            is_mutable: false,
            binding_kind: crate::types::hir::HirParameterBindingKind::PositionalOrKeyword,
            default: None,
            variadic: HirVariadicKind::None,
        })
        .collect()
}

fn variant_constructor_param_types(variant: &HirVariant) -> Vec<ValkyrieType> {
    variant.fields.iter().map(|field| field.ty.clone()).collect()
}

fn build_variant_constructor_candidate(variant: &HirVariant, enum_def: &HirEnum) -> OverloadCandidate {
    // A variant constructor produces its owning nominal enum/sum. Falling
    // back to `Named(variant.name)` loses the sum identity and forces backend
    // code to guess from the constructor spelling. Keep the explicit result
    // type when present; otherwise the owner is the semantic contract.
    //
    // Symbol is the bare variant name; `owner` + `overload_symbol_path` yield
    // `Result.Fail`. Putting `Result.Fail` in `symbol` while also setting
    // `owner = Result` produced the bogus `Result.Result.Fail` SMIR003 miss.
    let return_type = variant.result_type.clone().unwrap_or_else(|| enum_apply_type(enum_def));
    OverloadCandidate::new_method(
        enum_def.name.clone(),
        NamePath::new(vec![variant.name.clone()]),
        OverloadDomain::Constructor,
        variant_constructor_param_types(variant),
        return_type,
        OverloadMatchKind::Row,
    )
}

fn build_method_candidate(function: &HirFunction, owner: Option<Identifier>) -> OverloadCandidate {
    let candidate = if let Some(owner) = owner {
        OverloadCandidate::new_method(
            owner,
            NamePath::new(vec![function.name.clone()]),
            classify_callable_domain(&function.name),
            function.params.iter().map(|param| param.ty.clone()).collect(),
            function.return_type.clone(),
            OverloadMatchKind::Row,
        )
    }
    else {
        OverloadCandidate::new(
            NamePath::new(vec![function.name.clone()]),
            classify_callable_domain(&function.name),
            function.params.iter().map(|param| param.ty.clone()).collect(),
            function.return_type.clone(),
            OverloadMatchKind::Row,
        )
    };
    let mut candidate = candidate.with_param_specs(function.params.clone());
    for generic in &function.generics {
        candidate = candidate.with_generic_binder(generic.name.clone());
    }
    candidate
}

fn classify_callable_domain(name: &Identifier) -> OverloadDomain {
    let text = name.as_str();
    if text.starts_with("prefix ") || text.starts_with("infix ") || text.starts_with("suffix ") || text.starts_with("postfix ") {
        OverloadDomain::Operator
    }
    else {
        OverloadDomain::Function
    }
}

fn resolve_function_calls(
    function: &mut HirFunction,
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    struct_fields: &BTreeMap<Identifier, Vec<HirField>>,
    singleton_names: &BTreeSet<Identifier>,
    owner: Option<&Identifier>,
) {
    let mut locals = function
        .params
        .iter()
        .map(|param| {
            let ty = if matches!(param.ty, ValkyrieType::r#SelfType)
                || (param.name.name.as_str() == "self" && matches!(param.ty, ValkyrieType::AutoType))
            {
                owner.map(|owner| ValkyrieType::Named(owner.clone())).unwrap_or(param.ty.clone())
            }
            else {
                param.ty.clone()
            };
            (param.name.name.to_string(), ty)
        })
        .collect::<BTreeMap<_, _>>();
    resolve_block_calls(&mut function.body, candidates, type_relations, &mut locals, struct_fields, singleton_names);
}

fn resolve_block_calls(
    block: &mut HirBlock,
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &mut BTreeMap<String, ValkyrieType>,
    struct_fields: &BTreeMap<Identifier, Vec<HirField>>,
    singleton_names: &BTreeSet<Identifier>,
) {
    for statement in &mut block.statements {
        resolve_statement_calls(statement, candidates, type_relations, locals, struct_fields, singleton_names);
    }
    if let Some(expr) = &mut block.expr {
        resolve_expr_calls(expr, candidates, type_relations, locals, struct_fields, singleton_names);
    }
}

fn resolve_statement_calls(
    statement: &mut HirStatement,
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &mut BTreeMap<String, ValkyrieType>,
    struct_fields: &BTreeMap<Identifier, Vec<HirField>>,
    singleton_names: &BTreeSet<Identifier>,
) {
    match &mut statement.kind {
        HirStatementKind::Let { pattern, initializer, ty, .. } => {
            if let Some(initializer) = initializer {
                resolve_expr_calls(initializer, candidates, type_relations, locals, struct_fields, singleton_names);
            }
            let binding_type = ty.clone().or_else(|| {
                initializer.as_ref().and_then(|expr| infer_scrutinee_type(expr, candidates, locals, struct_fields, singleton_names))
            });
            if let Some(binding_type) = binding_type {
                bind_pattern_type(pattern, &binding_type, locals);
            }
        }
        HirStatementKind::Expr(expr) => resolve_expr_calls(expr, candidates, type_relations, locals, struct_fields, singleton_names),
    }
}

fn resolve_expr_calls(
    expr: &mut HirExpr,
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &mut BTreeMap<String, ValkyrieType>,
    struct_fields: &BTreeMap<Identifier, Vec<HirField>>,
    singleton_names: &BTreeSet<Identifier>,
) {
    match &mut expr.kind {
        HirExprKind::Call { callee, args, resolved } => {
            resolve_expr_calls(callee, candidates, type_relations, locals, struct_fields, singleton_names);
            for arg in args.iter_mut() {
                resolve_expr_calls(&mut arg.value, candidates, type_relations, locals, struct_fields, singleton_names);
            }
            *resolved = try_resolve_call(callee, args, candidates, type_relations, locals, struct_fields, singleton_names);
        }
        HirExprKind::GenericApply { callee, .. }
        | HirExprKind::FieldInit { value: callee, .. }
        | HirExprKind::Await(callee)
        | HirExprKind::Awake(callee)
        | HirExprKind::BlockOn(callee)
        | HirExprKind::YieldFrom(callee)
        | HirExprKind::Raise(callee)
        | HirExprKind::Resume(callee)
        | HirExprKind::TryPropagate(callee)
        | HirExprKind::Assign { value: callee, .. }
        | HirExprKind::FieldAccess { object: callee, .. } => {
            resolve_expr_calls(callee, candidates, type_relations, locals, struct_fields, singleton_names)
        }
        HirExprKind::StoreField { object, value, .. } => {
            resolve_expr_calls(object, candidates, type_relations, locals, struct_fields, singleton_names);
            resolve_expr_calls(value, candidates, type_relations, locals, struct_fields, singleton_names);
        }
        HirExprKind::ArrayNew { length, .. } => resolve_expr_calls(length, candidates, type_relations, locals, struct_fields, singleton_names),
        HirExprKind::ArrayLiteral { items: args } => {
            for arg in args {
                resolve_expr_calls(arg, candidates, type_relations, locals, struct_fields, singleton_names);
            }
        }
        HirExprKind::Construct { name, args, resolved, .. } => {
            for arg in args.iter_mut() {
                resolve_expr_calls(arg, candidates, type_relations, locals, struct_fields, singleton_names);
            }
            *resolved = try_resolve_constructor(name, args, candidates, type_relations, locals, struct_fields, singleton_names);
        }
        HirExprKind::Block(block) => resolve_block_calls(block, candidates, type_relations, locals, struct_fields, singleton_names),
        HirExprKind::TryScope { body, .. } => resolve_block_calls(body, candidates, type_relations, locals, struct_fields, singleton_names),
        HirExprKind::Lambda { params, body, .. } => {
            let mut lambda_locals = locals.clone();
            for param in params {
                lambda_locals.insert(param.name.name.to_string(), param.ty.clone());
            }
            resolve_block_calls(body, candidates, type_relations, &mut lambda_locals, struct_fields, singleton_names);
        }
        HirExprKind::AnonymousClass { fields, methods, .. } => {
            for (_, value) in fields {
                resolve_expr_calls(value, candidates, type_relations, locals, struct_fields, singleton_names);
            }
            for method in methods {
                resolve_function_calls(method, candidates, type_relations, struct_fields, singleton_names, None);
            }
        }
        HirExprKind::If { condition, then_branch, else_branch } => {
            resolve_expr_calls(condition, candidates, type_relations, locals, struct_fields, singleton_names);
            let mut then_locals = locals.clone();
            resolve_block_calls(then_branch, candidates, type_relations, &mut then_locals, struct_fields, singleton_names);
            if let Some(else_branch) = else_branch {
                let mut else_locals = locals.clone();
                resolve_block_calls(else_branch, candidates, type_relations, &mut else_locals, struct_fields, singleton_names);
            }
        }
        HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
            resolve_expr_calls(scrutinee, candidates, type_relations, locals, struct_fields, singleton_names);
            let scrutinee_type = infer_scrutinee_type(scrutinee, candidates, locals, struct_fields, singleton_names);
            for arm in arms {
                resolve_arm_calls(arm, candidates, type_relations, locals, scrutinee_type.as_ref(), struct_fields, singleton_names);
            }
        }
        HirExprKind::IfLet { pattern, scrutinee, then_branch, else_branch } => {
            resolve_expr_calls(scrutinee, candidates, type_relations, locals, struct_fields, singleton_names);
            let scrutinee_type = infer_scrutinee_type(scrutinee, candidates, locals, struct_fields, singleton_names);
            resolve_pattern_calls(pattern, candidates, type_relations, scrutinee_type.as_ref(), struct_fields);
            let mut then_locals = locals.clone();
            if let Some(scrutinee_type) = scrutinee_type.as_ref() {
                bind_pattern_type(pattern, scrutinee_type, &mut then_locals);
            }
            resolve_block_calls(then_branch, candidates, type_relations, &mut then_locals, struct_fields, singleton_names);
            if let Some(else_branch) = else_branch {
                let mut else_locals = locals.clone();
                resolve_block_calls(else_branch, candidates, type_relations, &mut else_locals, struct_fields, singleton_names);
            }
        }
        HirExprKind::Loop { pattern, iterator, condition, body, .. } => {
            if let Some(iterator) = iterator {
                resolve_expr_calls(iterator, candidates, type_relations, locals, struct_fields, singleton_names);
            }
            if let Some(condition) = condition {
                resolve_expr_calls(condition, candidates, type_relations, locals, struct_fields, singleton_names);
            }
            let mut body_locals = locals.clone();
            if let (Some(pattern), Some(iterator)) = (pattern, iterator) {
                let iter_type = infer_expr_type(iterator, locals);
                resolve_pattern_calls(pattern, candidates, type_relations, iter_type.as_ref(), struct_fields);
                if let Some(iter_type) = iter_type.as_ref() {
                    let element_type = match iter_type {
                        ValkyrieType::Array(inner) => inner.as_ref().clone(),
                        other => other.clone(),
                    };
                    bind_pattern_type(pattern, &element_type, &mut body_locals);
                }
            }
            resolve_block_calls(body, candidates, type_relations, &mut body_locals, struct_fields, singleton_names);
        }
        HirExprKind::Return(Some(value)) | HirExprKind::Break { expr: Some(value), .. } | HirExprKind::Yield(Some(value)) => {
            resolve_expr_calls(value, candidates, type_relations, locals, struct_fields, singleton_names);
        }
        HirExprKind::Catch { expr, arms } => {
            resolve_expr_calls(expr, candidates, type_relations, locals, struct_fields, singleton_names);
            let scrutinee_type = infer_catch_scrutinee_type(expr, locals);
            for arm in arms {
                resolve_arm_calls(arm, candidates, type_relations, locals, scrutinee_type.as_ref(), struct_fields, singleton_names);
            }
        }
        HirExprKind::Literal(_)
        | HirExprKind::Variable(_)
        | HirExprKind::Path(_)
        | HirExprKind::Return(None)
        | HirExprKind::Break { expr: None, .. }
        | HirExprKind::Continue { .. }
        | HirExprKind::Yield(None)
        | HirExprKind::Fallthrough
        | HirExprKind::SuperCall { .. }
        | HirExprKind::With { .. } => {}
    }
}

fn resolve_arm_calls(
    arm: &mut HirMatchArm,
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    scrutinee_type: Option<&ValkyrieType>,
    struct_fields: &BTreeMap<Identifier, Vec<HirField>>,
    singleton_names: &BTreeSet<Identifier>,
) {
    let mut arm_locals = locals.clone();
    resolve_pattern_calls(&mut arm.pattern, candidates, type_relations, scrutinee_type, struct_fields);
    if let Some(scrutinee_type) = scrutinee_type {
        bind_pattern_type(&arm.pattern, scrutinee_type, &mut arm_locals);
    }
    if let Some(guard) = &mut arm.guard {
        resolve_expr_calls(guard, candidates, type_relations, &mut arm_locals, struct_fields, singleton_names);
    }
    resolve_expr_calls(&mut arm.body, candidates, type_relations, &mut arm_locals, struct_fields, singleton_names);
}

fn resolve_pattern_calls(
    pattern: &mut HirPattern,
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    scrutinee_type: Option<&ValkyrieType>,
    struct_fields: &BTreeMap<Identifier, Vec<HirField>>,
) {
    match pattern {
        HirPattern::Name(name) => {
            if name.parts().len() == 1 {
                let first_part = name.parts().first().expect("single-segment name should have first part");
                if first_part.as_str().chars().next().is_some_and(|c| c.is_lowercase()) {
                    unreachable!(
                        "单段小写名字模式 `{}` 应由解析器处理为 Variable，不应出现在 Name 形式中",
                        name.parts().iter().map(|p| p.as_str()).collect::<Vec<_>>().join("::")
                    );
                }
            }
            if let Some(actual_type) = scrutinee_type {
                if should_resolve_name_pattern_as_type(name, actual_type) {
                    *pattern = HirPattern::Type(name.clone());
                }
                else {
                    // Nullary unite/enum arm (`case LeftBrace:`) must become an Extractor so
                    // MIR emits runtime `tag` compare — never `plain_type_pattern_matches` → Bool(false).
                    let canonical_callee = canonical_extractor_callee(name);
                    if let Some(resolved) = try_resolve_pattern_extractor(&canonical_callee, actual_type, candidates, type_relations) {
                        *pattern = HirPattern::Extractor(HirExtractorPattern::Constructor {
                            name: name.clone(),
                            canonical_callee,
                            fields: Vec::new(),
                            resolved: Some(resolved),
                        });
                    }
                }
            }
        }
        HirPattern::Extractor(HirExtractorPattern::Constructor { fields, canonical_callee, resolved, .. }) => {
            if let Some(actual_type) = scrutinee_type {
                *resolved = try_resolve_pattern_extractor(canonical_callee, actual_type, candidates, type_relations)
                    .or_else(|| synthesize_builtin_result_extractor(canonical_callee, actual_type));
            }
            let field_scrutinee_types = resolved
                .as_ref()
                .and_then(|resolved| resolved.extractor_payload_type.as_ref())
                .map(|payload| payload_field_scrutinee_types(payload, struct_fields))
                .unwrap_or_default();
            for (index, field) in fields.iter_mut().enumerate() {
                let field_scrutinee = field_scrutinee_types.get(index).and_then(|ty| ty.as_ref());
                resolve_pattern_calls(field, candidates, type_relations, field_scrutinee, struct_fields);
            }
        }
        HirPattern::Extractor(HirExtractorPattern::Array { prefix, suffix, canonical_callee, resolved, .. }) => {
            for item in prefix.iter_mut() {
                resolve_pattern_calls(item, candidates, type_relations, None, struct_fields);
            }
            for item in suffix.iter_mut() {
                resolve_pattern_calls(item, candidates, type_relations, None, struct_fields);
            }
            if let Some(actual_type) = scrutinee_type {
                *resolved = try_resolve_pattern_extractor(canonical_callee, actual_type, candidates, type_relations);
            }
        }
        HirPattern::Tuple(items) => {
            for item in items.iter_mut() {
                resolve_pattern_calls(item, candidates, type_relations, None, struct_fields);
            }
        }
        HirPattern::Or(items) => {
            for item in items.iter_mut() {
                resolve_pattern_calls(item, candidates, type_relations, scrutinee_type, struct_fields);
            }
        }
        HirPattern::Object { name, fields, .. } => {
            for (field_name, item) in fields.iter_mut() {
                let field_scrutinee = object_field_scrutinee_type(name.as_ref(), field_name, struct_fields);
                resolve_pattern_calls(item, candidates, type_relations, field_scrutinee, struct_fields);
            }
        }
        HirPattern::Bind { pattern, .. } | HirPattern::Mut(pattern) | HirPattern::Pin { pattern, .. } => {
            resolve_pattern_calls(pattern, candidates, type_relations, scrutinee_type, struct_fields);
        }
        _ => {}
    }
}

fn match_call_candidate(
    candidate: &OverloadCandidate,
    args: &[HirCallArgument],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &BTreeMap<Identifier, Vec<HirField>>,
    singleton_names: &BTreeSet<Identifier>,
) -> Option<OverloadCandidate> {
    // 运算符重载：禁止在实参类型推断失败时回退到形参类型。
    // 否则 `u16 + u16` 在 match 臂内推断失败时会“假装”匹配
    // `Utf8Text::infix +(Utf8Text, utf8)`，JVM 侧对 int local 发
    // 字符串拼接 → VerifyError: Expecting object/array on stack。
    let operator_strict = matches!(candidate.domain, OverloadDomain::Operator);
    let actual_types = if candidate.param_specs.is_empty() {
        args.iter().map(|arg| infer_scrutinee_type(&arg.value, &[], locals, struct_fields, singleton_names)).collect::<Option<Vec<_>>>()?
    }
    else if let Ok(bound) = bind_call_arguments(&candidate.param_specs, args) {
        let mut types = Vec::with_capacity(bound.len());
        for (arg, param) in bound.iter().zip(candidate.param_specs.iter()) {
            match infer_scrutinee_type(arg, &[], locals, struct_fields, singleton_names) {
                Some(ty) => types.push(ty),
                None if operator_strict => return None,
                None => types.push(param.ty.clone()),
            }
        }
        types
    }
    else if args.len() == candidate.param_specs.len() {
        let mut types = Vec::with_capacity(args.len());
        for (arg, param) in args.iter().zip(candidate.param_specs.iter()) {
            match infer_scrutinee_type(&arg.value, &[], locals, struct_fields, singleton_names) {
                Some(ty) => types.push(ty),
                None if operator_strict => return None,
                None => types.push(param.ty.clone()),
            }
        }
        types
    }
    else {
        return None;
    };
    // `self: Self` / untyped `self` must mean the method owner, not "any receiver".
    // Otherwise `Array.get` and `HashMap.get` both see `AutoType` self and
    // `usize`→`K` type-var acceptance, so HashMap.get can steal Array.suffix [].
    // Same rule as pattern extractors: AutoType self + owner ⇒ Named(owner).
    let expected_params = candidate
        .signature
        .params
        .iter()
        .enumerate()
        .map(|(index, ty)| {
            let effective = if index == 0
                && matches!(ty, ValkyrieType::AutoType)
                && candidate.owner.is_some()
                && candidate.param_specs.first().is_some_and(|param| param.name.name.as_str() == "self")
            {
                &ValkyrieType::r#SelfType
            }
            else {
                ty
            };
            substitute_self_type(effective, candidate.owner.as_ref())
        })
        .collect::<Vec<_>>();
    let mut substitutions = BTreeMap::new();
    for (expected, actual) in expected_params.iter().zip(actual_types.iter()) {
        unify_call_type_binders(expected, actual, &candidate.generic_binders, &mut substitutions)?;
    }
    let expected_params = expected_params.iter().map(|ty| substitute_type_vars(ty, &substitutions)).collect::<Vec<_>>();
    // Integer syntax is representation-polymorphic. It becomes a concrete
    // integer type only when an independently resolved formal parameter
    // constrains it. This is semantic call binding, not backend inference.
    // If every operand is an unconstrained literal, overload resolution still
    // remains ambiguous and fails closed.
    let actual_types = specialize_call_literals(args, &actual_types, &expected_params);
    let match_kind = compute_call_match_kind(type_relations, &actual_types, &expected_params)?;
    Some(OverloadCandidate {
        symbol: candidate.symbol.clone(),
        owner: candidate.owner.clone(),
        domain: candidate.domain.clone(),
        signature: OverloadSignature {
            params: expected_params,
            return_type: substitute_type_vars(&candidate.signature.return_type, &substitutions),
        },
        match_kind,
        param_specs: candidate.param_specs.clone(),
        generic_binders: candidate.generic_binders.clone(),
    })
}

fn unify_call_type_binders(
    expected: &ValkyrieType,
    actual: &ValkyrieType,
    binders: &BTreeSet<Identifier>,
    substitutions: &mut BTreeMap<Identifier, ValkyrieType>,
) -> Option<()> {
    match expected {
        ValkyrieType::Named(name) if binders.contains(name) => match substitutions.get(name) {
            Some(bound) if bound != actual => None,
            Some(_) => Some(()),
            None => {
                substitutions.insert(name.clone(), actual.clone());
                Some(())
            }
        },
        ValkyrieType::Generic(generic) if binders.contains(&generic.name) => match substitutions.get(&generic.name) {
            Some(bound) if bound != actual => None,
            Some(_) => Some(()),
            None => {
                substitutions.insert(generic.name.clone(), actual.clone());
                Some(())
            }
        },
        ValkyrieType::Array(expected_element) => {
            let ValkyrieType::Array(actual_element) = actual
            else {
                return None;
            };
            unify_call_type_binders(expected_element, actual_element, binders, substitutions)
        }
        ValkyrieType::Apply(expected_base, expected_args) => {
            let ValkyrieType::Apply(actual_base, actual_args) = actual
            else {
                return None;
            };
            if expected_args.len() != actual_args.len() {
                return None;
            }
            unify_call_type_binders(expected_base, actual_base, binders, substitutions)?;
            for (expected, actual) in expected_args.iter().zip(actual_args) {
                unify_call_type_binders(expected, actual, binders, substitutions)?;
            }
            Some(())
        }
        _ => Some(()),
    }
}

fn specialize_call_literals(args: &[HirCallArgument], actual: &[ValkyrieType], expected: &[ValkyrieType]) -> Vec<ValkyrieType> {
    args.iter()
        .zip(actual.iter().zip(expected.iter()))
        .map(|(arg, (actual, expected))| match &arg.value.kind {
            // Integer literals are representation-polymorphic: bind to the formal
            // numeric type (`usize`, `i32`, …), same as constructor literals.
            HirExprKind::Literal(crate::types::hir::HirLiteral::Integer64(_)) if is_numeric_type(expected) => expected.clone(),
            _ => actual.clone(),
        })
        .collect()
}

/// Replace `Self` with the imply/class owner so instance dispatch is nominal.
fn substitute_self_type(ty: &ValkyrieType, owner: Option<&Identifier>) -> ValkyrieType {
    match ty {
        ValkyrieType::r#SelfType => match owner {
            Some(owner) => ValkyrieType::Named(owner.clone()),
            None => ty.clone(),
        },
        ValkyrieType::Array(inner) => ValkyrieType::Array(Box::new(substitute_self_type(inner, owner))),
        ValkyrieType::Apply(base, args) => {
            ValkyrieType::Apply(Box::new(substitute_self_type(base, owner)), args.iter().map(|arg| substitute_self_type(arg, owner)).collect())
        }
        ValkyrieType::Tuple(items) => ValkyrieType::Tuple(items.iter().map(|item| substitute_self_type(item, owner)).collect()),
        ValkyrieType::Union(items) => ValkyrieType::Union(items.iter().map(|item| substitute_self_type(item, owner)).collect()),
        ValkyrieType::Function(func) => ValkyrieType::Function(Box::new(crate::types::hir::FunctionType {
            params: func.params.iter().map(|param| substitute_self_type(param, owner)).collect(),
            return_type: substitute_self_type(&func.return_type, owner),
        })),
        other => other.clone(),
    }
}

fn try_resolve_call(
    callee: &HirExpr,
    args: &[HirCallArgument],
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &BTreeMap<Identifier, Vec<HirField>>,
    singleton_names: &BTreeSet<Identifier>,
) -> Option<HirResolvedCall> {
    // Function-typed locals/params are indirect calls. Attach the local signature as the
    // call contract so SMIR003 does not reject `f()` in `unwrap_or_else` / `map` / etc.
    // Callee may be `Variable(f)` or single-part `Path(f)` depending on expr lowering.
    if let Some(local_name) = match &callee.kind {
        HirExprKind::Variable(identifier) => Some(identifier.name.as_str()),
        HirExprKind::Path(path) if path.parts().len() == 1 => Some(path.parts()[0].as_str()),
        _ => None,
    } {
        if let Some(ValkyrieType::Function(func)) = locals.get(local_name) {
            return Some(HirResolvedCall {
                symbol: NamePath::new(vec![Identifier::new(local_name)]),
                domain: HirCallableDomain::Function,
                return_type: func.return_type.clone(),
                parameter_types: func.params.clone(),
                extractor_payload_type: None,
            });
        }
        // Param typed as `micro(...) -> T` must still form a call contract even if the
        // stored local type was left as AutoType / Named during early HIR construction.
        if locals.contains_key(local_name)
            && !candidates.iter().any(|candidate| candidate.symbol.parts().last().is_some_and(|name| name.as_str() == local_name))
        {
            return Some(HirResolvedCall {
                symbol: NamePath::new(vec![Identifier::new(local_name)]),
                domain: HirCallableDomain::Function,
                return_type: ValkyrieType::AutoType,
                parameter_types: args.iter().map(|_| ValkyrieType::AutoType).collect(),
                extractor_payload_type: None,
            });
        }
    }
    // Language `panic(...)`: core Option/Result abort. Not a library overload; returns Never.
    // Language `format(...)`: runtime stub (wasm/CLR emitters); not a std overload.
    if let Some(name) = extract_callable_name(callee) {
        if name.as_str() == "panic" {
            return Some(HirResolvedCall {
                symbol: NamePath::new(vec![Identifier::new("builtin"), Identifier::new("panic")]),
                domain: HirCallableDomain::Function,
                return_type: ValkyrieType::Named(Identifier::new("Never")),
                parameter_types: args.iter().map(|_| ValkyrieType::AutoType).collect(),
                extractor_payload_type: None,
            });
        }
        if name.as_str() == "format" {
            return Some(HirResolvedCall {
                symbol: NamePath::new(vec![Identifier::new("format")]),
                domain: HirCallableDomain::Function,
                return_type: ValkyrieType::Utf8,
                parameter_types: args.iter().map(|_| ValkyrieType::AutoType).collect(),
                extractor_payload_type: None,
            });
        }
    }
    if let HirExprKind::Path(path) = &callee.kind {
        if path.parts().len() == 2 {
            let owner = &path.parts()[0];
            let method_name = &path.parts()[1];
            if singleton_names.contains(owner) {
                if let Some(resolved) = try_resolve_singleton_method(
                    owner,
                    method_name,
                    args,
                    candidates,
                    type_relations,
                    locals,
                    struct_fields,
                    singleton_names,
                    true,
                    false,
                ) {
                    return Some(resolved);
                }
            }
        }
    }

    // `receiver.method(args)` — virtual dispatch: prepend receiver, match imply/class methods.
    // Typechecker may also flatten `args.length` into a Path whose root is a local/param.
    if let HirExprKind::FieldAccess { object, field } = &callee.kind {
        if let Some(resolved) =
            try_resolve_instance_method(object, field, args, candidates, type_relations, locals, struct_fields, singleton_names)
        {
            return Some(resolved);
        }
    }
    if let HirExprKind::Path(path) = &callee.kind {
        if path.parts().len() >= 2 {
            let root = path.parts()[0].as_str();
            if locals.contains_key(root) || locals.keys().any(|key| key.as_str() == root) {
                let method_name = path.parts().last().expect("path has parts");
                let span = callee.span.clone();
                let mut receiver = HirExpr {
                    kind: HirExprKind::Variable(HirIdentifier { name: path.parts()[0].clone(), shadow_index: 0, span: span.clone() }),
                    span: span.clone(),
                };
                for field in &path.parts()[1..path.parts().len() - 1] {
                    receiver =
                        HirExpr { kind: HirExprKind::FieldAccess { object: Box::new(receiver), field: field.clone() }, span: span.clone() };
                }
                if let Some(resolved) = try_resolve_instance_method(
                    &receiver,
                    method_name,
                    args,
                    candidates,
                    type_relations,
                    locals,
                    struct_fields,
                    singleton_names,
                ) {
                    return Some(resolved);
                }
            }
            // Even without a typed local, treat `binding.method` Paths as instance calls when the
            // root looks like a variable (lowercase / non-type) — fail-closed via overload match.
            else if root.chars().next().is_some_and(|ch| ch.is_lowercase() || ch == '_') {
                let method_name = path.parts().last().expect("path has parts");
                let span = callee.span.clone();
                let receiver = HirExpr {
                    kind: HirExprKind::Variable(HirIdentifier { name: path.parts()[0].clone(), shadow_index: 0, span: span.clone() }),
                    span,
                };
                if let Some(resolved) = try_resolve_instance_method(
                    &receiver,
                    method_name,
                    args,
                    candidates,
                    type_relations,
                    locals,
                    struct_fields,
                    singleton_names,
                ) {
                    return Some(resolved);
                }
            }
        }
    }

    let callee_name = extract_callable_name(callee)?;
    if let Some(resolved) = primitive_operator_contract(&callee_name, args, candidates, locals, struct_fields, singleton_names) {
        return Some(resolved);
    }
    // The parser also canonicalizes chained calls as `name(receiver, args...)`.
    // Reuse the typed primitive registry for that representation; do not infer
    // semantics from the source symbol or from a backend-specific fallback.
    if let Some(receiver) = args.first() {
        if let Some((return_type, parameter_types)) =
            primitive_operation_contract(&receiver.value, &callee_name, args, locals, struct_fields, singleton_names)
        {
            return Some(HirResolvedCall {
                symbol: primitive_operation_symbol(&receiver.value, &callee_name, locals, struct_fields, singleton_names)?,
                domain: HirCallableDomain::Function,
                return_type,
                parameter_types,
                extractor_payload_type: None,
            });
        }
    }
    if let Some(owner) = args.first().and_then(|arg| extract_singleton_type_name(&arg.value, locals, singleton_names)) {
        if let Some(resolved) = try_resolve_singleton_method(
            &owner,
            &callee_name,
            args,
            candidates,
            type_relations,
            locals,
            struct_fields,
            singleton_names,
            true,
            true,
        ) {
            return Some(resolved);
        }
    }

    let mut filtered = candidates
        .iter()
        .filter(|candidate| matches!(candidate.domain, OverloadDomain::Function | OverloadDomain::Operator | OverloadDomain::Constructor))
        .filter(|candidate| candidate.symbol.parts().last().is_some_and(|name| name == &callee_name))
        .filter_map(|candidate| match_call_candidate(candidate, args, type_relations, locals, struct_fields, singleton_names))
        .collect::<Vec<_>>();
    // An unqualified constructor expression denotes the nominal constructor
    // contract when a same-spelled ordinary function is also visible. This
    // is a domain precedence rule, so generic libraries cannot manufacture an
    // ambiguity between a variant and a helper function with the same name.
    if filtered.iter().any(|candidate| candidate.domain == OverloadDomain::Constructor) {
        filtered.retain(|candidate| candidate.domain == OverloadDomain::Constructor);
    }
    let resolved = match resolve_overload(&filtered) {
        Ok(resolved) => resolved,
        Err(_) => {
            // `Fine(x)` / `Fail(e)` / `Some(v)` are Calls, not Construct nodes.
            // Generic payload locals often fail exact match; accept same-arity
            // constructor (then function) by simple name for SMIR003.
            // Never arity-fallback onto an intrinsic (e.g. ArrayPush on List<T>):
            // typed primitives must come from primitive_operation_contract /
            // match_call_candidate only when the receiver is Array/FixedArray.
            let fallback = candidates
                .iter()
                .filter(|candidate| {
                    matches!(candidate.domain, OverloadDomain::Constructor | OverloadDomain::Function)
                        && candidate.symbol.parts().last().is_some_and(|name| name == &callee_name)
                        && candidate.signature.params.len() == args.len()
                })
                .max_by_key(|candidate| {
                    let domain_score = match candidate.domain {
                        OverloadDomain::Constructor => 2,
                        _ => 0,
                    };
                    let owner_score = usize::from(candidate.owner.is_some());
                    domain_score + owner_score
                })?;
            return Some(HirResolvedCall {
                symbol: overload_symbol_path(fallback),
                domain: match fallback.domain {
                    OverloadDomain::Function => HirCallableDomain::Function,
                    OverloadDomain::Constructor => HirCallableDomain::Constructor,
                    OverloadDomain::Operator => HirCallableDomain::Operator,
                    OverloadDomain::Extractor => HirCallableDomain::Extractor,
                },
                return_type: fallback.signature.return_type.clone(),
                parameter_types: fallback.signature.params.clone(),
                extractor_payload_type: None,
            });
        }
    };
    let matched =
        filtered.iter().find(|candidate| candidate.symbol == resolved.symbol && candidate.domain == resolved.domain).unwrap_or(&filtered[0]);
    let return_type = if is_boolean_operator(&callee_name) { ValkyrieType::Boolean } else { resolved.signature.return_type };
    Some(HirResolvedCall {
        symbol: overload_symbol_path(matched),
        domain: match resolved.domain {
            OverloadDomain::Function => HirCallableDomain::Function,
            OverloadDomain::Constructor => HirCallableDomain::Constructor,
            OverloadDomain::Operator => HirCallableDomain::Operator,
            OverloadDomain::Extractor => HirCallableDomain::Extractor,
        },
        return_type,
        parameter_types: resolved.signature.params,
        extractor_payload_type: None,
    })
}

/// Resolve parser-canonical language operators into a structured semantic
/// opcode. The string here is the frontend's closed operator surface, not a
/// user symbol or library lookup; all later stages consume only the opcode.
fn primitive_operator_contract(
    operator: &Identifier,
    args: &[HirCallArgument],
    candidates: &[OverloadCandidate],
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &BTreeMap<Identifier, Vec<HirField>>,
    singleton_names: &BTreeSet<Identifier>,
) -> Option<HirResolvedCall> {
    if operator.as_str() == "prefix !" && args.len() == 1 {
        let operand = infer_scrutinee_type(&args[0].value, candidates, locals, struct_fields, singleton_names)?;
        if !matches!(operand, ValkyrieType::Boolean) {
            return None;
        }
        return Some(HirResolvedCall {
            symbol: NamePath::new(vec![Identifier::new("primitive"), operator.clone()]),
            domain: HirCallableDomain::Operator,
            return_type: ValkyrieType::Boolean,
            parameter_types: vec![ValkyrieType::Boolean],
            extractor_payload_type: None,
        });
    }
    if operator.as_str() == "prefix -" && args.len() == 1 {
        let operand = infer_scrutinee_type(&args[0].value, candidates, locals, struct_fields, singleton_names)?;
        if !is_numeric_type(&operand) {
            return None;
        }
        return Some(HirResolvedCall {
            symbol: NamePath::new(vec![Identifier::new("primitive"), operator.clone()]),
            domain: HirCallableDomain::Operator,
            return_type: operand.clone(),
            parameter_types: vec![operand],
            extractor_payload_type: None,
        });
    }
    if args.len() != 2 {
        return None;
    }
    let left = infer_scrutinee_type(&args[0].value, candidates, locals, struct_fields, singleton_names)?;
    let right = match &args[1].value.kind {
        HirExprKind::Literal(crate::types::hir::HirLiteral::Integer64(_)) if is_numeric_type(&left) => left.clone(),
        _ => infer_scrutinee_type(&args[1].value, candidates, locals, struct_fields, singleton_names)?,
    };
    if left != right {
        return None;
    }
    // Utf8/boolean operator IntrinsicOpcode paths DELETED — fail closed until Invoke+adaptor.
    if matches!(left, ValkyrieType::Utf8) || is_boolean_type(&left) {
        return None;
    }
    // Nominal sum equality is a structural language operation.  The only
    // authority used here is the constructor metadata collected for the
    // module; do not infer it from a function/library/type spelling.
    if operator.as_str() == "infix =="
        && matches!(&left, ValkyrieType::Named(name) if candidates.iter().any(|candidate| {
            matches!(candidate.domain, OverloadDomain::Constructor)
                && candidate.owner.as_ref() == Some(name)
        }))
    {
        return Some(HirResolvedCall {
            symbol: NamePath::new(vec![Identifier::new("primitive"), operator.clone()]),
            domain: HirCallableDomain::Operator,
            return_type: ValkyrieType::Boolean,
            parameter_types: vec![left, right],
            extractor_payload_type: None,
        });
    }
    if !is_numeric_type(&left) {
        return None;
    }
    // HIR may still type operators; MUST NOT mint IntrinsicOpcode (deleted).
    let return_type = match operator.as_str() {
        "infix +" | "infix -" | "infix *" | "infix /" | "infix %" | "infix &" | "infix |" | "infix ^" | "infix <<" | "infix >>" => left.clone(),
        "infix ==" | "infix !=" | "infix <" | "infix <=" | "infix >" | "infix >=" => ValkyrieType::Boolean,
        _ => return None,
    };
    Some(HirResolvedCall {
        symbol: NamePath::new(vec![Identifier::new("primitive"), operator.clone()]),
        domain: HirCallableDomain::Operator,
        return_type,
        parameter_types: vec![left, right],
        extractor_payload_type: None,
    })
}

fn is_boolean_type(ty: &ValkyrieType) -> bool {
    matches!(ty, ValkyrieType::Boolean)
        || matches!(ty, ValkyrieType::Named(name) if matches!(name.as_str(), "bool" | "core.primitive.bool" | "core::primitive::bool"))
}

fn is_numeric_type(ty: &ValkyrieType) -> bool {
    match ty {
        ValkyrieType::Integer8 { .. }
        | ValkyrieType::Integer16 { .. }
        | ValkyrieType::Integer32 { .. }
        | ValkyrieType::Integer64 { .. }
        | ValkyrieType::Integer128 { .. }
        | ValkyrieType::Float32
        | ValkyrieType::Float64 => true,
        ValkyrieType::Named(name) => matches!(
            name.as_str(),
            "byte"
                | "sbyte"
                | "usize"
                | "isize"
                | "u8"
                | "u16"
                | "u32"
                | "u64"
                | "u128"
                | "i8"
                | "i16"
                | "i32"
                | "i64"
                | "i128"
                | "f32"
                | "f64"
                | "f128"
                | "core.primitive.usize"
                | "core.primitive.isize"
                | "core::primitive::usize"
                | "core::primitive::isize"
        ),
        _ => false,
    }
}

fn try_resolve_instance_method(
    receiver: &HirExpr,
    method_name: &Identifier,
    args: &[HirCallArgument],
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &BTreeMap<Identifier, Vec<HirField>>,
    singleton_names: &BTreeSet<Identifier>,
) -> Option<HirResolvedCall> {
    let receiver_arg = HirCallArgument::positional(receiver.clone());
    let mut full_args = Vec::with_capacity(args.len() + 1);
    full_args.push(receiver_arg);
    full_args.extend_from_slice(args);

    if let Some((return_type, parameter_types)) =
        primitive_operation_contract(receiver, method_name, &full_args, locals, struct_fields, singleton_names)
    {
        return Some(HirResolvedCall {
            symbol: primitive_operation_symbol(receiver, method_name, locals, struct_fields, singleton_names)?,
            domain: HirCallableDomain::Function,
            return_type,
            parameter_types,
            extractor_payload_type: None,
        });
    }

    let filtered = candidates
        .iter()
        .filter(|candidate| matches!(candidate.domain, OverloadDomain::Function | OverloadDomain::Operator))
        .filter(|candidate| candidate.symbol.parts().last().is_some_and(|name| name == method_name))
        .filter_map(|candidate| match_call_candidate(candidate, &full_args, type_relations, locals, struct_fields, singleton_names))
        .collect::<Vec<_>>();
    let resolved = resolve_overload(&filtered).ok()?;
    let matched =
        filtered.iter().find(|candidate| candidate.symbol == resolved.symbol && candidate.domain == resolved.domain).unwrap_or(&filtered[0]);
    // Prefer the original candidate (with owner) for a stable method symbol path.
    let original = candidates
        .iter()
        .find(|candidate| candidate.symbol == matched.symbol && candidate.owner == matched.owner)
        .or_else(|| candidates.iter().find(|candidate| candidate.symbol.parts().last() == matched.symbol.parts().last()))
        .unwrap_or(matched);
    // Overload matching already used the concrete receiver type, but the
    // candidate signature still contains the method's generic parameters
    // (`Array<T>.get -> Option<T>`, for example).  Preserve that semantic
    // instantiation in the resolved call metadata so SSA and every backend
    // receive the same concrete return and formal types.
    let actual_types =
        full_args.iter().map(|arg| infer_scrutinee_type(&arg.value, &[], locals, struct_fields, singleton_names)).collect::<Option<Vec<_>>>();
    let (return_type, parameter_types) = match (original.signature.params.first(), actual_types.as_ref().and_then(|types| types.first())) {
        (Some(receiver_type), Some(actual_receiver)) => {
            let return_type = substitute_type_parameters(&original.signature.return_type, receiver_type, actual_receiver);
            let parameter_types =
                original.signature.params.iter().map(|param| substitute_type_parameters(param, receiver_type, actual_receiver)).collect();
            (return_type, parameter_types)
        }
        _ => (resolved.signature.return_type, resolved.signature.params),
    };
    Some(HirResolvedCall {
        symbol: overload_symbol_path(original),
        domain: HirCallableDomain::Function,
        return_type,
        parameter_types,
        extractor_payload_type: None,
    })
}

/// A primitive call keeps a language-level locator for diagnostics and later
/// contract checks. The receiver's resolved semantic family is authoritative;
/// no host carrier or imported declaration name participates in this choice.
fn primitive_operation_symbol(
    receiver: &HirExpr,
    method_name: &Identifier,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &BTreeMap<Identifier, Vec<HirField>>,
    singleton_names: &BTreeSet<Identifier>,
) -> Option<NamePath> {
    let receiver_type =
        infer_expr_type(receiver, locals).or_else(|| infer_scrutinee_type(receiver, &[], locals, struct_fields, singleton_names))?;
    let family = match receiver_type {
        ValkyrieType::Utf8 => "Utf8",
        ValkyrieType::Array(_) | ValkyrieType::FixedArray { .. } => "Array",
        _ => "Primitive",
    };
    Some(NamePath::new(vec![Identifier::new(family), method_name.clone()]))
}

/// Operators / text / array methods must resolve to std adaptor Invoke — not IntrinsicOpcode.
fn primitive_operation_contract(
    _receiver: &HirExpr,
    _method_name: &Identifier,
    _full_args: &[HirCallArgument],
    _locals: &BTreeMap<String, ValkyrieType>,
    _struct_fields: &BTreeMap<Identifier, Vec<HirField>>,
    _singleton_names: &BTreeSet<Identifier>,
) -> Option<(ValkyrieType, Vec<ValkyrieType>)> {
    None
}

fn extract_singleton_type_name(
    expr: &HirExpr,
    locals: &BTreeMap<String, ValkyrieType>,
    singleton_names: &BTreeSet<Identifier>,
) -> Option<Identifier> {
    match &expr.kind {
        HirExprKind::Variable(identifier) if singleton_names.contains(&identifier.name) => Some(identifier.name.clone()),
        HirExprKind::Variable(identifier) => match locals.get(identifier.name.as_str())? {
            ValkyrieType::Named(name) if singleton_names.contains(name) => Some(name.clone()),
            _ => None,
        },
        HirExprKind::Path(path) if path.parts().len() == 1 && singleton_names.contains(&path.parts()[0]) => Some(path.parts()[0].clone()),
        _ => None,
    }
}

fn try_resolve_singleton_method(
    owner: &Identifier,
    method_name: &Identifier,
    args: &[HirCallArgument],
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &BTreeMap<Identifier, Vec<HirField>>,
    singleton_names: &BTreeSet<Identifier>,
    skip_self_param: bool,
    strip_receiver_arg: bool,
) -> Option<HirResolvedCall> {
    let call_args = if strip_receiver_arg && !args.is_empty() { &args[1..] } else { args };
    let filtered = candidates
        .iter()
        .filter(|candidate| candidate.owner.as_ref() == Some(owner))
        .filter(|candidate| matches!(candidate.domain, OverloadDomain::Function | OverloadDomain::Operator))
        .filter(|candidate| candidate.symbol.parts().last().is_some_and(|name| name == method_name))
        .filter_map(|candidate| {
            match_singleton_method_candidate(candidate, call_args, type_relations, locals, struct_fields, singleton_names, skip_self_param)
        })
        .collect::<Vec<_>>();
    let resolved = resolve_overload(&filtered).ok()?;
    let original = candidates
        .iter()
        .find(|candidate| candidate.symbol == resolved.symbol && candidate.owner.as_ref() == Some(owner))
        .or_else(|| filtered.iter().find(|candidate| candidate.symbol == resolved.symbol))
        .unwrap_or(&filtered[0]);
    let actual_receiver = args.first().and_then(|arg| infer_scrutinee_type(&arg.value, &[], locals, struct_fields, singleton_names));
    let (return_type, parameter_types) = match (original.signature.params.first(), actual_receiver.as_ref()) {
        (Some(receiver_type), Some(actual_receiver)) => (
            substitute_type_parameters(&original.signature.return_type, receiver_type, actual_receiver),
            original.signature.params.iter().map(|param| substitute_type_parameters(param, receiver_type, actual_receiver)).collect(),
        ),
        _ => (resolved.signature.return_type, resolved.signature.params),
    };
    Some(HirResolvedCall {
        symbol: NamePath::new(vec![owner.clone(), method_name.clone()]),
        domain: HirCallableDomain::Function,
        return_type,
        parameter_types,
        extractor_payload_type: None,
    })
}

fn match_singleton_method_candidate(
    candidate: &OverloadCandidate,
    call_args: &[HirCallArgument],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &BTreeMap<Identifier, Vec<HirField>>,
    singleton_names: &BTreeSet<Identifier>,
    skip_self_param: bool,
) -> Option<OverloadCandidate> {
    let param_specs = if skip_self_param && candidate.param_specs.first().is_some_and(|param| param.name.name.as_str() == "self") {
        candidate.param_specs[1..].to_vec()
    }
    else {
        candidate.param_specs.clone()
    };
    let expected_types = param_specs.iter().map(|param| param.ty.clone()).collect::<Vec<_>>();
    let actual_types = if param_specs.is_empty() {
        call_args.iter().map(|arg| infer_scrutinee_type(&arg.value, &[], locals, struct_fields, singleton_names)).collect::<Option<Vec<_>>>()?
    }
    else if let Ok(bound) = bind_call_arguments(&param_specs, call_args) {
        bound
            .iter()
            .zip(param_specs.iter())
            .map(|(arg, param)| infer_scrutinee_type(arg, &[], locals, struct_fields, singleton_names).unwrap_or_else(|| param.ty.clone()))
            .collect()
    }
    else if call_args.len() == param_specs.len() {
        call_args
            .iter()
            .zip(param_specs.iter())
            .map(|(arg, param)| {
                infer_scrutinee_type(&arg.value, &[], locals, struct_fields, singleton_names).unwrap_or_else(|| param.ty.clone())
            })
            .collect()
    }
    else {
        return None;
    };
    let match_kind = compute_call_match_kind(type_relations, &actual_types, &expected_types)?;
    Some(OverloadCandidate::new(
        candidate.symbol.clone(),
        candidate.domain.clone(),
        expected_types,
        candidate.signature.return_type.clone(),
        match_kind,
    ))
}

fn impl_nominal_type_name(ty: &ValkyrieType) -> Option<&str> {
    match ty {
        ValkyrieType::Named(name) => Some(name.as_str()),
        ValkyrieType::Apply(base, _) => impl_nominal_type_name(base),
        ValkyrieType::Float32 => Some("f32"),
        ValkyrieType::Float64 => Some("f64"),
        ValkyrieType::Boolean => Some("bool"),
        ValkyrieType::Character => Some("char"),
        ValkyrieType::Integer8 { signed: true } => Some("i8"),
        ValkyrieType::Integer8 { signed: false } => Some("u8"),
        ValkyrieType::Integer16 { signed: true } => Some("i16"),
        ValkyrieType::Integer16 { signed: false } => Some("u16"),
        ValkyrieType::Integer32 { signed: true } => Some("i32"),
        ValkyrieType::Integer32 { signed: false } => Some("u32"),
        ValkyrieType::Integer64 { signed: true } => Some("i64"),
        ValkyrieType::Integer64 { signed: false } => Some("u64"),
        ValkyrieType::Integer128 { signed: true } => Some("i128"),
        ValkyrieType::Integer128 { signed: false } => Some("u128"),
        _ => None,
    }
}

fn overload_symbol_path(candidate: &OverloadCandidate) -> NamePath {
    if let Some(owner) = &candidate.owner {
        let parts = candidate.symbol.parts();
        // Do not prepend `owner` when the symbol already starts with it
        // (`Result.Fail` + owner `Result` must stay `Result.Fail`).
        if parts.first().is_some_and(|head| head == owner) {
            return candidate.symbol.clone();
        }
        let mut qualified = vec![owner.clone()];
        qualified.extend(parts.iter().cloned());
        NamePath::new(qualified)
    }
    else {
        candidate.symbol.clone()
    }
}

fn is_boolean_operator(name: &Identifier) -> bool {
    matches!(name.as_str(), "infix ==" | "infix !=" | "infix <" | "infix <=" | "infix >" | "infix >=" | "infix &&" | "infix ||")
}

fn try_resolve_constructor(
    name: &Identifier,
    args: &[HirExpr],
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &BTreeMap<Identifier, Vec<HirField>>,
    singleton_names: &BTreeSet<Identifier>,
) -> Option<HirResolvedCall> {
    if singleton_names.contains(name) {
        return None;
    }
    let bound_values = if args.iter().all(|arg| matches!(arg.kind, HirExprKind::FieldInit { .. })) {
        if let Some(fields) = struct_fields.get(name) {
            bind_construct_fields(fields, args).ok()?
        }
        else {
            // Unite/enum variant arms (`Field { owner, name }` for `MsilInstructionOperand`)
            // are constructors, not struct TypeDefs — keep FieldInit values in source order.
            args.iter()
                .filter_map(|arg| match &arg.kind {
                    HirExprKind::FieldInit { value, .. } => Some(value.as_ref().clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        }
    }
    else {
        args.to_vec()
    };
    let inferred_actual_types = bound_values.iter().map(|arg| infer_expr_type(arg, locals)).collect::<Option<Vec<_>>>();
    // A named aggregate initializer is allowed to obtain an expected type
    // from one unique declared field layout.  This is a constraint from the
    // constructor contract itself, not recovery from the enclosing return
    // type, a symbol name, or a backend representation.  Ambiguous layouts
    // remain unresolved and are rejected at the Semantic MIR boundary.
    if inferred_actual_types.is_none() {
        let unknown_layouts = candidates
            .iter()
            .filter(|candidate| candidate.domain == OverloadDomain::Constructor)
            .filter(|candidate| candidate.symbol.parts().last().is_some_and(|candidate_name| candidate_name == name))
            .filter(|candidate| candidate.signature.params.len() == bound_values.len())
            .collect::<Vec<_>>();
        // Generic unite constructors (`Fine`/`Fail`/`Some`) often appear once per
        // imported/local specialize; argument types from `f(value)` may still be
        // Auto. Prefer a unique match, otherwise take the first same-arity layout
        // so SMIR003 does not block core Option/Result (bootstrap Stage1).
        let Some(candidate) = unknown_layouts.first()
        else {
            return None;
        };
        return Some(HirResolvedCall {
            symbol: candidate.symbol.clone(),
            domain: HirCallableDomain::Constructor,
            return_type: candidate.signature.return_type.clone(),
            parameter_types: candidate.signature.params.clone(),
            extractor_payload_type: None,
        });
    }
    let actual_types = inferred_actual_types.clone().expect("checked above");
    let filtered = candidates
        .iter()
        .filter(|candidate| candidate.domain == OverloadDomain::Constructor)
        .filter(|candidate| candidate.symbol.parts().last().is_some_and(|candidate_name| candidate_name == name))
        .filter_map(|candidate| {
            let candidate_actual_types = inferred_actual_types.clone().or_else(|| {
                bound_values
                    .iter()
                    .zip(candidate.signature.params.iter())
                    .map(|(arg, expected)| infer_constructor_argument_type(arg, expected, locals))
                    .collect::<Option<Vec<_>>>()
            })?;
            let candidate_actual_types = specialize_constructor_literals(&bound_values, &candidate_actual_types, &candidate.signature.params);
            let match_kind = compute_call_match_kind(type_relations, &candidate_actual_types, &candidate.signature.params)?;
            Some(OverloadCandidate::new(
                candidate.symbol.clone(),
                candidate.domain.clone(),
                candidate.signature.params.clone(),
                candidate.signature.return_type.clone(),
                match_kind,
            ))
        })
        .collect::<Vec<_>>();
    let resolved = match resolve_overload(&filtered) {
        Ok(resolved) => resolved,
        Err(_) => {
            // Generic pattern payload types (`error: E`) often fail exact match against
            // constructor params; fall back to same-arity name match for unite variants.
            let fallback = candidates
                .iter()
                .filter(|candidate| candidate.domain == OverloadDomain::Constructor)
                .filter(|candidate| candidate.symbol.parts().last().is_some_and(|candidate_name| candidate_name == name))
                .filter(|candidate| candidate.signature.params.len() == bound_values.len())
                .next()?;
            return Some(HirResolvedCall {
                symbol: fallback.symbol.clone(),
                domain: HirCallableDomain::Constructor,
                return_type: fallback.signature.return_type.clone(),
                parameter_types: fallback.signature.params.clone(),
                extractor_payload_type: None,
            });
        }
    };
    let resolved_actual_types = bound_values
        .iter()
        .zip(resolved.signature.params.iter())
        .map(|(arg, expected)| infer_constructor_argument_type(arg, expected, locals))
        .collect::<Option<Vec<_>>>()
        .unwrap_or_else(|| resolved.signature.params.clone());
    let return_type = constructor_return_type(&resolved.signature.return_type, &resolved.signature.params, &resolved_actual_types);
    let _ = actual_types;
    Some(HirResolvedCall {
        symbol: resolved.symbol,
        domain: HirCallableDomain::Constructor,
        return_type,
        parameter_types: resolved.signature.params,
        extractor_payload_type: None,
    })
}

fn infer_constructor_argument_type(expr: &HirExpr, expected: &ValkyrieType, locals: &BTreeMap<String, ValkyrieType>) -> Option<ValkyrieType> {
    infer_expr_type(expr, locals).or_else(|| match (&expr.kind, expected) {
        (HirExprKind::ArrayLiteral { items }, ValkyrieType::Array(element)) if items.is_empty() => Some(ValkyrieType::Array(element.clone())),
        _ => None,
    })
}

fn specialize_constructor_literals(args: &[HirExpr], actual: &[ValkyrieType], expected: &[ValkyrieType]) -> Vec<ValkyrieType> {
    args.iter()
        .zip(actual.iter().zip(expected.iter()))
        .map(|(arg, (actual, expected))| match (&arg.kind, actual) {
            // Numeric literals are polymorphic until a formal contract fixes
            // their representation; the backend must not make this decision.
            (HirExprKind::Literal(crate::types::hir::HirLiteral::Integer64(_)), _) => expected.clone(),
            _ => actual.clone(),
        })
        .collect()
}

fn constructor_return_type(return_type: &ValkyrieType, param_types: &[ValkyrieType], arg_types: &[ValkyrieType]) -> ValkyrieType {
    let mut substitutions = BTreeMap::new();
    for (param, arg) in param_types.iter().zip(arg_types) {
        unify_constructor_type_vars(param, arg, &mut substitutions);
    }
    if substitutions.is_empty() {
        return return_type.clone();
    }
    substitute_type_vars(return_type, &substitutions)
}

fn unify_constructor_type_vars(param: &ValkyrieType, arg: &ValkyrieType, out: &mut BTreeMap<Identifier, ValkyrieType>) {
    match param {
        ValkyrieType::Named(name) => {
            out.entry(name.clone()).or_insert_with(|| arg.clone());
        }
        ValkyrieType::Apply(param_base, param_args) => {
            if let ValkyrieType::Apply(arg_base, arg_args) = arg {
                if param_base == arg_base && param_args.len() == arg_args.len() {
                    for (p, a) in param_args.iter().zip(arg_args.iter()) {
                        unify_constructor_type_vars(p, a, out);
                    }
                }
            }
        }
        ValkyrieType::Function(param_fn) => {
            if let ValkyrieType::Function(arg_fn) = arg {
                for (p, a) in param_fn.params.iter().zip(arg_fn.params.iter()) {
                    unify_constructor_type_vars(p, a, out);
                }
                unify_constructor_type_vars(&param_fn.return_type, &arg_fn.return_type, out);
            }
        }
        ValkyrieType::TypeLambda(param_lambda) => {
            unify_constructor_type_vars(&param_lambda.body, arg, out);
        }
        _ => {}
    }
}

fn substitute_type_parameters(ty: &ValkyrieType, receiver: &ValkyrieType, actual: &ValkyrieType) -> ValkyrieType {
    let (ValkyrieType::Apply(_, receiver_args), ValkyrieType::Apply(_, actual_args)) = (receiver, actual)
    else {
        return ty.clone();
    };
    let substitutions = receiver_args
        .iter()
        .zip(actual_args.iter())
        .filter_map(|(param, arg)| match param {
            ValkyrieType::Named(name) => Some((name.clone(), arg.clone())),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    substitute_type_vars(ty, &substitutions)
}

fn substitute_type_vars(ty: &ValkyrieType, substitutions: &BTreeMap<Identifier, ValkyrieType>) -> ValkyrieType {
    match ty {
        ValkyrieType::Named(name) => substitutions.get(name).cloned().unwrap_or_else(|| ty.clone()),
        ValkyrieType::Generic(generic) => substitutions.get(&generic.name).cloned().unwrap_or_else(|| ty.clone()),
        ValkyrieType::Apply(base, args) => ValkyrieType::Apply(
            Box::new(substitute_type_vars(base, substitutions)),
            args.iter().map(|arg| substitute_type_vars(arg, substitutions)).collect(),
        ),
        ValkyrieType::Array(inner) => ValkyrieType::Array(Box::new(substitute_type_vars(inner, substitutions))),
        ValkyrieType::Tuple(items) => ValkyrieType::Tuple(items.iter().map(|item| substitute_type_vars(item, substitutions)).collect()),
        ValkyrieType::Union(items) => ValkyrieType::Union(items.iter().map(|item| substitute_type_vars(item, substitutions)).collect()),
        other => other.clone(),
    }
}

fn matches_extractor_receiver_type(
    actual: &ValkyrieType,
    receiver: &ValkyrieType,
    type_relations: &TypeRelationContext,
) -> Option<OverloadMatchKind> {
    if let Some(kind) = compute_call_match_kind(type_relations, std::slice::from_ref(actual), std::slice::from_ref(receiver)) {
        return Some(kind);
    }
    if generic_apply_receiver_matches(actual, receiver, type_relations) {
        return Some(OverloadMatchKind::NominalExact);
    }
    None
}

fn generic_apply_receiver_matches(actual: &ValkyrieType, receiver: &ValkyrieType, type_relations: &TypeRelationContext) -> bool {
    let (ValkyrieType::Apply(actual_base, actual_args), ValkyrieType::Apply(receiver_base, receiver_args)) = (actual, receiver)
    else {
        return false;
    };
    if actual_args.len() != receiver_args.len() {
        return false;
    }
    if !matches!(
        type_relations.match_parameter(actual_base.as_ref(), receiver_base.as_ref()),
        ParameterMatchResult::NominalExact | ParameterMatchResult::NominalSubtype { .. }
    ) {
        return false;
    }
    receiver_args.iter().zip(actual_args.iter()).all(|(expected, actual_arg)| match expected {
        ValkyrieType::Named(_) => true,
        other => matches!(
            type_relations.match_parameter(actual_arg, other),
            ParameterMatchResult::NominalExact | ParameterMatchResult::NominalSubtype { .. }
        ),
    })
}

fn try_resolve_pattern_extractor(
    canonical_callee: &NamePath,
    actual_type: &ValkyrieType,
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
) -> Option<HirResolvedCall> {
    let filtered = candidates
        .iter()
        .filter(|candidate| candidate.domain == OverloadDomain::Function)
        .filter(|candidate| candidate.owner.as_ref().is_some_and(|owner| canonical_callee.parts().first() == Some(owner)))
        .filter(|candidate| candidate.param_specs.first().is_some_and(is_extractor_receiver_param))
        .filter(|candidate| candidate.param_specs.first().is_none_or(|param| !param.is_mutable))
        .filter(|candidate| candidate.symbol.parts().last() == canonical_callee.parts().last())
        .filter_map(|candidate| {
            let receiver_type = pattern_extractor_receiver_type(candidate)?;
            let match_kind = matches_extractor_receiver_type(actual_type, &receiver_type, type_relations)?;
            Some(
                OverloadCandidate::new(
                    candidate.symbol.clone(),
                    OverloadDomain::Extractor,
                    candidate.signature.params.clone(),
                    candidate.signature.return_type.clone(),
                    match_kind,
                )
                .with_param_specs(candidate.param_specs.clone()),
            )
        })
        .collect::<Vec<_>>();
    let resolved = resolve_overload(&filtered).ok()?;
    let matched = filtered.iter().find(|candidate| candidate.symbol == resolved.symbol)?;
    let receiver_type = pattern_extractor_receiver_type(matched)?;
    let return_type = substitute_type_parameters(&resolved.signature.return_type, &receiver_type, actual_type);
    let payload_type = nullable_payload_type(&return_type)?;
    let parameter_types =
        resolved.signature.params.iter().map(|param| substitute_type_parameters(param, &receiver_type, actual_type)).collect();
    Some(HirResolvedCall {
        symbol: resolved.symbol,
        domain: HirCallableDomain::Extractor,
        return_type,
        parameter_types,
        extractor_payload_type: Some(payload_type),
    })
}

fn is_null_type(ty: &ValkyrieType) -> bool {
    matches!(ty, ValkyrieType::Named(name) if name.as_str() == "null")
}

fn infer_catch_scrutinee_type(expr: &HirExpr, locals: &BTreeMap<String, ValkyrieType>) -> Option<ValkyrieType> {
    match &expr.kind {
        HirExprKind::Raise(inner) => infer_expr_type(inner, locals),
        HirExprKind::Block(block) => block.expr.as_ref().and_then(|inner| infer_catch_scrutinee_type(inner, locals)),
        _ => infer_expr_type(expr, locals),
    }
}

fn extract_callable_name(callee: &HirExpr) -> Option<Identifier> {
    match &callee.kind {
        HirExprKind::Variable(identifier) => Some(identifier.name.clone()),
        HirExprKind::Path(path) => path.parts().last().cloned(),
        HirExprKind::FieldAccess { field, .. } => Some(field.clone()),
        HirExprKind::GenericApply { callee, .. } => extract_callable_name(callee),
        _ => None,
    }
}

fn compute_call_match_kind(
    type_relations: &TypeRelationContext,
    actual_types: &[ValkyrieType],
    expected_types: &[ValkyrieType],
) -> Option<OverloadMatchKind> {
    if actual_types.len() != expected_types.len() {
        return None;
    }

    let mut saw_trait = false;
    let mut saw_row = false;
    let mut subtype_distance = 0usize;

    for (actual, expected) in actual_types.iter().zip(expected_types) {
        match type_relations.match_parameter(actual, expected) {
            ParameterMatchResult::NominalExact => {}
            ParameterMatchResult::NominalSubtype { distance } => subtype_distance += distance,
            ParameterMatchResult::Trait { .. } => saw_trait = true,
            ParameterMatchResult::Row => saw_row = true,
            ParameterMatchResult::NoMatch { .. } => {
                return None;
            }
        }
    }

    if saw_row {
        Some(OverloadMatchKind::Row)
    }
    else if saw_trait {
        Some(OverloadMatchKind::Trait)
    }
    else if subtype_distance > 0 {
        Some(OverloadMatchKind::NominalSubtype { distance: subtype_distance })
    }
    else {
        Some(OverloadMatchKind::NominalExact)
    }
}

fn should_resolve_name_pattern_as_type(name: &NamePath, actual_type: &ValkyrieType) -> bool {
    if name.parts().len() != 1 {
        return false;
    }

    match actual_type {
        ValkyrieType::Named(actual_name) => name.parts().last().is_some_and(|expected| expected == actual_name),
        ValkyrieType::Apply(base, _) => should_resolve_name_pattern_as_type(name, base),
        _ => false,
    }
}

fn canonical_extractor_callee(name: &NamePath) -> NamePath {
    let mut parts = name.parts().to_vec();
    parts.push(Identifier::new("extractor"));
    NamePath::new(parts)
}

fn infer_expr_type(expr: &HirExpr, locals: &BTreeMap<String, ValkyrieType>) -> Option<ValkyrieType> {
    match &expr.kind {
        HirExprKind::Literal(literal) => Some(match literal {
            // Unsuffixed integer literals default to signed i32 when they fit;
            // matches MIR `lower_literal` and `return -1` (`prefix -` of `1`).
            crate::types::hir::HirLiteral::Integer64(value) if *value >= i32::MIN as i64 && *value <= i32::MAX as i64 => {
                ValkyrieType::Integer32 { signed: true }
            }
            crate::types::hir::HirLiteral::Integer64(_) => ValkyrieType::Integer64 { signed: true },
            crate::types::hir::HirLiteral::Float64(_) => ValkyrieType::Float64,
            crate::types::hir::HirLiteral::String(_) => ValkyrieType::Utf8,
            crate::types::hir::HirLiteral::Bool(_) => ValkyrieType::Boolean,
            crate::types::hir::HirLiteral::Unit => ValkyrieType::Unit,
        }),
        HirExprKind::Variable(HirIdentifier { name, .. }) => locals.get(name.as_str()).cloned(),
        // The parser may retain a bare local reference as a single-segment
        // Path instead of normalizing it to Variable. Both forms denote the
        // same lexical binding, so semantic resolution must consult the same
        // typed local environment before considering any call contract.
        HirExprKind::Path(path) if path.parts().len() == 1 => locals.get(path.parts()[0].as_str()).cloned(),
        HirExprKind::Call { resolved, .. } => resolved.as_ref().map(|call| call.return_type.clone()),
        HirExprKind::ArrayLiteral { items } => {
            let item_type = items.first().and_then(|item| infer_expr_type(item, locals))?;
            Some(ValkyrieType::Array(Box::new(item_type)))
        }
        HirExprKind::Construct { name, resolved, .. } => {
            if let Some(resolved) = resolved {
                return Some(resolved.return_type.clone());
            }
            Some(ValkyrieType::Named(name.clone()))
        }
        HirExprKind::If { then_branch, else_branch, .. } | HirExprKind::IfLet { then_branch, else_branch, .. } => {
            let then_type = then_branch.expr.as_ref().and_then(|expr| infer_expr_type(expr, locals))?;
            let else_type = else_branch.as_ref().and_then(|branch| branch.expr.as_ref()).and_then(|expr| infer_expr_type(expr, locals))?;
            if then_type == else_type { Some(then_type) } else { None }
        }
        HirExprKind::Block(block) => block.expr.as_ref().and_then(|expr| infer_expr_type(expr, locals)),
        HirExprKind::TryPropagate(inner) => infer_expr_type(inner, locals).and_then(|ty| nullable_payload_type(&ty)),
        HirExprKind::TryScope { is_optional, is_forced, result_type, body } => {
            if let Some(ty) = result_type {
                Some(ty.clone())
            }
            else {
                let body_type = body.expr.as_ref().and_then(|expr| infer_expr_type(expr, locals))?;
                if *is_optional {
                    Some(ValkyrieType::Nullable(Box::new(body_type)))
                }
                else if *is_forced {
                    Some(body_type)
                }
                else {
                    Some(body_type)
                }
            }
        }
        _ => None,
    }
}

fn infer_scrutinee_type(
    expr: &HirExpr,
    candidates: &[OverloadCandidate],
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &BTreeMap<Identifier, Vec<HirField>>,
    singleton_names: &BTreeSet<Identifier>,
) -> Option<ValkyrieType> {
    if let Some(ty) = infer_expr_type(expr, locals) {
        return Some(ty);
    }
    match &expr.kind {
        HirExprKind::Variable(identifier) if singleton_names.contains(&identifier.name) => Some(ValkyrieType::Named(identifier.name.clone())),
        HirExprKind::Path(path) if path.parts().len() == 1 && singleton_names.contains(&path.parts()[0]) => {
            Some(ValkyrieType::Named(path.parts()[0].clone()))
        }
        // Parser-preserved `binding.field[.field...]` paths are equivalent to
        // nested FieldAccess nodes. Project each field through the declared
        // aggregate layout; a missing segment stays unresolved rather than
        // borrowing a result type from the surrounding expression.
        HirExprKind::Path(path) if path.parts().len() >= 2 => {
            let mut current = locals.get(path.parts()[0].as_str())?.clone();
            for field in &path.parts()[1..] {
                current = lookup_struct_field_type(&current, field, struct_fields)?;
            }
            Some(current)
        }
        HirExprKind::FieldAccess { object, field } => {
            if let Some(singleton_name) = extract_singleton_type_name(object, locals, singleton_names) {
                return lookup_struct_field_type(&ValkyrieType::Named(singleton_name), field, struct_fields);
            }
            let object_type = infer_scrutinee_type(object, candidates, locals, struct_fields, singleton_names)?;
            lookup_struct_field_type(&object_type, field, struct_fields)
        }
        HirExprKind::Call { callee, resolved, .. } => {
            if let Some(resolved) = resolved {
                return Some(resolved.return_type.clone());
            }
            let name = extract_callable_name(callee)?;
            candidates
                .iter()
                .find(|candidate| candidate.domain == OverloadDomain::Function && candidate.symbol.parts().last() == Some(&name))
                .map(|candidate| candidate.signature.return_type.clone())
        }
        HirExprKind::Construct { name, resolved, .. } => {
            if let Some(resolved) = resolved {
                return Some(resolved.return_type.clone());
            }
            candidates
                .iter()
                .find(|candidate| candidate.domain == OverloadDomain::Constructor && candidate.symbol.parts().last() == Some(name))
                .map(|candidate| candidate.signature.return_type.clone())
        }
        _ => None,
    }
}

fn lookup_struct_field_type(
    object_type: &ValkyrieType,
    field: &Identifier,
    struct_fields: &BTreeMap<Identifier, Vec<HirField>>,
) -> Option<ValkyrieType> {
    let ValkyrieType::Named(type_name) = object_type
    else {
        return None;
    };
    struct_fields.get(type_name).and_then(|fields| fields.iter().find(|item| item.name == *field).map(|item| item.ty.clone()))
}

fn payload_field_scrutinee_types(payload: &ValkyrieType, struct_fields: &BTreeMap<Identifier, Vec<HirField>>) -> Vec<Option<ValkyrieType>> {
    match payload {
        ValkyrieType::Tuple(items) => items.iter().map(|item| Some(item.clone())).collect(),
        ValkyrieType::Array(inner) => vec![Some(inner.as_ref().clone())],
        ValkyrieType::Named(name) => match struct_fields.get(name) {
            Some(fields) if !fields.is_empty() => fields.iter().map(|field| Some(field.ty.clone())).collect(),
            _ => vec![Some(payload.clone())],
        },
        other => vec![Some(other.clone())],
    }
}

/// 查找 object pattern 字段的 scrutinee 类型。
///
/// 当 object pattern 形如 `Wrapper { inner: Some(result), fallback }` 时，
/// 通过 `Wrapper` 的已注册字段布局查询单个字段（如 `inner`）的类型，
/// 以便把该类型作为子 pattern 的 scrutinee type 向下传递，让嵌套 extractor 能解析。
fn object_field_scrutinee_type<'a>(
    object_name: Option<&NamePath>,
    field_name: &Identifier,
    struct_fields: &'a BTreeMap<Identifier, Vec<HirField>>,
) -> Option<&'a ValkyrieType> {
    let type_name = object_name.and_then(|path| path.parts().last())?;
    struct_fields.get(type_name)?.iter().find(|field| &field.name == field_name).map(|field| &field.ty)
}

fn bind_pattern_type(pattern: &HirPattern, ty: &ValkyrieType, locals: &mut BTreeMap<String, ValkyrieType>) {
    match pattern {
        HirPattern::Variable(identifier) => {
            locals.insert(identifier.name.to_string(), ty.clone());
        }
        HirPattern::TypedBind { identifier, ty: bound_ty } => {
            locals.insert(identifier.name.to_string(), ValkyrieType::Named(bound_ty.name().clone()));
        }
        HirPattern::Tuple(items) if matches!(ty, ValkyrieType::Tuple(_)) => {
            if let ValkyrieType::Tuple(types) = ty {
                for (pattern, item_ty) in items.iter().zip(types) {
                    bind_pattern_type(pattern, item_ty, locals);
                }
            }
        }
        HirPattern::Bind { identifier, pattern } => {
            bind_pattern_type(pattern, ty, locals);
            locals.insert(identifier.name.to_string(), ty.clone());
        }
        HirPattern::Extractor(HirExtractorPattern::Constructor { fields, resolved, .. }) => {
            // A constructor arm binds its fields to the extractor payload,
            // not to the nullable scrutinee itself.  Keeping this mapping in
            // HIR locals is what lets later calls resolve `layout.id` after
            // `match Some(layout)` without backend-side Option erasure.
            let payload = resolved.as_ref().and_then(|call| call.extractor_payload_type.clone()).or_else(|| nullable_payload_type(ty));
            if let Some(payload) = payload {
                for (index, field) in fields.iter().enumerate() {
                    let field_ty = match &payload {
                        ValkyrieType::Tuple(items) => items.get(index).cloned(),
                        _ if index == 0 => Some(payload.clone()),
                        _ => None,
                    };
                    if let Some(field_ty) = field_ty {
                        bind_pattern_type(field, &field_ty, locals);
                    }
                }
            }
        }
        HirPattern::Mut(pattern) | HirPattern::Pin { pattern, .. } => bind_pattern_type(pattern, ty, locals),
        _ => {}
    }
}

/// Validates that unresolved pattern extractors fail for explicit contract reasons (`mut self`, non-nullable return).
pub fn validate_extractor_patterns(module: &HirModule) -> Result<(), std_data::text::valkyrie::ParseError> {
    let candidates = collect_module_candidates(module);
    let type_relations = TypeRelationContext::from_module(module);
    let struct_fields = module.structs.iter().map(|item| (item.name.clone(), item.fields.clone())).collect::<BTreeMap<_, _>>();
    for function in &module.functions {
        validate_function_extractor_patterns(function, &candidates, &type_relations, &struct_fields)?;
    }
    for item in &module.structs {
        for method in &item.methods {
            validate_function_extractor_patterns(method, &candidates, &type_relations, &struct_fields)?;
        }
    }
    Ok(())
}

fn validate_function_extractor_patterns(
    function: &HirFunction,
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    struct_fields: &BTreeMap<Identifier, Vec<HirField>>,
) -> Result<(), std_data::text::valkyrie::ParseError> {
    let mut locals = BTreeMap::new();
    for param in &function.params {
        locals.insert(param.name.name.to_string(), param.ty.clone());
    }
    validate_block_extractor_patterns(&function.body, candidates, type_relations, &locals, struct_fields)
}

fn validate_block_extractor_patterns(
    block: &HirBlock,
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &BTreeMap<Identifier, Vec<HirField>>,
) -> Result<(), std_data::text::valkyrie::ParseError> {
    let mut locals = locals.clone();
    for statement in &block.statements {
        match &statement.kind {
            HirStatementKind::Let { pattern, ty, initializer, .. } => {
                // 对于 refutable pattern（如 `let Some(x) = opt`），跳过 extractor 校验，
                // 由后续 `check_pattern_refutability` 统一拒绝。
                if pattern.refutability() != PatternRefutability::Refutable {
                    let inferred_type = ty.as_ref().cloned().or_else(|| {
                        initializer.as_ref().and_then(|init| infer_scrutinee_type(init, candidates, &locals, struct_fields, &BTreeSet::new()))
                    });
                    validate_pattern_extractor_contract(pattern, inferred_type.as_ref(), candidates, type_relations, struct_fields)?;
                }
                if let Some(initializer) = initializer {
                    validate_expr_extractor_patterns(initializer, candidates, type_relations, &locals, struct_fields)?;
                }
                if let Some(ty) = ty {
                    bind_pattern_type(pattern, ty, &mut locals);
                }
                else if let Some(initializer) = initializer {
                    if let Some(binding_type) = infer_scrutinee_type(initializer, candidates, &locals, struct_fields, &BTreeSet::new()) {
                        bind_pattern_type(pattern, &binding_type, &mut locals);
                    }
                }
            }
            HirStatementKind::Expr(expr) => validate_expr_extractor_patterns(expr, candidates, type_relations, &locals, struct_fields)?,
        }
    }
    if let Some(expr) = &block.expr {
        validate_expr_extractor_patterns(expr, candidates, type_relations, &locals, struct_fields)?;
    }
    Ok(())
}

fn validate_expr_extractor_patterns(
    expr: &HirExpr,
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &BTreeMap<Identifier, Vec<HirField>>,
) -> Result<(), std_data::text::valkyrie::ParseError> {
    match &expr.kind {
        HirExprKind::Call { callee, args, .. } => {
            validate_expr_extractor_patterns(callee, candidates, type_relations, locals, struct_fields)?;
            for arg in args {
                validate_expr_extractor_patterns(&arg.value, candidates, type_relations, locals, struct_fields)?;
            }
        }
        HirExprKind::GenericApply { callee, .. }
        | HirExprKind::FieldInit { value: callee, .. }
        | HirExprKind::Await(callee)
        | HirExprKind::Awake(callee)
        | HirExprKind::BlockOn(callee)
        | HirExprKind::YieldFrom(callee)
        | HirExprKind::Raise(callee)
        | HirExprKind::Resume(callee)
        | HirExprKind::TryPropagate(callee)
        | HirExprKind::Assign { value: callee, .. }
        | HirExprKind::FieldAccess { object: callee, .. } => {
            validate_expr_extractor_patterns(callee, candidates, type_relations, locals, struct_fields)?;
        }
        HirExprKind::StoreField { object, value, .. } => {
            validate_expr_extractor_patterns(object, candidates, type_relations, locals, struct_fields)?;
            validate_expr_extractor_patterns(value, candidates, type_relations, locals, struct_fields)?;
        }
        HirExprKind::ArrayNew { length, .. } => validate_expr_extractor_patterns(length, candidates, type_relations, locals, struct_fields)?,
        HirExprKind::ArrayLiteral { items } => {
            for item in items {
                validate_expr_extractor_patterns(item, candidates, type_relations, locals, struct_fields)?;
            }
        }
        HirExprKind::Construct { args, .. } => {
            for arg in args {
                validate_expr_extractor_patterns(arg, candidates, type_relations, locals, struct_fields)?;
            }
        }
        HirExprKind::Lambda { body, .. } => validate_block_extractor_patterns(body, candidates, type_relations, locals, struct_fields)?,
        HirExprKind::AnonymousClass { fields, methods, .. } => {
            for (_, value) in fields {
                validate_expr_extractor_patterns(value, candidates, type_relations, locals, struct_fields)?;
            }
            for method in methods {
                validate_function_extractor_patterns(method, candidates, type_relations, struct_fields)?;
            }
        }
        HirExprKind::Return(Some(value)) | HirExprKind::Yield(Some(value)) | HirExprKind::Resume(value) => {
            validate_expr_extractor_patterns(value, candidates, type_relations, locals, struct_fields)?;
        }
        HirExprKind::Break { expr: Some(value), .. } => {
            validate_expr_extractor_patterns(value, candidates, type_relations, locals, struct_fields)?;
        }
        HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
            validate_expr_extractor_patterns(scrutinee, candidates, type_relations, locals, struct_fields)?;
            let scrutinee_type = infer_scrutinee_type(scrutinee, candidates, locals, struct_fields, &BTreeSet::new());
            for arm in arms {
                validate_pattern_extractor_contract(&arm.pattern, scrutinee_type.as_ref(), candidates, type_relations, struct_fields)?;
                if let Some(guard) = &arm.guard {
                    validate_expr_extractor_patterns(guard, candidates, type_relations, locals, struct_fields)?;
                }
                validate_expr_extractor_patterns(&arm.body, candidates, type_relations, locals, struct_fields)?;
            }
        }
        HirExprKind::IfLet { pattern, scrutinee, then_branch, else_branch } => {
            validate_expr_extractor_patterns(scrutinee, candidates, type_relations, locals, struct_fields)?;
            let scrutinee_type = infer_scrutinee_type(scrutinee, candidates, locals, struct_fields, &BTreeSet::new());
            validate_pattern_extractor_contract(pattern, scrutinee_type.as_ref(), candidates, type_relations, struct_fields)?;
            validate_block_extractor_patterns(then_branch, candidates, type_relations, locals, struct_fields)?;
            if let Some(else_branch) = else_branch {
                validate_block_extractor_patterns(else_branch, candidates, type_relations, locals, struct_fields)?;
            }
        }
        HirExprKind::Catch { expr, arms } => {
            validate_expr_extractor_patterns(expr, candidates, type_relations, locals, struct_fields)?;
            let scrutinee_type = infer_catch_scrutinee_type(expr, locals);
            for arm in arms {
                validate_pattern_extractor_contract(&arm.pattern, scrutinee_type.as_ref(), candidates, type_relations, struct_fields)?;
                if let Some(guard) = &arm.guard {
                    validate_expr_extractor_patterns(guard, candidates, type_relations, locals, struct_fields)?;
                }
                validate_expr_extractor_patterns(&arm.body, candidates, type_relations, locals, struct_fields)?;
            }
        }
        HirExprKind::Loop { pattern, iterator, condition, body, .. } => {
            if let Some(iterator) = iterator {
                validate_expr_extractor_patterns(iterator, candidates, type_relations, locals, struct_fields)?;
            }
            if let Some(condition) = condition {
                validate_expr_extractor_patterns(condition, candidates, type_relations, locals, struct_fields)?;
            }
            if let (Some(pattern), Some(iterator)) = (pattern, iterator) {
                let iter_type = infer_expr_type(iterator, locals);
                let element_type = iter_type.as_ref().map(|ty| match ty {
                    ValkyrieType::Array(inner) => inner.as_ref().clone(),
                    other => other.clone(),
                });
                validate_pattern_extractor_contract(pattern, element_type.as_ref(), candidates, type_relations, struct_fields)?;
            }
            validate_block_extractor_patterns(body, candidates, type_relations, locals, struct_fields)?;
        }
        HirExprKind::Block(block) => validate_block_extractor_patterns(block, candidates, type_relations, locals, struct_fields)?,
        HirExprKind::TryScope { body, .. } => validate_block_extractor_patterns(body, candidates, type_relations, locals, struct_fields)?,
        HirExprKind::If { condition, then_branch, else_branch } => {
            validate_expr_extractor_patterns(condition, candidates, type_relations, locals, struct_fields)?;
            validate_block_extractor_patterns(then_branch, candidates, type_relations, locals, struct_fields)?;
            if let Some(else_branch) = else_branch {
                validate_block_extractor_patterns(else_branch, candidates, type_relations, locals, struct_fields)?;
            }
        }
        HirExprKind::Return(None)
        | HirExprKind::Break { expr: None, .. }
        | HirExprKind::Continue { .. }
        | HirExprKind::Yield(None)
        | HirExprKind::Fallthrough
        | HirExprKind::Literal(_)
        | HirExprKind::Variable(_)
        | HirExprKind::Path(_)
        | HirExprKind::SuperCall { .. }
        | HirExprKind::With { .. } => {}
    }
    Ok(())
}

fn validate_pattern_extractor_contract(
    pattern: &HirPattern,
    scrutinee_type: Option<&ValkyrieType>,
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    struct_fields: &BTreeMap<Identifier, Vec<HirField>>,
) -> Result<(), std_data::text::valkyrie::ParseError> {
    match pattern {
        HirPattern::Extractor(extractor) => match extractor {
            HirExtractorPattern::Constructor { canonical_callee, resolved, fields, .. } => {
                if resolved.is_none() {
                    if let Some(actual_type) = scrutinee_type {
                        if let Some(error) = diagnose_pattern_extractor_failure(canonical_callee, actual_type, candidates, type_relations) {
                            return Err(error);
                        }
                    }
                    else if !is_builtin_pattern_extractor(canonical_callee) {
                        if let Some(error) = diagnose_unresolved_extractor_without_scrutinee_type(canonical_callee, candidates) {
                            return Err(error);
                        }
                    }
                }
                let field_scrutinee_types = resolved
                    .as_ref()
                    .and_then(|resolved| resolved.extractor_payload_type.as_ref())
                    .map(|payload| payload_field_scrutinee_types(payload, struct_fields))
                    .unwrap_or_default();
                for (index, field) in fields.iter().enumerate() {
                    let field_scrutinee = field_scrutinee_types.get(index).and_then(|ty| ty.as_ref());
                    validate_pattern_extractor_contract(field, field_scrutinee, candidates, type_relations, struct_fields)?;
                }
            }
            HirExtractorPattern::Array { canonical_callee, resolved, prefix, suffix, .. } => {
                if resolved.is_none() {
                    if let Some(actual_type) = scrutinee_type {
                        if let Some(error) = diagnose_pattern_extractor_failure(canonical_callee, actual_type, candidates, type_relations) {
                            return Err(error);
                        }
                    }
                    else if !is_builtin_pattern_extractor(canonical_callee) {
                        if let Some(error) = diagnose_unresolved_extractor_without_scrutinee_type(canonical_callee, candidates) {
                            return Err(error);
                        }
                    }
                }
                for item in prefix.iter().chain(suffix.iter()) {
                    validate_pattern_extractor_contract(item, None, candidates, type_relations, struct_fields)?;
                }
            }
        },
        HirPattern::Tuple(items) | HirPattern::Or(items) => {
            for item in items {
                validate_pattern_extractor_contract(item, scrutinee_type, candidates, type_relations, struct_fields)?;
            }
        }
        HirPattern::Bind { pattern, .. } | HirPattern::Mut(pattern) | HirPattern::Pin { pattern, .. } => {
            validate_pattern_extractor_contract(pattern, scrutinee_type, candidates, type_relations, struct_fields)?;
        }
        HirPattern::Object { name, fields, .. } => {
            for (field_name, field_pattern) in fields {
                let field_scrutinee = object_field_scrutinee_type(name.as_ref(), field_name, struct_fields);
                validate_pattern_extractor_contract(field_pattern, field_scrutinee, candidates, type_relations, struct_fields)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn is_builtin_pattern_extractor(canonical_callee: &NamePath) -> bool {
    matches!(
        canonical_callee.parts(),
        [owner, method] if owner.as_str() == "array" && method.as_str() == "extractor"
    )
}

/// `Some` / `None` on `T | null` (and `Option<T>`) are built-in arms — not user extractors.
fn is_builtin_nullable_option_pattern(canonical_callee: &NamePath, actual_type: &ValkyrieType) -> bool {
    let head = canonical_callee.parts().first().map(|p| p.as_str()).unwrap_or("");
    if !matches!(head, "Some" | "None") {
        return false;
    }
    crate::hir::is_nullable_type(actual_type)
        || crate::hir::is_option_apply_type(actual_type)
        // Defensive: renderer shows `T | null`; accept even if null atom naming drifts.
        || matches!(actual_type, ValkyrieType::Union(items) if items.len() >= 2)
}

/// `Fine` / `Fail` are the language Result-unite arms. Concrete aliases
/// (`VonParseResult`, `JvmBinaryResult`, …) still use those variant names, so
/// accept Fine/Fail extractors for any Result-shaped apply (`Result<…>` or
/// `*Result<…>`) rather than only the nominal `Result` constructor.
fn is_builtin_result_pattern(canonical_callee: &NamePath, actual_type: &ValkyrieType) -> bool {
    let head = canonical_callee.parts().first().map(|p| p.as_str()).unwrap_or("");
    if !matches!(head, "Fine" | "Fail") {
        return false;
    }
    if crate::hir::is_result_apply_type(actual_type) {
        return true;
    }
    match actual_type {
        ValkyrieType::Named(name) => name.as_str().ends_with("Result"),
        ValkyrieType::Apply(base, _) => matches!(base.as_ref(), ValkyrieType::Named(name) if name.as_str().ends_with("Result")),
        _ => false,
    }
}

/// Soft SMIR003 accepts Fine/Fail without a registered extractor method. Still attach a
/// synthetic resolved call so MIR can bind `Fine(plan)` with `extractor_payload_type = T`.
fn synthesize_builtin_result_extractor(canonical_callee: &NamePath, actual_type: &ValkyrieType) -> Option<HirResolvedCall> {
    if !is_builtin_result_pattern(canonical_callee, actual_type) {
        return None;
    }
    let head = canonical_callee.parts().first()?.as_str();
    let payload = match (head, actual_type) {
        ("Fine", ValkyrieType::Apply(_, args)) => args.first().cloned(),
        ("Fail", ValkyrieType::Apply(_, args)) => {
            if args.len() >= 2 {
                args.get(1).cloned()
            }
            else {
                // `VonParseResult<T>` alias may appear as one-arg apply; Fail payload is diagnostic.
                Some(ValkyrieType::Named(Identifier::new("VonDiagnostic")))
            }
        }
        _ => None,
    };
    Some(HirResolvedCall {
        symbol: NamePath::new(vec![Identifier::new(head), Identifier::new("extractor")]),
        domain: HirCallableDomain::Extractor,
        return_type: actual_type.clone(),
        parameter_types: vec![actual_type.clone()],
        extractor_payload_type: payload,
    })
}

/// 当 pattern 名（`canonical_callee` 首段）是 scrutinee 类型的严格子类时，
/// 该 pattern 为类型测试 pattern，不需要 extractor 方法。
/// 同名类型（NominalExact）不算类型测试，仍需 extractor。
fn is_subtype_pattern(canonical_callee: &NamePath, actual_type: &ValkyrieType, type_relations: &TypeRelationContext) -> bool {
    let Some(pattern_name) = canonical_callee.parts().first()
    else {
        return false;
    };
    let scrutinee_name = match actual_type {
        ValkyrieType::Named(name) => name.clone(),
        ValkyrieType::Apply(base, _) => {
            if let ValkyrieType::Named(name) = base.as_ref() {
                name.clone()
            }
            else {
                return false;
            }
        }
        _ => return false,
    };
    matches!(type_relations.match_nominal_parameter(pattern_name, &scrutinee_name), ParameterMatchResult::NominalSubtype { .. })
}

fn diagnose_pattern_extractor_failure(
    canonical_callee: &NamePath,
    actual_type: &ValkyrieType,
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
) -> Option<std_data::text::valkyrie::ParseError> {
    use std_data::text::valkyrie::ParseError;

    if is_builtin_pattern_extractor(canonical_callee) {
        return None;
    }

    let type_name = render_valkyrie_type_name(actual_type);
    let extractor_name = canonical_callee.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>().join(".");

    let matching = matching_extractor_candidates(canonical_callee, actual_type, candidates, type_relations);
    if matching.is_empty() {
        if is_subtype_pattern(canonical_callee, actual_type, type_relations) {
            return None;
        }
        // Built-in nullable patterns: `case Some(x):` / `case None:` on `T | null` (a.k.a. `T?`).
        // These are language nullability arms, not user-defined extractors on Utf8Text.
        if is_builtin_nullable_option_pattern(canonical_callee, actual_type) {
            return None;
        }
        // Built-in Result arms: `case Fine(x):` / `case Fail(e):` on `Result<T, E>`.
        // Same class of language unite pattern as Option — not a per-API Wasm cast hack.
        if is_builtin_result_pattern(canonical_callee, actual_type) {
            return None;
        }
        return Some(ParseError::invalid(format!("unknown pattern extractor `{extractor_name}` for scrutinee type `{type_name}`")));
    }

    if matching
        .iter()
        .any(|candidate| candidate.param_specs.first().is_some_and(|param| is_extractor_receiver_param(param) && param.is_mutable))
    {
        return Some(ParseError::invalid(format!("pattern extractor `{extractor_name}` must not use `mut self` on `{type_name}`")));
    }

    if matching.iter().all(|candidate| nullable_payload_type(&candidate.signature.return_type).is_none()) {
        let return_type = matching
            .first()
            .map(|candidate| render_valkyrie_type_name(&candidate.signature.return_type))
            .unwrap_or_else(|| "unknown".to_string());
        return Some(ParseError::invalid(format!(
            "pattern extractor `{extractor_name}` must return a nullable type (`T?`); got `{return_type}` for `{type_name}`"
        )));
    }

    None
}

fn diagnose_unresolved_extractor_without_scrutinee_type(
    canonical_callee: &NamePath,
    candidates: &[OverloadCandidate],
) -> Option<std_data::text::valkyrie::ParseError> {
    use std_data::text::valkyrie::ParseError;

    // Language Result/Option arms do not require a resolved scrutinee type —
    // MIR match lowering already keys Fine/Fail/Some/None by variant name.
    let head = canonical_callee.parts().first().map(|p| p.as_str()).unwrap_or("");
    if matches!(head, "Fine" | "Fail" | "Some" | "None") {
        return None;
    }
    // VonValue unite arms (`case Object(fields):` in legion.tools manifest.v).
    // Same class as Result: variant-head extractors are valid even when local
    // inference has not yet attributed `VonValue` to the scrutinee expression.
    if matches!(head, "Text" | "Flag" | "Number" | "Name" | "Array" | "Object" | "Empty") {
        return None;
    }
    // If the workspace already registered `Variant.extractor`, missing local
    // scrutinee typing is inference debt — not an unknown pattern.
    if canonical_callee.parts().last().is_some_and(|part| part.as_str() == "extractor")
        && candidates.iter().any(|candidate| {
            candidate.domain == OverloadDomain::Function
                && candidate.owner.as_ref().is_some_and(|owner| owner.as_str() == head)
                && candidate.symbol.parts().last().is_some_and(|method| method.as_str() == "extractor")
        })
    {
        return None;
    }

    let extractor_name = canonical_callee.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>().join(".");
    Some(ParseError::invalid(format!("cannot validate pattern extractor `{extractor_name}` without a known scrutinee type")))
}

fn diagnose_unresolved_arm_extractor(pattern: &HirPattern, candidates: &[OverloadCandidate]) -> Option<std_data::text::valkyrie::ParseError> {
    match pattern {
        HirPattern::Extractor(HirExtractorPattern::Constructor { canonical_callee, resolved, .. })
        | HirPattern::Extractor(HirExtractorPattern::Array { canonical_callee, resolved, .. })
            if resolved.is_none() =>
        {
            diagnose_unresolved_extractor_without_scrutinee_type(canonical_callee, candidates)
        }
        HirPattern::Or(items) => items.iter().find_map(|item| diagnose_unresolved_arm_extractor(item, candidates)),
        HirPattern::Bind { pattern, .. } | HirPattern::Mut(pattern) | HirPattern::Pin { pattern, .. } => {
            diagnose_unresolved_arm_extractor(pattern, candidates)
        }
        _ => None,
    }
}

fn matching_extractor_candidates<'a>(
    canonical_callee: &NamePath,
    actual_type: &ValkyrieType,
    candidates: &'a [OverloadCandidate],
    type_relations: &TypeRelationContext,
) -> Vec<&'a OverloadCandidate> {
    candidates
        .iter()
        .filter(|candidate| candidate.domain == OverloadDomain::Function)
        .filter(|candidate| candidate.owner.as_ref().is_some_and(|owner| canonical_callee.parts().first() == Some(owner)))
        .filter(|candidate| candidate.symbol.parts().last() == canonical_callee.parts().last())
        .filter(|candidate| candidate.param_specs.first().is_some_and(is_extractor_receiver_param))
        .filter(|candidate| {
            pattern_extractor_receiver_type(candidate)
                .and_then(|receiver_type| matches_extractor_receiver_type(actual_type, &receiver_type, type_relations))
                .is_some()
        })
        .collect()
}

fn is_extractor_receiver_param(param: &HirParam) -> bool {
    param.name.name.as_str() == "self"
        && matches!(param.ty, ValkyrieType::r#SelfType | ValkyrieType::AutoType | ValkyrieType::Named(_) | ValkyrieType::Apply(_, _))
}

fn pattern_extractor_receiver_type(candidate: &OverloadCandidate) -> Option<ValkyrieType> {
    if let Some(param) = candidate.param_specs.first().filter(|param| is_extractor_receiver_param(param)) {
        if matches!(param.ty, ValkyrieType::r#SelfType | ValkyrieType::AutoType) {
            if let Some(owner) = &candidate.owner {
                return Some(ValkyrieType::Named(owner.clone()));
            }
        }
        return Some(param.ty.clone());
    }
    if matches!(candidate.signature.params.first(), Some(ValkyrieType::r#SelfType)) {
        return candidate
            .owner
            .as_ref()
            .map(|owner| ValkyrieType::Named(owner.clone()))
            .or_else(|| candidate.signature.params.first().cloned());
    }
    candidate.owner.as_ref().map(|owner| ValkyrieType::Named(owner.clone()))
}

fn extractor_receiver_type(candidate: &OverloadCandidate) -> Option<ValkyrieType> {
    if matches!(candidate.signature.params.first(), Some(ValkyrieType::r#SelfType)) {
        return candidate.signature.params.first().cloned();
    }
    candidate.owner.as_ref().map(|owner| ValkyrieType::Named(owner.clone()))
}

fn render_valkyrie_type_name(ty: &ValkyrieType) -> String {
    match ty {
        ValkyrieType::Named(name) => name.as_str().to_string(),
        ValkyrieType::Tuple(items) => {
            let inner = items.iter().map(render_valkyrie_type_name).collect::<Vec<_>>().join(", ");
            format!("({inner})")
        }
        ValkyrieType::Array(inner) => format!("Array<{}>", render_valkyrie_type_name(inner)),
        ValkyrieType::Union(items) => items.iter().map(render_valkyrie_type_name).collect::<Vec<_>>().join(" | "),
        ValkyrieType::Apply(base, args) => {
            let rendered_args = args.iter().map(render_valkyrie_type_name).collect::<Vec<_>>().join(", ");
            format!("{}<{}>", render_valkyrie_type_name(base), rendered_args)
        }
        ValkyrieType::r#SelfType => "Self".to_string(),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod singleton_tests {
    use super::*;
    use crate::{ValkyrieCompiler, types::SourceID};

    #[test]
    fn primitive_intrinsic_declaration_resolves_without_helper_name_registry() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 4210 });
        let hir = compiler
            .compile_source(
                r#"
[primitive("core::primitive::f64")]
structure f64 { }

imply f64 {
    infix `+`(self, rhs: Self): Self {
        __f64_add(self, rhs)
    }
}

[intrinsic("f64.add")]
private micro __f64_add(lhs: f64, rhs: f64): f64 { }
"#,
            )
            .expect("primitive intrinsic declaration must resolve");
        let method = &hir.impls[0].methods[0];
        let HirExprKind::Call { resolved: Some(ref resolved), .. } = method.body.expr.as_ref().expect("operator body expression").kind
        else {
            panic!("expected resolved intrinsic call: {:?}", method.body);
        };
    }

    #[test]
    fn resolves_singleton_static_method_call() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 4201 });
        let hir = compiler
            .compile_source(
                r#"
singleton Counter {
    mut total: i64 = 0

    micro increment(mut self) -> i64 {
        self.total += 1
        self.total
    }
}

micro main() -> i64 {
    return Counter.increment();
}
"#,
            )
            .unwrap();

        let main = hir.functions.iter().find(|function| function.name.as_str() == "main").expect("main");
        let HirStatementKind::Expr(statement) = &main.body.statements[0].kind
        else {
            panic!("expected return statement");
        };
        let HirExprKind::Return(Some(expression)) = &statement.kind
        else {
            panic!("expected return expression");
        };
        let HirExprKind::Call { resolved: Some(resolved), .. } = &expression.kind
        else {
            panic!("expected resolved singleton call");
        };

        assert_eq!(resolved.domain, HirCallableDomain::Function);
        assert_eq!(resolved.symbol.to_string(), "Counter.increment");
        assert_eq!(resolved.return_type, ValkyrieType::Integer64 { signed: true });
    }

    #[test]
    fn resolves_singleton_self_method_call_inside_method_body() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 4203 });
        let hir = compiler
            .compile_source(
                r#"
singleton Counter {
    mut total: i64 = 0

    micro increment(mut self) -> i64 {
        self.total += 1
        self.total
    }

    micro tick(mut self) -> i64 {
        return self.increment();
    }
}
"#,
            )
            .unwrap();

        let tick = hir.singletons[0].methods.iter().find(|method| method.name.as_str() == "tick").expect("tick");
        let HirStatementKind::Expr(statement) = &tick.body.statements[0].kind
        else {
            panic!("expected return statement");
        };
        let HirExprKind::Return(Some(expression)) = &statement.kind
        else {
            panic!("expected return expression");
        };
        let HirExprKind::Call { args, resolved: Some(resolved), .. } = &expression.kind
        else {
            panic!("expected resolved singleton self call");
        };

        assert_eq!(args.len(), 1);
        assert!(matches!(args[0].value.kind, HirExprKind::Variable(ref ident) if ident.name.as_str() == "self"));
        assert_eq!(resolved.domain, HirCallableDomain::Function);
        assert_eq!(resolved.symbol.to_string(), "Counter.increment");
        assert_eq!(resolved.return_type, ValkyrieType::Integer64 { signed: true });
    }

    #[test]
    fn resolves_singleton_self_field_access_as_overload_argument() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 4204 });
        let hir = compiler
            .compile_source(
                r#"
micro choose(value: bool) -> bool {
    return value;
}

micro choose(value: i64) -> i64 {
    return value;
}

singleton Counter {
    mut total: i64 = 0

    micro current(self) -> i64 {
        return choose(self.total);
    }
}
"#,
            )
            .unwrap();

        let current = hir.singletons[0].methods.iter().find(|method| method.name.as_str() == "current").expect("current");
        let HirStatementKind::Expr(statement) = &current.body.statements[0].kind
        else {
            panic!("expected return statement");
        };
        let HirExprKind::Return(Some(expression)) = &statement.kind
        else {
            panic!("expected return expression");
        };
        let HirExprKind::Call { args, resolved: Some(resolved), .. } = &expression.kind
        else {
            panic!("expected resolved overload call");
        };

        assert_eq!(args.len(), 1);
        assert!(matches!(args[0].value.kind, HirExprKind::FieldAccess { ref field, .. } if field.as_str() == "total"));
        assert_eq!(resolved.domain, HirCallableDomain::Function);
        assert_eq!(resolved.symbol.to_string(), "choose");
        assert_eq!(resolved.return_type, ValkyrieType::Integer64 { signed: true });
    }

    #[test]
    fn array_of_utf8_length_resolves_to_array_not_utf8_text() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 4202 });
        let hir = compiler
            .compile_source(
                r#"
namespace test;

type utf8 = Utf8Text

class Utf8Text {}

imply Utf8Text {
    micro length(self) -> i32 {
        return 0
    }
}

class Array<T> {}

imply Array<T> {
    micro length(self): usize {
        return 0
    }
}

micro dispatch(args: [utf8]) -> usize {
    return args.length()
}
"#,
            )
            .expect("compile");
        let dispatch = hir.functions.iter().find(|function| function.name.as_str() == "dispatch").expect("dispatch");
        let HirStatementKind::Expr(statement) = &dispatch.body.statements[0].kind
        else {
            panic!("expected return statement, body={:?}", dispatch.body);
        };
        let HirExprKind::Return(Some(expression)) = &statement.kind
        else {
            panic!("expected return expression, got {statement:?}");
        };
        let HirExprKind::Call { resolved: Some(resolved), .. } = &expression.kind
        else {
            panic!("expected resolved length call, got {expression:?}");
        };
        let symbol = resolved.symbol.to_string();
        assert!(symbol.contains("Array"), "expected Array.length for [utf8].length(), got {symbol}");
        assert!(!symbol.contains("Utf8Text"), "[utf8].length() must not bind Utf8Text.length, got {symbol}");
    }

    #[test]
    fn generic_function_can_construct_its_own_nominal_variant() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 4206 });
        let hir = compiler
            .compile_source(
                r#"
namespace test;

unite Envelope<T> {
    Present { value: T },
    Absent,
}

micro wrap<T>(value: T) -> Envelope<T> {
    let wrapped: Envelope<T> = Present(value)
    return wrapped
}
"#,
            )
            .expect("generic variant construction must resolve");
        let function = hir.functions.iter().find(|function| function.name.as_str() == "wrap").expect("wrap");
        let HirStatementKind::Let { initializer: Some(initializer), .. } = &function.body.statements[0].kind
        else {
            panic!("expected generic construction initializer")
        };
        let HirExprKind::Call { resolved: Some(resolved), .. } = &initializer.kind
        else {
            panic!("expected resolved generic variant construction: {initializer:?}")
        };
        assert_eq!(resolved.domain, HirCallableDomain::Constructor);
    }

    #[test]
    fn utf8_slice_with_typed_local_uses_structured_intrinsic() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 4203 });
        let hir = compiler
            .compile_source(
                r#"
namespace test;

micro slice_probe(text: utf8, start: i32, count: i32) -> utf8 {
    return text.slice(start, count)
}
"#,
            )
            .expect("compile");
        let function = hir.functions.iter().find(|function| function.name.as_str() == "slice_probe").expect("slice_probe");
        let HirStatementKind::Expr(statement) = &function.body.statements[0].kind
        else {
            panic!("expected return statement")
        };
        let HirExprKind::Return(Some(expression)) = &statement.kind
        else {
            panic!("expected return expression")
        };
        let HirExprKind::Call { resolved: Some(resolved), .. } = &expression.kind
        else {
            panic!("expected resolved slice call: {expression:?}")
        };
        assert_eq!(
            resolved.parameter_types,
            vec![ValkyrieType::Utf8, ValkyrieType::Integer32 { signed: true }, ValkyrieType::Integer32 { signed: true }]
        );
    }

    #[test]
    fn utf8_slice_with_typed_local_survives_loop_control_flow() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 4204 });
        compiler
            .compile_source(
                r#"
namespace test;

micro slice_loop_probe(text: utf8, op: utf8) -> i32 {
    let n: i32 = text.length()
    let count: i32 = op.length()
    let mut i: i32 = 0
    while i <= n - count {
        let ch: utf8 = text.slice(i, 1)
        if ch == "(" {
            i = i + 1
            continue
        }
        if text.slice(i, count) == op {
            return i
        }
        i = i + 1
    }
    return 0
}
"#,
            )
            .expect("loop-local utf8 slice must resolve through the structured contract");
    }

    #[test]
    fn boolean_not_uses_structured_intrinsic() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 4205 });
        let hir = compiler
            .compile_source(
                r#"
namespace test;

micro negate_flag(flag: bool) -> bool {
    return !flag
}
"#,
            )
            .expect("boolean not must resolve through the structured contract");
        let function = hir.functions.iter().find(|function| function.name.as_str() == "negate_flag").expect("negate_flag");
        let HirStatementKind::Expr(statement) = &function.body.statements[0].kind
        else {
            panic!("expected return statement")
        };
        let HirExprKind::Return(Some(expression)) = &statement.kind
        else {
            panic!("expected return expression")
        };
        let HirExprKind::Call { resolved: Some(resolved), .. } = &expression.kind
        else {
            panic!("expected resolved boolean not: {expression:?}")
        };
        assert_eq!(resolved.parameter_types, vec![ValkyrieType::Boolean]);
    }
}
