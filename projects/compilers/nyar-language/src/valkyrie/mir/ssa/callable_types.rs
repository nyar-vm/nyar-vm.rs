//! callable 返回类型索引与 impl 方法签名推导。

use super::{MirSumDeclaration, hir_identity::impl_owner_type_name, struct_facts::resolve_self_type_with_owner};
use crate::{
    symbols::stable_hir_function_symbol,
    types::hir::{HirFunction, HirModule, ValkyrieType},
};
use nyar_types::ItemInstanceId;
use std::collections::BTreeMap;

#[derive(Clone)]
pub(super) struct CallableReturnTypes {
    pub(super) by_symbol: BTreeMap<String, ValkyrieType>,
    pub(super) by_instance: BTreeMap<ItemInstanceId, ValkyrieType>,
}

pub(super) fn register_callable_return(index: &mut CallableReturnTypes, symbol: String, function: &HirFunction, return_type: ValkyrieType) {
    index.by_symbol.insert(symbol, return_type.clone());
    if let Some(instance) = function.instance {
        index.by_instance.insert(instance, return_type);
    }
}

pub(super) fn collect_callable_return_types(module: &HirModule) -> CallableReturnTypes {
    let mut index = CallableReturnTypes { by_symbol: BTreeMap::new(), by_instance: BTreeMap::new() };
    for function in &module.functions {
        register_callable_return(&mut index, stable_hir_function_symbol(&module.name, function), function, function.return_type.clone());
    }
    for impl_block in &module.impls {
        let Some(type_name) = impl_owner_type_name(module, &impl_block.target)
        else {
            continue;
        };
        for method in &impl_block.methods {
            register_callable_return(
                &mut index,
                format!("{type_name}.{}", method.name),
                method,
                resolve_self_type_with_owner(&method.return_type, Some(&impl_block.target)),
            );
        }
    }
    for item in &module.structs {
        let owner = ValkyrieType::Named(item.name.clone());
        for method in &item.methods {
            register_callable_return(
                &mut index,
                format!("{}.{}", item.name, method.name),
                method,
                resolve_self_type_with_owner(&method.return_type, Some(&owner)),
            );
        }
    }
    for trait_def in &module.traits {
        for method in &trait_def.default_methods {
            register_callable_return(
                &mut index,
                format!("{}.{}", trait_def.name, method.name),
                method,
                resolve_self_type_with_owner(&method.return_type, None),
            );
        }
    }
    for singleton in &module.singletons {
        let owner = ValkyrieType::Named(singleton.name.clone());
        for method in singleton
            .methods
            .iter()
            .chain(singleton.constructor.iter().map(Box::as_ref))
            .chain(singleton.finalizer.iter().map(Box::as_ref))
        {
            register_callable_return(
                &mut index,
                format!("{}.{}", singleton.name, method.name),
                method,
                resolve_self_type_with_owner(&method.return_type, Some(&owner)),
            );
        }
    }
    index
}

pub(super) fn impl_type_arguments_from_generics(generics: &[crate::types::hir::GenericType]) -> Vec<ValkyrieType> {
    generics.iter().map(|generic| ValkyrieType::Generic(generic.clone())).collect()
}

/// `imply Option<T>` 在 HIR 里常把 owner 收成 `Named(Option)`；为 sum 判别与 payload 代入补齐 Apply 实参。
pub(super) fn impl_receiver_type(
    owner: &ValkyrieType,
    impl_type_arguments: &[ValkyrieType],
    sum_types: &[super::MirSumDeclaration],
) -> ValkyrieType {
    if matches!(owner, ValkyrieType::Nominal { arguments, .. } if !arguments.is_empty())
        || matches!(owner, ValkyrieType::Apply(_, args) if !args.is_empty())
    {
        return owner.clone();
    }
    let Some(declaration) = owner.nominal_declaration()
    else {
        // 未绑定 declaration 的短名 owner 不得在 MIR 猜成 sum 身份。
        return owner.clone();
    };
    let type_args: Vec<ValkyrieType> = if !impl_type_arguments.is_empty() {
        impl_type_arguments.to_vec()
    }
    else {
        MirSumDeclaration::find_by_declaration(sum_types, declaration)
            .map(|sum| sum.generics.iter().map(|generic| ValkyrieType::Generic(generic.clone())).collect())
            .unwrap_or_default()
    };
    if type_args.is_empty() {
        return ValkyrieType::Nominal { declaration, arguments: Vec::new() };
    }
    ValkyrieType::Nominal { declaration, arguments: type_args }
}

/// imply 方法返回类型：只消费已冻结的 `Nominal{ItemId}` / Self 代入。
/// 禁止按 `Named("Option"|"Result")` 短名在 MIR 补 declaration。
pub(super) fn impl_method_return_type(
    ty: &ValkyrieType,
    function_generics: &[crate::types::hir::GenericType],
    impl_owner_type: Option<&ValkyrieType>,
    impl_type_arguments: &[ValkyrieType],
    sum_types: &[super::MirSumDeclaration],
) -> ValkyrieType {
    let resolved = resolve_self_type_with_owner(ty, impl_owner_type);
    let Some(owner_declaration) = impl_owner_type.and_then(|owner| owner.nominal_declaration())
    else {
        return resolved;
    };
    let Some(owner_sum) = MirSumDeclaration::find_by_declaration(sum_types, owner_declaration)
    else {
        return resolved;
    };
    // 返回类型已是 owner 自身的无参 Nominal 时，按 imply 实参补齐 binder（仍不查短名）。
    match resolved {
        ValkyrieType::Nominal { declaration, arguments } if declaration == owner_declaration && arguments.is_empty() => {
            let type_args = if owner_sum.generics.len() == 2 {
                result_method_return_type_args(function_generics, impl_type_arguments, owner_sum)
            }
            else {
                sum_method_return_type_args(function_generics, impl_type_arguments, owner_sum)
            };
            if type_args.is_empty() {
                ValkyrieType::Nominal { declaration, arguments: Vec::new() }
            }
            else {
                ValkyrieType::Nominal { declaration, arguments: type_args }
            }
        }
        other => other,
    }
}

pub(super) fn result_method_return_type_args(
    function_generics: &[crate::types::hir::GenericType],
    impl_type_arguments: &[ValkyrieType],
    owner_sum: &MirSumDeclaration,
) -> Vec<ValkyrieType> {
    if let Some(generic) = function_generics.last() {
        let mut args = vec![ValkyrieType::Generic(generic.clone())];
        if let Some(error) = impl_type_arguments.get(1) {
            args.push(error.clone());
        }
        else if let Some(error_generic) = owner_sum.generics.get(1) {
            args.push(ValkyrieType::Generic(error_generic.clone()));
        }
        return args;
    }
    sum_method_return_type_args(function_generics, impl_type_arguments, owner_sum)
}

pub(super) fn sum_method_return_type_args(
    function_generics: &[crate::types::hir::GenericType],
    impl_type_arguments: &[ValkyrieType],
    owner_sum: &MirSumDeclaration,
) -> Vec<ValkyrieType> {
    if let Some(generic) = function_generics.last() {
        vec![ValkyrieType::Generic(generic.clone())]
    }
    else if !impl_type_arguments.is_empty() {
        impl_type_arguments.to_vec()
    }
    else {
        owner_sum.generics.iter().map(|generic| ValkyrieType::Generic(generic.clone())).collect()
    }
}

/// 接收者/`Self` 参数：只认 Auto/Self → owner 的已冻结 Nominal；禁止按 `Named("Option")` 补身份。
pub(super) fn impl_method_param_type(
    ty: &ValkyrieType,
    param_index: usize,
    impl_owner_type: Option<&ValkyrieType>,
    impl_type_arguments: &[ValkyrieType],
    sum_types: &[super::MirSumDeclaration],
) -> ValkyrieType {
    let _ = param_index;
    match (ty, impl_owner_type) {
        (ValkyrieType::AutoType | ValkyrieType::SelfType, Some(owner)) => impl_receiver_type(owner, impl_type_arguments, sum_types),
        // 已绑定为 owner 声明的无参 Nominal 时补齐 imply 实参。
        (ValkyrieType::Nominal { declaration, arguments }, Some(owner))
            if arguments.is_empty() && owner.nominal_declaration() == Some(*declaration) =>
        {
            impl_receiver_type(owner, impl_type_arguments, sum_types)
        }
        (ty, owner) => resolve_self_type_with_owner(ty, owner),
    }
}
