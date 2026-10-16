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
        is_nullable_type,
        try_propagate::{
            is_option_apply_type, is_result_apply_type, nullable_payload_type, option_apply_payload_type, result_variant_payload_type,
        },
        type_relation::{ParameterMatchResult, TypeRelationContext},
    },
    types::{
        Identifier, NamePath, QualifiedName,
        hir::{
            GenericBinderId, GenericType, HirAttribute, HirBlock, HirCallArgument, HirCallKind, HirCallableDomain, HirDependencySemanticExport,
            HirEnum, HirExpr, HirExprKind, HirExtractorPattern, HirField, HirFunction, HirIdentifier, HirMatchArm, HirModule, HirParam,
            HirPattern, HirResolvedCall, HirSingleton, HirStatement, HirStatementKind, HirStruct, HirTrait, HirTraitBound, HirVariadicKind,
            HirVariant, HirWhereConstraint, ValkyrieType,
        },
    },
    valkyrie::{
        hir::PatternRefutability,
        mir::{AggregateFieldIndex, collect_aggregate_field_map},
    },
};
use nyar_types::{IntrinsicId, OperatorFixity, OperatorId, builtin_operator, parse_operator_display_name};

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
    pub declaration: Option<nyar_types::ItemId>,
    pub instance: Option<nyar_types::ItemInstanceId>,
    pub sum_owner: Option<nyar_types::ItemId>,
    pub sum_variant: Option<nyar_types::ItemId>,
    pub operator_id: Option<OperatorId>,
    pub symbol: NamePath,
    pub owner: Option<Identifier>,
    pub trait_owner: Option<nyar_types::ItemId>,
    pub domain: OverloadDomain,
    pub signature: OverloadSignature,
    pub match_kind: OverloadMatchKind,
    pub param_specs: Vec<HirParam>,
    /// Formal type variables which must be bound from actual arguments.
    pub generic_binders: BTreeSet<GenericBinderId>,
    /// 声明级 where 合同；实参代入后必须取得对应 witness。
    pub where_constraints: Vec<HirWhereConstraint>,
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
            declaration: None,
            instance: None,
            sum_owner: None,
            sum_variant: None,
            operator_id: None,
            symbol,
            owner: None,
            trait_owner: None,
            domain,
            signature: OverloadSignature { params, return_type },
            match_kind,
            param_specs: Vec::new(),
            generic_binders: BTreeSet::new(),
            where_constraints: Vec::new(),
        }
    }

    pub fn with_param_specs(mut self, param_specs: Vec<HirParam>) -> Self {
        self.param_specs = param_specs;
        self
    }

    pub fn with_generic_binder(mut self, binder: GenericBinderId) -> Self {
        self.generic_binders.insert(binder);
        self
    }

    fn with_where_constraints(mut self, constraints: Vec<HirWhereConstraint>) -> Self {
        self.where_constraints = constraints;
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
            declaration: None,
            instance: None,
            sum_owner: None,
            sum_variant: None,
            operator_id: None,
            symbol,
            owner: Some(owner),
            trait_owner: None,
            domain,
            signature: OverloadSignature { params, return_type },
            match_kind,
            param_specs: Vec::new(),
            generic_binders: BTreeSet::new(),
            where_constraints: Vec::new(),
        }
    }
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

pub fn resolve_overload(candidates: &[OverloadCandidate]) -> Result<OverloadCandidate, OverloadResolutionError> {
    let best_rank = candidates.iter().map(|candidate| rank(&candidate.match_kind)).min().ok_or(OverloadResolutionError::NoMatch)?;
    let mut tied = Vec::new();
    for candidate in candidates.iter().filter(|candidate| rank(&candidate.match_kind) == best_rank) {
        if !tied.contains(&candidate) {
            tied.push(candidate);
        }
    }
    match tied.as_slice() {
        [selected] => Ok((*selected).clone()),
        [] => Err(OverloadResolutionError::NoMatch),
        ambiguous => {
            Err(OverloadResolutionError::Ambiguous { candidates: ambiguous.iter().map(|candidate| candidate.symbol.clone()).collect() })
        }
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

    resolve_submodule_calls(module, &candidates, &type_relations, &struct_fields, &singleton_names);
}

fn resolve_submodule_calls(
    module: &mut HirModule,
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    struct_fields: &AggregateFieldIndex,
    singleton_names: &BTreeSet<Identifier>,
) {
    for function in &mut module.functions {
        resolve_function_calls(function, candidates, type_relations, struct_fields, singleton_names, None);
    }
    for item in &mut module.structs {
        let owner_type = struct_apply_type(item);
        for method in &mut item.methods {
            resolve_function_calls(method, candidates, type_relations, struct_fields, singleton_names, Some(&owner_type));
        }
    }
    for item in &mut module.singletons {
        for method in &mut item.methods {
            let owner_type = ValkyrieType::Named(item.name.clone());
            resolve_function_calls(method, candidates, type_relations, struct_fields, singleton_names, Some(&owner_type));
        }
        if let Some(constructor) = &mut item.constructor {
            let owner_type = ValkyrieType::Named(item.name.clone());
            resolve_function_calls(constructor, candidates, type_relations, struct_fields, singleton_names, Some(&owner_type));
        }
        if let Some(finalizer) = &mut item.finalizer {
            let owner_type = ValkyrieType::Named(item.name.clone());
            resolve_function_calls(finalizer, candidates, type_relations, struct_fields, singleton_names, Some(&owner_type));
        }
    }
    for item in &mut module.traits {
        for method in item.methods.iter_mut().chain(item.default_methods.iter_mut()) {
            resolve_function_calls(method, candidates, type_relations, struct_fields, singleton_names, None);
        }
    }
    for item in &mut module.impls {
        for method in &mut item.methods {
            resolve_function_calls(method, candidates, type_relations, struct_fields, singleton_names, Some(&item.target));
        }
    }
    for submodule in &mut module.submodules {
        resolve_submodule_calls(submodule, candidates, type_relations, struct_fields, singleton_names);
    }
}

fn collect_module_candidates(module: &HirModule) -> Vec<OverloadCandidate> {
    let mut candidates = language_builtin_candidates();
    candidates.extend(collect_declared_candidates(module));
    candidates
}

fn collect_declared_candidates(module: &HirModule) -> Vec<OverloadCandidate> {
    let mut candidates = Vec::new();
    candidates.extend(module.functions.iter().map(|function| build_function_candidate(&module.name, function)));
    let local_function_names = module.functions.iter().map(|function| function.name.clone()).collect::<std::collections::BTreeSet<_>>();
    for submodule in &module.submodules {
        candidates.extend(collect_declared_candidates(submodule));
    }
    for item in &module.structs {
        candidates.push(build_struct_constructor_candidate(item));
    }
    for item in &module.structs {
        candidates.extend(item.methods.iter().map(|method| {
            specialize_method_candidate(build_method_candidate(method, Some(item.name.clone()), None), &struct_apply_type(item), &item.generics)
        }));
    }
    for item in &module.singletons {
        let owner_type = ValkyrieType::Named(item.name.clone());
        candidates.extend(
            item.methods
                .iter()
                .map(|method| specialize_method_candidate(build_method_candidate(method, Some(item.name.clone()), None), &owner_type, &[])),
        );
    }
    for item in &module.traits {
        candidates.extend(item.methods.iter().map(|method| build_method_candidate(method, None, Some(item))));
        candidates.extend(item.default_methods.iter().map(|method| build_method_candidate(method, None, Some(item))));
    }
    for item in &module.impls {
        // owner 必须从 declaration 反查短名；`Nominal{ItemId}` 不能丢进 `impl_nominal_type_name` 的 Named 分支。
        let owner = impl_owner_identifier(module, &item.target);
        candidates.extend(
            item.methods
                .iter()
                .map(|method| specialize_method_candidate(build_method_candidate(method, owner.clone(), None), &item.target, &item.generics)),
        );
    }
    // Dependency exports contribute only their declared call contracts. Their
    // bodies remain owned by the exporting module and are never reparsed or
    // emitted as part of this consumer.
    for export in &module.imported_semantic_exports {
        candidates.extend(
            export
                .functions
                .iter()
                .filter(|function| !local_function_names.contains(&function.name))
                .filter(|function| crate::valkyrie::backend_contract::interop::function_host_provider_target(function).is_none())
                .map(|function| build_function_candidate(&export.module, function)),
        );
        for item in &export.structs {
            candidates.push(build_struct_constructor_candidate(item));
            candidates.extend(item.methods.iter().map(|method| {
                specialize_method_candidate(
                    build_method_candidate(method, Some(item.name.clone()), None),
                    &struct_apply_type(item),
                    &item.generics,
                )
            }));
        }
        // Imported sum declarations carry the same constructor contracts as
        // local sums. Keep their variant identities in the shared candidate
        // set; importing only functions/structs makes `Some`/`None` appear as
        // unresolved names in dependent adaptor modules.
        for item in &export.enums {
            candidates.extend(item.variants.iter().map(|variant| build_variant_constructor_candidate(variant, item)));
            if item.is_unity {
                candidates.extend(item.variants.iter().map(|variant| build_unite_variant_extractor_candidate(item, variant)));
            }
        }
        for item in &export.traits {
            candidates.extend(item.methods.iter().map(|method| build_method_candidate(method, None, Some(item))));
            candidates.extend(item.default_methods.iter().map(|method| build_method_candidate(method, None, Some(item))));
        }
        for item in &export.impls {
            let owner = impl_owner_identifier(module, &item.target).or_else(|| impl_owner_identifier_from_export(export, &item.target));
            candidates.extend(
                item.methods.iter().map(|method| {
                    specialize_method_candidate(build_method_candidate(method, owner.clone(), None), &item.target, &item.generics)
                }),
            );
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
    let index_type = ValkyrieType::Named(Identifier::new("usize"));
    let length = OverloadCandidate::new(
        NamePath::new(vec![Identifier::new("builtin"), Identifier::new("array"), Identifier::new("length")]),
        OverloadDomain::Function,
        vec![array_type.clone()],
        index_type.clone(),
        OverloadMatchKind::NominalExact,
    );
    let push = OverloadCandidate::new(
        NamePath::new(vec![Identifier::new("builtin"), Identifier::new("array"), Identifier::new("push")]),
        OverloadDomain::Function,
        vec![array_type.clone(), element_type.clone()],
        array_type.clone(),
        OverloadMatchKind::NominalExact,
    );
    // `array::[i]` / `array::[i] = v` 降低为 postfix `[]` / `[]=`；必须挂 operator_id，
    // 否则 Operator 调用点找不到候选（不能按 Array.get 短名猜）。
    let mut get = OverloadCandidate::new(
        NamePath::new(vec![Identifier::new("builtin"), Identifier::new("array"), Identifier::new("get")]),
        OverloadDomain::Operator,
        vec![array_type.clone(), index_type.clone()],
        element_type.clone(),
        OverloadMatchKind::NominalExact,
    );
    get.operator_id = builtin_operator::lookup(OperatorFixity::Postfix, "[]");
    let mut set = OverloadCandidate::new(
        NamePath::new(vec![Identifier::new("builtin"), Identifier::new("array"), Identifier::new("set")]),
        OverloadDomain::Operator,
        vec![array_type, index_type, element_type],
        ValkyrieType::Unit,
        OverloadMatchKind::NominalExact,
    );
    set.operator_id = builtin_operator::lookup(OperatorFixity::Postfix, "[]=");
    vec![length, push, get, set]
}

/// 按候选符号的 [`IntrinsicId`] 做形实参合同（非调用点拼写）。
fn match_intrinsic_builtin_candidate(
    candidate: &OverloadCandidate,
    args: &[HirCallArgument],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &AggregateFieldIndex,
    singleton_names: &BTreeSet<Identifier>,
) -> Option<OverloadCandidate> {
    let parts = candidate.symbol.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>();
    match IntrinsicId::resolve_from_segments(&parts)? {
        IntrinsicId::ArrayPush => {
            if args.len() != 2 {
                return None;
            }
            let array_ty = infer_scrutinee_type(&args[0].value, locals, struct_fields, singleton_names)?;
            let ValkyrieType::Array(element) = &array_ty
            else {
                return None;
            };
            let element = element.as_ref().clone();
            let value_ty = infer_scrutinee_type(&args[1].value, locals, struct_fields, singleton_names)?;
            if matches!(value_ty, ValkyrieType::AutoType | ValkyrieType::r#SelfType)
                || matches!(type_relations.match_parameter(&value_ty, &element), ParameterMatchResult::NoMatch { .. })
            {
                return None;
            }
            Some(OverloadCandidate {
                declaration: candidate.declaration,
                instance: candidate.instance,
                sum_owner: None,
                sum_variant: None,
                operator_id: None,
                symbol: candidate.symbol.clone(),
                owner: None,
                trait_owner: candidate.trait_owner.clone(),
                domain: OverloadDomain::Function,
                signature: OverloadSignature { params: vec![array_ty.clone(), element], return_type: array_ty },
                match_kind: OverloadMatchKind::NominalExact,
                param_specs: Vec::new(),
                generic_binders: candidate.generic_binders.clone(),
                where_constraints: candidate.where_constraints.clone(),
            })
        }
        IntrinsicId::ArrayLen => {
            if args.len() != 1 {
                return None;
            }
            let array_ty = infer_scrutinee_type(&args[0].value, locals, struct_fields, singleton_names)?;
            // 只认结构化数组形状；不得按接收者字段名 `length` 猜。
            if !matches!(array_ty, ValkyrieType::Array(_) | ValkyrieType::FixedArray { .. }) {
                return None;
            }
            Some(OverloadCandidate {
                declaration: candidate.declaration,
                instance: candidate.instance,
                sum_owner: None,
                sum_variant: None,
                operator_id: None,
                symbol: candidate.symbol.clone(),
                owner: None,
                trait_owner: candidate.trait_owner.clone(),
                domain: OverloadDomain::Function,
                signature: OverloadSignature { params: vec![array_ty], return_type: candidate.signature.return_type.clone() },
                match_kind: OverloadMatchKind::NominalExact,
                param_specs: Vec::new(),
                generic_binders: candidate.generic_binders.clone(),
                where_constraints: candidate.where_constraints.clone(),
            })
        }
        IntrinsicId::RefDeref => {
            if args.len() != 1 {
                return None;
            }
            let operand_ty = infer_scrutinee_type(&args[0].value, locals, struct_fields, singleton_names)?;
            let return_type = nullable_payload_type(&operand_ty).unwrap_or_else(|| candidate.signature.return_type.clone());
            Some(OverloadCandidate {
                declaration: candidate.declaration,
                instance: candidate.instance,
                sum_owner: None,
                sum_variant: None,
                operator_id: None,
                symbol: candidate.symbol.clone(),
                owner: None,
                trait_owner: candidate.trait_owner.clone(),
                domain: OverloadDomain::Function,
                signature: OverloadSignature { params: vec![operand_ty], return_type },
                match_kind: OverloadMatchKind::NominalExact,
                param_specs: Vec::new(),
                generic_binders: candidate.generic_binders.clone(),
                where_constraints: candidate.where_constraints.clone(),
            })
        }
        IntrinsicId::IsNull => {
            if args.len() != 1 {
                return None;
            }
            let operand_ty = infer_scrutinee_type(&args[0].value, locals, struct_fields, singleton_names)?;
            if !is_nullable_type(&operand_ty) {
                return None;
            }
            Some(OverloadCandidate {
                declaration: candidate.declaration,
                instance: candidate.instance,
                sum_owner: None,
                sum_variant: None,
                operator_id: None,
                symbol: candidate.symbol.clone(),
                owner: None,
                trait_owner: candidate.trait_owner.clone(),
                domain: OverloadDomain::Function,
                signature: OverloadSignature { params: vec![operand_ty], return_type: ValkyrieType::Boolean },
                match_kind: OverloadMatchKind::NominalExact,
                param_specs: Vec::new(),
                generic_binders: candidate.generic_binders.clone(),
                where_constraints: candidate.where_constraints.clone(),
            })
        }
        IntrinsicId::UnwrapNull => {
            if args.len() != 1 {
                return None;
            }
            let operand_ty = infer_scrutinee_type(&args[0].value, locals, struct_fields, singleton_names)?;
            let Some(payload) = nullable_payload_type(&operand_ty)
            else {
                return None;
            };
            Some(OverloadCandidate {
                declaration: candidate.declaration,
                instance: candidate.instance,
                sum_owner: None,
                sum_variant: None,
                operator_id: None,
                symbol: candidate.symbol.clone(),
                owner: None,
                trait_owner: candidate.trait_owner.clone(),
                domain: OverloadDomain::Function,
                signature: OverloadSignature { params: vec![operand_ty], return_type: payload },
                match_kind: OverloadMatchKind::NominalExact,
                param_specs: Vec::new(),
                generic_binders: candidate.generic_binders.clone(),
                where_constraints: candidate.where_constraints.clone(),
            })
        }
        IntrinsicId::ArrayGet => {
            if args.len() != 2 {
                return None;
            }
            let array_ty = infer_scrutinee_type(&args[0].value, locals, struct_fields, singleton_names)?;
            let ValkyrieType::Array(element) = &array_ty
            else {
                return None;
            };
            let element = element.as_ref().clone();
            let index_ty = infer_scrutinee_type(&args[1].value, locals, struct_fields, singleton_names)?;
            // 绑定后索引常为 `Nominal{usize}`；不得只认 Integer* / Named。
            if !type_relations.is_numeric_type(&index_ty) {
                return None;
            }
            // 保留候选已冻结的一元 unity / Nullable 返回；禁止按 Named("Option") 短名认亲。
            let return_type = match &candidate.signature.return_type {
                ty if is_option_apply_type(ty) || matches!(ty, ValkyrieType::Nullable(_)) => candidate.signature.return_type.clone(),
                _ => element.clone(),
            };
            Some(OverloadCandidate {
                declaration: candidate.declaration,
                instance: candidate.instance,
                sum_owner: None,
                sum_variant: None,
                operator_id: candidate.operator_id,
                symbol: candidate.symbol.clone(),
                owner: None,
                trait_owner: candidate.trait_owner.clone(),
                domain: OverloadDomain::Function,
                signature: OverloadSignature { params: vec![array_ty, index_ty], return_type },
                match_kind: OverloadMatchKind::NominalExact,
                param_specs: Vec::new(),
                generic_binders: candidate.generic_binders.clone(),
                where_constraints: candidate.where_constraints.clone(),
            })
        }
        IntrinsicId::ArraySet => {
            if args.len() != 3 {
                return None;
            }
            let array_ty = infer_scrutinee_type(&args[0].value, locals, struct_fields, singleton_names)?;
            let ValkyrieType::Array(element) = &array_ty
            else {
                return None;
            };
            let element = element.as_ref().clone();
            let index_ty = infer_scrutinee_type(&args[1].value, locals, struct_fields, singleton_names)?;
            // 与 ArrayGet 相同：绑定后索引常为 `Nominal{usize}`。
            if !type_relations.is_numeric_type(&index_ty) {
                return None;
            }
            let value_ty = infer_scrutinee_type(&args[2].value, locals, struct_fields, singleton_names)?;
            if matches!(value_ty, ValkyrieType::AutoType | ValkyrieType::r#SelfType)
                || matches!(type_relations.match_parameter(&value_ty, &element), ParameterMatchResult::NoMatch { .. })
            {
                return None;
            }
            Some(OverloadCandidate {
                declaration: candidate.declaration,
                instance: candidate.instance,
                sum_owner: None,
                sum_variant: None,
                operator_id: candidate.operator_id,
                symbol: candidate.symbol.clone(),
                owner: None,
                trait_owner: candidate.trait_owner.clone(),
                domain: OverloadDomain::Function,
                signature: OverloadSignature { params: vec![array_ty, index_ty, element], return_type: ValkyrieType::Unit },
                match_kind: OverloadMatchKind::NominalExact,
                param_specs: Vec::new(),
                generic_binders: candidate.generic_binders.clone(),
                where_constraints: candidate.where_constraints.clone(),
            })
        }
        // 其它 Intrinsic 仍走通用形实参路径，直至各自合同闭合。
        _ => None,
    }
}

fn enum_apply_type(enum_def: &HirEnum) -> ValkyrieType {
    let arguments = enum_def.generics.iter().map(|generic| ValkyrieType::Generic(generic.clone())).collect::<Vec<_>>();
    if let Some(declaration) = enum_def.declaration {
        return ValkyrieType::Nominal { declaration, arguments };
    }
    // 缺 declaration 时不得伪装成功身份；保留表面 Named 供诊断，后续绑定/校验应失败。
    if arguments.is_empty() {
        ValkyrieType::Named(enum_def.name.clone())
    }
    else {
        ValkyrieType::Apply(Box::new(ValkyrieType::Named(enum_def.name.clone())), arguments)
    }
}

fn struct_apply_type(struct_def: &HirStruct) -> ValkyrieType {
    let arguments = struct_def.generics.iter().map(|generic| ValkyrieType::Generic(generic.clone())).collect::<Vec<_>>();
    if let Some(declaration) = struct_def.constructor_declaration {
        return ValkyrieType::Nominal { declaration, arguments };
    }
    if arguments.is_empty() {
        ValkyrieType::Named(struct_def.name.clone())
    }
    else {
        ValkyrieType::Apply(Box::new(ValkyrieType::Named(struct_def.name.clone())), arguments)
    }
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

    let mut candidate = OverloadCandidate::new_method(
        variant.name.clone(),
        NamePath::new(vec![Identifier::new("extractor")]),
        OverloadDomain::Function,
        vec![enum_type],
        return_type,
        OverloadMatchKind::NominalSubtype { distance: 0 },
    )
    .with_param_specs(vec![self_param]);
    candidate.sum_owner = enum_def.declaration;
    candidate.sum_variant = variant.declaration;
    candidate
}

fn build_function_candidate(module_name: &NamePath, function: &HirFunction) -> OverloadCandidate {
    let symbol = crate::valkyrie::symbols::stable_hir_function_name_path(module_name, function);
    let operator_id = declared_operator_id_from_function(function);
    let mut candidate = OverloadCandidate::new(
        symbol,
        callable_domain(operator_id),
        function.params.iter().map(|param| param.ty.clone()).collect(),
        function.return_type.clone(),
        OverloadMatchKind::Row,
    )
    .with_param_specs(function.params.clone());
    candidate.declaration = function.declaration;
    candidate.instance = function.instance;
    candidate.operator_id = operator_id;
    for generic in &function.generics {
        if let Some(binder) = generic.binder {
            candidate = candidate.with_generic_binder(binder);
        }
    }
    candidate.with_where_constraints(function.where_constraints.clone())
}

/// Simple name used when matching an unqualified call site to overload candidates.
///
/// Module-level MIR symbols are stored as a single `NamePath` segment (`main::answer`);
/// instance methods keep dotted paths (`Counter.increment`).
fn overload_symbol_simple_name(symbol: &NamePath) -> &str {
    let part = symbol.parts().last().map(|identifier| identifier.as_str()).unwrap_or("");
    part.rsplit_once("::").map(|(_, simple)| simple).unwrap_or(part)
}

fn symbol_matches_callee_name(symbol: &NamePath, callee_name: &Identifier) -> bool {
    overload_symbol_simple_name(symbol) == callee_name.as_str()
}

fn build_struct_constructor_candidate(item: &HirStruct) -> OverloadCandidate {
    let param_specs = synthetic_field_params(item);
    let mut candidate = OverloadCandidate::new(
        struct_constructor_path(item),
        OverloadDomain::Constructor,
        item.fields.iter().map(|field| field.ty.clone()).collect(),
        struct_apply_type(item),
        OverloadMatchKind::Row,
    )
    .with_param_specs(param_specs);
    for generic in &item.generics {
        if let Some(binder) = generic.binder {
            candidate = candidate.with_generic_binder(binder);
        }
    }
    candidate.declaration = item.constructor_declaration;
    candidate.instance = item.constructor_instance;
    candidate
}

/// 返回聚合构造器的声明路径。跨 package 导入不能把同名类型压扁到短名，
/// 否则字段绑定会在错误的 aggregate 上完成，随后只能以 `_` 占位失败。
fn struct_constructor_path(item: &HirStruct) -> NamePath {
    let mut parts = item.namespace.clone();
    parts.push(item.name.clone());
    NamePath::new(parts)
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
    let mut candidate = OverloadCandidate::new_method(
        enum_def.name.clone(),
        NamePath::new(vec![variant.name.clone()]),
        OverloadDomain::Constructor,
        variant_constructor_param_types(variant),
        return_type,
        OverloadMatchKind::Row,
    );
    for generic in &enum_def.generics {
        if let Some(binder) = generic.binder {
            candidate = candidate.with_generic_binder(binder);
        }
    }
    candidate.declaration = variant.declaration;
    candidate.instance = variant.instance;
    candidate.sum_owner = enum_def.declaration;
    candidate.sum_variant = variant.declaration;
    candidate
}

fn build_method_candidate(function: &HirFunction, owner: Option<Identifier>, trait_owner: Option<&HirTrait>) -> OverloadCandidate {
    let operator_id = declared_operator_id_from_function(function);
    let candidate = if let Some(owner) = owner {
        OverloadCandidate::new_method(
            owner,
            NamePath::new(vec![function.name.clone()]),
            callable_domain(operator_id),
            function.params.iter().map(|param| param.ty.clone()).collect(),
            function.return_type.clone(),
            OverloadMatchKind::Row,
        )
    }
    else {
        OverloadCandidate::new(
            trait_owner
                .as_ref()
                .map(|owner| NamePath::new(vec![owner.name.clone(), function.name.clone()]))
                .unwrap_or_else(|| NamePath::new(vec![function.name.clone()])),
            callable_domain(operator_id),
            function.params.iter().map(|param| param.ty.clone()).collect(),
            function.return_type.clone(),
            OverloadMatchKind::Row,
        )
    };
    let mut candidate = candidate.with_param_specs(function.params.clone());
    candidate.declaration = function.declaration;
    candidate.instance = function.instance;
    candidate.operator_id = operator_id;
    for generic in &function.generics {
        if let Some(binder) = generic.binder {
            candidate = candidate.with_generic_binder(binder);
        }
    }
    let mut candidate = candidate.with_where_constraints(function.where_constraints.clone());
    candidate.trait_owner = trait_owner.and_then(|owner| owner.declaration);
    if let Some(owner) = trait_owner {
        for generic in &owner.generics {
            if let Some(binder) = generic.binder {
                candidate.generic_binders.insert(binder);
            }
        }
    }
    candidate
}

/// 将方法声明绑定到完整的名义 owner；仅保留 owner 的短名用于 callable identity。
/// `Self` 和 impl 泛型必须在候选进入实参统一前完成替换，否则泛型实例方法会
/// 被降成非泛型 nominal 类型，函数体中的 receiver 也会失去同一份代入事实。
fn specialize_method_candidate(
    mut candidate: OverloadCandidate,
    owner_type: &ValkyrieType,
    owner_generics: &[GenericType],
) -> OverloadCandidate {
    candidate.signature.params = candidate
        .signature
        .params
        .iter()
        .enumerate()
        .map(|(index, ty)| specialize_method_param_type(ty, index, &candidate.param_specs, owner_type))
        .collect();
    candidate.signature.return_type = substitute_type_self(&candidate.signature.return_type, owner_type);
    candidate.param_specs = candidate
        .param_specs
        .iter()
        .enumerate()
        .map(|(index, param)| {
            let mut param = param.clone();
            param.ty = specialize_method_param_type(&param.ty, index, &candidate.param_specs, owner_type);
            param
        })
        .collect();
    for generic in owner_generics {
        if let Some(binder) = generic.binder {
            candidate = candidate.with_generic_binder(binder);
        }
    }
    for generic in owner_type_generic_binders(owner_type) {
        candidate = candidate.with_generic_binder(generic);
    }
    candidate
}

/// `self` 上的 `AutoType`/`Self` 必须在候选阶段换成完整 owner（可为 `Nominal{ItemId}`）。
/// 禁止留给 `match_call_candidate` 再收成 `Named(短名)`，否则与已绑定字段类型对不上。
fn specialize_method_param_type(ty: &ValkyrieType, index: usize, param_specs: &[HirParam], owner_type: &ValkyrieType) -> ValkyrieType {
    let is_self_slot = index == 0 && param_specs.first().is_some_and(|param| param.name.name.as_str() == "self");
    if is_self_slot && matches!(ty, ValkyrieType::AutoType | ValkyrieType::r#SelfType) {
        return owner_type.clone();
    }
    substitute_type_self(ty, owner_type)
}

fn owner_type_generic_binders(ty: &ValkyrieType) -> BTreeSet<GenericBinderId> {
    fn collect(ty: &ValkyrieType, binders: &mut BTreeSet<GenericBinderId>) {
        match ty {
            ValkyrieType::Generic(generic) => {
                if let Some(binder) = generic.binder {
                    binders.insert(binder);
                }
            }
            ValkyrieType::Nominal { arguments, .. } | ValkyrieType::Tuple(arguments) => {
                for argument in arguments {
                    collect(argument, binders);
                }
            }
            ValkyrieType::Apply(base, arguments) => {
                collect(base, binders);
                for argument in arguments {
                    collect(argument, binders);
                }
            }
            ValkyrieType::Array(element) | ValkyrieType::Nullable(element) => collect(element, binders),
            _ => {}
        }
    }
    let mut binders = BTreeSet::new();
    collect(ty, &mut binders);
    binders
}

fn candidate_has_receiver_parameter(candidate: &OverloadCandidate) -> bool {
    candidate.param_specs.first().is_some_and(|parameter| parameter.name.name.as_str() == "self")
}

fn declared_operator_id(name: &Identifier) -> Option<OperatorId> {
    let (fixity, lexeme) = parse_operator_display_name(name.as_str())?;
    builtin_operator::lookup(fixity, lexeme)
}

fn hir_has_modifier(annotations: &[HirAttribute], name: &str) -> bool {
    annotations.iter().any(|attribute| attribute.name.parts().len() == 1 && attribute.name.parts()[0].as_str() == name)
}

/// `infix `==`` 等方法名仅含词素；fixity 来自 `prefix`/`infix`/`postfix` 修饰符注解。
fn declared_operator_id_from_function(function: &HirFunction) -> Option<OperatorId> {
    let fixity = if hir_has_modifier(&function.annotations, "prefix") {
        Some(OperatorFixity::Prefix)
    }
    else if hir_has_modifier(&function.annotations, "infix") {
        Some(OperatorFixity::Infix)
    }
    else if hir_has_modifier(&function.annotations, "postfix") || hir_has_modifier(&function.annotations, "suffix") {
        Some(OperatorFixity::Postfix)
    }
    else {
        None
    };
    if let Some(fixity) = fixity {
        if let Some(operator_id) = builtin_operator::lookup(fixity, function.name.as_str()) {
            return Some(operator_id);
        }
    }
    declared_operator_id(&function.name)
}

fn callable_domain(operator_id: Option<OperatorId>) -> OverloadDomain {
    if operator_id.is_some() { OverloadDomain::Operator } else { OverloadDomain::Function }
}

fn resolve_function_calls(
    function: &mut HirFunction,
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    struct_fields: &AggregateFieldIndex,
    singleton_names: &BTreeSet<Identifier>,
    owner_type: Option<&ValkyrieType>,
) {
    let type_relations = type_relations.with_callable_constraints(&function.where_constraints);
    let mut locals = function
        .params
        .iter()
        .map(|param| {
            let ty = if matches!(param.ty, ValkyrieType::r#SelfType)
                || (param.name.name.as_str() == "self" && matches!(param.ty, ValkyrieType::AutoType))
            {
                owner_type.cloned().unwrap_or(param.ty.clone())
            }
            else {
                param.ty.clone()
            };
            (param.name.name.to_string(), ty)
        })
        .collect::<BTreeMap<_, _>>();
    let contextual_type = Some(&function.return_type);
    resolve_block_calls(&mut function.body, candidates, &type_relations, &mut locals, struct_fields, singleton_names, contextual_type);
}

fn resolve_block_calls(
    block: &mut HirBlock,
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &mut BTreeMap<String, ValkyrieType>,
    struct_fields: &AggregateFieldIndex,
    singleton_names: &BTreeSet<Identifier>,
    contextual_type: Option<&ValkyrieType>,
) {
    for statement in &mut block.statements {
        resolve_statement_calls(statement, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
    }
    if let Some(expr) = &mut block.expr {
        resolve_expr_calls(expr, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
    }
}

fn resolve_statement_calls(
    statement: &mut HirStatement,
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &mut BTreeMap<String, ValkyrieType>,
    struct_fields: &AggregateFieldIndex,
    singleton_names: &BTreeSet<Identifier>,
    contextual_type: Option<&ValkyrieType>,
) {
    match &mut statement.kind {
        HirStatementKind::Let { pattern, initializer, ty, .. } => {
            if let Some(initializer) = initializer {
                let hint = ty.as_ref().or(contextual_type);
                resolve_expr_calls(initializer, candidates, type_relations, locals, struct_fields, singleton_names, hint);
            }
            let binding_type =
                ty.clone().or_else(|| initializer.as_ref().and_then(|expr| infer_scrutinee_type(expr, locals, struct_fields, singleton_names)));
            if let Some(binding_type) = binding_type {
                bind_pattern_type(pattern, &binding_type, locals);
            }
        }
        HirStatementKind::Expr(expr) => {
            resolve_expr_calls(expr, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type)
        }
    }
}

fn resolve_expr_calls(
    expr: &mut HirExpr,
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &mut BTreeMap<String, ValkyrieType>,
    struct_fields: &AggregateFieldIndex,
    singleton_names: &BTreeSet<Identifier>,
    contextual_type: Option<&ValkyrieType>,
) {
    try_resolve_nullary_variant_expr(expr, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
    match &mut expr.kind {
        HirExprKind::Call { call_kind, callee, args, resolved } => {
            resolve_expr_calls(callee, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
            for arg in args.iter_mut() {
                resolve_expr_calls(&mut arg.value, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
            }
            *resolved = try_resolve_call(*call_kind, callee, args, candidates, type_relations, locals, struct_fields, singleton_names);
            if let Some(resolved) = resolved.as_mut() {
                enrich_unite_variant_constructor_facts(
                    callee,
                    args,
                    resolved,
                    contextual_type,
                    candidates,
                    locals,
                    struct_fields,
                    singleton_names,
                );
            }
        }
        HirExprKind::FieldAccess { object, field } => {
            resolve_expr_calls(object, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
            // 裸 `array.length`（非 `length()` 调用）在 HIR 冻结为 ArrayLen Invoke；
            // MIR 不得再按字段名改写。名义类型上已有 `length` FieldId 时保持字段访问。
            if field.as_str() == "length" {
                let receiver_ty = infer_scrutinee_type(object, locals, struct_fields, singleton_names);
                let is_array = receiver_ty.as_ref().is_some_and(|ty| matches!(ty, ValkyrieType::Array(_) | ValkyrieType::FixedArray { .. }));
                let has_nominal_length_field = receiver_ty.as_ref().is_some_and(|ty| {
                    struct_fields.fields_of(ty).is_some_and(|fields| fields.iter().any(|item| item.name.as_str() == "length"))
                });
                if is_array && !has_nominal_length_field {
                    let span = expr.span.clone();
                    let args = vec![HirCallArgument::positional(object.as_ref().clone())];
                    let callee = HirExpr {
                        kind: HirExprKind::Path(NamePath::new(vec![
                            Identifier::new("builtin"),
                            Identifier::new("array"),
                            Identifier::new("length"),
                        ])),
                        span: span.clone(),
                    };
                    let resolved = try_resolve_call(
                        HirCallKind::Function,
                        &callee,
                        &args,
                        candidates,
                        type_relations,
                        locals,
                        struct_fields,
                        singleton_names,
                    );
                    expr.kind = HirExprKind::Call { call_kind: HirCallKind::Function, callee: Box::new(callee), args, resolved };
                }
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
        | HirExprKind::Assign { value: callee, .. } => {
            resolve_expr_calls(callee, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type)
        }
        HirExprKind::StoreField { object, value, .. } => {
            resolve_expr_calls(object, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
            resolve_expr_calls(value, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
        }
        HirExprKind::ArrayNew { length, .. } => {
            resolve_expr_calls(length, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type)
        }
        HirExprKind::ArrayLiteral { items: args } | HirExprKind::TupleLiteral { items: args } => {
            for arg in args {
                resolve_expr_calls(arg, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
            }
        }
        HirExprKind::TryPropagate { inner, resolved_ok_variant } => {
            resolve_expr_calls(inner, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
            if resolved_ok_variant.is_none() {
                let operand_type = infer_scrutinee_type(inner, locals, struct_fields, singleton_names);
                *resolved_ok_variant = operand_type.as_ref().and_then(|ty| try_resolve_try_propagate_ok_variant(ty, candidates));
            }
        }
        HirExprKind::Construct { path, name, args, resolved, .. } => {
            for arg in args.iter_mut() {
                let field_context = match &arg.kind {
                    HirExprKind::FieldInit { name: field_name, .. } => struct_fields
                        .fields_of(&ValkyrieType::Named(name.clone()))
                        .and_then(|fields| fields.iter().find(|field| field.name == *field_name).map(|field| &field.ty))
                        .or_else(|| {
                            candidates
                                .iter()
                                .filter(|candidate| candidate.domain == OverloadDomain::Constructor)
                                .filter(|candidate| candidate.symbol.parts().last().is_some_and(|candidate_name| candidate_name == name))
                                .find_map(|candidate| {
                                    candidate.param_specs.iter().find(|param| param.name.name == *field_name).map(|param| &param.ty)
                                })
                        }),
                    _ => None,
                };
                if let HirExprKind::FieldInit { value, .. } = &mut arg.kind {
                    resolve_expr_calls(
                        value,
                        candidates,
                        type_relations,
                        locals,
                        struct_fields,
                        singleton_names,
                        field_context.or(contextual_type),
                    );
                }
                else {
                    resolve_expr_calls(arg, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
                }
            }
            if resolved.is_none() {
                *resolved = try_resolve_constructor(path, args, candidates, type_relations, locals, struct_fields, singleton_names);
            }
            if let Some(resolved) = resolved.as_mut() {
                enrich_construct_unite_variant_facts(name, args, resolved, contextual_type, candidates);
            }
        }
        HirExprKind::Cast { expr, .. } => {
            resolve_expr_calls(expr, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
        }
        HirExprKind::Block(block) => {
            resolve_block_calls(block, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type)
        }
        HirExprKind::TryScope { body, .. } => {
            resolve_block_calls(body, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type)
        }
        HirExprKind::Lambda { params, body, .. } => {
            let mut lambda_locals = locals.clone();
            for param in params {
                lambda_locals.insert(param.name.name.to_string(), param.ty.clone());
            }
            resolve_block_calls(body, candidates, type_relations, &mut lambda_locals, struct_fields, singleton_names, None);
        }
        HirExprKind::AnonymousClass { fields, methods, .. } => {
            for (_, value) in fields {
                resolve_expr_calls(value, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
            }
            for method in methods {
                resolve_function_calls(method, candidates, type_relations, struct_fields, singleton_names, None);
            }
        }
        HirExprKind::If { condition, then_branch, else_branch } => {
            resolve_expr_calls(condition, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
            let mut then_locals = locals.clone();
            resolve_block_calls(then_branch, candidates, type_relations, &mut then_locals, struct_fields, singleton_names, contextual_type);
            if let Some(else_branch) = else_branch {
                let mut else_locals = locals.clone();
                resolve_block_calls(else_branch, candidates, type_relations, &mut else_locals, struct_fields, singleton_names, contextual_type);
            }
        }
        HirExprKind::Match { scrutinee, arms } | HirExprKind::Case { scrutinee, arms } => {
            resolve_expr_calls(scrutinee, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
            let scrutinee_type = infer_scrutinee_type(scrutinee, locals, struct_fields, singleton_names);
            for arm in arms {
                resolve_arm_calls(
                    arm,
                    candidates,
                    type_relations,
                    locals,
                    scrutinee_type.as_ref(),
                    struct_fields,
                    singleton_names,
                    contextual_type,
                );
            }
        }
        HirExprKind::IfLet { pattern, scrutinee, then_branch, else_branch } => {
            resolve_expr_calls(scrutinee, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
            let scrutinee_type = infer_scrutinee_type(scrutinee, locals, struct_fields, singleton_names);
            resolve_pattern_calls(pattern, candidates, type_relations, scrutinee_type.as_ref(), struct_fields);
            let mut then_locals = locals.clone();
            if let Some(scrutinee_type) = scrutinee_type.as_ref() {
                bind_pattern_type(pattern, scrutinee_type, &mut then_locals);
            }
            resolve_block_calls(then_branch, candidates, type_relations, &mut then_locals, struct_fields, singleton_names, contextual_type);
            if let Some(else_branch) = else_branch {
                let mut else_locals = locals.clone();
                resolve_block_calls(else_branch, candidates, type_relations, &mut else_locals, struct_fields, singleton_names, contextual_type);
            }
        }
        HirExprKind::Loop { pattern, iterator, condition, body, .. } => {
            if let Some(iterator) = iterator {
                resolve_expr_calls(iterator, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
            }
            if let Some(condition) = condition {
                resolve_expr_calls(condition, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
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
            resolve_block_calls(body, candidates, type_relations, &mut body_locals, struct_fields, singleton_names, contextual_type);
        }
        HirExprKind::Return(Some(value)) | HirExprKind::Break { expr: Some(value), .. } | HirExprKind::Yield(Some(value)) => {
            resolve_expr_calls(value, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
        }
        HirExprKind::Catch { expr, arms } => {
            resolve_expr_calls(expr, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
            let scrutinee_type = infer_catch_scrutinee_type(expr, locals);
            for arm in arms {
                resolve_arm_calls(
                    arm,
                    candidates,
                    type_relations,
                    locals,
                    scrutinee_type.as_ref(),
                    struct_fields,
                    singleton_names,
                    contextual_type,
                );
            }
        }
        HirExprKind::Variable(_)
        | HirExprKind::Literal(_)
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
    struct_fields: &AggregateFieldIndex,
    singleton_names: &BTreeSet<Identifier>,
    contextual_type: Option<&ValkyrieType>,
) {
    let mut arm_locals = locals.clone();
    resolve_pattern_calls(&mut arm.pattern, candidates, type_relations, scrutinee_type, struct_fields);
    if let Some(scrutinee_type) = scrutinee_type {
        bind_pattern_type(&arm.pattern, scrutinee_type, &mut arm_locals);
    }
    if let Some(guard) = &mut arm.guard {
        resolve_expr_calls(guard, candidates, type_relations, &mut arm_locals, struct_fields, singleton_names, contextual_type);
    }
    resolve_expr_calls(&mut arm.body, candidates, type_relations, &mut arm_locals, struct_fields, singleton_names, contextual_type);
}

fn resolve_pattern_calls(
    pattern: &mut HirPattern,
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    scrutinee_type: Option<&ValkyrieType>,
    struct_fields: &AggregateFieldIndex,
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
                *resolved = try_resolve_pattern_extractor(canonical_callee, actual_type, candidates, type_relations);
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
        HirPattern::Object { name, fields, rest, .. } => {
            if rest.is_none() {
                if let Some(name_path) = name.clone() {
                    if let Some(actual_type) = scrutinee_type {
                        let canonical_callee = canonical_extractor_callee(&name_path);
                        let resolved = try_resolve_pattern_extractor(&canonical_callee, actual_type, candidates, type_relations);
                        let builtin = is_builtin_nullable_option_pattern(&canonical_callee, actual_type)
                            || is_builtin_result_pattern(&canonical_callee, actual_type);
                        if resolved.is_some() || builtin {
                            let extractor_fields = fields.iter().map(|(_, field)| field.clone()).collect::<Vec<_>>();
                            *pattern = HirPattern::Extractor(HirExtractorPattern::Constructor {
                                name: name_path.clone(),
                                canonical_callee,
                                fields: extractor_fields,
                                resolved,
                            });
                            if let HirPattern::Extractor(HirExtractorPattern::Constructor { fields, resolved, .. }) = pattern {
                                if let Some(payload) = resolved
                                    .as_ref()
                                    .and_then(|call| call.extractor_payload_type.clone())
                                    .or_else(|| nullable_payload_type(actual_type))
                                {
                                    let field_types = payload_field_scrutinee_types(&payload, struct_fields);
                                    for (index, field) in fields.iter_mut().enumerate() {
                                        let field_scrutinee = field_types.get(index).and_then(|ty| ty.as_ref());
                                        resolve_pattern_calls(field, candidates, type_relations, field_scrutinee, struct_fields);
                                    }
                                }
                                else if is_option_owner_type(actual_type) || is_result_owner_type(actual_type) {
                                    for field in fields.iter_mut() {
                                        resolve_pattern_calls(field, candidates, type_relations, None, struct_fields);
                                    }
                                }
                            }
                            return;
                        }
                    }
                }
            }
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
    explicit_generic_arguments: &[ValkyrieType],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &AggregateFieldIndex,
    singleton_names: &BTreeSet<Identifier>,
) -> Option<OverloadCandidate> {
    // 语言 builtin：以候选符号的 IntrinsicId 为准（ADR 0013），不得按调用点表面名特判。
    if let Some(matched) = match_intrinsic_builtin_candidate(candidate, args, type_relations, locals, struct_fields, singleton_names) {
        return Some(matched);
    }
    let bound_exprs: Vec<HirExpr> = if candidate.param_specs.is_empty() {
        args.iter().map(|arg| arg.value.clone()).collect()
    }
    else {
        bind_call_arguments(&candidate.param_specs, args).ok()?
    };
    let actual_types =
        bound_exprs.iter().map(|expr| infer_scrutinee_type(expr, locals, struct_fields, singleton_names)).collect::<Option<Vec<_>>>()?;
    // `self: Self` / untyped `self` 必须是 owner 的完整类型（优先 `Nominal{ItemId}`）。
    // 禁止再收成 `Named(短名)`：字段/参数在 bind 后已是 declaration identity。
    let owner_type = candidate.owner.as_ref().and_then(|owner| type_relations.nominal_type_for_name(owner));
    let expected_params = candidate
        .signature
        .params
        .iter()
        .enumerate()
        .map(|(index, ty)| {
            let is_self_slot = index == 0
                && candidate.owner.is_some()
                && candidate.param_specs.first().is_some_and(|param| param.name.name.as_str() == "self");
            if is_self_slot && matches!(ty, ValkyrieType::AutoType | ValkyrieType::r#SelfType) {
                if let Some(owner_type) = &owner_type {
                    return owner_type.clone();
                }
            }
            match &owner_type {
                Some(owner_type) => substitute_type_self(ty, owner_type),
                None => substitute_self_type(ty, candidate.owner.as_ref()),
            }
        })
        .collect::<Vec<_>>();
    // 整数/字符串字面量特化必须在 unify 之前完成：`Nominal{usize}` vs 未特化的
    // `Integer32` 会在 unify 的 Nominal 臂直接失败，导致 `usize == 0` 永远选不中。
    let actual_types = specialize_call_literal_exprs(&bound_exprs, &actual_types, &expected_params, type_relations);
    let mut substitutions = BTreeMap::new();
    if !explicit_generic_arguments.is_empty() {
        let binders = candidate.generic_binders.iter().cloned().collect::<Vec<_>>();
        if explicit_generic_arguments.len() != binders.len() {
            return None;
        }
        for (binder, argument) in binders.iter().zip(explicit_generic_arguments.iter()) {
            substitutions.insert(binder.clone(), argument.clone());
        }
    }
    for (expected, actual) in expected_params.iter().zip(actual_types.iter()) {
        unify_call_type_binders(expected, actual, &candidate.generic_binders, &mut substitutions)?;
    }
    let expected_params = expected_params.iter().map(|ty| substitute_type_vars(ty, &substitutions)).collect::<Vec<_>>();
    if !type_relations.satisfies_where_constraints(&candidate.where_constraints, &substitutions) {
        return None;
    }
    let match_kind = compute_call_match_kind(type_relations, &actual_types, &expected_params)?;
    Some(OverloadCandidate {
        declaration: candidate.declaration,
        instance: candidate.instance,
        sum_owner: candidate.sum_owner,
        sum_variant: candidate.sum_variant,
        operator_id: candidate.operator_id,
        symbol: candidate.symbol.clone(),
        owner: candidate.owner.clone(),
        trait_owner: candidate.trait_owner.clone(),
        domain: candidate.domain.clone(),
        signature: OverloadSignature {
            params: expected_params,
            return_type: substitute_type_vars(&candidate.signature.return_type, &substitutions),
        },
        match_kind,
        param_specs: candidate.param_specs.clone(),
        generic_binders: candidate.generic_binders.clone(),
        where_constraints: candidate.where_constraints.clone(),
    })
}

fn unify_call_type_binders(
    expected: &ValkyrieType,
    actual: &ValkyrieType,
    binders: &BTreeSet<GenericBinderId>,
    substitutions: &mut BTreeMap<GenericBinderId, ValkyrieType>,
) -> Option<()> {
    match expected {
        ValkyrieType::Generic(generic) => {
            let binder = generic.binder?;
            if !binders.contains(&binder) {
                return (expected == actual).then_some(());
            }
            match substitutions.get(&binder) {
                Some(bound) if bound != actual => None,
                Some(_) => Some(()),
                None => {
                    substitutions.insert(binder, actual.clone());
                    Some(())
                }
            }
        }
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
        ValkyrieType::Nominal { declaration: expected_decl, arguments: expected_args } => {
            let ValkyrieType::Nominal { declaration: actual_decl, arguments: actual_args } = actual
            else {
                return None;
            };
            if expected_decl != actual_decl || expected_args.len() != actual_args.len() {
                return None;
            }
            for (expected, actual) in expected_args.iter().zip(actual_args) {
                unify_call_type_binders(expected, actual, binders, substitutions)?;
            }
            Some(())
        }
        _ => Some(()),
    }
}

fn specialize_call_literals(
    args: &[HirCallArgument],
    actual: &[ValkyrieType],
    expected: &[ValkyrieType],
    type_relations: &TypeRelationContext,
) -> Vec<ValkyrieType> {
    let exprs = args.iter().map(|arg| arg.value.clone()).collect::<Vec<_>>();
    specialize_call_literal_exprs(&exprs, actual, expected, type_relations)
}

fn specialize_call_literal_exprs(
    exprs: &[HirExpr],
    actual: &[ValkyrieType],
    expected: &[ValkyrieType],
    type_relations: &TypeRelationContext,
) -> Vec<ValkyrieType> {
    exprs
        .iter()
        .zip(actual.iter().zip(expected.iter()))
        .map(|(expr, (actual, expected))| match &expr.kind {
            // Integer literals are representation-polymorphic: bind to the formal
            // numeric type (`usize`, `i32`, …), same as constructor literals.
            // `Nominal{ItemId}` 的 primitive 也必须认，不能只认 `Named("usize")`。
            HirExprKind::Literal(crate::types::hir::HirLiteral::Integer64(_)) if type_relations.is_numeric_type(expected) => expected.clone(),
            // String literals infer as `Utf8` but core APIs take `text`.
            HirExprKind::Literal(crate::types::hir::HirLiteral::String(_)) if is_text_type(expected) => expected.clone(),
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
    call_kind: HirCallKind,
    callee: &HirExpr,
    args: &[HirCallArgument],
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &AggregateFieldIndex,
    singleton_names: &BTreeSet<Identifier>,
) -> Option<HirResolvedCall> {
    let (explicit_generic_arguments, callee) = peel_explicit_generic_arguments(callee);
    if let HirCallKind::Operator(operator_id) = call_kind {
        let filtered = candidates
            .iter()
            .filter(|candidate| candidate.operator_id == Some(operator_id))
            .filter_map(|candidate| {
                match_call_candidate(candidate, args, &explicit_generic_arguments, type_relations, locals, struct_fields, singleton_names)
            })
            .collect::<Vec<_>>();
        let resolved = resolve_overload(&filtered).ok()?;
        let has_receiver = candidate_has_receiver_parameter(&resolved);
        return Some(HirResolvedCall {
            declaration: resolved.declaration,
            instance: resolved.instance,
            sum_owner: resolved.sum_owner,
            sum_variant: resolved.sum_variant,
            symbol: overload_symbol_path(&resolved),
            domain: HirCallableDomain::Operator,
            return_type: resolved.signature.return_type,
            parameter_types: resolved.signature.params,
            has_receiver,
            extractor_payload_type: None,
        });
    }
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
                declaration: None,
                instance: None,
                sum_owner: None,
                sum_variant: None,
                symbol: NamePath::new(vec![Identifier::new(local_name)]),
                domain: HirCallableDomain::Function,
                return_type: func.return_type.clone(),
                parameter_types: func.params.clone(),
                has_receiver: false,
                extractor_payload_type: None,
            });
        }
        if locals.contains_key(local_name) {
            return None;
        }
    }
    if let HirExprKind::Path(path) = &callee.kind {
        if path.parts().len() == 2 {
            let owner = &path.parts()[0];
            let method_name = &path.parts()[1];
            let subjects = type_relations.callable_constraints().iter().filter(|constraint| {
                matches!(&constraint.target, ValkyrieType::Generic(generic) if generic.binder.is_some() && generic.name == *owner)
            }).collect::<Vec<_>>();
            if let [subject] = subjects.as_slice() {
                let mut matches = Vec::new();
                for bound in &subject.bounds {
                    for candidate in candidates
                        .iter()
                        .filter(|candidate| !candidate_has_receiver_parameter(candidate))
                        .filter(|candidate| candidate.symbol.parts().last() == Some(method_name))
                        .filter(|candidate| bound.declaration.is_some() && candidate.trait_owner == bound.declaration)
                    {
                        let candidate = instantiate_trait_bound_candidate(candidate, &subject.target, bound)?;
                        if let Some(matched) = match_call_candidate(
                            &candidate,
                            args,
                            &explicit_generic_arguments,
                            type_relations,
                            locals,
                            struct_fields,
                            singleton_names,
                        ) {
                            matches.push(matched);
                        }
                    }
                }
                let matched = resolve_overload(&matches).ok()?;
                return Some(HirResolvedCall {
                    declaration: matched.declaration,
                    instance: matched.instance,
                    sum_owner: matched.sum_owner,
                    sum_variant: matched.sum_variant,
                    symbol: matched.symbol,
                    domain: HirCallableDomain::Function,
                    return_type: matched.signature.return_type,
                    parameter_types: matched.signature.params,
                    has_receiver: false,
                    extractor_payload_type: None,
                });
            }
            if singleton_names.contains(owner) {
                return try_resolve_singleton_method(
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
                );
            }
        }
    }

    // `receiver.method(args)` — virtual dispatch: prepend receiver, match imply/class methods.
    // Typechecker may also flatten `args.length` into a Path whose root is a local/param.
    if let HirExprKind::FieldAccess { object, field } = &callee.kind {
        return try_resolve_instance_method(object, field, args, candidates, type_relations, locals, struct_fields, singleton_names);
    }
    if let HirExprKind::Path(path) = &callee.kind {
        if path.parts().len() >= 2 && locals.contains_key(path.parts()[0].as_str()) {
            let method_name = path.parts().last().expect("path has parts");
            let span = callee.span.clone();
            let mut receiver = HirExpr {
                kind: HirExprKind::Variable(HirIdentifier { name: path.parts()[0].clone(), shadow_index: 0, span: span.clone() }),
                span: span.clone(),
            };
            for field in &path.parts()[1..path.parts().len() - 1] {
                receiver = HirExpr { kind: HirExprKind::FieldAccess { object: Box::new(receiver), field: field.clone() }, span: span.clone() };
            }
            return try_resolve_instance_method(
                &receiver,
                method_name,
                args,
                candidates,
                type_relations,
                locals,
                struct_fields,
                singleton_names,
            );
        }
    }
    if let HirExprKind::Path(path) = &callee.kind {
        if path.parts().len() >= 2 {
            let type_owner = &path.parts()[path.parts().len() - 2];
            let method_name = path.parts().last().expect("qualified call path must have a final segment");
            if singleton_names.contains(type_owner) {
                return try_resolve_singleton_method(
                    type_owner,
                    method_name,
                    args,
                    candidates,
                    type_relations,
                    locals,
                    struct_fields,
                    singleton_names,
                    true,
                    false,
                );
            }
            if candidates.iter().any(|candidate| {
                candidate.owner.as_ref() == Some(type_owner) && candidate.symbol.parts().last().is_some_and(|name| name == method_name)
            }) {
                return try_resolve_type_static_method(
                    type_owner,
                    method_name,
                    args,
                    candidates,
                    type_relations,
                    locals,
                    struct_fields,
                    singleton_names,
                );
            }
            return try_resolve_qualified_free_function(path, args, candidates, type_relations, locals, struct_fields, singleton_names);
        }
    }

    let callee_name = extract_callable_name(callee)?;
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
        .filter(|candidate| symbol_matches_callee_name(&candidate.symbol, &callee_name))
        .filter_map(|candidate| {
            match_call_candidate(candidate, args, &explicit_generic_arguments, type_relations, locals, struct_fields, singleton_names)
        })
        .collect::<Vec<_>>();
    // An unqualified constructor expression denotes the nominal constructor
    // contract when a same-spelled ordinary function is also visible. This
    // is a domain precedence rule, so generic libraries cannot manufacture an
    // ambiguity between a variant and a helper function with the same name.
    if filtered.iter().any(|candidate| candidate.domain == OverloadDomain::Constructor) {
        filtered.retain(|candidate| candidate.domain == OverloadDomain::Constructor);
    }
    let resolved = resolve_overload(&filtered).ok()?;
    let has_receiver = candidate_has_receiver_parameter(&resolved);
    let symbol = overload_symbol_path(&resolved);
    let return_type = resolved.signature.return_type;
    let mut call = HirResolvedCall {
        declaration: resolved.declaration,
        instance: resolved.instance,
        sum_owner: resolved.sum_owner,
        sum_variant: resolved.sum_variant,
        symbol,
        domain: match resolved.domain {
            OverloadDomain::Function => HirCallableDomain::Function,
            OverloadDomain::Constructor => HirCallableDomain::Constructor,
            OverloadDomain::Operator => HirCallableDomain::Operator,
            OverloadDomain::Extractor => HirCallableDomain::Extractor,
        },
        return_type,
        parameter_types: resolved.signature.params,
        has_receiver,
        extractor_payload_type: None,
    };
    if matches!(callee_name.as_str(), "unwrap" | "is_some") {
        let receiver_type = args.first().and_then(|arg| infer_scrutinee_type(&arg.value, locals, struct_fields, singleton_names));
        enrich_sum_variant_operation_facts(&mut call, receiver_type.as_ref(), candidates);
    }
    Some(call)
}

fn is_text_type(ty: &ValkyrieType) -> bool {
    match ty {
        ValkyrieType::Utf8 | ValkyrieType::Utf16 => true,
        ValkyrieType::Named(name) => {
            matches!(name.as_str(), "text" | "utf8" | "Utf8Text" | "utf16" | "Utf16Text" | "core::text::text" | "core.text.text")
        }
        _ => false,
    }
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

fn try_resolve_qualified_free_function(
    path: &NamePath,
    args: &[HirCallArgument],
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &AggregateFieldIndex,
    singleton_names: &BTreeSet<Identifier>,
) -> Option<HirResolvedCall> {
    let canonical_text = QualifiedName::new(path.parts().to_vec()).to_string();
    let canonical_path = NamePath::new(vec![Identifier::new(&canonical_text)]);
    let filtered = candidates
        .iter()
        .filter(|candidate| candidate.owner.is_none() && candidate.symbol == canonical_path)
        .filter_map(|candidate| match_call_candidate(candidate, args, &[], type_relations, locals, struct_fields, singleton_names))
        .collect::<Vec<_>>();
    let resolved = resolve_overload(&filtered).ok()?;
    let domain = match resolved.domain {
        OverloadDomain::Function => HirCallableDomain::Function,
        OverloadDomain::Constructor => HirCallableDomain::Constructor,
        OverloadDomain::Operator => HirCallableDomain::Operator,
        OverloadDomain::Extractor => HirCallableDomain::Extractor,
    };
    Some(HirResolvedCall {
        declaration: resolved.declaration,
        instance: resolved.instance,
        sum_owner: resolved.sum_owner,
        sum_variant: resolved.sum_variant,
        symbol: resolved.symbol,
        domain,
        return_type: resolved.signature.return_type,
        parameter_types: resolved.signature.params,
        has_receiver: false,
        extractor_payload_type: None,
    })
}

fn try_resolve_instance_method(
    receiver: &HirExpr,
    method_name: &Identifier,
    args: &[HirCallArgument],
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &AggregateFieldIndex,
    singleton_names: &BTreeSet<Identifier>,
) -> Option<HirResolvedCall> {
    let receiver_arg = HirCallArgument::positional(receiver.clone());
    let mut full_args = Vec::with_capacity(args.len() + 1);
    full_args.push(receiver_arg);
    full_args.extend_from_slice(args);

    let receiver_type = infer_expr_type(receiver, locals).or_else(|| infer_scrutinee_type(receiver, locals, struct_fields, singleton_names));
    let mut filtered = Vec::new();
    for candidate in candidates
        .iter()
        .filter(|candidate| matches!(candidate.domain, OverloadDomain::Function | OverloadDomain::Operator))
        .filter(|candidate| candidate_has_receiver_parameter(candidate))
        .filter(|candidate| candidate.symbol.parts().last().is_some_and(|name| name == method_name))
    {
        if let Some(ValkyrieType::Generic(generic)) = &receiver_type {
            if generic.binder.is_none() {
                continue;
            }
            for bound in type_relations
                .callable_constraints()
                .iter()
                .filter(|constraint| Some(&constraint.target) == receiver_type.as_ref())
                .flat_map(|constraint| &constraint.bounds)
                .filter(|bound| bound.declaration.is_some() && bound.declaration == candidate.trait_owner)
            {
                let candidate = instantiate_trait_bound_candidate(candidate, receiver_type.as_ref().unwrap(), bound)?;
                if let Some(matched) = match_call_candidate(&candidate, &full_args, &[], type_relations, locals, struct_fields, singleton_names)
                {
                    filtered.push(matched);
                }
            }
        }
        else if candidate.trait_owner.is_none() {
            if let Some(matched) = match_call_candidate(candidate, &full_args, &[], type_relations, locals, struct_fields, singleton_names) {
                filtered.push(matched);
            }
        }
    }
    let resolved = resolve_overload(&filtered).ok()?;
    let has_receiver = candidate_has_receiver_parameter(&resolved);
    let mut call = HirResolvedCall {
        declaration: resolved.declaration,
        instance: resolved.instance,
        sum_owner: resolved.sum_owner,
        sum_variant: resolved.sum_variant,
        symbol: overload_symbol_path(&resolved),
        domain: HirCallableDomain::Function,
        return_type: resolved.signature.return_type,
        parameter_types: resolved.signature.params,
        has_receiver,
        extractor_payload_type: None,
    };
    enrich_sum_variant_operation_facts(&mut call, receiver_type.as_ref(), candidates);
    Some(call)
}

/// 用 bound 的 trait 声明、实参与关联等式实例化成员签名，不能按 trait 短名筛选。
fn instantiate_trait_bound_candidate(
    candidate: &OverloadCandidate,
    subject: &ValkyrieType,
    bound: &HirTraitBound,
) -> Option<OverloadCandidate> {
    let trait_id = bound.declaration?;
    if candidate.trait_owner != Some(trait_id) {
        return None;
    }
    let binders = candidate.generic_binders.iter().filter(|binder| binder.owner == trait_id).copied().collect::<Vec<_>>();
    if binders.len() != bound.type_arguments.len() {
        return None;
    }
    let substitution = binders.into_iter().zip(bound.type_arguments.iter().cloned()).collect();
    let mut candidate = substitute_trait_self(candidate, subject);
    let equations = bound.associated_types.iter().map(|equation| (equation.name.clone(), equation.ty.clone())).collect();
    candidate.signature.params = candidate
        .signature
        .params
        .iter()
        .map(|ty| substitute_associated_equations(&substitute_type_vars(ty, &substitution), subject, &equations))
        .collect();
    candidate.signature.return_type =
        substitute_associated_equations(&substitute_type_vars(&candidate.signature.return_type, &substitution), subject, &equations);
    candidate.generic_binders.retain(|binder| binder.owner != trait_id);
    // 未标注 self 仍是声明接收者槽，bound 已提供精确类型。
    if candidate_has_receiver_parameter(&candidate)
        && matches!(candidate.signature.params.first(), Some(ValkyrieType::AutoType | ValkyrieType::SelfType))
    {
        candidate.signature.params[0] = subject.clone();
    }
    Some(candidate)
}

fn substitute_associated_equations(
    ty: &ValkyrieType,
    constrained_type: &ValkyrieType,
    equations: &BTreeMap<Identifier, ValkyrieType>,
) -> ValkyrieType {
    match ty {
        ValkyrieType::Nominal { declaration, arguments } => ValkyrieType::Nominal {
            declaration: *declaration,
            arguments: arguments.iter().map(|argument| substitute_associated_equations(argument, constrained_type, equations)).collect(),
        },
        ValkyrieType::Associated(associated) if &associated.base == constrained_type => {
            equations.get(&associated.name).cloned().unwrap_or_else(|| {
                ValkyrieType::Associated(Box::new(crate::types::hir::AssociatedType {
                    base: substitute_associated_equations(&associated.base, constrained_type, equations),
                    name: associated.name.clone(),
                    type_arguments: associated
                        .type_arguments
                        .iter()
                        .map(|argument| substitute_associated_equations(argument, constrained_type, equations))
                        .collect(),
                }))
            })
        }
        ValkyrieType::Associated(associated) => ValkyrieType::Associated(Box::new(crate::types::hir::AssociatedType {
            base: substitute_associated_equations(&associated.base, constrained_type, equations),
            name: associated.name.clone(),
            type_arguments: associated
                .type_arguments
                .iter()
                .map(|argument| substitute_associated_equations(argument, constrained_type, equations))
                .collect(),
        })),
        ValkyrieType::Apply(base, arguments) => ValkyrieType::Apply(
            Box::new(substitute_associated_equations(base, constrained_type, equations)),
            arguments.iter().map(|argument| substitute_associated_equations(argument, constrained_type, equations)).collect(),
        ),
        ValkyrieType::Array(element) => ValkyrieType::Array(Box::new(substitute_associated_equations(element, constrained_type, equations))),
        ValkyrieType::FixedArray { element, length } => ValkyrieType::FixedArray {
            element: Box::new(substitute_associated_equations(element, constrained_type, equations)),
            length: *length,
        },
        ValkyrieType::Tuple(items) => {
            ValkyrieType::Tuple(items.iter().map(|item| substitute_associated_equations(item, constrained_type, equations)).collect())
        }
        ValkyrieType::Union(items) => {
            ValkyrieType::Union(items.iter().map(|item| substitute_associated_equations(item, constrained_type, equations)).collect())
        }
        ValkyrieType::Intersection(items) => {
            ValkyrieType::Intersection(items.iter().map(|item| substitute_associated_equations(item, constrained_type, equations)).collect())
        }
        ValkyrieType::Nullable(inner) => ValkyrieType::Nullable(Box::new(substitute_associated_equations(inner, constrained_type, equations))),
        ValkyrieType::Function(function) => ValkyrieType::Function(Box::new(crate::types::hir::FunctionType {
            params: function.params.iter().map(|param| substitute_associated_equations(param, constrained_type, equations)).collect(),
            return_type: substitute_associated_equations(&function.return_type, constrained_type, equations),
        })),
        other => other.clone(),
    }
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

fn try_resolve_type_static_method(
    type_owner: &Identifier,
    method_name: &Identifier,
    args: &[HirCallArgument],
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &AggregateFieldIndex,
    singleton_names: &BTreeSet<Identifier>,
) -> Option<HirResolvedCall> {
    let filtered = candidates
        .iter()
        .filter(|candidate| candidate.owner.as_ref() == Some(type_owner))
        .filter(|candidate| matches!(candidate.domain, OverloadDomain::Function | OverloadDomain::Constructor))
        .filter(|candidate| !candidate_has_receiver_parameter(candidate))
        .filter(|candidate| candidate.symbol.parts().last().is_some_and(|name| name == method_name))
        .filter_map(|candidate| match_call_candidate(candidate, args, &[], type_relations, locals, struct_fields, singleton_names))
        .collect::<Vec<_>>();
    if filtered.is_empty() {
        return None;
    }
    let resolved = resolve_overload(&filtered).ok()?;
    Some(HirResolvedCall {
        declaration: resolved.declaration,
        instance: resolved.instance,
        sum_owner: resolved.sum_owner,
        sum_variant: resolved.sum_variant,
        symbol: overload_symbol_path(&resolved),
        domain: match resolved.domain {
            OverloadDomain::Constructor => HirCallableDomain::Constructor,
            _ => HirCallableDomain::Function,
        },
        return_type: substitute_self_type(&resolved.signature.return_type, Some(type_owner)),
        parameter_types: resolved.signature.params.iter().map(|ty| substitute_self_type(ty, Some(type_owner))).collect(),
        has_receiver: candidate_has_receiver_parameter(&resolved),
        extractor_payload_type: None,
    })
}

fn substitute_trait_self(candidate: &OverloadCandidate, owner_type: &ValkyrieType) -> OverloadCandidate {
    let mut candidate = candidate.clone();
    candidate.signature.params = candidate.signature.params.iter().map(|ty| substitute_type_self(ty, owner_type)).collect();
    candidate.signature.return_type = substitute_type_self(&candidate.signature.return_type, owner_type);
    candidate.where_constraints = candidate
        .where_constraints
        .iter()
        .map(|constraint| HirWhereConstraint {
            target: substitute_type_self(&constraint.target, owner_type),
            bounds: constraint
                .bounds
                .iter()
                .map(|bound| crate::types::hir::HirTraitBound {
                    declaration: bound.declaration,
                    trait_path: bound.trait_path.clone(),
                    type_arguments: bound.type_arguments.iter().map(|ty| substitute_type_self(ty, owner_type)).collect(),
                    associated_types: bound
                        .associated_types
                        .iter()
                        .map(|equation| crate::types::hir::HirAssociatedTypeBinding {
                            name: equation.name.clone(),
                            ty: substitute_type_self(&equation.ty, owner_type),
                        })
                        .collect(),
                })
                .collect(),
            span: constraint.span.clone(),
        })
        .collect();
    candidate
}

fn substitute_type_self(ty: &ValkyrieType, replacement: &ValkyrieType) -> ValkyrieType {
    match ty {
        ValkyrieType::SelfType => replacement.clone(),
        ValkyrieType::Nominal { declaration, arguments } => ValkyrieType::Nominal {
            declaration: *declaration,
            arguments: arguments.iter().map(|argument| substitute_type_self(argument, replacement)).collect(),
        },
        ValkyrieType::Array(inner) => ValkyrieType::Array(Box::new(substitute_type_self(inner, replacement))),
        ValkyrieType::Apply(base, args) => ValkyrieType::Apply(
            Box::new(substitute_type_self(base, replacement)),
            args.iter().map(|arg| substitute_type_self(arg, replacement)).collect(),
        ),
        ValkyrieType::Tuple(items) => ValkyrieType::Tuple(items.iter().map(|item| substitute_type_self(item, replacement)).collect()),
        ValkyrieType::Union(items) => ValkyrieType::Union(items.iter().map(|item| substitute_type_self(item, replacement)).collect()),
        ValkyrieType::Nullable(inner) => ValkyrieType::Nullable(Box::new(substitute_type_self(inner, replacement))),
        ValkyrieType::Associated(associated) => ValkyrieType::Associated(Box::new(crate::types::hir::AssociatedType {
            base: substitute_type_self(&associated.base, replacement),
            name: associated.name.clone(),
            type_arguments: associated.type_arguments.iter().map(|ty| substitute_type_self(ty, replacement)).collect(),
        })),
        other => other.clone(),
    }
}

fn try_resolve_singleton_method(
    owner: &Identifier,
    method_name: &Identifier,
    args: &[HirCallArgument],
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &AggregateFieldIndex,
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
    let has_receiver = candidate_has_receiver_parameter(&resolved);
    let symbol = overload_symbol_path(&resolved);
    let actual_receiver = if strip_receiver_arg && has_receiver {
        args.first().and_then(|arg| infer_scrutinee_type(&arg.value, locals, struct_fields, singleton_names))
    }
    else {
        None
    };
    let (return_type, parameter_types) = match (resolved.signature.params.first(), actual_receiver.as_ref()) {
        (Some(receiver_type), Some(actual_receiver)) => (
            substitute_type_parameters(&resolved.signature.return_type, receiver_type, actual_receiver),
            resolved.signature.params.iter().map(|param| substitute_type_parameters(param, receiver_type, actual_receiver)).collect(),
        ),
        _ => (resolved.signature.return_type, resolved.signature.params),
    };
    Some(HirResolvedCall {
        declaration: resolved.declaration,
        instance: resolved.instance,
        sum_owner: resolved.sum_owner,
        sum_variant: resolved.sum_variant,
        symbol,
        domain: HirCallableDomain::Function,
        return_type,
        parameter_types,
        has_receiver,
        extractor_payload_type: None,
    })
}

fn match_singleton_method_candidate(
    candidate: &OverloadCandidate,
    call_args: &[HirCallArgument],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &AggregateFieldIndex,
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
        call_args.iter().map(|arg| infer_scrutinee_type(&arg.value, locals, struct_fields, singleton_names)).collect::<Option<Vec<_>>>()?
    }
    else {
        let bound = bind_call_arguments(&param_specs, call_args).ok()?;
        bound.iter().map(|arg| infer_scrutinee_type(arg, locals, struct_fields, singleton_names)).collect::<Option<Vec<_>>>()?
    };
    let match_kind = compute_call_match_kind(type_relations, &actual_types, &expected_types)?;
    let mut matched = candidate.clone();
    matched.match_kind = match_kind;
    Some(matched)
}

fn impl_nominal_type_name(ty: &ValkyrieType) -> Option<&str> {
    match ty {
        ValkyrieType::Array(_) => Some("Array"),
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
        // Nominal 无表面名；必须经 `impl_owner_identifier` 按 declaration 反查。
        ValkyrieType::Nominal { .. } => None,
        _ => None,
    }
}

/// imply owner 短名：优先 declaration → 结构/sum 名；禁止把 Nominal 当无名丢弃。
fn impl_owner_identifier(module: &HirModule, ty: &ValkyrieType) -> Option<Identifier> {
    match ty {
        ValkyrieType::Nominal { declaration, .. } => {
            find_struct_name_by_declaration(module, *declaration).or_else(|| find_enum_name_by_declaration(module, *declaration))
        }
        ValkyrieType::Apply(base, _) => impl_owner_identifier(module, base),
        other => impl_nominal_type_name(other).map(Identifier::new),
    }
}

fn impl_owner_identifier_from_export(export: &HirDependencySemanticExport, ty: &ValkyrieType) -> Option<Identifier> {
    match ty {
        ValkyrieType::Nominal { declaration, .. } => export
            .structs
            .iter()
            .find(|item| item.constructor_declaration == Some(*declaration))
            .map(|item| item.name.clone())
            .or_else(|| export.enums.iter().find(|item| item.declaration == Some(*declaration)).map(|item| item.name.clone())),
        ValkyrieType::Apply(base, _) => impl_owner_identifier_from_export(export, base),
        other => impl_nominal_type_name(other).map(Identifier::new),
    }
}

fn find_struct_name_by_declaration(module: &HirModule, declaration: nyar_types::ItemId) -> Option<Identifier> {
    for item in &module.structs {
        if item.constructor_declaration == Some(declaration) {
            return Some(item.name.clone());
        }
    }
    for export in &module.imported_semantic_exports {
        for item in &export.structs {
            if item.constructor_declaration == Some(declaration) {
                return Some(item.name.clone());
            }
        }
    }
    for submodule in &module.submodules {
        if let Some(name) = find_struct_name_by_declaration(submodule, declaration) {
            return Some(name);
        }
    }
    None
}

fn find_enum_name_by_declaration(module: &HirModule, declaration: nyar_types::ItemId) -> Option<Identifier> {
    for item in &module.enums {
        if item.declaration == Some(declaration) {
            return Some(item.name.clone());
        }
    }
    for export in &module.imported_semantic_exports {
        for item in &export.enums {
            if item.declaration == Some(declaration) {
                return Some(item.name.clone());
            }
        }
    }
    for submodule in &module.submodules {
        if let Some(name) = find_enum_name_by_declaration(submodule, declaration) {
            return Some(name);
        }
    }
    None
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

fn sum_declaration_of_type(ty: &ValkyrieType) -> Option<nyar_types::ItemId> {
    match ty {
        ValkyrieType::Nominal { declaration, .. } => Some(*declaration),
        ValkyrieType::Apply(base, _) => sum_declaration_of_type(base),
        _ => None,
    }
}

fn try_resolve_nullary_constructor(
    name: &Identifier,
    contextual_type: Option<&ValkyrieType>,
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &AggregateFieldIndex,
    singleton_names: &BTreeSet<Identifier>,
) -> Option<HirResolvedCall> {
    if singleton_names.contains(name) || locals.contains_key(name.as_str()) {
        return None;
    }
    let narrowed = candidates
        .iter()
        .filter(|candidate| candidate.domain == OverloadDomain::Constructor)
        .filter(|candidate| candidate.signature.params.is_empty())
        .filter(|candidate| candidate.symbol.parts().last().is_some_and(|variant| variant == name))
        .filter_map(|candidate| {
            let Some(context) = contextual_type
            else {
                return Some(candidate.clone());
            };
            let mut substitutions = BTreeMap::new();
            unify_call_type_binders(&candidate.signature.return_type, context, &candidate.generic_binders, &mut substitutions)?;
            let instantiated = substitute_type_vars(&candidate.signature.return_type, &substitutions);
            if matches!(type_relations.match_parameter(&instantiated, context), ParameterMatchResult::NoMatch { .. }) {
                return None;
            }
            let mut candidate = candidate.clone();
            candidate.signature.return_type = instantiated;
            Some(candidate)
        })
        .collect::<Vec<_>>();
    if narrowed.is_empty() {
        return None;
    }
    try_resolve_constructor(&NamePath::new(vec![name.clone()]), &[], &narrowed, type_relations, locals, struct_fields, singleton_names)
}

fn try_resolve_nullary_variant_expr(
    expr: &mut HirExpr,
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &AggregateFieldIndex,
    singleton_names: &BTreeSet<Identifier>,
    contextual_type: Option<&ValkyrieType>,
) {
    let name = match &expr.kind {
        HirExprKind::Variable(identifier) if !locals.contains_key(identifier.name.as_str()) => identifier.name.clone(),
        HirExprKind::Path(path) if path.parts().len() == 1 && !locals.contains_key(path.parts()[0].as_str()) => path.parts()[0].clone(),
        _ => return,
    };
    try_rewrite_nullary_variant_reference(expr, &name, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
}

fn try_rewrite_nullary_variant_reference(
    expr: &mut HirExpr,
    name: &Identifier,
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &AggregateFieldIndex,
    singleton_names: &BTreeSet<Identifier>,
    contextual_type: Option<&ValkyrieType>,
) {
    let Some(resolved) =
        try_resolve_nullary_constructor(name, contextual_type, candidates, type_relations, locals, struct_fields, singleton_names)
    else {
        return;
    };
    *expr = HirExpr {
        span: expr.span.clone(),
        kind: HirExprKind::Construct {
            path: NamePath::new(vec![name.clone()]),
            name: name.clone(),
            args: Vec::new(),
            resolved: Some(resolved),
        },
    };
}

fn paired_sum_identity_facts(sum_owner: Option<nyar_types::ItemId>, sum_variant: Option<nyar_types::ItemId>) -> bool {
    sum_owner.is_none() == sum_variant.is_none()
}

fn peel_explicit_generic_arguments(callee: &HirExpr) -> (Vec<ValkyrieType>, &HirExpr) {
    match &callee.kind {
        HirExprKind::GenericApply { callee, arguments } => (arguments.clone(), callee.as_ref()),
        _ => (Vec::new(), callee),
    }
}

fn sum_apply_context(ty: &ValkyrieType) -> Option<ValkyrieType> {
    if is_option_apply_type(ty) || is_result_apply_type(ty) { Some(ty.clone()) } else { None }
}

fn option_apply_hint(
    contextual_type: Option<&ValkyrieType>,
    call_return_type: &ValkyrieType,
    explicit_generic_arguments: &[ValkyrieType],
) -> Option<ValkyrieType> {
    // 只消费已冻结的 Nominal unity 上下文；禁止用实参拼出 Named("Option")。
    let _ = explicit_generic_arguments;
    contextual_type.and_then(sum_apply_context).or_else(|| sum_apply_context(call_return_type))
}

fn enrich_unite_variant_constructor_facts(
    callee: &HirExpr,
    args: &[HirCallArgument],
    call: &mut HirResolvedCall,
    contextual_type: Option<&ValkyrieType>,
    candidates: &[OverloadCandidate],
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &AggregateFieldIndex,
    singleton_names: &BTreeSet<Identifier>,
) {
    if call.sum_owner.is_some() && call.sum_variant.is_some() {
        return;
    }
    let Some(variant_name) = extract_callable_name(callee).or_else(|| call.symbol.parts().last().cloned())
    else {
        return;
    };
    let (explicit_generic_arguments, _) = peel_explicit_generic_arguments(callee);
    let hint = sum_apply_context(&call.return_type)
        .or_else(|| contextual_type.and_then(sum_apply_context))
        .or_else(|| option_apply_hint(contextual_type, &call.return_type, &explicit_generic_arguments))
        .or_else(|| {
            if args.len() != 1 {
                return None;
            }
            let _payload = infer_scrutinee_type(&args[0].value, locals, struct_fields, singleton_names)?;
            candidates
                .iter()
                .filter(|candidate| candidate.domain == OverloadDomain::Constructor)
                .filter(|candidate| candidate.symbol.parts().last() == Some(&variant_name))
                .find_map(|candidate| candidate.signature.return_type.clone().into())
        });
    let Some(declaration) = hint.as_ref().and_then(|ty| sum_declaration_of_type(ty))
    else {
        return;
    };
    let Some((owner, variant)) = variant_constructor_facts_from_declaration(candidates, declaration, variant_name.as_str())
    else {
        return;
    };
    call.sum_owner = Some(owner);
    call.sum_variant = Some(variant);
    call.domain = HirCallableDomain::Constructor;
    call.declaration = None;
    call.instance = None;
    call.symbol = NamePath::new(vec![variant_name]);
    if let Some(ctx) = hint {
        call.return_type = ctx;
    }
}

fn enrich_construct_unite_variant_facts(
    name: &Identifier,
    args: &[HirExpr],
    call: &mut HirResolvedCall,
    contextual_type: Option<&ValkyrieType>,
    candidates: &[OverloadCandidate],
) {
    if call.sum_owner.is_some() && call.sum_variant.is_some() {
        return;
    }
    let hint = contextual_type
        .and_then(sum_apply_context)
        .or_else(|| sum_apply_context(&call.return_type))
        .or_else(|| option_apply_hint(contextual_type, &call.return_type, &[]));
    let Some(declaration) = hint.as_ref().and_then(|ty| sum_declaration_of_type(ty))
    else {
        return;
    };
    let Some((owner, variant)) = variant_constructor_facts_from_declaration(candidates, declaration, name.as_str())
    else {
        return;
    };
    call.sum_owner = Some(owner);
    call.sum_variant = Some(variant);
    call.domain = HirCallableDomain::Constructor;
    call.declaration = None;
    call.instance = None;
    call.symbol = NamePath::new(vec![name.clone()]);
    if let Some(ctx) = hint {
        call.return_type = ctx;
    }
}

fn variant_constructor_facts_from_declaration(
    candidates: &[OverloadCandidate],
    sum_owner: nyar_types::ItemId,
    variant_name: &str,
) -> Option<(nyar_types::ItemId, nyar_types::ItemId)> {
    candidates
        .iter()
        .filter(|candidate| candidate.domain == OverloadDomain::Constructor)
        .filter(|candidate| candidate.sum_owner == Some(sum_owner))
        .filter(|candidate| candidate.symbol.parts().last().is_some_and(|variant| variant.as_str() == variant_name))
        .find_map(|candidate| candidate.sum_owner.zip(candidate.sum_variant))
}

fn try_resolve_try_propagate_ok_variant(operand_type: &ValkyrieType, candidates: &[OverloadCandidate]) -> Option<HirResolvedCall> {
    let declaration = sum_declaration_of_type(operand_type)?;
    let (variant_name, payload_type) = if is_result_apply_type(operand_type) {
        ("Fine", result_variant_payload_type(operand_type, "Fine"))
    }
    else if is_option_apply_type(operand_type) {
        ("Some", option_apply_payload_type(operand_type))
    }
    else {
        return None;
    };
    let Some((owner, variant)) = variant_constructor_facts_from_declaration(candidates, declaration, variant_name)
    else {
        return None;
    };
    Some(HirResolvedCall {
        declaration: None,
        instance: None,
        sum_owner: Some(owner),
        sum_variant: Some(variant),
        symbol: NamePath::new(vec![Identifier::new(variant_name)]),
        domain: HirCallableDomain::Extractor,
        return_type: payload_type.clone().unwrap_or(ValkyrieType::Unit),
        parameter_types: vec![operand_type.clone()],
        has_receiver: false,
        extractor_payload_type: payload_type,
    })
}

fn enrich_sum_variant_operation_facts(call: &mut HirResolvedCall, receiver_type: Option<&ValkyrieType>, candidates: &[OverloadCandidate]) {
    if call.sum_owner.is_some() && call.sum_variant.is_some() {
        return;
    }
    let Some(receiver_type) = receiver_type
    else {
        return;
    };
    let method_name = call.symbol.parts().last().map(|part| part.as_str());
    let variant_name = match method_name {
        Some("unwrap") | Some("is_some") if is_option_apply_type(receiver_type) => "Some",
        _ => return,
    };
    let Some(declaration) = sum_declaration_of_type(receiver_type)
    else {
        return;
    };
    let Some((owner, variant)) = variant_constructor_facts_from_declaration(candidates, declaration, variant_name)
    else {
        return;
    };
    call.sum_owner = Some(owner);
    call.sum_variant = Some(variant);
    if method_name == Some("unwrap") {
        call.domain = HirCallableDomain::Extractor;
        if call.extractor_payload_type.is_none() {
            call.extractor_payload_type = option_apply_payload_type(receiver_type).or_else(|| nullable_payload_type(&call.return_type));
        }
    }
}

fn try_resolve_constructor(
    path: &NamePath,
    args: &[HirExpr],
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &AggregateFieldIndex,
    singleton_names: &BTreeSet<Identifier>,
) -> Option<HirResolvedCall> {
    let name = path.parts().last()?.clone();
    if singleton_names.contains(&name) {
        return None;
    }
    let bound_values = if args.iter().all(|arg| matches!(arg.kind, HirExprKind::FieldInit { .. })) {
        if let Some(fields) = struct_fields.fields_of(&ValkyrieType::Named(name.clone())) {
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
    let arguments = HirCallArgument::from_exprs(bound_values);
    let filtered = candidates
        .iter()
        .filter(|candidate| candidate.domain == OverloadDomain::Constructor)
        .filter(|candidate| {
            candidate.symbol == *path
                || (path.parts().len() == 1 && candidate.symbol.parts().last().is_some_and(|candidate_name| candidate_name == &name))
        })
        .filter_map(|candidate| match_call_candidate(candidate, &arguments, &[], type_relations, locals, struct_fields, singleton_names))
        .collect::<Vec<_>>();
    let resolved = resolve_overload(&filtered).ok()?;
    if !paired_sum_identity_facts(resolved.sum_owner, resolved.sum_variant) {
        return None;
    }
    Some(HirResolvedCall {
        declaration: resolved.declaration,
        instance: resolved.instance,
        sum_owner: resolved.sum_owner,
        sum_variant: resolved.sum_variant,
        symbol: overload_symbol_path(&resolved),
        domain: HirCallableDomain::Constructor,
        return_type: resolved.signature.return_type,
        parameter_types: resolved.signature.params,
        has_receiver: false,
        extractor_payload_type: None,
    })
}

fn substitute_type_parameters(ty: &ValkyrieType, receiver: &ValkyrieType, actual: &ValkyrieType) -> ValkyrieType {
    // Apply 与 Nominal 的类型参数槽同位：按槽位把声明 binder 代入实际参数。
    // 不得再要求表面名为 Option/Result；declaration identity 已由候选 sum_owner 携带。
    let (receiver_args, actual_args) = match (receiver, actual) {
        (ValkyrieType::Apply(_, receiver_args), ValkyrieType::Apply(_, actual_args)) => (receiver_args.as_slice(), actual_args.as_slice()),
        (
            ValkyrieType::Nominal { declaration: receiver_decl, arguments: receiver_args },
            ValkyrieType::Nominal { declaration: actual_decl, arguments: actual_args },
        ) if receiver_decl == actual_decl => (receiver_args.as_slice(), actual_args.as_slice()),
        _ => return ty.clone(),
    };
    if receiver_args.len() != actual_args.len() {
        return ty.clone();
    }
    let substitutions = receiver_args
        .iter()
        .zip(actual_args.iter())
        .filter_map(|(param, arg)| match param {
            ValkyrieType::Generic(generic) => generic.binder.map(|binder| (binder, arg.clone())),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    substitute_type_vars(ty, &substitutions)
}

pub(crate) fn substitute_type_vars(ty: &ValkyrieType, substitutions: &BTreeMap<GenericBinderId, ValkyrieType>) -> ValkyrieType {
    match ty {
        ValkyrieType::Generic(generic) => generic.binder.and_then(|binder| substitutions.get(&binder)).cloned().unwrap_or_else(|| ty.clone()),
        ValkyrieType::Nominal { declaration, arguments } => ValkyrieType::Nominal {
            declaration: *declaration,
            arguments: arguments.iter().map(|arg| substitute_type_vars(arg, substitutions)).collect(),
        },
        ValkyrieType::Apply(base, args) => ValkyrieType::Apply(
            Box::new(substitute_type_vars(base, substitutions)),
            args.iter().map(|arg| substitute_type_vars(arg, substitutions)).collect(),
        ),
        ValkyrieType::Array(inner) => ValkyrieType::Array(Box::new(substitute_type_vars(inner, substitutions))),
        ValkyrieType::FixedArray { element, length } => {
            ValkyrieType::FixedArray { element: Box::new(substitute_type_vars(element, substitutions)), length: *length }
        }
        ValkyrieType::Tuple(items) => ValkyrieType::Tuple(items.iter().map(|item| substitute_type_vars(item, substitutions)).collect()),
        ValkyrieType::Union(items) => ValkyrieType::Union(items.iter().map(|item| substitute_type_vars(item, substitutions)).collect()),
        ValkyrieType::Intersection(items) => {
            ValkyrieType::Intersection(items.iter().map(|item| substitute_type_vars(item, substitutions)).collect())
        }
        ValkyrieType::Nullable(inner) => ValkyrieType::Nullable(Box::new(substitute_type_vars(inner, substitutions))),
        ValkyrieType::Function(function) => ValkyrieType::Function(Box::new(crate::types::hir::FunctionType {
            params: function.params.iter().map(|param| substitute_type_vars(param, substitutions)).collect(),
            return_type: substitute_type_vars(&function.return_type, substitutions),
        })),
        ValkyrieType::Associated(associated) => ValkyrieType::Associated(Box::new(crate::types::hir::AssociatedType {
            base: substitute_type_vars(&associated.base, substitutions),
            name: associated.name.clone(),
            type_arguments: associated.type_arguments.iter().map(|ty| substitute_type_vars(ty, substitutions)).collect(),
        })),
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
    match (actual, receiver) {
        (ValkyrieType::Apply(actual_base, actual_args), ValkyrieType::Apply(receiver_base, receiver_args)) => {
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
                ValkyrieType::Named(_) | ValkyrieType::Generic(_) => true,
                other => matches!(
                    type_relations.match_parameter(actual_arg, other),
                    ParameterMatchResult::NominalExact | ParameterMatchResult::NominalSubtype { .. }
                ),
            })
        }
        (
            ValkyrieType::Nominal { declaration: actual_decl, arguments: actual_args },
            ValkyrieType::Nominal { declaration: receiver_decl, arguments: receiver_args },
        ) if actual_decl == receiver_decl && actual_args.len() == receiver_args.len() => {
            receiver_args.iter().zip(actual_args.iter()).all(|(expected, actual_arg)| match expected {
                ValkyrieType::Named(_) | ValkyrieType::Generic(_) => true,
                other => matches!(
                    type_relations.match_parameter(actual_arg, other),
                    ParameterMatchResult::NominalExact | ParameterMatchResult::NominalSubtype { .. }
                ),
            })
        }
        _ => false,
    }
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
            let mut matched = candidate.clone();
            matched.domain = OverloadDomain::Extractor;
            matched.match_kind = match_kind;
            Some(matched)
        })
        .collect::<Vec<_>>();
    let resolved = resolve_overload(&filtered).ok()?;
    if !paired_sum_identity_facts(resolved.sum_owner, resolved.sum_variant) {
        return None;
    }
    let has_receiver = candidate_has_receiver_parameter(&resolved);
    let receiver_type = pattern_extractor_receiver_type(&resolved)?;
    let return_type = substitute_type_parameters(&resolved.signature.return_type, &receiver_type, actual_type);
    let payload_type = nullable_payload_type(&return_type)?;
    let parameter_types =
        resolved.signature.params.iter().map(|param| substitute_type_parameters(param, &receiver_type, actual_type)).collect();
    Some(HirResolvedCall {
        declaration: resolved.declaration,
        instance: resolved.instance,
        sum_owner: resolved.sum_owner,
        sum_variant: resolved.sum_variant,
        symbol: resolved.symbol,
        domain: HirCallableDomain::Extractor,
        return_type,
        parameter_types,
        has_receiver,
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
        HirExprKind::TupleLiteral { items } => {
            let element_types = items.iter().map(|item| infer_expr_type(item, locals)).collect::<Option<Vec<_>>>()?;
            Some(ValkyrieType::Tuple(element_types))
        }
        HirExprKind::Construct { resolved, .. } => resolved.as_ref().map(|call| call.return_type.clone()),
        HirExprKind::If { then_branch, else_branch, .. } | HirExprKind::IfLet { then_branch, else_branch, .. } => {
            let then_type = then_branch.expr.as_ref().and_then(|expr| infer_expr_type(expr, locals))?;
            let else_type = else_branch.as_ref().and_then(|branch| branch.expr.as_ref()).and_then(|expr| infer_expr_type(expr, locals))?;
            if then_type == else_type { Some(then_type) } else { None }
        }
        HirExprKind::Block(block) => block.expr.as_ref().and_then(|expr| infer_expr_type(expr, locals)),
        HirExprKind::Cast { ty, .. } => Some(ty.clone()),
        HirExprKind::TryPropagate { inner, .. } => infer_expr_type(inner, locals).and_then(|ty| nullable_payload_type(&ty)),
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
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &AggregateFieldIndex,
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
            let object_type = infer_scrutinee_type(object, locals, struct_fields, singleton_names)?;
            lookup_struct_field_type(&object_type, field, struct_fields)
        }
        _ => None,
    }
}

fn lookup_struct_field_type(object_type: &ValkyrieType, field: &Identifier, struct_fields: &AggregateFieldIndex) -> Option<ValkyrieType> {
    struct_fields.fields_of(object_type).and_then(|fields| fields.iter().find(|item| item.name == *field).map(|item| item.ty.clone()))
}

fn payload_field_scrutinee_types(payload: &ValkyrieType, struct_fields: &AggregateFieldIndex) -> Vec<Option<ValkyrieType>> {
    match payload {
        ValkyrieType::Tuple(items) => items.iter().map(|item| Some(item.clone())).collect(),
        ValkyrieType::Array(inner) => vec![Some(inner.as_ref().clone())],
        ValkyrieType::Named(_) | ValkyrieType::Nominal { .. } | ValkyrieType::Apply(_, _) => match struct_fields.fields_of(payload) {
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
    struct_fields: &'a AggregateFieldIndex,
) -> Option<&'a ValkyrieType> {
    let type_name = object_name.and_then(|path| path.parts().last())?;
    struct_fields.fields_of(&ValkyrieType::Named(type_name.clone()))?.iter().find(|field| &field.name == field_name).map(|field| &field.ty)
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
        HirPattern::Extractor(HirExtractorPattern::Constructor { name, fields, resolved, .. }) => {
            // A constructor arm binds its fields to the extractor payload,
            // not to the nullable scrutinee itself.  Keeping this mapping in
            // HIR locals is what lets later calls resolve `layout.id` after
            // `match Some(layout)` without backend-side Option erasure.
            let variant = name.parts().first().map(|part| part.as_str());
            let payload = resolved
                .as_ref()
                .and_then(|call| call.extractor_payload_type.clone())
                .or_else(|| nullable_payload_type(ty))
                .or_else(|| option_apply_payload_type(ty))
                .or_else(|| variant.and_then(|variant| result_variant_payload_type(ty, variant)));
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
            else {
                bind_unite_extractor_fields(fields, ty, variant, locals);
            }
        }
        HirPattern::Mut(pattern) | HirPattern::Pin { pattern, .. } => bind_pattern_type(pattern, ty, locals),
        _ => {}
    }
}

/// Validates that unresolved pattern extractors fail for explicit contract reasons (`mut self`, non-nullable return).
pub fn validate_extractor_patterns(module: &HirModule) -> Result<(), crate::valkyrie::frontend::ParseError> {
    let candidates = collect_module_candidates(module);
    let type_relations = TypeRelationContext::from_module(module);
    // 与 resolve_hir_calls 共用唯一生产者；禁止再拼一份短名 BTreeMap。
    let struct_fields = collect_aggregate_field_map(module);
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
    struct_fields: &AggregateFieldIndex,
) -> Result<(), crate::valkyrie::frontend::ParseError> {
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
    struct_fields: &AggregateFieldIndex,
) -> Result<(), crate::valkyrie::frontend::ParseError> {
    let mut locals = locals.clone();
    for statement in &block.statements {
        match &statement.kind {
            HirStatementKind::Let { pattern, ty, initializer, .. } => {
                // 对于 refutable pattern（如 `let Some(x) = opt`），跳过 extractor 校验，
                // 由后续 `check_pattern_refutability` 统一拒绝。
                if pattern.refutability() != PatternRefutability::Refutable {
                    let inferred_type = ty
                        .as_ref()
                        .cloned()
                        .or_else(|| initializer.as_ref().and_then(|init| infer_scrutinee_type(init, &locals, struct_fields, &BTreeSet::new())));
                    validate_pattern_extractor_contract(pattern, inferred_type.as_ref(), candidates, type_relations, struct_fields)?;
                }
                if let Some(initializer) = initializer {
                    validate_expr_extractor_patterns(initializer, candidates, type_relations, &locals, struct_fields)?;
                }
                if let Some(ty) = ty {
                    bind_pattern_type(pattern, ty, &mut locals);
                }
                else if let Some(initializer) = initializer {
                    if let Some(binding_type) = infer_scrutinee_type(initializer, &locals, struct_fields, &BTreeSet::new()) {
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
    struct_fields: &AggregateFieldIndex,
) -> Result<(), crate::valkyrie::frontend::ParseError> {
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
        | HirExprKind::Assign { value: callee, .. }
        | HirExprKind::FieldAccess { object: callee, .. } => {
            validate_expr_extractor_patterns(callee, candidates, type_relations, locals, struct_fields)?;
        }
        HirExprKind::TryPropagate { inner, .. } => {
            validate_expr_extractor_patterns(inner, candidates, type_relations, locals, struct_fields)?;
        }
        HirExprKind::Cast { expr, .. } => {
            validate_expr_extractor_patterns(expr, candidates, type_relations, locals, struct_fields)?;
        }
        HirExprKind::StoreField { object, value, .. } => {
            validate_expr_extractor_patterns(object, candidates, type_relations, locals, struct_fields)?;
            validate_expr_extractor_patterns(value, candidates, type_relations, locals, struct_fields)?;
        }
        HirExprKind::ArrayNew { length, .. } => validate_expr_extractor_patterns(length, candidates, type_relations, locals, struct_fields)?,
        HirExprKind::ArrayLiteral { items } | HirExprKind::TupleLiteral { items } => {
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
            let scrutinee_type = infer_scrutinee_type(scrutinee, locals, struct_fields, &BTreeSet::new());
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
            let scrutinee_type = infer_scrutinee_type(scrutinee, locals, struct_fields, &BTreeSet::new());
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
    struct_fields: &AggregateFieldIndex,
) -> Result<(), crate::valkyrie::frontend::ParseError> {
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

fn is_option_owner_type(ty: &ValkyrieType) -> bool {
    crate::hir::try_propagate::is_option_apply_type(ty)
}

fn is_result_owner_type(ty: &ValkyrieType) -> bool {
    crate::hir::try_propagate::is_result_apply_type(ty)
}

fn bind_unite_extractor_fields(fields: &[HirPattern], ty: &ValkyrieType, variant: Option<&str>, locals: &mut BTreeMap<String, ValkyrieType>) {
    if let Some(payload) = option_apply_payload_type(ty) {
        for (index, field) in fields.iter().enumerate() {
            let field_ty = if index == 0 { Some(payload.clone()) } else { None };
            if let Some(field_ty) = field_ty {
                bind_pattern_type(field, &field_ty, locals);
            }
        }
        return;
    }
    if let Some(variant) = variant {
        if let Some(payload) = result_variant_payload_type(ty, variant) {
            for (index, field) in fields.iter().enumerate() {
                let field_ty = if index == 0 { Some(payload.clone()) } else { None };
                if let Some(field_ty) = field_ty {
                    bind_pattern_type(field, &field_ty, locals);
                }
            }
            return;
        }
    }
    if is_option_owner_type(ty) || is_result_owner_type(ty) {
        for field in fields {
            bind_pattern_type(field, &ValkyrieType::AutoType, locals);
        }
    }
}

/// `Some` / `None` on `T | null` (and `Option<T>`) are built-in arms — not user extractors.
fn is_builtin_nullable_option_pattern(canonical_callee: &NamePath, actual_type: &ValkyrieType) -> bool {
    let head = canonical_callee.parts().first().map(|p| p.as_str()).unwrap_or("");
    if !matches!(head, "Some" | "None") {
        return false;
    }
    crate::hir::is_nullable_type(actual_type) || is_option_owner_type(actual_type)
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
    if crate::hir::is_result_apply_type(actual_type) || is_result_owner_type(actual_type) {
        return true;
    }
    is_result_owner_type(actual_type)
}

/// `Text` / `Object` / … on `VonValue` are language VON unite arms (legion.tools manifest.v).
fn is_builtin_von_value_pattern(canonical_callee: &NamePath, actual_type: &ValkyrieType) -> bool {
    let head = canonical_callee.parts().first().map(|p| p.as_str()).unwrap_or("");
    if !matches!(head, "Text" | "Flag" | "Number" | "Name" | "Array" | "Object" | "Empty") {
        return false;
    }
    matches!(actual_type, ValkyrieType::Named(name) if name.as_str() == "VonValue")
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
) -> Option<crate::valkyrie::frontend::ParseError> {
    use crate::valkyrie::frontend::ParseError;

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
        if is_builtin_von_value_pattern(canonical_callee, actual_type) {
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
) -> Option<crate::valkyrie::frontend::ParseError> {
    use crate::valkyrie::frontend::ParseError;

    let head = canonical_callee.parts().first().map(|p| p.as_str()).unwrap_or("");
    if matches!(head, "Fine" | "Fail" | "Some" | "None") {
        return Some(ParseError::invalid(format!("pattern extractor `{head}` 需要已解析的 scrutinee 类型，不能仅凭变体名放行")));
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

fn diagnose_unresolved_arm_extractor(pattern: &HirPattern, candidates: &[OverloadCandidate]) -> Option<crate::valkyrie::frontend::ParseError> {
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
        && matches!(
            param.ty,
            ValkyrieType::r#SelfType
                | ValkyrieType::AutoType
                | ValkyrieType::Named(_)
                | ValkyrieType::Nominal { .. }
                | ValkyrieType::Apply(_, _)
        )
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
fn test_binder(owner: u32, ordinal: u32) -> GenericBinderId {
    GenericBinderId { owner: nyar_types::ItemId::from_index(owner).unwrap(), ordinal }
}
#[cfg(test)]
fn test_generic(name: &str) -> ValkyrieType {
    ValkyrieType::Generic(GenericType {
        binder: Some(test_binder(900, 0)),
        name: Identifier::new(name),
        kind: crate::types::hir::HirKind::Type,
        bounds: vec![],
    })
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    use crate::{ValkyrieCompiler, types::SourceID};
    #[test]
    fn nominal_arguments_preserve_identity_and_substitute_associated_self() {
        let declaration = nyar_types::ItemId::from_index(41).unwrap();
        let subject = test_generic("I");
        let item = Identifier::new("Item");
        let original = ValkyrieType::Nominal {
            declaration,
            arguments: vec![ValkyrieType::Associated(Box::new(crate::types::hir::AssociatedType {
                base: ValkyrieType::SelfType,
                name: item.clone(),
                type_arguments: vec![],
            }))],
        };
        let bound = substitute_type_self(&original, &subject);
        let equations = BTreeMap::from([(item, ValkyrieType::Utf8)]);
        assert_eq!(
            substitute_associated_equations(&bound, &subject, &equations),
            ValkyrieType::Nominal { declaration, arguments: vec![ValkyrieType::Utf8] }
        );
    }

    #[test]
    #[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
    fn chained_generic_trait_result_resolves_declared_member() {
        let compiler = ValkyrieCompiler::default();
        let root = crate::valkyrie::frontend::parse_source(
            r#"
class Parcel<T> { value: T }
imply Parcel<T> { micro take(self): T { return self.value } }
trait Cursor { type Item; micro next(self): Parcel<Item> }
micro collect<I, T>(iter: I): T where I: Cursor<Item = T> { return iter.next().take() }
"#,
        )
        .expect("解析链式调用");
        let hir = compiler.lower_root(&root).expect("绑定链式调用");
        compiler.validate_hir_semantic_contract(&hir).expect("链式调用必须绑定完整声明合同");
        let function = hir.functions.iter().find(|function| function.name.as_str() == "collect").unwrap();
        let HirExprKind::Return(Some(result)) = &function.body.expr.as_ref().unwrap().kind
        else {
            panic!("返回调用")
        };
        let HirExprKind::Call { callee, resolved: Some(call), .. } = &result.kind
        else {
            panic!("一次成员调用")
        };
        assert!(matches!(callee.kind, HirExprKind::FieldAccess { .. }));
        assert!(call.declaration.is_some());
        assert_eq!(call.return_type, function.return_type);
    }

    #[test]
    fn nested_call_type_requires_the_selected_contract() {
        let compiler = ValkyrieCompiler::default();
        let mut hir = compiler
            .compile_source(
                "micro produce() -> i64 { 7 } micro consume(value: i64) -> i64 { value } micro caller() -> i64 { consume(produce()) }",
            )
            .expect("嵌套调用必须从源码完成声明选择");
        compiler.validate_hir_semantic_contract(&hir).expect("完整调用合同必须通过");
        let caller = hir.functions.iter_mut().find(|function| function.name.as_str() == "caller").unwrap();
        let HirExprKind::Call { args, .. } = &mut caller.body.expr.as_mut().unwrap().kind
        else {
            panic!("预期外层调用");
        };
        let nested = &mut args[0].value;
        let expected = Some(ValkyrieType::Integer64 { signed: true });
        assert_eq!(infer_expr_type(nested, &BTreeMap::new()), expected);
        assert_eq!(infer_scrutinee_type(nested, &BTreeMap::new(), &AggregateFieldIndex::default(), &BTreeSet::new()), expected);
        let HirExprKind::Call { resolved, .. } = &mut nested.kind
        else {
            panic!("预期嵌套调用");
        };
        assert!(resolved.take().is_some());
        assert_eq!(infer_expr_type(nested, &BTreeMap::new()), None);
        assert_eq!(infer_scrutinee_type(nested, &BTreeMap::new(), &AggregateFieldIndex::default(), &BTreeSet::new()), None);
        assert!(compiler.validate_hir_semantic_contract(&hir).is_err());
    }

    #[test]
    fn panic_and_format_names_require_source_callable_declarations() {
        let compiler = ValkyrieCompiler::default();
        for source in ["micro caller(value: i64) -> i64 { panic(value) }", "micro formatter(value: i64) -> i64 { format(value) }"] {
            let error = compiler.compile_source(source).expect_err("未声明名称不得获得合成 callable 合同");
            assert!(error.to_string().contains("SMIR003"), "应在 Semantic HIR 合同边界失败：{error}");
        }

        let hir = compiler
            .compile_source(
                r#"
micro panic(value: i64) -> i64 { value }
micro caller(value: i64) -> i64 { panic(value) }
"#,
            )
            .expect("显式声明应通过普通 callable resolver");
        let declaration = hir.functions.iter().find(|function| function.name.as_str() == "panic").unwrap();
        let caller = hir.functions.iter().find(|function| function.name.as_str() == "caller").unwrap();
        let HirExprKind::Call { resolved: Some(call), .. } = &caller.body.expr.as_ref().unwrap().kind
        else {
            panic!("显式声明必须得到正式调用合同");
        };
        assert_eq!(call.instance, declaration.instance);
    }

    #[test]
    fn construction_type_requires_the_selected_contract() {
        let compiler = ValkyrieCompiler::default();
        let mut hir = compiler
            .compile_source("structure Parcel { value: i64 } micro create() -> Parcel { Parcel { value: 7 } }")
            .expect("构造必须从源码完成字段绑定与声明选择");
        compiler.validate_hir_semantic_contract(&hir).expect("完整构造合同必须通过");
        let parcel_declaration = hir.structs[0].constructor_declaration.expect("结构声明身份");
        let create = hir.functions.iter_mut().find(|function| function.name.as_str() == "create").unwrap();
        let construction = create.body.expr.as_mut().unwrap();
        let expected = Some(ValkyrieType::Nominal { declaration: parcel_declaration, arguments: vec![] });
        assert_eq!(infer_expr_type(construction, &BTreeMap::new()), expected);
        assert_eq!(infer_scrutinee_type(construction, &BTreeMap::new(), &AggregateFieldIndex::default(), &BTreeSet::new()), expected);
        let HirExprKind::Construct { resolved, .. } = &mut construction.kind
        else {
            panic!("预期结构构造");
        };
        assert!(resolved.take().is_some());
        assert_eq!(infer_expr_type(construction, &BTreeMap::new()), None);
        assert_eq!(infer_scrutinee_type(construction, &BTreeMap::new(), &AggregateFieldIndex::default(), &BTreeSet::new()), None);
        assert!(compiler.validate_hir_semantic_contract(&hir).is_err());
    }

    #[test]
    fn selected_overload_preserves_all_declaration_facts_in_any_order() {
        let selected = declaration_candidate();
        let mut other = selected.clone();
        other.owner = Some(Identifier::new("OtherOwner"));
        other.trait_owner = Some(nyar_types::ItemId::from_index(910).unwrap());
        other.param_specs.clear();
        other.generic_binders.clear();
        other.where_constraints.clear();
        other.match_kind = OverloadMatchKind::Trait;

        for candidates in [vec![other.clone(), selected.clone()], vec![selected.clone(), other]] {
            assert_eq!(resolve_overload(&candidates).unwrap(), selected);
        }
    }

    #[test]
    fn equal_rank_declarations_cannot_merge_after_erasing_facts() {
        let declaration = declaration_candidate();
        let mut different_owner = declaration.clone();
        different_owner.owner = Some(Identifier::new("OtherOwner"));
        let mut different_trait = declaration.clone();
        different_trait.trait_owner = Some(nyar_types::ItemId::from_index(910).unwrap());
        let mut different_receiver = declaration.clone();
        different_receiver.param_specs[0].name.name = Identifier::new("value");
        let mut different_binders = declaration.clone();
        different_binders.generic_binders.insert(test_binder(901, 0));
        let mut different_constraints = declaration.clone();
        different_constraints.where_constraints[0].bounds[0].trait_path = path("OtherBound");

        for (fact, other) in [
            ("owner", different_owner),
            ("trait_owner", different_trait),
            ("receiver", different_receiver),
            ("generic_binders", different_binders),
            ("where_constraints", different_constraints),
        ] {
            for candidates in [vec![declaration.clone(), other.clone()], vec![other, declaration.clone()]] {
                assert_eq!(
                    resolve_overload(&candidates),
                    Err(OverloadResolutionError::Ambiguous { candidates: vec![path("apply"), path("apply")] }),
                    "{fact}"
                );
            }
        }
    }

    #[test]
    fn repeated_complete_candidate_preserves_the_declaration() {
        let declaration = declaration_candidate();
        assert_eq!(resolve_overload(&[declaration.clone(), declaration.clone()]).unwrap(), declaration);
    }

    #[test]
    fn empty_overload_set_fails_without_a_default_candidate() {
        assert_eq!(resolve_overload(&[]), Err(OverloadResolutionError::NoMatch));
    }

    #[test]
    fn singleton_matching_preserves_the_complete_declared_signature() {
        let declaration = declaration_candidate();
        let matched = match_singleton_method_candidate(
            &declaration,
            &[],
            &TypeRelationContext::from_module(&HirModule::default()),
            &BTreeMap::new(),
            &AggregateFieldIndex::default(),
            &BTreeSet::new(),
            true,
        )
        .expect("singleton 匹配不得擦除 self 或声明合同");
        assert_eq!(matched, declaration);
    }

    #[test]
    fn singleton_matching_rejects_an_argument_without_a_type_fact() {
        let declaration = declaration_candidate();
        let span = crate::types::SourceSpan::new(SourceID::default(), 0, 0);
        let unknown = HirCallArgument::positional(HirExpr {
            kind: HirExprKind::Variable(HirIdentifier { name: Identifier::new("unknown"), shadow_index: 0, span: span.clone() }),
            span,
        });
        assert!(
            match_singleton_method_candidate(
                &declaration,
                &[unknown],
                &TypeRelationContext::from_module(&HirModule::default()),
                &BTreeMap::new(),
                &AggregateFieldIndex::default(),
                &BTreeSet::new(),
                true,
            )
            .is_none()
        );
    }

    fn declaration_candidate() -> OverloadCandidate {
        let mut declaration = candidate("apply", OverloadMatchKind::NominalExact);
        declaration.owner = Some(Identifier::new("Owner"));
        declaration.trait_owner = Some(nyar_types::ItemId::from_index(911).unwrap());
        let mut receiver = HirParam::default();
        receiver.name.name = Identifier::new("self");
        receiver.ty = ValkyrieType::Named(Identifier::new("Owner"));
        declaration.signature.params = vec![receiver.ty.clone()];
        declaration.where_constraints.push(HirWhereConstraint {
            target: ValkyrieType::Named(Identifier::new("Element")),
            bounds: vec![HirTraitBound { declaration: None, trait_path: path("Bound"), type_arguments: vec![], associated_types: vec![] }],
            span: receiver.name.span.clone(),
        });
        declaration.param_specs = vec![receiver];
        declaration.generic_binders.insert(test_binder(900, 0));
        declaration
    }

    fn candidate(name: &str, match_kind: OverloadMatchKind) -> OverloadCandidate {
        OverloadCandidate::new(path(name), OverloadDomain::Function, vec![], ValkyrieType::Unit, match_kind)
    }

    fn path(name: &str) -> NamePath {
        NamePath::new(vec![Identifier::new(name)])
    }

    fn resolve_constructor_contract(candidates: &[OverloadCandidate], actual: &[Option<ValkyrieType>]) -> Option<HirResolvedCall> {
        let mut locals = BTreeMap::new();
        let arguments = actual
            .iter()
            .enumerate()
            .map(|(index, ty)| {
                let name = Identifier::new(&format!("argument_{index}"));
                let span = crate::types::SourceSpan::new(SourceID::default(), 0, 0);
                if let Some(ty) = ty {
                    locals.insert(name.to_string(), ty.clone());
                }
                HirExpr { kind: HirExprKind::Variable(HirIdentifier { name, shadow_index: 0, span: span.clone() }), span }
            })
            .collect::<Vec<_>>();
        try_resolve_constructor(
            &NamePath::new(vec![Identifier::new("Present")]),
            &arguments,
            candidates,
            &TypeRelationContext::from_module(&HirModule::default()),
            &locals,
            &AggregateFieldIndex::default(),
            &BTreeSet::new(),
        )
    }

    fn generic_constructor(owner: &str, parameters: Vec<ValkyrieType>) -> OverloadCandidate {
        OverloadCandidate::new_method(
            Identifier::new(owner),
            NamePath::new(vec![Identifier::new("Present")]),
            OverloadDomain::Constructor,
            parameters,
            ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new(owner))), vec![test_generic("T")]),
            OverloadMatchKind::Row,
        )
        .with_generic_binder(test_binder(900, 0))
    }

    #[test]
    fn constructor_contract_instantiates_parameters_and_return_with_declared_owner() {
        let candidate = generic_constructor("Envelope", vec![test_generic("T")]);
        let resolved = resolve_constructor_contract(&[candidate], &[Some(ValkyrieType::Utf8)]).expect("声明 binder 应完成代入");
        assert_eq!(resolved.symbol, NamePath::new(vec![Identifier::new("Envelope"), Identifier::new("Present")]));
        assert_eq!(resolved.parameter_types, vec![ValkyrieType::Utf8]);
        assert_eq!(
            resolved.return_type,
            ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Envelope"))), vec![ValkyrieType::Utf8])
        );
        assert!(!resolved.has_receiver);
    }

    #[test]
    fn constructor_contract_rejects_conflicting_repeated_binder() {
        let parameter = test_generic("T");
        let candidate = generic_constructor("Envelope", vec![parameter.clone(), parameter]);
        assert!(resolve_constructor_contract(&[candidate], &[Some(ValkyrieType::Utf8), Some(ValkyrieType::Boolean)]).is_none());
    }

    #[test]
    fn constructor_contract_rejects_missing_argument_type_and_ambiguous_owner() {
        let parameter = test_generic("T");
        let candidates = vec![generic_constructor("Alpha", vec![parameter.clone()]), generic_constructor("Beta", vec![parameter])];
        assert!(resolve_constructor_contract(&candidates[..1], &[None]).is_none());
        assert!(resolve_constructor_contract(&candidates, &[None]).is_none());
        assert!(resolve_constructor_contract(&candidates, &[Some(ValkyrieType::Utf8)]).is_none());
    }

    #[test]
    fn constructor_contract_rejects_partial_sum_identity_facts() {
        let mut candidate = generic_constructor("Envelope", vec![test_generic("T")]);
        candidate.sum_owner = nyar_types::ItemId::from_index(1);
        candidate.sum_variant = None;
        assert!(resolve_constructor_contract(&[candidate], &[Some(ValkyrieType::Utf8)]).is_none());
    }

    #[test]
    #[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
    fn generic_aggregate_constructor_provides_field_context_to_nullary_variant() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 5230 });
        let hir = compiler
            .compile_source(
                r#"
unite Option<T> {
    Some { value: T },
    None,
}

class Promise<T> { value: Option<T>, done: bool, cancelled: bool }

trait Future {
    micro poll(self): bool
}

imply Promise<T>: Future {
    micro new(): Self {
        return Promise { value: None, done: false, cancelled: false }
    }
}
"#,
            )
            .expect("generic aggregate constructor should resolve its field context");
        assert!(hir.structs.iter().any(|item| item.name == Identifier::new("Promise")));
    }

    #[test]
    #[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
    fn option_none_turbofish_call_is_resolved_in_hir() {
        let source = r#"
namespace test;
unite Option<T> {
    Some { value: T },
    None,
}
micro option_none<T>() -> Option<T> {
    let empty: Option<T> = None
    return empty
}
micro take(flag: bool) -> Option<i64> {
    if flag {
        return option_none::<i64>()
    }
    let value: i64 = 1
    return Some(value)
}
"#;
        let root = crate::valkyrie::frontend::parse_source(source).expect("parse option_none fixture");
        let take_ast = root
            .items
            .iter()
            .filter_map(|item| match item {
                crate::valkyrie::frontend::ast::StatementNode::Micro(micro) if micro.name.name == "take" => Some(micro),
                _ => None,
            })
            .next()
            .expect("take micro in ast");
        let if_stmt = take_ast.body.statements.iter().find_map(|statement| match statement {
            crate::valkyrie::frontend::ast::Statement::Expression(expr)
                if matches!(expr.expr, crate::valkyrie::frontend::ast::TermExpression::If { .. }) =>
            {
                Some(())
            }
            _ => None,
        });
        let ast_stmt_kinds = take_ast
            .body
            .statements
            .iter()
            .map(|statement| match statement {
                crate::valkyrie::frontend::ast::Statement::Let(_) => "let".to_string(),
                crate::valkyrie::frontend::ast::Statement::Expression(expr) => match &expr.expr {
                    crate::valkyrie::frontend::ast::TermExpression::If { .. } => "if-expr".to_string(),
                    crate::valkyrie::frontend::ast::TermExpression::Return(_) => "return-expr".to_string(),
                    other => format!("expr({:?})", std::mem::discriminant(other)),
                },
                other => format!("other({:?})", std::mem::discriminant(other)),
            })
            .collect::<Vec<_>>();
        assert!(if_stmt.is_some(), "parser must preserve if statement before option_none turbofish call, got {ast_stmt_kinds:?}");
        let hir = crate::ValkyrieCompiler::default().compile_source(source).expect("option_none hir compile");
        let take = hir.functions.iter().find(|function| function.name.as_str() == "take").expect("take");
        let mut call_debug = Vec::new();
        fn walk_calls(expr: &HirExpr, out: &mut Vec<String>) {
            match &expr.kind {
                HirExprKind::Call { callee, resolved, .. } => {
                    let name = extract_callable_name(callee).map(|id| id.to_string()).unwrap_or_else(|| format!("{:?}", callee.kind));
                    let resolved = resolved
                        .as_ref()
                        .map(|call| {
                            format!("{} domain={:?} sum={}/{}", call.symbol, call.domain, call.sum_owner.is_some(), call.sum_variant.is_some())
                        })
                        .unwrap_or_else(|| "unresolved".to_string());
                    out.push(format!("{name} => {resolved}"));
                    if let HirExprKind::GenericApply { callee, .. } = &callee.kind {
                        walk_calls(callee, out);
                    }
                }
                HirExprKind::Return(Some(inner)) | HirExprKind::Yield(Some(inner)) => walk_calls(inner, out),
                HirExprKind::If { condition, then_branch, else_branch, .. } => {
                    walk_calls(condition, out);
                    if let Some(expr) = &then_branch.expr {
                        walk_calls(expr, out);
                    }
                    for statement in &then_branch.statements {
                        if let HirStatementKind::Expr(expr) = &statement.kind {
                            walk_calls(expr, out);
                        }
                    }
                    if let Some(else_branch) = else_branch {
                        if let Some(expr) = &else_branch.expr {
                            walk_calls(expr, out);
                        }
                        for statement in &else_branch.statements {
                            if let HirStatementKind::Expr(expr) = &statement.kind {
                                walk_calls(expr, out);
                            }
                        }
                    }
                }
                HirExprKind::Block(block) => {
                    if let Some(expr) = &block.expr {
                        walk_calls(expr, out);
                    }
                    for statement in &block.statements {
                        if let HirStatementKind::Let { initializer: Some(initializer), .. } = &statement.kind {
                            walk_calls(initializer, out);
                        }
                        if let HirStatementKind::Expr(expr) = &statement.kind {
                            walk_calls(expr, out);
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some(expr) = &take.body.expr {
            walk_calls(expr, &mut call_debug);
        }
        for statement in &take.body.statements {
            if let HirStatementKind::Expr(expr) = &statement.kind {
                walk_calls(expr, &mut call_debug);
            }
            if let HirStatementKind::Let { initializer: Some(initializer), .. } = &statement.kind {
                walk_calls(initializer, &mut call_debug);
            }
        }
        let if_shape = take.body.statements.first().map(|statement| format!("{:?}", statement.kind));
        let option_none_call = call_debug.iter().find(|line| line.starts_with("option_none"));
        assert!(
            option_none_call.is_some(),
            "expected option_none call in take body, got calls={call_debug:?} stmts={} tail={} first_stmt={if_shape:?}",
            take.body.statements.len(),
            take.body.expr.is_some()
        );
        let line = option_none_call.expect("option_none line");
        assert!(!line.contains("unresolved"), "option_none must resolve: {line}");
        assert!(line.contains("domain=Function"), "option_none must stay a normal function call: {line}");
        assert!(!line.contains("sum=true/true"), "option_none must not carry sum constructor facts: {line}");
    }

    #[test]
    #[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
    fn turbofish_generic_call_substitutes_return_type_and_keeps_callable_instance() {
        let hir = crate::ValkyrieCompiler::default()
            .compile_source(
                r#"
namespace test;
unite Option<T> {
    Some { value: T },
    None,
}
micro option_none<T>() -> Option<T> {
    let empty: Option<T> = None
    return empty
}
micro pick(flag: bool) -> Option<i64> {
    if flag {
        return option_none::<i64>()
    }
    let value: i64 = 1
    return Some(value)
}
"#,
            )
            .expect("turbofish generic call compile");
        let pick = hir.functions.iter().find(|function| function.name.as_str() == "pick").expect("pick function");
        let mut resolved_call = None;
        fn walk_for_option_none_call(expr: &HirExpr, resolved_call: &mut Option<HirResolvedCall>) {
            match &expr.kind {
                HirExprKind::Call { callee, resolved, .. }
                    if extract_callable_name(callee).is_some_and(|name| name.as_str() == "option_none") =>
                {
                    *resolved_call = resolved.clone();
                }
                HirExprKind::Return(Some(inner)) | HirExprKind::Yield(Some(inner)) => walk_for_option_none_call(inner, resolved_call),
                HirExprKind::If { condition, then_branch, else_branch, .. } => {
                    walk_for_option_none_call(condition, resolved_call);
                    if let Some(expr) = &then_branch.expr {
                        walk_for_option_none_call(expr, resolved_call);
                    }
                    for statement in &then_branch.statements {
                        if let HirStatementKind::Expr(expr) = &statement.kind {
                            walk_for_option_none_call(expr, resolved_call);
                        }
                    }
                    if let Some(else_branch) = else_branch {
                        if let Some(expr) = &else_branch.expr {
                            walk_for_option_none_call(expr, resolved_call);
                        }
                        for statement in &else_branch.statements {
                            if let HirStatementKind::Expr(expr) = &statement.kind {
                                walk_for_option_none_call(expr, resolved_call);
                            }
                        }
                    }
                }
                HirExprKind::Block(block) => {
                    if let Some(expr) = &block.expr {
                        walk_for_option_none_call(expr, resolved_call);
                    }
                    for statement in &block.statements {
                        if let HirStatementKind::Expr(expr) = &statement.kind {
                            walk_for_option_none_call(expr, resolved_call);
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some(expr) = &pick.body.expr {
            walk_for_option_none_call(expr, &mut resolved_call);
        }
        for statement in &pick.body.statements {
            if let HirStatementKind::Expr(expr) = &statement.kind {
                walk_for_option_none_call(expr, &mut resolved_call);
            }
        }
        let resolved = resolved_call.expect("pick must contain resolved option_none turbofish call");
        assert_eq!(resolved.domain, HirCallableDomain::Function);
        assert!(resolved.instance.is_some(), "option_none turbofish call must keep callable instance");
        assert!(resolved.declaration.is_some(), "option_none turbofish call must keep declaration identity");
        assert!(resolved.sum_owner.is_none(), "option_none must not be rewritten into None constructor");
        assert!(resolved.sum_variant.is_none(), "option_none must not be rewritten into None constructor");
        assert_eq!(
            resolved.return_type,
            ValkyrieType::Nominal {
                declaration: hir.enums[0].declaration.expect("sum 声明身份"),
                arguments: vec![ValkyrieType::Integer64 { signed: true }],
            }
        );
    }

    fn test_sum_nominal(declaration_index: u32, arguments: Vec<ValkyrieType>) -> ValkyrieType {
        ValkyrieType::Nominal { declaration: nyar_types::ItemId::from_index(declaration_index).expect("test ItemId"), arguments }
    }

    #[test]
    fn nullary_none_resolution_uses_return_type_owner_context() {
        let option_none = nullary_variant_constructor("Option", "None", 1);
        let other_none = nullary_variant_constructor("Other", "None", 99);
        let contextual = test_sum_nominal(1, vec![ValkyrieType::Integer32 { signed: true }]);
        let resolved = try_resolve_nullary_constructor(
            &Identifier::new("None"),
            Some(&contextual),
            &[option_none, other_none],
            &TypeRelationContext::from_module(&HirModule::default()),
            &BTreeMap::new(),
            &AggregateFieldIndex::default(),
            &BTreeSet::new(),
        )
        .expect("return Nominal Option<i32> should close Option::None");
        assert_eq!(resolved.sum_owner, nyar_types::ItemId::from_index(1));
        assert!(resolved.sum_variant.is_some());
        assert_eq!(resolved.domain, HirCallableDomain::Constructor);
    }

    #[test]
    fn enrich_sum_variant_operation_facts_attaches_option_some_for_unwrap() {
        let option_some = nullary_variant_constructor("Option", "Some", 1);
        let option_ty = test_sum_nominal(1, vec![ValkyrieType::Integer32 { signed: true }]);
        let mut call = HirResolvedCall {
            declaration: None,
            instance: None,
            sum_owner: None,
            sum_variant: None,
            symbol: NamePath::new(vec![Identifier::new("unwrap")]),
            domain: HirCallableDomain::Function,
            return_type: ValkyrieType::Integer32 { signed: true },
            parameter_types: vec![option_ty.clone()],
            has_receiver: true,
            extractor_payload_type: None,
        };
        enrich_sum_variant_operation_facts(&mut call, Some(&option_ty), &[option_some]);
        assert!(call.sum_owner.is_some());
        assert!(call.sum_variant.is_some());
        assert_eq!(call.domain, HirCallableDomain::Extractor);
        assert_eq!(call.extractor_payload_type, Some(ValkyrieType::Integer32 { signed: true }));
    }

    fn nullary_variant_constructor(owner: &str, variant: &str, sum_owner_index: u32) -> OverloadCandidate {
        let sum_owner = nyar_types::ItemId::from_index(sum_owner_index).expect("test sum_owner");
        let mut candidate = OverloadCandidate::new_method(
            Identifier::new(owner),
            NamePath::new(vec![Identifier::new(variant)]),
            OverloadDomain::Constructor,
            Vec::new(),
            test_sum_nominal(sum_owner_index, vec![test_generic("T")]),
            OverloadMatchKind::Row,
        )
        .with_generic_binder(test_binder(900, 0));
        candidate.sum_owner = Some(sum_owner);
        candidate.sum_variant = nyar_types::ItemId::from_index(sum_owner_index + 1);
        candidate
    }

    #[test]
    fn constructor_contract_does_not_substitute_nominal_parameter_as_generic() {
        let candidate = generic_constructor("Envelope", vec![ValkyrieType::Named(Identifier::new("DeclaredPayload"))]);
        assert!(resolve_constructor_contract(&[candidate], &[Some(ValkyrieType::Utf8)]).is_none());
    }

    #[test]
    #[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
    fn constructor_contract_from_source_preserves_generic_sum_signature() {
        let hir = ValkyrieCompiler::new(SourceID { version_id: 5210 })
            .compile_source(
                r#"
unite Envelope<T> {
    Present { value: T },
    Absent,
}
micro wrap(value: utf8) -> Envelope<utf8> {
    let wrapped = Present { value: value }
    return wrapped
}
"#,
            )
            .expect("当前源码中的构造声明应完成实例化");
        let function = hir.functions.iter().find(|function| function.name.as_str() == "wrap").expect("wrap");
        let HirStatementKind::Let { initializer: Some(initializer), .. } = &function.body.statements[0].kind
        else {
            panic!("缺少构造表达式");
        };
        let HirExprKind::Construct { resolved: Some(resolved), .. } = &initializer.kind
        else {
            panic!("构造身份未解析：{initializer:?}");
        };
        assert_eq!(resolved.symbol, NamePath::new(vec![Identifier::new("Envelope"), Identifier::new("Present")]));
        assert_eq!(resolved.parameter_types, vec![ValkyrieType::Utf8]);
        let envelope = hir.enums.iter().find(|item| item.name.as_str() == "Envelope").expect("Envelope");
        let declaration = envelope.declaration.expect("Envelope declaration");
        assert_eq!(resolved.return_type, ValkyrieType::Nominal { declaration, arguments: vec![ValkyrieType::Utf8] });
    }

    #[test]
    fn ambiguous_bare_new_does_not_resolve_via_simple_name_fallback() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 5201 });
        let hir = compiler.compile_source(
            r#"
structure Alpha { value: i64 }

structure Beta { value: i64 }

imply Alpha {
    micro new(value: i64): Self {
        return Alpha { value: value }
    }
}

imply Beta {
    micro new(value: i64): Self {
        return Beta { value: value }
    }
}

micro main() {
    let x = new(1)
}
"#,
        );

        match hir {
            Ok(hir) => {
                let main = hir.functions.iter().find(|function| function.name.as_str() == "main").expect("main");
                let mut saw_bare_new = false;
                for statement in &main.body.statements {
                    visit_calls_looking_for_bare_new(&statement.kind, &mut saw_bare_new);
                }
                assert!(saw_bare_new, "expected bare `new` call in main");
            }
            Err(error) => {
                let text = error.to_string();
                assert!(
                    text.contains("new") || text.contains("SMIR003") || text.contains("unresolved") || text.contains("ambiguous"),
                    "ambiguous bare `new` must fail closed without inventing a binding, got {text}"
                );
            }
        }
    }

    fn visit_calls_looking_for_bare_new(kind: &HirStatementKind, saw_bare_new: &mut bool) {
        let expression = match kind {
            HirStatementKind::Let { initializer: Some(value), .. } | HirStatementKind::Expr(value) => value.as_ref(),
            _ => return,
        };
        if let HirExprKind::Call { callee, resolved, .. } = &expression.kind {
            if extract_callable_name(callee).is_some_and(|name| name.as_str() == "new") {
                *saw_bare_new = true;
                assert!(resolved.is_none(), "ambiguous bare `new` must stay unresolved, got {resolved:?}");
            }
        }
    }

    #[test]
    #[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
    fn qualified_type_static_call_does_not_fall_back_to_unrelated_new() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 5202 });
        let result = compiler.compile_source(
            r#"
structure HashMap<K, V> {
    capacity: i64
}

structure TuiRuntime {
    capacity: i64
}

imply HashMap<K, V> {
    micro new(capacity: i64): Self {
        return HashMap { capacity: capacity }
    }
}

imply TuiRuntime {
    micro new(capacity: i64): Self {
        return TuiRuntime { capacity: capacity }
    }
}

micro main() {
    let map: HashMap<i64, i64> = HashMap::new(0)
}
"#,
        );
        let hir = result.expect("qualified static call must resolve from its declared signature");
        let main = hir.functions.iter().find(|function| function.name.as_str() == "main").expect("main");
        let resolved = main
            .body
            .statements
            .iter()
            .find_map(|statement| match &statement.kind {
                HirStatementKind::Let { initializer: Some(value), .. } => match &value.kind {
                    HirExprKind::Call { resolved: Some(resolved), .. } => Some(resolved),
                    _ => None,
                },
                _ => None,
            })
            .expect("HashMap::new must carry a resolved call contract");
        assert_eq!(resolved.symbol.to_string(), "HashMap.new");
        assert_eq!(resolved.parameter_types, vec![ValkyrieType::Integer64 { signed: true }]);
    }

    #[test]
    fn receiver_presence_is_derived_from_the_declared_self_parameter() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 5203 });
        let hir = compiler
            .compile_source(
                r#"
structure Counter { value: i64 }

imply Counter {
    micro read(self): i64 { self.value }
    micro make(value: i64): i64 { value }
}

micro main(counter: Counter) {
    let current = counter.read()
    let created = Counter.make(1)
}
"#,
            )
            .expect("declared instance and static methods must resolve");
        let main = hir.functions.iter().find(|function| function.name.as_str() == "main").expect("main");
        let resolved = main
            .body
            .statements
            .iter()
            .filter_map(|statement| match &statement.kind {
                HirStatementKind::Let { initializer: Some(value), .. } => match &value.kind {
                    HirExprKind::Call { resolved: Some(resolved), .. } => Some(resolved),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].symbol.to_string(), "Counter.read");
        assert_eq!(
            resolved[0].parameter_types,
            vec![ValkyrieType::Nominal {
                declaration: hir.structs[0].constructor_declaration.expect("receiver 声明身份"), arguments: vec![]
            }]
        );
        assert!(resolved[0].has_receiver);
        assert_eq!(resolved[1].symbol.to_string(), "Counter.make");
        assert_eq!(resolved[1].parameter_types, vec![ValkyrieType::Integer64 { signed: true }]);
        assert!(!resolved[1].has_receiver);
    }

    #[test]
    fn instance_syntax_cannot_supply_a_missing_self_parameter() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 5204 });
        let error = compiler
            .compile_source(
                r#"
structure Counter { }

imply Counter {
    micro make(value: i64): i64 { value }
}

micro main(counter: Counter) {
    counter.make(1)
}
"#,
            )
            .expect_err("a static method must not gain a receiver from dot syntax");
        let message = error.to_string();
        assert!(message.contains("SMIR003") || message.contains("unresolved"), "unexpected failure: {message}");
    }
}

#[cfg(test)]
mod singleton_tests {
    use super::*;
    use crate::{ValkyrieCompiler, types::SourceID};

    #[test]
    #[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
    fn imply_bool_infix_not_eq_resolves_eq_operator_call() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 4211 });
        compiler
            .compile_source(
                r#"
[primitive("core::primitive::bool")]
structure bool { }

imply bool {
    infix `==`(self, rhs: Self): bool {
        if self { return rhs }
        return !rhs
    }
    infix `!=`(self, rhs: Self): bool {
        !(self == rhs)
    }
}
"#,
            )
            .expect("bool operator imply must resolve");
    }

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
    #[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
    fn resolves_singleton_instance_method_from_singleton_owner() {
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
    #[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
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
    #[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
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
        assert_eq!(resolved.symbol.to_string(), "main::choose");
        assert_eq!(resolved.return_type, ValkyrieType::Integer64 { signed: true });
    }

    #[test]
    fn resolves_same_module_helper_with_stable_qualified_symbol() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 4207 });
        let hir = compiler
            .compile_source(
                r#"
micro main(): i64 {
    let value: i64 = answer()
    return value
}

micro answer(): i64 {
    return 42
}
"#,
            )
            .unwrap();

        let main = hir.functions.iter().find(|function| function.name.as_str() == "main").expect("main");
        let HirStatementKind::Let { initializer: Some(initializer), .. } = &main.body.statements[0].kind
        else {
            panic!("expected let initializer");
        };
        let HirExprKind::Call { resolved: Some(resolved), .. } = &initializer.kind
        else {
            panic!("expected resolved helper call");
        };
        assert_eq!(resolved.domain, HirCallableDomain::Function);
        assert_eq!(resolved.symbol.to_string(), "main::answer");
    }

    #[test]
    fn qualified_namespace_call_uses_canonical_identity() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 4210 });
        let hir = compiler
            .compile_source(
                r#"
namespace library.console;
micro emit(flag: bool): unit {}
namespace library.client;
micro invoke(): unit { library.console.emit(true) }
"#,
            )
            .expect("qualified source call");
        fn find(module: &HirModule) -> Option<&HirFunction> {
            module.functions.iter().find(|function| function.name.as_str() == "invoke").or_else(|| module.submodules.iter().find_map(find))
        }
        let function = find(&hir).expect("invoke");
        let expression = function.body.expr.as_ref().expect("call expression");
        let HirExprKind::Call { resolved: Some(resolved), .. } = &expression.kind
        else {
            panic!("qualified call must resolve");
        };
        assert_eq!(resolved.symbol.to_string(), "library::console::emit");
    }

    #[test]
    fn submodule_imply_body_resolves_qualified_call() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 4211 });
        let hir = compiler
            .compile_source(
                r#"
namespace library.console;
micro emit(flag: bool): unit {}
namespace library.client;
class Client {}
imply Client {
    micro invoke(): unit { library.console.emit(true) }
}
"#,
            )
            .expect("qualified imply source call");
        fn find(module: &HirModule) -> Option<&HirFunction> {
            module
                .impls
                .iter()
                .flat_map(|item| &item.methods)
                .find(|function| function.name.as_str() == "invoke")
                .or_else(|| module.submodules.iter().find_map(find))
        }
        let function = find(&hir).expect("invoke");
        let expression = function.body.expr.as_ref().expect("call expression");
        let HirExprKind::Call { resolved: Some(resolved), .. } = &expression.kind
        else {
            panic!("qualified imply call must resolve");
        };
        assert_eq!(resolved.symbol.to_string(), "library::console::emit");
    }

    #[test]
    #[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
    fn method_generic_parameters_survive_hir_lowering() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 4212 });
        let hir = compiler
            .compile_source(
                r#"
class Collector {
    micro identity<T>(value: T) -> T where T: Iterator<Item = i32> { return value; }
}
"#,
            )
            .expect("generic method source");
        let method = &hir.structs[0].methods[0];
        assert_eq!(method.generics.len(), 1);
        assert_eq!(method.generics[0].name.as_str(), "T");
        assert_eq!(method.params.len(), 1);
        assert_eq!(method.params[0].ty, method.return_type);
        assert_eq!(method.where_constraints.len(), 1);
        let constraint = &method.where_constraints[0];
        assert_eq!(constraint.bounds.len(), 1);
        let bound = &constraint.bounds[0];
        assert_eq!(bound.associated_types.len(), 1);
        assert_eq!(bound.trait_path.to_string(), "Iterator");
        assert_eq!(bound.associated_types[0].name.as_str(), "Item");
        assert_eq!(bound.associated_types[0].ty, ValkyrieType::Integer32 { signed: true });
    }

    #[test]
    #[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
    fn where_bounds_preserve_positional_arguments_and_equation_ownership() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 4214 });
        let hir = compiler
            .compile_source(
                r#"
micro identity<T>(value: T) -> T
    where T: Mapper<i32, bool, Item = i32> + Mapper<bool, i32, Item = bool>
{ return value; }
"#,
            )
            .expect("structured generic bounds");
        let bounds = &hir.functions[0].where_constraints[0].bounds;
        assert_eq!(bounds.len(), 2);
        assert_eq!(bounds[0].trait_path, bounds[1].trait_path);
        let integer = ValkyrieType::Integer32 { signed: true };
        assert_eq!(bounds[0].type_arguments, vec![integer.clone(), ValkyrieType::Boolean]);
        assert_eq!(bounds[1].type_arguments, vec![ValkyrieType::Boolean, integer.clone()]);
        assert_eq!(bounds[0].associated_types[0].ty, integer);
        assert_eq!(bounds[1].associated_types[0].ty, ValkyrieType::Boolean);
    }

    #[test]
    fn unrelated_generic_declaration_does_not_make_nominal_type_a_wildcard() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 4213 });
        let hir = compiler
            .compile_source(
                r#"
class T {}
micro identity<T>(value: T) -> T { return value; }
"#,
            )
            .expect("nominal and scoped generic declarations");
        let relations = TypeRelationContext::from_module(&hir);
        assert!(matches!(relations.match_parameter(&ValkyrieType::Boolean, &test_generic("T")), ParameterMatchResult::NoMatch { .. }));
    }

    #[test]
    #[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
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
    fn utf8_slice_without_adaptor_is_rejected_before_mir() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 4203 });
        let error = compiler
            .compile_source(
                r#"
namespace test;

micro slice_probe(text: utf8, start: i32, count: i32) -> utf8 {
    return text.slice(start, count)
}
"#,
            )
            .expect_err("missing adaptor contract must fail before MIR");
        assert!(error.to_string().contains("SMIR003"), "{error}");
        assert!(error.to_string().contains("slice_probe"), "{error}");
    }

    #[test]
    fn utf8_loop_without_adaptor_is_rejected_before_mir() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 4204 });
        let error = compiler
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
            .expect_err("loop control flow must not bypass missing adaptor contracts");
        assert!(error.to_string().contains("SMIR003"), "{error}");
        assert!(error.to_string().contains("slice_loop_probe"), "{error}");
    }

    #[test]
    #[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
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

    #[test]
    fn array_len_intrinsic_contract_specializes_generic_array() {
        let span = crate::types::SourceSpan::new(crate::types::SourceID::default(), 0, 0);
        let candidate = OverloadCandidate::new(
            NamePath::new(vec![Identifier::new("builtin"), Identifier::new("array"), Identifier::new("length")]),
            OverloadDomain::Function,
            vec![ValkyrieType::Array(Box::new(test_generic("T")))],
            ValkyrieType::Named(Identifier::new("usize")),
            OverloadMatchKind::NominalExact,
        )
        .with_generic_binder(test_binder(900, 0));
        let items_ty = ValkyrieType::Array(Box::new(ValkyrieType::Integer32 { signed: true }));
        let locals = BTreeMap::from([("items".to_string(), items_ty.clone())]);
        let args = vec![HirCallArgument::positional(HirExpr {
            kind: HirExprKind::Variable(HirIdentifier { name: Identifier::new("items"), shadow_index: 0, span: span.clone() }),
            span: span.clone(),
        })];
        let matched = match_call_candidate(
            &candidate,
            &args,
            &[],
            &TypeRelationContext::from_module(&HirModule::default()),
            &locals,
            &AggregateFieldIndex::default(),
            &BTreeSet::new(),
        )
        .expect("ArrayLen intrinsic must match array receiver");
        assert_eq!(matched.signature.params, vec![items_ty]);
        assert_eq!(matched.signature.return_type, ValkyrieType::Named(Identifier::new("usize")));
    }

    #[test]
    fn array_len_intrinsic_contract_rejects_non_array_receiver() {
        let span = crate::types::SourceSpan::new(crate::types::SourceID::default(), 0, 0);
        let candidate = OverloadCandidate::new(
            NamePath::new(vec![Identifier::new("builtin"), Identifier::new("array"), Identifier::new("length")]),
            OverloadDomain::Function,
            vec![ValkyrieType::Array(Box::new(test_generic("T")))],
            ValkyrieType::Named(Identifier::new("usize")),
            OverloadMatchKind::NominalExact,
        )
        .with_generic_binder(test_binder(900, 0));
        let locals = BTreeMap::from([("text".to_string(), ValkyrieType::Utf8)]);
        let args = vec![HirCallArgument::positional(HirExpr {
            kind: HirExprKind::Variable(HirIdentifier { name: Identifier::new("text"), shadow_index: 0, span: span.clone() }),
            span: span.clone(),
        })];
        assert!(
            match_call_candidate(
                &candidate,
                &args,
                &[],
                &TypeRelationContext::from_module(&HirModule::default()),
                &locals,
                &AggregateFieldIndex::default(),
                &BTreeSet::new(),
            )
            .is_none(),
            "ArrayLen must not match non-array receivers"
        );
    }

    #[test]
    fn nullable_intrinsic_contracts_specialize_nullable_operands() {
        let span = crate::types::SourceSpan::new(crate::types::SourceID::default(), 0, 0);
        let payload = ValkyrieType::Integer32 { signed: true };
        let nullable = ValkyrieType::Nullable(Box::new(payload.clone()));
        let locals = BTreeMap::from([("value".to_string(), nullable.clone())]);
        let args = vec![HirCallArgument::positional(HirExpr {
            kind: HirExprKind::Variable(HirIdentifier { name: Identifier::new("value"), shadow_index: 0, span: span.clone() }),
            span: span.clone(),
        })];
        let relations = TypeRelationContext::from_module(&HirModule::default());

        let is_null = OverloadCandidate::new(
            NamePath::new(vec![Identifier::new("builtin"), Identifier::new("null"), Identifier::new("is_null")]),
            OverloadDomain::Function,
            vec![nullable.clone()],
            ValkyrieType::Boolean,
            OverloadMatchKind::NominalExact,
        );
        let matched_is_null =
            match_call_candidate(&is_null, &args, &[], &relations, &locals, &AggregateFieldIndex::default(), &BTreeSet::new())
                .expect("IsNull intrinsic must match nullable receiver");
        assert_eq!(matched_is_null.signature.return_type, ValkyrieType::Boolean);

        let unwrap_null = OverloadCandidate::new(
            NamePath::new(vec![Identifier::new("builtin"), Identifier::new("null"), Identifier::new("unwrap")]),
            OverloadDomain::Function,
            vec![nullable.clone()],
            payload.clone(),
            OverloadMatchKind::NominalExact,
        );
        let matched_unwrap =
            match_call_candidate(&unwrap_null, &args, &[], &relations, &locals, &AggregateFieldIndex::default(), &BTreeSet::new())
                .expect("UnwrapNull intrinsic must match nullable receiver");
        assert_eq!(matched_unwrap.signature.return_type, payload);

        let ref_deref = OverloadCandidate::new(
            NamePath::new(vec![Identifier::new("builtin"), Identifier::new("ref"), Identifier::new("deref")]),
            OverloadDomain::Function,
            vec![nullable.clone()],
            payload.clone(),
            OverloadMatchKind::NominalExact,
        );
        let matched_deref =
            match_call_candidate(&ref_deref, &args, &[], &relations, &locals, &AggregateFieldIndex::default(), &BTreeSet::new())
                .expect("RefDeref intrinsic must match nullable receiver");
        assert_eq!(matched_deref.signature.return_type, payload);
    }

    #[test]
    fn nullable_intrinsic_contracts_reject_non_nullable_operands() {
        let span = crate::types::SourceSpan::new(crate::types::SourceID::default(), 0, 0);
        let locals = BTreeMap::from([("value".to_string(), ValkyrieType::Integer32 { signed: true })]);
        let args = vec![HirCallArgument::positional(HirExpr {
            kind: HirExprKind::Variable(HirIdentifier { name: Identifier::new("value"), shadow_index: 0, span: span.clone() }),
            span: span.clone(),
        })];
        let relations = TypeRelationContext::from_module(&HirModule::default());
        let is_null = OverloadCandidate::new(
            NamePath::new(vec![Identifier::new("builtin"), Identifier::new("null"), Identifier::new("is_null")]),
            OverloadDomain::Function,
            vec![ValkyrieType::Nullable(Box::new(ValkyrieType::Integer32 { signed: true }))],
            ValkyrieType::Boolean,
            OverloadMatchKind::NominalExact,
        );
        assert!(
            match_call_candidate(&is_null, &args, &[], &relations, &locals, &AggregateFieldIndex::default(), &BTreeSet::new()).is_none(),
            "IsNull must not match non-nullable receivers"
        );
        let unwrap_null = OverloadCandidate::new(
            NamePath::new(vec![Identifier::new("builtin"), Identifier::new("null"), Identifier::new("unwrap")]),
            OverloadDomain::Function,
            vec![ValkyrieType::Nullable(Box::new(ValkyrieType::Integer32 { signed: true }))],
            ValkyrieType::Integer32 { signed: true },
            OverloadMatchKind::NominalExact,
        );
        assert!(
            match_call_candidate(&unwrap_null, &args, &[], &relations, &locals, &AggregateFieldIndex::default(), &BTreeSet::new()).is_none(),
            "UnwrapNull must not match non-nullable receivers"
        );
    }

    #[test]
    fn array_push_resolves_to_intrinsic_builtin_symbol() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 4210 });
        let hir = compiler
            .compile_source(
                r#"
namespace test;

micro grow(text: utf8) -> [utf8] {
    let mut result: [utf8] = []
    push(result, text)
    return result
}
"#,
            )
            .expect("array push must compile");
        let function = hir.functions.iter().find(|function| function.name.as_str() == "grow").expect("grow");
        let HirStatementKind::Expr(statement) = &function.body.statements[1].kind
        else {
            panic!("expected push statement")
        };
        let HirExprKind::Call { resolved: Some(resolved), .. } = &statement.kind
        else {
            panic!("push must resolve via IntrinsicId builtin candidate: {statement:?}")
        };
        let parts = resolved.symbol.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>();
        assert_eq!(IntrinsicId::resolve_from_segments(&parts), Some(IntrinsicId::ArrayPush));
    }

    #[test]
    fn array_push_resolves_inside_imply_with_declared_element_type() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 4211 });
        let hir = compiler
            .compile_source(
                r#"
namespace test;

class Utf8Text {}

imply Utf8Text {
    micro split(self, separator: utf8) -> [utf8] {
        let mut result: [utf8] = []
        push(result, separator)
        push(result, "")
        return result
    }
}
"#,
            )
            .expect("imply array push must compile");
        let method = hir.impls.iter().flat_map(|item| item.methods.iter()).find(|method| method.name.as_str() == "split").expect("split");
        let mut push_calls = 0usize;
        for statement in &method.body.statements {
            fn walk(expr: &HirExpr, push_calls: &mut usize) {
                match &expr.kind {
                    HirExprKind::Call { resolved, args, .. } => {
                        if let Some(resolved) = resolved {
                            let parts = resolved.symbol.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>();
                            if IntrinsicId::resolve_from_segments(&parts) == Some(IntrinsicId::ArrayPush) {
                                *push_calls += 1;
                            }
                        }
                        for arg in args {
                            walk(&arg.value, push_calls);
                        }
                    }
                    HirExprKind::If { then_branch, else_branch, condition, .. } => {
                        walk(condition, push_calls);
                        for statement in &then_branch.statements {
                            if let HirStatementKind::Expr(inner) = &statement.kind {
                                walk(inner, push_calls);
                            }
                        }
                        if let Some(else_branch) = else_branch {
                            for statement in &else_branch.statements {
                                if let HirStatementKind::Expr(inner) = &statement.kind {
                                    walk(inner, push_calls);
                                }
                            }
                        }
                    }
                    HirExprKind::Return(Some(inner)) => walk(inner, push_calls),
                    HirExprKind::Block(block) => {
                        if let Some(inner) = &block.expr {
                            walk(inner, push_calls);
                        }
                    }
                    _ => {}
                }
            }
            if let HirStatementKind::Expr(expr) = &statement.kind {
                walk(expr, &mut push_calls);
            }
        }
        assert_eq!(push_calls, 2, "both calls must bind the declared ArrayPush identity");
    }
}
