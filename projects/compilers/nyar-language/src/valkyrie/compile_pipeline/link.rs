//! Semantic MIR 的实例身份链接边界。
//!
//! 链接器只接受前端已经冻结的 ItemInstanceId 闭包，不从名称或布局补造身份。

use std::collections::{BTreeMap, BTreeSet};

use crate::valkyrie::mir::{MirModule, MirOperand, MirOperation};
use std_data::text::valkyrie::ParseError;

/// 合并已由 Compiler 统一注册的依赖实例闭包。
pub(crate) fn link_reachable_dependency_mir(
    consumer: &mut MirModule,
    dependency_mirs: &[MirModule],
) -> Result<(), std_data::text::valkyrie::ParseError> {
    if dependency_mirs.is_empty() {
        return Ok(());
    }
    let mut definitions = BTreeMap::new();
    let mut imports = BTreeMap::new();
    for module in std::iter::once(&*consumer).chain(dependency_mirs) {
        if module.type_identities != consumer.type_identities {
            return Err(ParseError::invalid("依赖链接必须消费 Compiler 统一 TypeId 表"));
        }
        for function in &module.functions {
            if let Some(instance) = function.instance {
                if function.declaration.is_none() || definitions.insert(instance, (module, function)).is_some() {
                    return Err(ParseError::invalid(format!("ItemInstanceId `{instance}` 的声明缺失或定义重复")));
                }
            }
        }
        for contract in &module.external_calls {
            let instance = contract.instance.ok_or_else(|| ParseError::invalid("外部导入缺少 ItemInstanceId"))?;
            if contract.declaration.is_none() {
                return Err(ParseError::invalid("外部导入缺少 ItemId"));
            }
            if let Some(previous) = imports.insert(instance, contract)
                && previous != contract {
                return Err(ParseError::invalid(format!("导入实例 `{instance}` 合同冲突")));
            }
        }
    }
    let mut linked = consumer.clone();
    let mut visited = BTreeSet::new();
    let mut pending = consumer.functions.iter().filter_map(|function| function.instance).collect::<Vec<_>>();
    while let Some(instance) = pending.pop() {
        if !visited.insert(instance) {
            continue;
        }
        if let Some((module, function)) = definitions.get(&instance) {
            for block in &function.blocks {
                for instruction in &block.instructions {
                    if let MirOperation::Call { callee: MirOperand::Callable(callee), .. } = &instruction.kind {
                        pending.push(*callee);
                    }
                }
            }
            if !std::ptr::eq(*module, &*consumer) {
                if linked.callable_identities.insert(function.symbol.clone(), instance).is_some() {
                    return Err(ParseError::invalid(format!("实例 `{instance}` 的 ABI 标签重复")));
                }
                linked.functions.push((*function).clone());
                for declaration in &module.structs {
                    if !linked.structs.contains(declaration) {
                        linked.structs.push(declaration.clone());
                    }
                }
                for declaration in &module.sum_types {
                    if !linked.sum_types.contains(declaration) {
                        linked.sum_types.push(declaration.clone());
                    }
                }
            }
        } else if let Some(contract) = imports.get(&instance) {
            if !linked.external_calls.contains(contract) {
                linked.external_calls.push((*contract).clone());
                if let Some(previous) = linked.callable_identities.insert(contract.symbol.to_string(), instance)
                    && previous != instance {
                    return Err(ParseError::invalid("外部导入 ABI 标签冲突"));
                }
            }
        } else {
            return Err(ParseError::invalid(format!("调用实例 `{instance}` 没有定义或显式导入合同")));
        }
    }
    *consumer = linked;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ValkyrieCompiler, mir::MirLowerer};

    #[test]
    fn dependency_linking_without_shared_instances_fails_without_mutation() {
        let compiler = ValkyrieCompiler::default();
        let consumer_hir = compiler.compile_source("micro consumer() -> unit { return }").expect("源码解析");
        let dependency_hir = compiler.compile_source("micro helper() -> unit { return }").expect("依赖源码解析");
        let mut consumer = MirLowerer::lower_module_semantic(&consumer_hir);
        let dependency = MirLowerer::lower_module_semantic(&dependency_hir);
        let original = consumer.clone();
        let error = link_reachable_dependency_mir(&mut consumer, &[dependency]).expect_err("缺少统一实例表不能链接");
        assert!(error.to_string().contains("ItemInstanceId"));
        assert_eq!(consumer, original);
    }
}
