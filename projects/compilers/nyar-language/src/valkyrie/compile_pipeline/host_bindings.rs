//! Resolver 已选定的 `[host_provider]` 绑定进入 Compiler linker 的合同。
//!
//! 装配层提交的是 inventory 声明身份；Compiler 在 HIR 物化后把它映射成冻结的
//! `ItemInstanceId`。Semantic MIR 只携带实例身份，不再保存声明索引 key。

use std::collections::{BTreeMap, BTreeSet};

use nyar_types::ItemInstanceId;

use crate::valkyrie::{
    frontend::ParseError,
    mir::{MirFunction, MirModule, MirOperand, MirOperation},
    semantic_closure::{DeclarationKey, DeclarationKind},
    types::hir::{HirFunction, HirModule, ValkyrieType},
};

use super::context::CompilerHostProviderBinding;

/// Resolver 选定且已映射为 Compiler 实例身份的 host provider 绑定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedHostProviderBinding {
    /// `[host_contract]` 在源码闭包中的 callable 实例。
    pub contract_instance: ItemInstanceId,
    /// 选定 `[host_provider]` 实现的 callable 实例。
    pub provider_instance: ItemInstanceId,
}

struct PlannedBinding {
    contract_instance: ItemInstanceId,
    provider_instance: ItemInstanceId,
    provider_function: Option<MirFunction>,
}

/// 把 Resolver 提交的声明身份映射为 Compiler 冻结的 callable 实例。
///
/// 未被源码闭包引用的合同不产生绑定；provider 身份缺失即结构化失败。
#[doc(hidden)]
pub fn resolve_host_provider_bindings(
    hirs: &[&HirModule],
    bindings: &[CompilerHostProviderBinding],
) -> Result<Vec<ResolvedHostProviderBinding>, ParseError> {
    if bindings.is_empty() {
        return Ok(Vec::new());
    }
    let instances = host_callable_instances(hirs)?;
    let mut resolved = Vec::new();
    for binding in bindings {
        let Some(contract_instance) = instances.get(&binding.contract).copied()
        else {
            continue;
        };
        let provider_instance = instances.get(&binding.provider).copied().ok_or_else(|| {
            ParseError::Invalid { message: format!("host provider `{}` 未在源码闭包中解析到 callable identity", binding.provider.encoded()), span: None }
        })?;
        resolved.push(ResolvedHostProviderBinding { contract_instance, provider_instance });
    }
    Ok(resolved)
}

/// HIR 已注册 callable 身份 → inventory 声明身份的唯一映射。
///
/// 映射只消费 HIR 已注册的 `declaration` / `instance`；同一声明身份对应多个实例
/// 或同一实例对应多个声明身份都按结构化失败处理。声明种类与拥有者必须与语义闭包
/// `collect_item` / `collect_members` / `collect_imply` 的登记一致，否则成员合同
/// 会被当成“未进入闭包”而静默跳过。
fn host_callable_instances(hirs: &[&HirModule]) -> Result<BTreeMap<DeclarationKey, ItemInstanceId>, ParseError> {
    let mut collected = BTreeMap::<DeclarationKey, BTreeSet<ItemInstanceId>>::new();
    for module in hirs {
        for function in &module.functions {
            collect_callable_instance(
                &mut collected,
                declaration_key(module, DeclarationKind::Micro, &namespace_parts(&function.declaring_namespace), None, function.name.as_str()),
                function,
            );
        }
        for structure in &module.structs {
            let namespace: Vec<String> = structure.namespace.iter().map(|part| part.as_str().to_string()).collect();
            for method in &structure.methods {
                collect_callable_instance(
                    &mut collected,
                    declaration_key(module, DeclarationKind::Method, &namespace, Some(structure.name.as_str()), method.name.as_str()),
                    method,
                );
            }
        }
        for trait_decl in &module.traits {
            let namespace = namespace_parts(&trait_decl.declaring_namespace);
            for method in trait_decl.methods.iter().chain(trait_decl.default_methods.iter()) {
                collect_callable_instance(
                    &mut collected,
                    declaration_key(module, DeclarationKind::Method, &namespace, Some(trait_decl.name.as_str()), method.name.as_str()),
                    method,
                );
            }
        }
        for implementation in &module.impls {
            let Some(owner) = target_owner_name(&implementation.target)
            else {
                continue;
            };
            let namespace = namespace_parts(&implementation.declaring_namespace);
            for method in &implementation.methods {
                collect_callable_instance(
                    &mut collected,
                    declaration_key(module, DeclarationKind::ImplyMethod, &namespace, Some(owner), method.name.as_str()),
                    method,
                );
            }
        }
        for export in &module.imported_semantic_exports {
            let module_name = export.module.to_string();
            for function in &export.functions {
                collect_callable_instance(
                    &mut collected,
                    DeclarationKey {
                        dependency_key: module_name.clone(),
                        source_path: module_name.clone(),
                        namespace: namespace_parts(&function.declaring_namespace),
                        kind: DeclarationKind::Micro,
                        owner: None,
                        name: function.name.as_str().to_string(),
                    },
                    function,
                );
            }
        }
    }
    let mut instances = BTreeMap::new();
    for (key, found) in collected {
        if found.len() != 1 {
            return Err(ParseError::Invalid { message: format!("callable 声明 `{}` 对应多个 ItemInstanceId", key.encoded()), span: None });
        }
        instances.insert(key, *found.iter().next().expect("single callable instance"));
    }
    Ok(instances)
}

fn collect_callable_instance(collected: &mut BTreeMap<DeclarationKey, BTreeSet<ItemInstanceId>>, key: DeclarationKey, function: &HirFunction) {
    if let Some(instance) = function.instance {
        collected.entry(key).or_default().insert(instance);
    }
}

fn declaration_key(module: &HirModule, kind: DeclarationKind, namespace: &[String], owner: Option<&str>, name: &str) -> DeclarationKey {
    DeclarationKey {
        dependency_key: module.dependency_key.clone(),
        source_path: module.source_path.clone(),
        namespace: namespace.to_vec(),
        kind,
        owner: owner.map(str::to_string),
        name: name.to_string(),
    }
}

fn namespace_parts(namespace: &crate::NamePath) -> Vec<String> {
    namespace.parts().iter().map(|part| part.as_str().to_string()).collect()
}

/// `imply` 目标类型的聚合短名：与闭包 `imply_owner_name` 保持同一事实来源。
fn target_owner_name(ty: &ValkyrieType) -> Option<&str> {
    match ty {
        ValkyrieType::Named(name) => Some(name.as_str()),
        ValkyrieType::QualifiedPath { name, .. } => Some(name.as_str()),
        ValkyrieType::Apply(base, _) => target_owner_name(base),
        _ => None,
    }
}

/// 将 Resolver 选定的 host provider 绑定应用到已链接的 Semantic MIR。
#[doc(hidden)]
pub fn apply_host_provider_bindings(
    module: &mut MirModule,
    dependency_mirs: &[MirModule],
    bindings: &[ResolvedHostProviderBinding],
) -> Result<(), ParseError> {
    if bindings.is_empty() {
        return Ok(());
    }

    let modules: Vec<&MirModule> = std::iter::once(&*module).chain(dependency_mirs.iter()).collect();
    let mut plans = Vec::new();

    for binding in bindings {
        if !callable_present(&modules, binding.contract_instance) {
            continue;
        }
        if !callable_present(&modules, binding.provider_instance) {
            return Err(ParseError::Invalid { message: format!("host provider `{}` 未在源码闭包中定义", binding.provider_instance), span: None });
        }
        let provider_function = modules
            .iter()
            .flat_map(|dependency| dependency.functions.iter())
            .find(|function| function.instance == Some(binding.provider_instance))
            .cloned();
        plans.push(PlannedBinding {
            contract_instance: binding.contract_instance,
            provider_instance: binding.provider_instance,
            provider_function,
        });
    }

    // 先完成全部合同的身份映射校验，再安装 provider 定义；冲突绑定不得留下部分副作用。
    let mut remaps = BTreeMap::<ItemInstanceId, ItemInstanceId>::new();
    for plan in &plans {
        if plan.contract_instance == plan.provider_instance {
            continue;
        }
        if let Some(previous) = remaps.insert(plan.contract_instance, plan.provider_instance)
            && previous != plan.provider_instance
        {
            return Err(ParseError::Invalid { message: "host contract 不能同时绑定到多个 provider".into(), span: None });
        }
    }

    for plan in plans {
        if let Some(provider_function) = plan.provider_function {
            install_provider_definition(module, plan.provider_instance, provider_function)?;
        }
    }

    if remaps.is_empty() {
        return Ok(());
    }

    for function in &mut module.functions {
        remap_call_targets(function, &remaps);
    }

    for (contract_instance, _provider_instance) in &remaps {
        module.callable_symbols.remove(contract_instance);
        module.external_calls.retain(|contract| contract.instance != Some(*contract_instance));
    }

    Ok(())
}

fn callable_present(modules: &[&MirModule], instance: ItemInstanceId) -> bool {
    modules.iter().any(|module| {
        module.functions.iter().any(|function| function.instance == Some(instance))
            || module.external_calls.iter().any(|contract| contract.instance == Some(instance))
    })
}

fn install_provider_definition(
    module: &mut MirModule,
    provider_instance: ItemInstanceId,
    provider_function: MirFunction,
) -> Result<(), ParseError> {
    if module.functions.iter().any(|function| function.instance == Some(provider_instance)) {
        module.register_callable_symbol(provider_instance, provider_function.display_symbol.clone()).map_err(|e| ParseError::Invalid { message: e, span: None })?;
        return Ok(());
    }
    module.functions.push(provider_function.clone());
    module.register_callable_symbol(provider_instance, provider_function.display_symbol).map_err(|e| ParseError::Invalid { message: e, span: None })?;
    Ok(())
}

fn remap_call_targets(function: &mut MirFunction, remaps: &BTreeMap<ItemInstanceId, ItemInstanceId>) {
    for block in &mut function.blocks {
        for instruction in &mut block.instructions {
            if let MirOperation::Call { callee, arguments } = &mut instruction.kind {
                if let MirOperand::Callable(instance) = callee {
                    if let Some(target) = remaps.get(instance) {
                        *callee = MirOperand::Callable(*target);
                    }
                }
                for argument in arguments {
                    if let MirOperand::Callable(instance) = argument {
                        if let Some(target) = remaps.get(instance) {
                            *argument = MirOperand::Callable(*target);
                        }
                    }
                }
            }
        }
    }
}
