//! Minimal overload ranking helpers.
//!
//! These helpers model only the settled precedence:
//! nominal exact > nominal subtype > trait > row.

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
            GenericBinderId, GenericType, HirAttribute, HirBlock, HirCallArgument, HirCallableDomain, HirDependencySemanticExport, HirEnum,
            HirExpr, HirExprKind, HirExtractorPattern, HirField, HirFunction, HirIdentifier, HirMatchArm, HirModule, HirParam, HirPattern,
            HirResolvedCall, HirSingleton, HirStatement, HirStatementKind, HirStruct, HirTrait, HirTraitBound, HirVariadicKind, HirVariant,
            HirWhereConstraint, ValkyrieType,
        },
    },
    valkyrie::{
        hir::PatternRefutability,
        mir::{AggregateFieldIndex, collect_aggregate_field_map},
    },
};
use nyar_types::IntrinsicId;

/// 候选所属的重载域。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverloadDomain {
    /// 普通函数调用。
    Function,
    /// 构造调用。
    Constructor,
    /// 模式提取器调用。
    Extractor,
}

/// 候选的调用签名：形参类型列表与返回类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverloadSignature {
    /// 形参类型列表。
    pub params: Vec<ValkyrieType>,
    /// 返回类型。
    pub return_type: ValkyrieType,
}

/// 一个重载候选：稳定的声明身份、调用签名与匹配类别。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverloadCandidate {
    /// 候选的声明身份；未冻结时为 `None`。
    pub declaration: Option<nyar_types::ItemId>,
    /// 候选的实例身份；未冻结时为 `None`。
    pub instance: Option<nyar_types::ItemInstanceId>,
    /// 所属 unity 的声明身份。
    pub sum_owner: Option<nyar_types::ItemId>,
    /// 所属 unity 变体的声明身份。
    pub sum_variant: Option<nyar_types::ItemId>,
    /// 语言 builtin 的结构化身份；只有语言内建候选持有，源码声明候选恒为 `None`。
    pub intrinsic: Option<IntrinsicId>,
    /// 候选符号的限定路径。
    pub symbol: NamePath,
    /// 拥有该候选的类型名（方法候选）。
    pub owner: Option<Identifier>,
    /// 该候选所属 trait 的声明身份。
    pub trait_owner: Option<nyar_types::ItemId>,
    /// 候选所属的重载域。
    pub domain: OverloadDomain,
    /// 候选的调用签名。
    pub signature: OverloadSignature,
    /// 该候选的匹配类别。
    pub match_kind: OverloadMatchKind,
    /// 形参描述（含绑定方式与默认值）。
    pub param_specs: Vec<HirParam>,
    /// Formal type variables which must be bound from actual arguments.
    pub generic_binders: BTreeSet<GenericBinderId>,
    /// 声明级 where 合同；实参代入后必须取得对应 witness。
    pub where_constraints: Vec<HirWhereConstraint>,
}

impl OverloadCandidate {
    /// 以符号、域、形参列表、返回类型与匹配类别构造候选。
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
            intrinsic: None,
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

    /// 附加形参描述。
    pub fn with_param_specs(mut self, param_specs: Vec<HirParam>) -> Self {
        self.param_specs = param_specs;
        self
    }

    /// 登记一个必须由实参绑定的形式类型变量。
    pub fn with_generic_binder(mut self, binder: GenericBinderId) -> Self {
        self.generic_binders.insert(binder);
        self
    }

    fn with_where_constraints(mut self, constraints: Vec<HirWhereConstraint>) -> Self {
        self.where_constraints = constraints;
        self
    }

    /// 构造带拥有者类型的方法候选。
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
            intrinsic: None,
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

/// 候选参数匹配的类别，决定重载优先级。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverloadMatchKind {
    /// 名义类型完全一致。
    NominalExact,
    /// 名义子类型，距离越小越优先。
    NominalSubtype {
        /// 继承距离。
        distance: usize,
    },
    /// 通过具名 trait 满足。
    Trait,
    /// 通过匿名 row 形状满足。
    Row,
}

/// 重载解析失败的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverloadResolutionError {
    /// 没有任何候选匹配。
    NoMatch,
    /// 存在多个同优先级候选。
    Ambiguous {
        /// 并列候选的符号路径。
        candidates: Vec<NamePath>,
    },
}

/// 按匹配类别的既定优先级选出唯一候选；并列时报歧义。
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

/// 对模块内全部 HIR 调用执行重载解析与绑定。
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

/// 收集模块内全部候选（语言内建候选加已声明候选）；外部测试入口。
#[doc(hidden)]
pub fn collect_module_candidates(module: &HirModule) -> Vec<OverloadCandidate> {
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
    let mut length = OverloadCandidate::new(
        NamePath::new(vec![Identifier::new("builtin"), Identifier::new("array"), Identifier::new("length")]),
        OverloadDomain::Function,
        vec![array_type.clone()],
        index_type.clone(),
        OverloadMatchKind::NominalExact,
    );
    length.intrinsic = Some(IntrinsicId::ArrayLen);
    let mut push = OverloadCandidate::new(
        NamePath::new(vec![Identifier::new("builtin"), Identifier::new("array"), Identifier::new("push")]),
        OverloadDomain::Function,
        vec![array_type.clone(), element_type.clone()],
        array_type.clone(),
        OverloadMatchKind::NominalExact,
    );
    push.intrinsic = Some(IntrinsicId::ArrayPush);
    // 序数下标 `receiver[i]` 展开为成员名 `suffix []` / `suffix []=`；
    // 基数下标 `receiver::[i]` 展开为成员名 `suffix ⁅⁆` / `suffix ⁅⁆=`。
    // 成员名与 `operator_member_name` 的源码声明名一致，因此下标与用户声明的
    // `suffix \`[]\`` 走同一套普通成员解析；内建身份只由 [`IntrinsicId`] 承载。
    let mut get = OverloadCandidate::new(
        NamePath::new(vec![Identifier::new("suffix []")]),
        OverloadDomain::Function,
        vec![array_type.clone(), index_type.clone()],
        element_type.clone(),
        OverloadMatchKind::NominalExact,
    );
    get.intrinsic = Some(IntrinsicId::ArrayGet);
    let mut set = OverloadCandidate::new(
        NamePath::new(vec![Identifier::new("suffix []=")]),
        OverloadDomain::Function,
        vec![array_type.clone(), index_type.clone(), element_type.clone()],
        ValkyrieType::Unit,
        OverloadMatchKind::NominalExact,
    );
    set.intrinsic = Some(IntrinsicId::ArraySet);
    let mut cardinal_get = OverloadCandidate::new(
        NamePath::new(vec![Identifier::new("suffix ⁅⁆")]),
        OverloadDomain::Function,
        vec![array_type.clone(), index_type.clone()],
        element_type.clone(),
        OverloadMatchKind::NominalExact,
    );
    cardinal_get.intrinsic = Some(IntrinsicId::ArrayGet);
    let mut cardinal_set = OverloadCandidate::new(
        NamePath::new(vec![Identifier::new("suffix ⁅⁆=")]),
        OverloadDomain::Function,
        vec![array_type, index_type, element_type],
        ValkyrieType::Unit,
        OverloadMatchKind::NominalExact,
    );
    cardinal_set.intrinsic = Some(IntrinsicId::ArraySet);
    vec![length, push, get, set, cardinal_get, cardinal_set]
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
    // 结构化 intrinsic 优先；源码 `[intrinsic(...)]` 种子仍由符号路径通道识别。
    let parts = candidate.symbol.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>();
    match candidate.intrinsic.or_else(|| IntrinsicId::resolve_from_segments(&parts))? {
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
                intrinsic: None,
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
            let return_type =
                type_relations.nominal_type_for_name(&Identifier::new("usize")).unwrap_or_else(|| candidate.signature.return_type.clone());
            Some(OverloadCandidate {
                declaration: candidate.declaration,
                instance: candidate.instance,
                sum_owner: None,
                sum_variant: None,
                intrinsic: None,
                symbol: candidate.symbol.clone(),
                owner: None,
                trait_owner: candidate.trait_owner.clone(),
                domain: OverloadDomain::Function,
                signature: OverloadSignature { params: vec![array_ty], return_type },
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
                intrinsic: None,
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
                intrinsic: None,
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
                intrinsic: None,
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
            let element = match &array_ty {
                ValkyrieType::Array(element) => element.as_ref().clone(),
                ValkyrieType::FixedArray { element, .. } => element.as_ref().clone(),
                _ => return None,
            };
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
                intrinsic: candidate.intrinsic,
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
            let element = match &array_ty {
                ValkyrieType::Array(element) => element.as_ref().clone(),
                ValkyrieType::FixedArray { element, .. } => element.as_ref().clone(),
                _ => return None,
            };
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
                intrinsic: candidate.intrinsic,
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
    let mut candidate = OverloadCandidate::new(
        symbol,
        OverloadDomain::Function,
        function.params.iter().map(|param| param.ty.clone()).collect(),
        function.return_type.clone(),
        OverloadMatchKind::Row,
    )
    .with_param_specs(function.params.clone());
    candidate.declaration = function.declaration;
    candidate.instance = function.instance;
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
    let candidate = if let Some(owner) = owner {
        OverloadCandidate::new_method(
            owner,
            NamePath::new(vec![function.name.clone()]),
            OverloadDomain::Function,
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
            OverloadDomain::Function,
            function.params.iter().map(|param| param.ty.clone()).collect(),
            function.return_type.clone(),
            OverloadMatchKind::Row,
        )
    };
    let mut candidate = candidate.with_param_specs(function.params.clone());
    candidate.declaration = function.declaration;
    candidate.instance = function.instance;
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

/// 值位置成员访问（`receiver.member`，不带括号）的调用合同。
///
/// 语言没有「成员读取」这一独立语义：成员是接收者名义类型已声明的字段才是字段访问，
/// 否则按成员选择名解析为「接收者作首个实参」的普通调用，与 `receiver.member()`、
/// `a + b`、`a⁅i⁆` 共用同一条 `Invoke(callee, arguments)` 通道。
///
/// 命中即把节点改写为 `Call { callee: Path([member]), args: [receiver], resolved }`；
/// 未命中保持字段访问，由后续阶段结构化拒绝。禁止按成员名猜接收者类型或伪造 ABI。
fn try_resolve_value_member_access(
    expr: &mut HirExpr,
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &mut BTreeMap<String, ValkyrieType>,
    struct_fields: &AggregateFieldIndex,
    singleton_names: &BTreeSet<Identifier>,
    contextual_type: Option<&ValkyrieType>,
) -> bool {
    let HirExprKind::FieldAccess { object, field } = &mut expr.kind
    else {
        return false;
    };
    resolve_expr_calls(object, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
    let receiver = object.as_ref().clone();
    let member = field.clone();
    let span = expr.span.clone();
    // 声明字段优先于同名成员调用；接收者类型未知时判不出字段，保持字段访问。
    let receiver_type = infer_expr_type(&receiver, locals).or_else(|| infer_scrutinee_type(&receiver, locals, struct_fields, singleton_names));
    if receiver_type.as_ref().is_some_and(|ty| lookup_struct_field_type(ty, &member, struct_fields).is_some()) {
        return false;
    }
    let callee = HirExpr { kind: HirExprKind::Path(NamePath::new(vec![member])), span };
    let args = vec![HirCallArgument::positional(receiver)];
    let Some(resolved) = try_resolve_call(&callee, &args, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type)
    else {
        return false;
    };
    expr.kind = HirExprKind::Call { callee: Box::new(callee), args, resolved: Some(resolved) };
    true
}

/// 表达式在父节点中的位置。
///
/// `Callee` 只用于 `Call` / `GenericApply` 的被调用者本身：那里的 `receiver.member`
/// 已经是成员调用的选择名，不得再按值位置成员访问改写一次。其余位置一律 `Value`。
#[derive(Clone, Copy, PartialEq, Eq)]
enum ExprPosition {
    Callee,
    Value,
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
    resolve_expr_calls_at(expr, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type, ExprPosition::Value);
}

fn resolve_expr_calls_at(
    expr: &mut HirExpr,
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &mut BTreeMap<String, ValkyrieType>,
    struct_fields: &AggregateFieldIndex,
    singleton_names: &BTreeSet<Identifier>,
    contextual_type: Option<&ValkyrieType>,
    position: ExprPosition,
) {
    try_resolve_nullary_variant_expr(expr, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
    // 值位置成员访问先于按形状分发：命中后节点已经是普通调用，不再走字段访问分支。
    if position == ExprPosition::Value
        && try_resolve_value_member_access(expr, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type)
    {
        return;
    }
    match &mut expr.kind {
        HirExprKind::Call { callee, args, resolved } => {
            resolve_expr_calls_at(
                callee,
                candidates,
                type_relations,
                locals,
                struct_fields,
                singleton_names,
                contextual_type,
                ExprPosition::Callee,
            );
            for arg in args.iter_mut() {
                resolve_expr_calls(&mut arg.value, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
            }
            *resolved = try_resolve_call(callee, args, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
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
        HirExprKind::FieldAccess { object, .. } => {
            resolve_expr_calls(object, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
        }
        // `GenericApply` 的内层是调用选择名（`peel_explicit_generic_arguments` 会剥开它），
        // 因此与 `Call.callee` 同为被调用者位置。
        HirExprKind::GenericApply { callee, .. } => resolve_expr_calls_at(
            callee,
            candidates,
            type_relations,
            locals,
            struct_fields,
            singleton_names,
            contextual_type,
            ExprPosition::Callee,
        ),
        HirExprKind::FieldInit { value: callee, .. }
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
                *resolved =
                    try_resolve_constructor(path, args, candidates, type_relations, locals, struct_fields, singleton_names, contextual_type);
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

/// 以实参类型判定单个候选是否匹配；外部测试入口。
#[doc(hidden)]
pub fn match_call_candidate(
    candidate: &OverloadCandidate,
    args: &[HirCallArgument],
    explicit_generic_arguments: &[ValkyrieType],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &AggregateFieldIndex,
    singleton_names: &BTreeSet<Identifier>,
) -> Option<OverloadCandidate> {
    // trait 声明的方法（`owner: None`）没有具体 owner 身份，接收者槽仍是 `Self`/未标注类型。
    // 这类候选只能经 bound 实例化（`instantiate_trait_bound_candidate` 把接收者槽代入 subject）后参与匹配；
    // 未实例化就按名命中会让任意接收者与同名 trait 方法「认亲」，与真实候选并列成歧义。此处 fail-closed。
    if candidate.owner.is_none()
        && candidate.trait_owner.is_some()
        && matches!(candidate.signature.params.first(), Some(ValkyrieType::AutoType | ValkyrieType::r#SelfType))
    {
        return None;
    }
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
        intrinsic: candidate.intrinsic,
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
    callee: &HirExpr,
    args: &[HirCallArgument],
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &AggregateFieldIndex,
    singleton_names: &BTreeSet<Identifier>,
    contextual_type: Option<&ValkyrieType>,
) -> Option<HirResolvedCall> {
    let (explicit_generic_arguments, callee) = peel_explicit_generic_arguments(callee);
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
                intrinsic: None,
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
                    intrinsic: None,
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
            let segments = path.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>();
            // `builtin.array.length` 等语言 intrinsic 路径不得走限定自由函数解析；
            // 否则会提前 `return None` 且永远匹配不到 `language_builtin_candidates`。
            if IntrinsicId::resolve_from_segments(&segments).is_some() {
                // fall through to callee_name / intrinsic matching below
            }
            else {
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
        .filter(|candidate| matches!(candidate.domain, OverloadDomain::Function | OverloadDomain::Constructor))
        .filter(|candidate| symbol_matches_callee_name(&candidate.symbol, &callee_name))
        .filter_map(|candidate| {
            if let Some(context) = contextual_type {
                if let Some(instantiated) = instantiate_constructor_candidate_for_context(candidate, context) {
                    if let Some(matched) = match_call_candidate(
                        &instantiated,
                        args,
                        &explicit_generic_arguments,
                        type_relations,
                        locals,
                        struct_fields,
                        singleton_names,
                    ) {
                        return Some(matched);
                    }
                }
            }
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
        intrinsic: resolved.intrinsic,
        symbol,
        domain: match resolved.domain {
            OverloadDomain::Function => HirCallableDomain::Function,
            OverloadDomain::Constructor => HirCallableDomain::Constructor,
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
        OverloadDomain::Extractor => HirCallableDomain::Extractor,
    };
    Some(HirResolvedCall {
        declaration: resolved.declaration,
        instance: resolved.instance,
        sum_owner: resolved.sum_owner,
        sum_variant: resolved.sum_variant,
        intrinsic: resolved.intrinsic,
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

    // 成员解析必须由接收者类型事实驱动。接收者类型缺失时不得退化为「按方法名命中同名候选」：
    // 名称认亲会把不相关的同名方法当成已解析调用。缺类型事实即视为未解析，
    // 由调用方给出结构化诊断，而不是在此处过近似。
    let Some(receiver_type) =
        infer_expr_type(receiver, locals).or_else(|| infer_scrutinee_type(receiver, locals, struct_fields, singleton_names))
    else {
        return None;
    };
    let mut filtered = Vec::new();
    for candidate in candidates
        .iter()
        .filter(|candidate| matches!(candidate.domain, OverloadDomain::Function))
        .filter(|candidate| candidate_has_receiver_parameter(candidate))
        .filter(|candidate| candidate.symbol.parts().last().is_some_and(|name| name == method_name))
    {
        if let ValkyrieType::Generic(generic) = &receiver_type {
            if generic.binder.is_none() {
                continue;
            }
            for bound in type_relations
                .callable_constraints()
                .iter()
                .filter(|constraint| Some(&constraint.target) == Some(&receiver_type))
                .flat_map(|constraint| &constraint.bounds)
                .filter(|bound| bound.declaration.is_some() && bound.declaration == candidate.trait_owner)
            {
                let candidate = instantiate_trait_bound_candidate(candidate, &receiver_type, bound)?;
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
        intrinsic: resolved.intrinsic,
        symbol: overload_symbol_path(&resolved),
        domain: HirCallableDomain::Function,
        return_type: resolved.signature.return_type,
        parameter_types: resolved.signature.params,
        has_receiver,
        extractor_payload_type: None,
    };
    enrich_sum_variant_operation_facts(&mut call, Some(&receiver_type), candidates);
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

/// 按关联类型等式替换 `Associated` 形状；外部测试入口。
#[doc(hidden)]
pub fn substitute_associated_equations(
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
        intrinsic: resolved.intrinsic,
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

/// 把类型中的 `SelfType` 替换为给定主体；外部测试入口。
#[doc(hidden)]
pub fn substitute_type_self(ty: &ValkyrieType, replacement: &ValkyrieType) -> ValkyrieType {
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
        .filter(|candidate| matches!(candidate.domain, OverloadDomain::Function))
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
        intrinsic: resolved.intrinsic,
        symbol,
        domain: HirCallableDomain::Function,
        return_type,
        parameter_types,
        has_receiver,
        extractor_payload_type: None,
    })
}

/// 以 singleton 方法规则匹配候选；外部测试入口。
#[doc(hidden)]
pub fn match_singleton_method_candidate(
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

/// 依据返回类型上下文解析无参变体构造；外部测试入口。
#[doc(hidden)]
pub fn try_resolve_nullary_constructor(
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
    try_resolve_constructor(
        &NamePath::new(vec![name.clone()]),
        &[],
        &narrowed,
        type_relations,
        locals,
        struct_fields,
        singleton_names,
        contextual_type,
    )
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
        intrinsic: None,
        symbol: NamePath::new(vec![Identifier::new(variant_name)]),
        domain: HirCallableDomain::Extractor,
        return_type: payload_type.clone().unwrap_or(ValkyrieType::Unit),
        parameter_types: vec![operand_type.clone()],
        has_receiver: false,
        extractor_payload_type: payload_type,
    })
}

/// 为 sum 变体操作补写 `sum_owner`/`sum_variant` 事实；外部测试入口。
#[doc(hidden)]
pub fn enrich_sum_variant_operation_facts(call: &mut HirResolvedCall, receiver_type: Option<&ValkyrieType>, candidates: &[OverloadCandidate]) {
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

fn instantiate_constructor_candidate_for_context(candidate: &OverloadCandidate, contextual_type: &ValkyrieType) -> Option<OverloadCandidate> {
    let mut substitutions = BTreeMap::new();
    unify_call_type_binders(&candidate.signature.return_type, contextual_type, &candidate.generic_binders, &mut substitutions)?;
    let mut candidate = candidate.clone();
    candidate.signature.return_type = substitute_type_vars(&candidate.signature.return_type, &substitutions);
    candidate.signature.params = candidate.signature.params.iter().map(|param| substitute_type_vars(param, &substitutions)).collect();
    Some(candidate)
}

/// 解析构造调用并返回带声明身份的调用合同；外部测试入口。
#[doc(hidden)]
pub fn try_resolve_constructor(
    path: &NamePath,
    args: &[HirExpr],
    candidates: &[OverloadCandidate],
    type_relations: &TypeRelationContext,
    locals: &BTreeMap<String, ValkyrieType>,
    struct_fields: &AggregateFieldIndex,
    singleton_names: &BTreeSet<Identifier>,
    contextual_type: Option<&ValkyrieType>,
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
        .filter_map(|candidate| {
            if let Some(context) = contextual_type {
                if let Some(instantiated) = instantiate_constructor_candidate_for_context(candidate, context) {
                    if let Some(matched) =
                        match_call_candidate(&instantiated, &arguments, &[], type_relations, locals, struct_fields, singleton_names)
                    {
                        return Some(matched);
                    }
                }
            }
            match_call_candidate(candidate, &arguments, &[], type_relations, locals, struct_fields, singleton_names)
        })
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
        intrinsic: resolved.intrinsic,
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
        intrinsic: resolved.intrinsic,
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

/// 从被调表达式提取可调用名称；外部测试入口。
#[doc(hidden)]
pub fn extract_callable_name(callee: &HirExpr) -> Option<Identifier> {
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

/// 依据已解析调用合同推断表达式类型；外部测试入口。
#[doc(hidden)]
pub fn infer_expr_type(expr: &HirExpr, locals: &BTreeMap<String, ValkyrieType>) -> Option<ValkyrieType> {
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

/// 推断 `match` 被检表达式的类型；外部测试入口。
#[doc(hidden)]
pub fn infer_scrutinee_type(
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
