//! Resolver 已选定的 `[host_provider]` 绑定进入 Compiler linker 的合同。
//!
//! 装配层只能提供 contract → provider 符号选择；Compiler 必须把该选择冻结为
//! stable `ItemInstanceId`，并改写成功路径中的调用目标。

use std::collections::BTreeMap;

use nyar_types::{ExternalImportLink, ItemInstanceId, ItemId};
use crate::types::{Identifier, NamePath};
use crate::types::hir::ValkyrieType;

use crate::valkyrie::{
    frontend::ParseError,
    mir::{
        MirBlock, MirBlockRef, MirExternalCallContract, MirFunction, MirInstruction, MirModule, MirOperand, MirOperation,
        MirTerminator, MirValueOrigin,
    },
};

use super::context::CompilerHostProviderBinding;

/// 将 Resolver 选定的 host provider 绑定应用到已链接的 Semantic MIR。
pub(crate) fn apply_host_provider_bindings(
    module: &mut MirModule,
    dependency_mirs: &[MirModule],
    bindings: &[CompilerHostProviderBinding],
) -> Result<(), ParseError> {
    if bindings.is_empty() {
        return Ok(());
    }

    let mut remaps = BTreeMap::<ItemInstanceId, ItemInstanceId>::new();

    for binding in bindings {
        let (provider_instance, provider_function) = resolve_callable_binding(module, dependency_mirs, &binding.symbol).ok_or_else(|| {
            ParseError::invalid(format!(
                "host provider `{}` 未在源码闭包中解析到 callable identity",
                binding.symbol
            ))
        })?;
        if let Some(provider_function) = provider_function {
            install_provider_definition(module, provider_instance, provider_function)?;
        }

        let contract_keys = symbol_lookup_keys(&binding.contract);
        let contract_external = module
            .external_calls
            .iter()
            .find(|contract| contract_keys.iter().any(|key| symbols_equivalent(&contract.symbol.to_string(), key)));

        if let Some(contract) = contract_external {
            let contract_instance = contract
                .instance
                .ok_or_else(|| ParseError::invalid(format!("host contract `{}` 缺少 callable identity", binding.contract)))?;
            if contract_instance == provider_instance {
                continue;
            }
            if let Some(previous) = remaps.insert(contract_instance, provider_instance)
                && previous != provider_instance
            {
                return Err(ParseError::invalid(format!(
                    "host contract `{}` 不能同时绑定到多个 provider",
                    binding.contract
                )));
            }
        }
    }

    if remaps.is_empty() {
        return Ok(());
    }

    for function in &mut module.functions {
        remap_call_targets(function, &remaps);
    }

    for (contract_instance, provider_instance) in &remaps {
        module
            .callable_identities
            .retain(|_, instance| *instance != *contract_instance || *instance == *provider_instance);
        module.external_calls.retain(|contract| contract.instance != Some(*contract_instance));
    }

    Ok(())
}

fn resolve_callable_binding(
    module: &MirModule,
    dependency_mirs: &[MirModule],
    symbol: &str,
) -> Option<(ItemInstanceId, Option<MirFunction>)> {
    if let Some(instance) = resolve_registered_callable(module, symbol) {
        return Some((instance, None));
    }

    for dependency in dependency_mirs {
        for function in &dependency.functions {
            if function.instance.is_some() && symbol_matches(&function.symbol, symbol) {
                return function.instance.map(|instance| (instance, Some(function.clone())));
            }
        }
    }
    None
}

fn resolve_registered_callable(module: &MirModule, symbol: &str) -> Option<ItemInstanceId> {
    symbol_lookup_keys(symbol)
        .into_iter()
        .find_map(|key| module.callable_identities.get(&key).copied())
}

fn symbol_matches(candidate: &str, symbol: &str) -> bool {
    symbol_lookup_keys(symbol).iter().any(|key| key == candidate)
}

fn install_provider_definition(
    module: &mut MirModule,
    provider_instance: ItemInstanceId,
    provider_function: MirFunction,
) -> Result<(), ParseError> {
    if module.functions.iter().any(|function| function.instance == Some(provider_instance)) {
        module
            .callable_identities
            .entry(provider_function.symbol.clone())
            .or_insert(provider_instance);
        return Ok(());
    }
    module.functions.push(provider_function.clone());
    if let Some(previous) = module.callable_identities.insert(provider_function.symbol.clone(), provider_instance)
        && previous != provider_instance
    {
        return Err(ParseError::invalid(format!(
            "host provider `{}` 的 callable identity 冲突",
            provider_function.symbol
        )));
    }
    Ok(())
}

fn symbol_lookup_keys(symbol: &str) -> Vec<String> {
    let mut keys = Vec::new();
    keys.push(symbol.to_string());
    keys.push(symbol.replace('.', "::"));
    keys.push(symbol.replace("::", "."));
    if let Some((namespace, name)) = symbol.rsplit_once('.') {
        keys.push(format!("{}::{}", namespace, name));
    }
    if let Some((namespace, name)) = symbol.rsplit_once("::") {
        keys.push(format!("{}.{}", namespace, name));
    }
    keys.sort();
    keys.dedup();
    keys
}

fn symbols_equivalent(left: &str, right: &str) -> bool {
    symbol_lookup_keys(left).iter().any(|candidate| candidate == right)
        || symbol_lookup_keys(right).iter().any(|candidate| candidate == left)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::valkyrie::mir::{AggregateLayoutPlan, MirValueRef};

    fn module_with_contract_call(contract_instance: ItemInstanceId, provider_instance: ItemInstanceId) -> MirModule {
        MirModule {
            name: "demo".into(),
            functions: vec![MirFunction {
                symbol: "demo::main".into(),
                declaration: Some(ItemId::from_index(0).unwrap()),
                instance: Some(ItemInstanceId::from_index(0).unwrap()),
                return_type: ValkyrieType::Integer32 { signed: true },
                param_types: Vec::new(),
                value_types: BTreeMap::new(),
                entry: MirBlockRef(0),
                values: Vec::new(),
                blocks: vec![MirBlock {
                    id: MirBlockRef(0),
                    label: "entry".into(),
                    parameters: Vec::new(),
                    instructions: vec![MirInstruction::from_operation_with_results(
                        MirOperation::Call {
                            callee: MirOperand::Callable(contract_instance),
                            arguments: vec![MirOperand::Value(MirValueRef(0))],
                        },
                        vec![],
                    )],
                    terminator: MirTerminator::Return { value: Some(MirOperand::Value(MirValueRef(0))) },
                }],
            }],
            structs: Vec::new(),
            imports: Vec::new(),
            external_calls: vec![MirExternalCallContract {
                declaration: Some(ItemId::from_index(1).unwrap()),
                instance: Some(contract_instance),
                symbol: NamePath::new(vec![Identifier::new("demo::write")]),
                link: ExternalImportLink::host(None, vec!["demo".to_owned(), "write".to_owned()]),
                parameter_types: vec![ValkyrieType::Integer32 { signed: true }],
                return_type: ValkyrieType::Integer32 { signed: true },
            }],
            exports: Vec::new(),
            entries: Vec::new(),
            callable_identities: BTreeMap::from([
                ("demo::main".to_owned(), ItemInstanceId::from_index(0).unwrap()),
                ("demo::write".to_owned(), contract_instance),
                ("std.adaptor.clr::write".to_owned(), provider_instance),
            ]),
            type_identities: BTreeMap::new(),
            aggregate_layouts: AggregateLayoutPlan::default(),
            sum_types: Vec::new(),
            flags_types: Vec::new(),
            singleton_instances: Vec::new(),
            semantic_fragments: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn selected_host_provider_rewrites_contract_calls_to_provider_instance() {
        let contract_instance = ItemInstanceId::from_index(1).unwrap();
        let provider_instance = ItemInstanceId::from_index(2).unwrap();
        let mut module = module_with_contract_call(contract_instance, provider_instance);
        let bindings = vec![CompilerHostProviderBinding {
            contract: "demo.write".into(),
            symbol: "std.adaptor.clr.write".into(),
        }];

        apply_host_provider_bindings(&mut module, &[], &bindings).expect("host provider binding must close adaptor contract");

        let call = module.functions[0]
            .blocks[0]
            .instructions[0]
            .kind
            .clone();
        assert_eq!(
            call,
            MirOperation::Call {
                callee: MirOperand::Callable(provider_instance),
                arguments: vec![MirOperand::Value(MirValueRef(0))],
            }
        );
        assert!(module.external_calls.is_empty());
        assert!(!module.callable_identities.values().any(|instance| *instance == contract_instance));
        assert!(module.callable_identities.values().any(|instance| *instance == provider_instance));
    }

    #[test]
    fn missing_provider_symbol_fails_closed() {
        let contract_instance = ItemInstanceId::from_index(1).unwrap();
        let provider_instance = ItemInstanceId::from_index(2).unwrap();
        let mut module = module_with_contract_call(contract_instance, provider_instance);
        module.callable_identities.remove("std.adaptor.clr::write");
        let bindings = vec![CompilerHostProviderBinding {
            contract: "demo.write".into(),
            symbol: "std.adaptor.clr.write".into(),
        }];

        let error = apply_host_provider_bindings(&mut module, &[], &bindings).expect_err("missing provider must fail");
        assert!(error.to_string().contains("host provider"));
    }
}
