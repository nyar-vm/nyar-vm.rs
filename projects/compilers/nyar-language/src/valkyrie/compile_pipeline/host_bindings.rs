//! Resolver 已选定的 `[host_provider]` 绑定进入 Compiler linker 的合同。
//!
//! 装配层必须提供冻结的声明身份；linker 只按 `ItemInstanceId` 改写调用目标，
//! 不得再接收字符串后反查 callable。

use std::collections::BTreeMap;

use nyar_types::ItemInstanceId;

use crate::valkyrie::{
    frontend::ParseError,
    mir::{MirFunction, MirModule, MirOperand, MirOperation},
    semantic_closure::DeclarationKey,
};

use super::context::CompilerHostProviderBinding;

struct PlannedBinding {
    contract_instance: ItemInstanceId,
    provider_instance: ItemInstanceId,
    provider_function: Option<MirFunction>,
}

/// 将 Resolver 选定的 host provider 绑定应用到已链接的 Semantic MIR。
#[doc(hidden)]
pub fn apply_host_provider_bindings(
    module: &mut MirModule,
    dependency_mirs: &[MirModule],
    bindings: &[CompilerHostProviderBinding],
) -> Result<(), ParseError> {
    if bindings.is_empty() {
        return Ok(());
    }

    let modules: Vec<&MirModule> = std::iter::once(&*module).chain(dependency_mirs.iter()).collect();
    let mut plans = Vec::new();

    for binding in bindings {
        let contract_instance = match find_callable_instance(&modules, &binding.contract)? {
            Some(instance) => instance,
            None => continue,
        };
        let provider_instance = find_callable_instance(&modules, &binding.provider)?.ok_or_else(|| {
            ParseError::invalid(format!("host provider `{}` 未在源码闭包中解析到 callable identity", binding.provider.encoded()))
        })?;
        let provider_function = modules
            .iter()
            .flat_map(|dependency| dependency.functions.iter())
            .find(|function| function.instance == Some(provider_instance))
            .cloned();
        plans.push(PlannedBinding { contract_instance, provider_instance, provider_function });
    }

    let mut remaps = BTreeMap::<ItemInstanceId, ItemInstanceId>::new();
    for plan in plans {
        if let Some(provider_function) = plan.provider_function {
            install_provider_definition(module, plan.provider_instance, provider_function)?;
        }
        if plan.contract_instance == plan.provider_instance {
            continue;
        }
        if let Some(previous) = remaps.insert(plan.contract_instance, plan.provider_instance)
            && previous != plan.provider_instance
        {
            return Err(ParseError::invalid("host contract 不能同时绑定到多个 provider"));
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

fn find_callable_instance(modules: &[&MirModule], key: &DeclarationKey) -> Result<Option<ItemInstanceId>, ParseError> {
    let mut matches = BTreeMap::new();
    for module in modules {
        for contract in &module.external_calls {
            if contract.declaration_key.as_ref() == Some(key) {
                if let Some(instance) = contract.instance {
                    matches.insert(instance, contract.declaration);
                }
            }
        }
        for function in &module.functions {
            if function.declaration_key.as_ref() == Some(key) {
                if let Some(instance) = function.instance {
                    matches.insert(instance, function.declaration);
                }
            }
        }
    }
    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.keys().next().copied()),
        _ => Err(ParseError::invalid(format!("callable 声明 `{}` 对应多个 ItemInstanceId", key.encoded()))),
    }
}

fn install_provider_definition(
    module: &mut MirModule,
    provider_instance: ItemInstanceId,
    provider_function: MirFunction,
) -> Result<(), ParseError> {
    if module.functions.iter().any(|function| function.instance == Some(provider_instance)) {
        module.register_callable_symbol(provider_instance, provider_function.display_symbol.clone()).map_err(ParseError::invalid)?;
        return Ok(());
    }
    module.functions.push(provider_function.clone());
    module.register_callable_symbol(provider_instance, provider_function.display_symbol).map_err(ParseError::invalid)?;
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
