//! Resolver 已选定的 `[host_provider]` 绑定进入 Compiler linker 的合同。
//!
//! 装配层只能提供 contract → provider 符号选择；Compiler 必须把该选择冻结为
//! stable `ItemInstanceId`，并改写成功路径中的调用目标。

use std::collections::BTreeMap;

use nyar_types::ItemInstanceId;

use crate::valkyrie::{
    frontend::ParseError,
    mir::{MirFunction, MirModule, MirOperand, MirOperation},
    symbols::stable_symbol_key,
};

use super::context::CompilerHostProviderBinding;

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

    let mut remaps = BTreeMap::<ItemInstanceId, ItemInstanceId>::new();

    for binding in bindings {
        // 装配层按 target/profile 选择 provider 时看到的是声明库存，不掌握入口可达性。
        // 未被本次可观察入口闭包引用的 contract 没有需要改写的调用点，其 provider
        // 也就不需要出现在依赖 MIR 中：跳过，不得因未使用而拒绝整个编译。
        let contract_instance = {
            let contracts =
                module.external_calls.iter().filter(|contract| stable_symbol_key(&contract.symbol) == binding.contract).collect::<Vec<_>>();
            match contracts.as_slice() {
                [] => continue,
                [contract] => contract
                    .instance
                    .ok_or_else(|| ParseError::invalid(format!("host contract `{}` 缺少 callable identity", binding.contract)))?,
                _ => return Err(ParseError::invalid(format!("host contract `{}` 在 Semantic MIR 中有多个候选", binding.contract))),
            }
        };

        let (provider_instance, provider_function) = resolve_callable_binding(module, dependency_mirs, &binding.symbol)?;
        if let Some(provider_function) = provider_function {
            install_provider_definition(module, provider_instance, provider_function)?;
        }
        if contract_instance == provider_instance {
            continue;
        }
        if let Some(previous) = remaps.insert(contract_instance, provider_instance)
            && previous != provider_instance
        {
            return Err(ParseError::invalid(format!("host contract `{}` 不能同时绑定到多个 provider", binding.contract)));
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

fn resolve_callable_binding(
    module: &MirModule,
    dependency_mirs: &[MirModule],
    symbol: &str,
) -> Result<(ItemInstanceId, Option<MirFunction>), ParseError> {
    if let Some((instance, _)) = module.callable_symbols.iter().find(|(_, label)| label.as_str() == symbol) {
        return Ok((*instance, None));
    }

    let consumer_matches =
        module.functions.iter().filter(|function| function.instance.is_some() && function.display_symbol == symbol).collect::<Vec<_>>();
    if consumer_matches.len() > 1 {
        return Err(ParseError::invalid(format!("host provider `{symbol}` 的 callable identity 有歧义")));
    }
    if let Some(function) = consumer_matches.first() {
        return Ok((function.instance.expect("已过滤 callable instance"), None));
    }

    let candidates = dependency_mirs
        .iter()
        .flat_map(|dependency| dependency.functions.iter())
        .filter(|function| function.instance.is_some() && function.display_symbol == symbol)
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [] => Err(ParseError::invalid(format!("host provider `{symbol}` 未在源码闭包中解析到 callable identity"))),
        [function] => Ok((function.instance.expect("已过滤 callable instance"), Some((*function).clone()))),
        _ => Err(ParseError::invalid(format!("host provider `{symbol}` 在依赖闭包中有多个候选"))),
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
