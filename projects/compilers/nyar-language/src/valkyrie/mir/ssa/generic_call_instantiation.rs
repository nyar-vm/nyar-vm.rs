//! 将泛型种子调用点物化为具体签名的 `ItemInstanceId`。
//!
//! Semantic MIR 允许调用点 SSA 携带已代入返回类型，而种子 `MirFunction` 仍保留 binder。
//! Canonical `CallResultTypeMismatch` 要求 Invoke callee 的 `ItemInstanceRecord.return_type`
//! 与结果 SSA 的 `TypeId` 精确相等，因此必须在进入 Canonical 前为每次具体代入分配实例并改写 callee。

use std::collections::BTreeMap;

use nyar_types::{ItemId, ItemInstanceId};

use crate::types::hir::{GenericBinderId, HirModule, ValkyrieType};

use super::{MirDiagnostic, MirFunction, MirOperand, MirOperation, hir_identity::nominal_type_name};
use crate::types::hir::HirCallRequirement;

/// 声明期已冻结的成员分派条目。
///
/// `target` 是声明侧接收者类型（可含 owner 的泛型 binder），`instance` 是对应
/// 方法已物化的 callable 身份。分派只做结构化目标统一，不从短名或符号字符串恢复身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MirDispatchEntry {
    /// trait 实现的 trait 声明身份；inherent 成员/运算符为空。
    pub(crate) trait_declaration: Option<ItemId>,
    /// 声明侧接收者类型。
    pub(crate) target: ValkyrieType,
    /// 成员选择名（运算符为 `{fixity} {lexeme}`）。
    pub(crate) member: crate::Identifier,
    /// 方法实例身份。
    pub(crate) instance: ItemInstanceId,
}

/// 从完整源码闭包的 HIR 收集成员分派表。
///
/// 只收已分配 callable instance 的方法：泛型种子的实例由单态化按调用点生成，
/// 未实例化时不在声明期猜测 witness。
pub(crate) fn collect_dispatch_entries(modules: &[&HirModule]) -> Vec<MirDispatchEntry> {
    let mut entries = Vec::new();
    for module in modules {
        collect_module_dispatch(module, &mut entries);
    }
    entries
}

fn collect_module_dispatch(module: &HirModule, entries: &mut Vec<MirDispatchEntry>) {
    for implementation in &module.impls {
        for method in &implementation.methods {
            let Some(instance) = method.instance
            else {
                continue;
            };
            entries.push(MirDispatchEntry {
                trait_declaration: implementation.trait_declaration,
                target: implementation.target.clone(),
                member: method.name.clone(),
                instance,
            });
        }
    }
    for structure in &module.structs {
        let Some(declaration) = structure.constructor_declaration
        else {
            continue;
        };
        let target = ValkyrieType::Nominal {
            declaration,
            arguments: structure.generics.iter().map(|generic| ValkyrieType::Generic(generic.clone())).collect(),
        };
        for method in &structure.methods {
            let Some(instance) = method.instance
            else {
                continue;
            };
            entries.push(MirDispatchEntry { trait_declaration: None, target: target.clone(), member: method.name.clone(), instance });
        }
    }
    for submodule in &module.submodules {
        collect_module_dispatch(submodule, entries);
    }
}

/// 类型中是否仍有未代入的泛型 binder。
pub(crate) fn contains_unresolved_generic(ty: &ValkyrieType) -> bool {
    match ty {
        ValkyrieType::Generic(_) => true,
        ValkyrieType::Nominal { arguments, .. } => arguments.iter().any(contains_unresolved_generic),
        ValkyrieType::Apply(base, arguments) => contains_unresolved_generic(base) || arguments.iter().any(contains_unresolved_generic),
        ValkyrieType::Array(element) | ValkyrieType::Nullable(element) => contains_unresolved_generic(element),
        ValkyrieType::FixedArray { element, .. } => contains_unresolved_generic(element),
        ValkyrieType::Tuple(items) | ValkyrieType::Union(items) | ValkyrieType::Intersection(items) => {
            items.iter().any(contains_unresolved_generic)
        }
        ValkyrieType::Function(function) => {
            function.params.iter().any(contains_unresolved_generic) || contains_unresolved_generic(&function.return_type)
        }
        _ => false,
    }
}

/// 若 `concrete` 实例化 `template`，返回 binder → 实参代入表。
pub(crate) fn collect_generic_substitutions(
    template: &ValkyrieType,
    concrete: &ValkyrieType,
) -> Option<BTreeMap<GenericBinderId, ValkyrieType>> {
    let mut substitutions = BTreeMap::new();
    if unify_generic_instantiation(template, concrete, &mut substitutions) { Some(substitutions) } else { None }
}

/// 判断 `concrete` 是否为 `template` 在某一组 binder 代入下的实例。
pub(crate) fn type_instantiates_generic_signature(template: &ValkyrieType, concrete: &ValkyrieType) -> bool {
    collect_generic_substitutions(template, concrete).is_some()
}

/// 按声明参数身份代入类型；未解析名称不能被当作泛型形参。
pub(crate) fn apply_type_substitution(ty: &ValkyrieType, substitutions: &BTreeMap<GenericBinderId, ValkyrieType>) -> ValkyrieType {
    match ty {
        ValkyrieType::Generic(generic) => generic.binder.and_then(|binder| substitutions.get(&binder)).cloned().unwrap_or_else(|| ty.clone()),
        ValkyrieType::Nominal { declaration, arguments } => ValkyrieType::Nominal {
            declaration: *declaration,
            arguments: arguments.iter().map(|argument| apply_type_substitution(argument, substitutions)).collect(),
        },
        ValkyrieType::Apply(base, arguments) => ValkyrieType::Apply(
            Box::new(apply_type_substitution(base, substitutions)),
            arguments.iter().map(|argument| apply_type_substitution(argument, substitutions)).collect(),
        ),
        ValkyrieType::Array(element) => ValkyrieType::Array(Box::new(apply_type_substitution(element, substitutions))),
        ValkyrieType::FixedArray { element, length } => {
            ValkyrieType::FixedArray { element: Box::new(apply_type_substitution(element, substitutions)), length: *length }
        }
        ValkyrieType::Nullable(element) => ValkyrieType::Nullable(Box::new(apply_type_substitution(element, substitutions))),
        ValkyrieType::Tuple(items) => ValkyrieType::Tuple(items.iter().map(|item| apply_type_substitution(item, substitutions)).collect()),
        ValkyrieType::Union(items) => ValkyrieType::Union(items.iter().map(|item| apply_type_substitution(item, substitutions)).collect()),
        ValkyrieType::Intersection(items) => {
            ValkyrieType::Intersection(items.iter().map(|item| apply_type_substitution(item, substitutions)).collect())
        }
        ValkyrieType::Function(function) => {
            let mut function = function.as_ref().clone();
            function.params = function.params.iter().map(|param| apply_type_substitution(param, substitutions)).collect();
            function.return_type = apply_type_substitution(&function.return_type, substitutions);
            ValkyrieType::Function(Box::new(function))
        }
        _ => ty.clone(),
    }
}

pub(crate) fn unify_generic_instantiation(
    template: &ValkyrieType,
    concrete: &ValkyrieType,
    substitutions: &mut BTreeMap<GenericBinderId, ValkyrieType>,
) -> bool {
    match (template, concrete) {
        (left, right) if left == right => true,
        (
            ValkyrieType::Nominal { declaration: template_decl, arguments: template_args },
            ValkyrieType::Nominal { declaration: concrete_decl, arguments: concrete_args },
        ) => {
            template_decl == concrete_decl
                && template_args.len() == concrete_args.len()
                && template_args
                    .iter()
                    .zip(concrete_args)
                    .all(|(template_arg, concrete_arg)| unify_generic_instantiation(template_arg, concrete_arg, substitutions))
        }
        (ValkyrieType::Apply(template_base, template_args), ValkyrieType::Apply(concrete_base, concrete_args)) => {
            template_args.len() == concrete_args.len()
                && unify_generic_instantiation(template_base, concrete_base, substitutions)
                && template_args
                    .iter()
                    .zip(concrete_args)
                    .all(|(template_arg, concrete_arg)| unify_generic_instantiation(template_arg, concrete_arg, substitutions))
        }
        (ValkyrieType::Generic(generic), concrete) => {
            let Some(binder) = generic.binder
            else {
                return false;
            };
            match substitutions.get(&binder) {
                Some(bound) => bound == concrete,
                None => {
                    substitutions.insert(binder, concrete.clone());
                    true
                }
            }
        }
        (ValkyrieType::Array(template_element), ValkyrieType::Array(concrete_element)) => {
            unify_generic_instantiation(template_element, concrete_element, substitutions)
        }
        (
            ValkyrieType::FixedArray { element: template_element, length: template_length },
            ValkyrieType::FixedArray { element: concrete_element, length: concrete_length },
        ) => template_length == concrete_length && unify_generic_instantiation(template_element, concrete_element, substitutions),
        (ValkyrieType::Function(template_fn), ValkyrieType::Function(concrete_fn)) => {
            template_fn.params.len() == concrete_fn.params.len()
                && template_fn
                    .params
                    .iter()
                    .zip(&concrete_fn.params)
                    .all(|(template_param, concrete_param)| unify_generic_instantiation(template_param, concrete_param, substitutions))
                && unify_generic_instantiation(&template_fn.return_type, &concrete_fn.return_type, substitutions)
        }
        _ => false,
    }
}

/// 按接收者类型与成员名查找已冻结的 callable 实例（inherent 成员/运算符）。
pub(crate) fn lookup_member_dispatch(
    dispatch: &[MirDispatchEntry],
    subject: &ValkyrieType,
    member: &crate::Identifier,
    trait_declaration: Option<ItemId>,
) -> Option<ItemInstanceId> {
    let mut matches = Vec::new();
    for entry in dispatch {
        if entry.member != *member || entry.trait_declaration != trait_declaration {
            continue;
        }
        if dispatch_subject_matches(&entry.target, subject) {
            matches.push(entry.instance);
        }
    }
    match matches.len() {
        0 => None,
        1 => Some(matches[0]),
        _ => None,
    }
}

fn dispatch_subject_matches(entry_target: &ValkyrieType, subject: &ValkyrieType) -> bool {
    let mut substitutions = BTreeMap::new();
    if unify_generic_instantiation(entry_target, subject, &mut substitutions) {
        return true;
    }
    match (nominal_type_name(entry_target), nominal_type_name(subject)) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

/// 单态化缓存：同一 `(种子实例, 代入)` 复用同一个 monomorph 实例。
type MonomorphCache = BTreeMap<(ItemInstanceId, BTreeMap<GenericBinderId, ValkyrieType>), ItemInstanceId>;

/// 扫描全部 Call 站点并物化具体实例，改写 callee。
///
/// `next_instance` 必须是整份源码闭包的全局游标（已覆盖全部包的 HIR 实例），
/// 不得仅取当前模块 `max(instance)+1`——否则会与后注册包的声明实例撞号（link 定义重复）。
///
/// `seed_pool` 提供已 lowering 的依赖模块泛型种子：consumer 的调用点必须能在
/// 完整源码闭包内实例化依赖函数，不能因为种子不在本模块而跳过。
/// `dispatch` 是声明期冻结的成员分派表，用于解析 `Call.deferred_member`。
pub(super) fn materialize_generic_call_instances(
    functions: &mut Vec<MirFunction>,
    next_instance: &mut u32,
    seed_pool: &[MirFunction],
    dispatch: &[MirDispatchEntry],
    diagnostics: &mut Vec<MirDiagnostic>,
) {
    let mut cache = MonomorphCache::new();
    let mut cursor = 0usize;
    while cursor < functions.len() {
        let sites = collect_instantiation_sites(&functions[cursor], functions, seed_pool);
        for (block_index, instruction_index, seed, concrete_return, substitutions) in sites {
            let Some(seed_function) = functions
                .iter()
                .find(|function| function.instance == Some(seed))
                .or_else(|| seed_pool.iter().find(|function| function.instance == Some(seed)))
                .cloned()
            else {
                diagnostics
                    .push(MirDiagnostic::UnresolvedCallableIdentity { symbol: format!("generic seed `{seed}` has no lowered definition") });
                continue;
            };
            let Some(mono) = instantiate_seed(&seed_function, &concrete_return, &substitutions, functions, next_instance, &mut cache)
            else {
                diagnostics.push(MirDiagnostic::UnresolvedCallableIdentity {
                    symbol: format!("generic seed `{}` cannot be fully substituted", seed_function.display_symbol),
                });
                continue;
            };
            if let MirOperation::Call { callee, .. } = &mut functions[cursor].blocks[block_index].instructions[instruction_index].kind {
                *callee = MirOperand::Callable(mono);
            }
        }
        let requirement_sites = collect_requirement_sites(&functions[cursor]);
        for (block_index, instruction_index, requirement) in requirement_sites {
            // 种子自身仍带泛型 binder：具体 witness 由调用点的 monomorph 决定。
            // 未实例化的种子不产生诊断，也不会被当成入口链接。
            if contains_unresolved_generic(&requirement.subject) {
                continue;
            }
            let result_type = functions[cursor].blocks[block_index].instructions[instruction_index]
                .results
                .first()
                .and_then(|result| functions[cursor].value_types.get(result).cloned());
            match resolve_requirement_callee(&requirement, result_type.as_ref(), functions, seed_pool, dispatch, next_instance, &mut cache) {
                Ok(mono) => {
                    if let MirOperation::Call { callee, deferred_member, .. } =
                        &mut functions[cursor].blocks[block_index].instructions[instruction_index].kind
                    {
                        *callee = MirOperand::Callable(mono);
                        *deferred_member = None;
                    }
                }
                Err(detail) => diagnostics.push(MirDiagnostic::UnresolvedCallableIdentity { symbol: detail }),
            }
        }
        cursor += 1;
    }
}

/// 按完整代入物化种子；签名仍含未代入 binder 时失败，不生成半成品实例。
fn instantiate_seed(
    seed: &MirFunction,
    concrete_return: &ValkyrieType,
    substitutions: &BTreeMap<GenericBinderId, ValkyrieType>,
    functions: &mut Vec<MirFunction>,
    next_instance: &mut u32,
    cache: &mut MonomorphCache,
) -> Option<ItemInstanceId> {
    let seed_instance = seed.instance?;
    if let Some(existing) = cache.get(&(seed_instance, substitutions.clone())).copied() {
        return Some(existing);
    }
    let specialized = specialize_function(seed, concrete_return, substitutions, *next_instance);
    if specialized.param_types.iter().any(contains_unresolved_generic) || contains_unresolved_generic(&specialized.return_type) {
        return None;
    }
    let instance = specialized.instance?;
    *next_instance = next_instance.saturating_add(1);
    cache.insert((seed_instance, substitutions.clone()), instance);
    functions.push(specialized);
    Some(instance)
}

/// 用成员分派表解析延迟要求：按 trait 身份、成员名与目标类型统一选唯一 witness。
///
/// 0 个或多个匹配都必须结构化失败；不得按同名候选、默认 impl 或目标特判补选。
fn resolve_requirement_callee(
    requirement: &HirCallRequirement,
    result_type: Option<&ValkyrieType>,
    functions: &mut Vec<MirFunction>,
    seed_pool: &[MirFunction],
    dispatch: &[MirDispatchEntry],
    next_instance: &mut u32,
    cache: &mut MonomorphCache,
) -> Result<ItemInstanceId, String> {
    let mut matches = Vec::new();
    for entry in dispatch {
        if entry.member != requirement.member || entry.trait_declaration != requirement.trait_declaration {
            continue;
        }
        let mut substitutions = BTreeMap::new();
        if !unify_generic_instantiation(&entry.target, &requirement.subject, &mut substitutions) {
            continue;
        }
        matches.push((entry, substitutions));
    }
    let (entry, substitutions) = match matches.len() {
        0 => {
            return Err(format!(
                "member `{}` on `{:?}` has no impl witness in the frozen dispatch table",
                requirement.member, requirement.subject
            ));
        }
        1 => matches.pop().expect("one dispatch match"),
        count => {
            return Err(format!("member `{}` on `{:?}` has {count} ambiguous impl witnesses", requirement.member, requirement.subject));
        }
    };
    let Some(seed) = functions
        .iter()
        .find(|function| function.instance == Some(entry.instance))
        .or_else(|| seed_pool.iter().find(|function| function.instance == Some(entry.instance)))
        .cloned()
    else {
        return Err(format!("member `{}` dispatch instance `{}` has no lowered definition", requirement.member, entry.instance));
    };
    if !contains_unresolved_generic(&seed.return_type) && !seed.param_types.iter().any(contains_unresolved_generic) {
        return Ok(entry.instance);
    }
    let concrete_return = result_type.cloned().unwrap_or_else(|| apply_type_substitution(&seed.return_type, &substitutions));
    instantiate_seed(&seed, &concrete_return, &substitutions, functions, next_instance, cache)
        .ok_or_else(|| format!("member `{}` witness cannot be fully substituted", requirement.member))
}

fn collect_requirement_sites(function: &MirFunction) -> Vec<(usize, usize, HirCallRequirement)> {
    let mut sites = Vec::new();
    for (block_index, block) in function.blocks.iter().enumerate() {
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            if let MirOperation::Call { deferred_member: Some(requirement), .. } = &instruction.kind {
                sites.push((block_index, instruction_index, requirement.clone()));
            }
        }
    }
    sites
}

fn collect_instantiation_sites(
    function: &MirFunction,
    functions: &[MirFunction],
    seed_pool: &[MirFunction],
) -> Vec<(usize, usize, ItemInstanceId, ValkyrieType, BTreeMap<GenericBinderId, ValkyrieType>)> {
    let mut sites = Vec::new();
    for (block_index, block) in function.blocks.iter().enumerate() {
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            let MirOperation::Call { callee: MirOperand::Callable(seed), arguments, deferred_member: None } = &instruction.kind
            else {
                continue;
            };
            // 结果被丢弃的调用（含 `unit` 返回）同样必须物化：callee 仍是泛型种子时
            // 链接会把未闭合的种子当作可达实例拉入，绕过调用点的完整代入。
            // 没有结果 SSA 时以形参统一得到的代入改写种子返回类型。
            let observed_return = instruction.results.first().and_then(|result| function.value_types.get(result).cloned());
            let Some(seed_function) = functions
                .iter()
                .find(|candidate| candidate.instance == Some(*seed))
                .or_else(|| seed_pool.iter().find(|candidate| candidate.instance == Some(*seed)))
            else {
                continue;
            };
            let mut substitutions = BTreeMap::new();
            if arguments.len() != seed_function.param_types.len() {
                continue;
            }
            let parameters_match = seed_function.param_types.iter().zip(arguments).all(|(formal, operand)| {
                let MirOperand::Value(value) = operand
                else {
                    return false;
                };
                let Some(actual) = function.value_types.get(value)
                else {
                    return false;
                };
                unify_generic_instantiation(formal, actual, &mut substitutions)
            });
            if !parameters_match {
                continue;
            }
            let concrete_return = match observed_return {
                Some(concrete_return) => {
                    if !unify_generic_instantiation(&seed_function.return_type, &concrete_return, &mut substitutions) {
                        continue;
                    }
                    concrete_return
                }
                None => apply_type_substitution(&seed_function.return_type, &substitutions),
            };
            if substitutions.is_empty() {
                continue;
            }
            sites.push((block_index, instruction_index, *seed, concrete_return, substitutions));
        }
    }
    sites
}

fn specialize_function(
    seed: &MirFunction,
    concrete_return: &ValkyrieType,
    substitutions: &BTreeMap<GenericBinderId, ValkyrieType>,
    instance_index: u32,
) -> MirFunction {
    let instance = ItemInstanceId::from_index(instance_index).expect("ItemInstanceId overflow");
    let mut specialized = seed.clone();
    specialized.instance = Some(instance);
    // monomorph 必须使用独立 ABI 诊断标签，避免覆盖种子 callable_symbols 条目。
    specialized.display_symbol = format!("{}#{}", seed.display_symbol, instance_index);
    specialized.return_type = concrete_return.clone();
    specialized.param_types = seed.param_types.iter().map(|ty| apply_type_substitution(ty, substitutions)).collect();
    specialized.value_types = seed.value_types.iter().map(|(value, ty)| (*value, apply_type_substitution(ty, substitutions))).collect();
    for block in &mut specialized.blocks {
        for instruction in &mut block.instructions {
            specialize_operation(&mut instruction.kind, substitutions);
        }
    }
    specialized
}

fn specialize_operation(operation: &mut MirOperation, substitutions: &BTreeMap<GenericBinderId, ValkyrieType>) {
    match operation {
        MirOperation::LoadConstant { ty: Some(ty), .. }
        | MirOperation::StoreVar { ty: Some(ty), .. }
        | MirOperation::Cast { ty, .. }
        | MirOperation::ArrayNew { array_type: ty, .. }
        | MirOperation::ArrayFromElements { array_type: ty, .. } => {
            *ty = apply_type_substitution(ty, substitutions);
        }
        MirOperation::SumNew { type_args, payload_type, .. } => {
            for argument in type_args.iter_mut() {
                *argument = apply_type_substitution(argument, substitutions);
            }
            if let Some(payload) = payload_type.as_mut() {
                *payload = apply_type_substitution(payload, substitutions);
            }
        }
        MirOperation::SumPayloadGet { type_args, payload_type, .. } => {
            for argument in type_args.iter_mut() {
                *argument = apply_type_substitution(argument, substitutions);
            }
            *payload_type = apply_type_substitution(payload_type, substitutions);
        }
        MirOperation::SumVariantIs { type_args, .. } => {
            for argument in type_args.iter_mut() {
                *argument = apply_type_substitution(argument, substitutions);
            }
        }
        // 种子内的延迟成员要求随种子代入改写接收者；未闭合的要求保留到下一次
        // 实例化，由分派表按完整代入选择 witness。
        MirOperation::Call { deferred_member: Some(requirement), .. } => {
            requirement.subject = apply_type_substitution(&requirement.subject, substitutions);
        }
        _ => {}
    }
}
