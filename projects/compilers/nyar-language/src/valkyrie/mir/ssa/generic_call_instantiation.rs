//! 声明期成员分派表与类型替换工具。
//!
//! 本模块只承载 Semantic MIR lowering 阶段的**声明期**已冻结事实：
//! `MirDispatchEntry` 记录 HIR 里 impl/struct 方法的接收者类型与 callable 实例，
//! 配合 `unify_generic_instantiation` / `apply_type_substitution` 等类型工具，
//! 让 lowering 内部（builder/pending_requirements 处理）能在 frontend 语义闭包内
//! 完成调用点到具体 callable 的解析。
//!
//! **后置泛型物化已删除**：泛型 substitution 和 evidence 必须在 frontend 内形成
//! 固定点，不能留到 MIR 层。历史上由 `materialize_generic_call_instances` /
//! `instantiate_seed` / `specialize_function` 等函数承担的 MIR 后置单态化职责，
//! 已按架构合同要求归回 frontend，不再在 Semantic MIR 层执行。

use std::collections::BTreeMap;

use nyar_types::{ItemId, ItemInstanceId};

use crate::types::hir::{GenericBinderId, HirModule, ValkyrieType};

use super::hir_identity::nominal_type_name;

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
