//! Semantic MIR 的实例身份链接边界。
//!
//! 链接器只接受前端已经冻结的 ItemInstanceId 闭包，不从名称或布局补造身份。

use std::collections::{BTreeMap, BTreeSet};

use crate::valkyrie::mir::{MirFunction, MirModule, MirOperand, MirOperation, MirStruct, MirSumDeclaration};
use nyar_types::layout::{AggregateLayout, AggregateLayoutPlan};
use nyar_types::{FieldId, NominalInstanceId};
use std_data::text::valkyrie::ParseError;

/// 合并已由 Compiler 统一注册的依赖实例闭包。
pub(crate) fn link_reachable_dependency_mir(
    consumer: &mut MirModule,
    dependency_mirs: &[MirModule],
) -> Result<(), std_data::text::valkyrie::ParseError> {
    if dependency_mirs.is_empty() {
        return Ok(());
    }
    let (structs, sum_types, remaps) = freeze_aggregate_identities(consumer, dependency_mirs)?;
    let aggregate_layouts = merge_aggregate_layouts(consumer, dependency_mirs)?;
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
    linked.structs = structs;
    linked.sum_types = sum_types;
    linked.aggregate_layouts = aggregate_layouts;
    for function in &mut linked.functions {
        remap_function_aggregates(function, &remaps[0])?;
    }
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
                let module_index = std::iter::once(&*consumer)
                    .chain(dependency_mirs)
                    .position(|candidate| std::ptr::eq(candidate, *module))
                    .ok_or_else(|| ParseError::invalid("依赖模块身份不在当前源码闭包中"))?;
                let mut function = (*function).clone();
                remap_function_aggregates(&mut function, &remaps[module_index])?;
                linked.functions.push(function);
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

fn merge_aggregate_layouts(consumer: &MirModule, dependencies: &[MirModule]) -> Result<AggregateLayoutPlan, ParseError> {
    let mut merged = consumer.aggregate_layouts.clone();
    for module in dependencies {
        for layout in &module.aggregate_layouts.layouts {
            if let Some(existing) = merged.layouts.iter().find(|candidate| candidate.name == layout.name && candidate.namespace == layout.namespace) {
                if existing.storage != layout.storage
                    || existing.size != layout.size
                    || existing.align != layout.align
                    || existing.fields != layout.fields
                {
                    return Err(ParseError::invalid(format!("布局 `{}` 合同冲突", qualified_layout_name(layout))));
                }
                continue;
            }
            let new_id = merged.layouts.iter().map(|candidate| candidate.id).max().unwrap_or(0).saturating_add(1);
            let mut copied = layout.clone();
            copied.id = new_id;
            let qualified = qualified_layout_name(&copied);
            if merged.type_name_to_layout.insert(qualified.clone(), new_id).is_some() {
                return Err(ParseError::invalid(format!("布局键 `{qualified}` 重复")));
            }
            if copied.storage == nyar_types::layout::StorageKind::Value {
                merged.value_type_names.insert(qualified);
            }
            merged.layouts.push(copied);
        }
    }
    Ok(merged)
}

fn qualified_layout_name(layout: &AggregateLayout) -> String {
    if layout.namespace.is_empty() { layout.name.clone() } else { format!("{}.{}", layout.namespace, layout.name) }
}

#[derive(Default)]
struct AggregateRemap {
    nominals: BTreeMap<NominalInstanceId, NominalInstanceId>,
    variants: BTreeMap<nyar_types::VariantId, nyar_types::VariantId>,
    fields: BTreeMap<FieldId, FieldId>,
}

fn same_struct_contract(left: &MirStruct, right: &MirStruct) -> bool {
    left.declaration.is_some()
        && left.declaration == right.declaration
        && left.qualified_name() == right.qualified_name()
        && left.generics == right.generics
        && left.is_value_type == right.is_value_type
        && left.fields.iter().map(|field| (&field.name, &field.ty)).eq(right.fields.iter().map(|field| (&field.name, &field.ty)))
}

fn same_sum_contract(left: &MirSumDeclaration, right: &MirSumDeclaration) -> bool {
    left.declaration.is_some()
        && left.declaration == right.declaration
        && left.name == right.name
        && left.is_unite == right.is_unite
        && left.generics == right.generics
        && left.variants.iter().map(|variant| (&variant.name, variant.tag, &variant.result_type, variant.fields.iter().map(|field| (&field.name, &field.ty)).collect::<Vec<_>>()))
            .eq(right.variants.iter().map(|variant| (&variant.name, variant.tag, &variant.result_type, variant.fields.iter().map(|field| (&field.name, &field.ty)).collect::<Vec<_>>())))
}

fn freeze_aggregate_identities(
    consumer: &MirModule,
    dependencies: &[MirModule],
) -> Result<(Vec<MirStruct>, Vec<MirSumDeclaration>, Vec<AggregateRemap>), ParseError> {
    let modules = std::iter::once(consumer).chain(dependencies).collect::<Vec<_>>();
    let mut global_structs: Vec<MirStruct> = Vec::new();
    let mut global_sums: Vec<MirSumDeclaration> = Vec::new();
    let mut remaps = Vec::with_capacity(modules.len());
    for module in modules {
        let mut remap = AggregateRemap::default();
        for declaration in &module.structs {
            if let Some(existing) = global_structs.iter().find(|existing| {
                existing.declaration.is_some() && existing.declaration == declaration.declaration
            }) {
                if !same_struct_contract(existing, declaration) {
                    return Err(ParseError::invalid(format!("聚合声明 `{}` 合同冲突", declaration.qualified_name())));
                }
                remap.nominals.insert(declaration.nominal, existing.nominal);
                for (local, global) in declaration.fields.iter().zip(&existing.fields) {
                    if remap.fields.insert(local.id, global.id).is_some() {
                        return Err(ParseError::invalid(format!("字段身份 `{}` 在模块 `{}` 中重复", local.id, module.name)));
                    }
                }
            } else {
                let nominal = NominalInstanceId::from_index(global_structs.len() as u32).ok_or_else(|| ParseError::invalid("NominalInstanceId 溢出"))?;
                let mut frozen = declaration.clone();
                frozen.nominal = nominal;
                remap.nominals.insert(declaration.nominal, nominal);
                let field_start = total_field_count(&global_structs, &global_sums);
                for (index, field) in frozen.fields.iter_mut().enumerate() {
                    let id = FieldId::from_index((field_start + index) as u32).ok_or_else(|| ParseError::invalid("FieldId 溢出"))?;
                    remap.fields.insert(field.id, id);
                    field.id = id;
                }
                global_structs.push(frozen);
            }
        }
        for declaration in &module.sum_types {
            if let Some(existing) = global_sums.iter().find(|existing| {
                existing.declaration.is_some() && existing.declaration == declaration.declaration
            }) {
                if !same_sum_contract(existing, declaration) {
                    return Err(ParseError::invalid(format!("sum 声明 `{}` 合同冲突", declaration.name)));
                }
                remap.nominals.insert(declaration.nominal, existing.nominal);
                for (local_variant, global_variant) in declaration.variants.iter().zip(&existing.variants) {
                    remap.variants.insert(local_variant.id, global_variant.id);
                    for (local, global) in local_variant.fields.iter().zip(&global_variant.fields) {
                        if remap.fields.insert(local.id, global.id).is_some() {
                            return Err(ParseError::invalid(format!("字段身份 `{}` 在模块 `{}` 中重复", local.id, module.name)));
                        }
                    }
                }
            } else {
                let mut frozen = declaration.clone();
                let global_nominal = NominalInstanceId::from_index((global_structs.len() + global_sums.len()) as u32)
                    .ok_or_else(|| ParseError::invalid("NominalInstanceId 溢出"))?;
                remap.nominals.insert(declaration.nominal, global_nominal);
                frozen.nominal = global_nominal;
                let mut next_field = total_field_count(&global_structs, &global_sums);
                for (variant_index, variant) in frozen.variants.iter_mut().enumerate() {
                    let global_variant = nyar_types::VariantId::from_index(
                        global_sums.iter().flat_map(|sum| sum.variants.iter()).count() as u32 + variant_index as u32,
                    ).ok_or_else(|| ParseError::invalid("VariantId 溢出"))?;
                    remap.variants.insert(variant.id, global_variant);
                    variant.id = global_variant;
                    for field in &mut variant.fields {
                        let id = FieldId::from_index(next_field as u32).ok_or_else(|| ParseError::invalid("FieldId 溢出"))?;
                        next_field += 1;
                        remap.fields.insert(field.id, id);
                        field.id = id;
                    }
                }
                global_sums.push(frozen);
            }
        }
        remaps.push(remap);
    }
    Ok((global_structs, global_sums, remaps))
}

fn total_field_count(structs: &[MirStruct], sums: &[MirSumDeclaration]) -> usize {
    structs.iter().map(|declaration| declaration.fields.len()).sum::<usize>()
        + sums.iter().flat_map(|declaration| &declaration.variants).map(|variant| variant.fields.len()).sum::<usize>()
}

fn remap_function_aggregates(function: &mut MirFunction, remap: &AggregateRemap) -> Result<(), ParseError> {
    for block in &mut function.blocks {
        for instruction in &mut block.instructions {
            match &mut instruction.kind {
                MirOperation::StructNew { nominal, fields } => {
                    *nominal = *remap.nominals.get(nominal).ok_or_else(|| ParseError::invalid("StructNew 引用了未冻结的名义身份"))?;
                    for (field, _) in fields {
                        *field = *remap.fields.get(field).ok_or_else(|| ParseError::invalid("StructNew 引用了未冻结的字段身份"))?;
                    }
                }
                MirOperation::FieldGet { field, .. } | MirOperation::FieldSet { field, .. } => {
                    *field = *remap.fields.get(field).ok_or_else(|| ParseError::invalid("字段操作引用了未冻结的字段身份"))?;
                }
                MirOperation::SumNew { nominal, variant, .. }
                | MirOperation::SumPayloadGet { nominal, variant, .. }
                | MirOperation::SumVariantIs { nominal, variant, .. } => {
                    *nominal = *remap.nominals.get(nominal).ok_or_else(|| ParseError::invalid("sum 操作引用了未冻结的名义身份"))?;
                    *variant = *remap.variants.get(variant).ok_or_else(|| ParseError::invalid("sum 操作引用了未冻结的 variant 身份"))?;
                }
                _ => {}
            }
        }
    }
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
