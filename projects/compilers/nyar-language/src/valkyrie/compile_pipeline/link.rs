//! Semantic MIR 的实例身份链接边界。
//!
//! 链接器只接受前端已经冻结的 ItemInstanceId 闭包，不从名称或布局补造身份。

use std::collections::{BTreeMap, BTreeSet};

use crate::valkyrie::{
    frontend::ParseError,
    mir::{MirFunction, MirModule, MirOperand, MirOperation, MirStruct, MirSumDeclaration},
};
use nyar_types::{
    FieldId, NominalInstanceId,
    layout::{AggregateLayout, AggregateLayoutPlan},
};

/// 合并已由 Compiler 统一注册的依赖实例闭包。
#[doc(hidden)]
pub fn link_reachable_dependency_mir(
    consumer: &mut MirModule,
    dependency_mirs: &[MirModule],
) -> Result<(), crate::valkyrie::frontend::ParseError> {
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
                if function.declaration.is_none() {
                    return Err(ParseError::invalid(format!(
                        "ItemInstanceId `{instance}` 缺少声明 ItemId；symbol=`{}`; module=`{}`",
                        function.display_symbol, module.name
                    )));
                }
                if let Some((previous_module, previous)) = definitions.insert(instance, (module, function)) {
                    return Err(ParseError::invalid(format!(
                        "ItemInstanceId `{instance}` 定义重复；first=`{}`@`{}`; second=`{}`@`{}`",
                        previous.display_symbol, previous_module.name, function.display_symbol, module.name
                    )));
                }
            }
        }
        for contract in &module.external_calls {
            let instance = contract.instance.ok_or_else(|| ParseError::invalid("外部导入缺少 ItemInstanceId"))?;
            if contract.declaration.is_none() {
                return Err(ParseError::invalid("外部导入缺少 ItemId"));
            }
            if let Some(previous) = imports.insert(instance, contract)
                && previous != contract
            {
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
    // 链接根只取已冻结的出口/入口实例；泛型种子等不可达实例不参与传播，
    // 否则会拉入未闭合的 requirement 或无关依赖。无出口合同的分析用闭包
    // 以已实例化函数为根。
    let mut pending = consumer
        .exports
        .iter()
        .filter_map(|export| export.instance)
        .chain(consumer.entries.iter().filter_map(|entry| entry.instance))
        .collect::<Vec<_>>();
    if pending.is_empty() {
        pending = consumer.functions.iter().filter_map(|function| function.instance).collect();
    }
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
                if let Err(detail) = linked.register_callable_symbol(instance, function.display_symbol.clone()) {
                    return Err(ParseError::invalid(detail));
                }
                let module_index = std::iter::once(&*consumer)
                    .chain(dependency_mirs)
                    .position(|candidate| std::ptr::eq(candidate, *module))
                    .ok_or_else(|| ParseError::invalid("依赖模块身份不在当前源码闭包中"))?;
                let mut function = (*function).clone();
                remap_function_aggregates(&mut function, &remaps[module_index])?;
                linked.functions.push(function);
            }
        }
        else if let Some(contract) = imports.get(&instance) {
            if !linked.external_calls.contains(contract) {
                linked.external_calls.push((*contract).clone());
                let symbol = contract
                    .declaration_key
                    .as_ref()
                    .map(|key| key.encoded())
                    .unwrap_or_else(|| crate::valkyrie::symbols::stable_symbol_key(&contract.symbol));
                if let Err(detail) = linked.register_callable_symbol(instance, symbol) {
                    return Err(ParseError::invalid(detail));
                }
            }
        }
        else {
            let callers = consumer
                .functions
                .iter()
                .chain(dependency_mirs.iter().flat_map(|module| module.functions.iter()))
                .filter(|function| {
                    function.blocks.iter().any(|block| {
                        block.instructions.iter().any(|instruction| {
                            matches!(
                                &instruction.kind,
                                MirOperation::Call { callee: MirOperand::Callable(callee), .. } if *callee == instance
                            )
                        })
                    })
                })
                .map(|function| function.display_symbol.clone())
                .collect::<Vec<_>>();
            let local_imports = consumer.external_calls.iter().map(|c| c.symbol.to_string()).take(12).collect::<Vec<_>>();
            return Err(ParseError::invalid(format!(
                "调用实例 `{instance}` 没有定义或显式导入合同；callers={callers:?}; consumer_imports={local_imports:?}"
            )));
        }
    }
    *consumer = linked;
    Ok(())
}

/// 把入口组的公开表面并入唯一程序模块。
///
/// 程序模块是全部入口组共同的链接根宿主。合并只按已冻结的合同身份进行：
/// 同一实例的导出/入口必须完全一致；同一片段身份只允许 union；片段
/// 入口 operation 或 wasm 导出名冲突必须失败，不能择一覆盖。
#[doc(hidden)]
pub fn merge_entry_group_surface(consumer: &mut MirModule, entry: &MirModule) -> Result<(), crate::valkyrie::frontend::ParseError> {
    for contract in &entry.exports {
        match consumer.exports.iter().find(|existing| existing.instance == contract.instance) {
            Some(existing) if existing != contract => {
                return Err(ParseError::invalid(format!("入口组导出合同冲突: `{}`", contract.symbol)));
            }
            Some(_) => {}
            None => consumer.exports.push(contract.clone()),
        }
    }
    for contract in &entry.entries {
        match consumer.entries.iter().find(|existing| existing.instance == contract.instance) {
            Some(existing) if existing != contract => {
                return Err(ParseError::invalid(format!("入口组入口合同冲突: `{}`", contract.symbol)));
            }
            Some(_) => {}
            None => consumer.entries.push(contract.clone()),
        }
    }
    for fragment in &entry.semantic_fragments {
        let Some(existing) = consumer.semantic_fragments.iter_mut().find(|existing| existing.id == fragment.id)
        else {
            consumer.semantic_fragments.push(fragment.clone());
            continue;
        };
        for operation in &fragment.exported_operations {
            if !existing.exported_operations.contains(operation) {
                existing.exported_operations.push(*operation);
            }
        }
        for (instance, name) in &fragment.wasm_export_names {
            if let Some(previous) = existing.wasm_export_names.insert(*instance, name.clone())
                && previous != *name
            {
                return Err(ParseError::invalid(format!("片段 `{}` 的 wasm 导出名冲突", fragment.id)));
            }
        }
        match (existing.entry_operation, fragment.entry_operation) {
            (Some(previous), Some(next)) if previous != next => {
                return Err(ParseError::invalid(format!("片段 `{}` 包含多个入口 operation", fragment.id)));
            }
            (None, Some(next)) => existing.entry_operation = Some(next),
            _ => {}
        }
        for capability in &fragment.required_capabilities {
            if !existing.required_capabilities.contains(capability) {
                existing.required_capabilities.push(capability.clone());
            }
        }
        if existing.reference_management_hint.is_none() {
            existing.reference_management_hint = fragment.reference_management_hint.clone();
        }
    }
    Ok(())
}

/// 按 declaration identity 合并依赖的聚合布局；外部测试入口。
#[doc(hidden)]
pub fn merge_aggregate_layouts(consumer: &MirModule, dependencies: &[MirModule]) -> Result<AggregateLayoutPlan, ParseError> {
    let mut merged = consumer.aggregate_layouts.clone();
    for module in dependencies {
        for layout in &module.aggregate_layouts.layouts {
            let owner = declaration_for_layout(module, layout);
            let existing = owner
                .and_then(|item| merged.declaration_to_layout.get(&item).copied())
                .and_then(|id| merged.layouts.iter().find(|candidate| candidate.id == id));
            if let Some(existing) = existing {
                if existing.storage != layout.storage
                    || existing.size != layout.size
                    || existing.align != layout.align
                    || existing.fields != layout.fields
                {
                    return Err(ParseError::invalid(format!("布局 `{}` 合同冲突", qualified_layout_name(layout))));
                }
                continue;
            }
            let qualified = qualified_layout_name(layout);
            // 只按声明身份合并；名称或物理形状相同不能补出语义身份。
            let new_id = merged.layouts.iter().map(|candidate| candidate.id).max().unwrap_or(0).saturating_add(1);
            let mut copied = layout.clone();
            copied.id = new_id;
            if let Some(item) = owner {
                if merged.declaration_to_layout.insert(item, new_id).is_some() {
                    return Err(ParseError::invalid(format!("布局 `{}` declaration identity 重复", qualified_layout_name(layout))));
                }
                if copied.storage == nyar_types::layout::StorageKind::Value {
                    merged.value_type_declarations.insert(item);
                }
            }
            else if is_synthetic_layout_key(&qualified) {
                // 仅合成 tuple/fixedarray 保留名称键；名义布局不得进 type_name_to_layout。
                // 存储类以 layout.storage 为准；旧的名称视图表已删除。
                if merged.type_name_to_layout.insert(qualified.clone(), new_id).is_some() {
                    return Err(ParseError::invalid(format!("布局键 `{qualified}` 重复")));
                }
            }
            else {
                return Err(ParseError::invalid(format!("布局 `{}` 缺少 declaration identity，禁止按名合并", qualified_layout_name(layout))));
            }
            merged.layouts.push(copied);
        }
    }
    Ok(merged)
}

fn declaration_for_layout(module: &MirModule, layout: &AggregateLayout) -> Option<nyar_types::ItemId> {
    module.aggregate_layouts.declaration_to_layout.iter().find_map(|(item, id)| (*id == layout.id).then_some(*item))
}

fn qualified_layout_name(layout: &AggregateLayout) -> String {
    if layout.namespace.is_empty() { layout.name.clone() } else { format!("{}.{}", layout.namespace, layout.name) }
}

fn is_synthetic_layout_key(key: &str) -> bool {
    key.starts_with("__tuple_") || key.starts_with("__fixedarray_")
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
        && left.generics == right.generics
        && left.is_value_type == right.is_value_type
        && left.fields.iter().map(|field| (&field.name, &field.ty)).eq(right.fields.iter().map(|field| (&field.name, &field.ty)))
}

fn same_sum_contract(left: &MirSumDeclaration, right: &MirSumDeclaration) -> bool {
    left.declaration.is_some()
        && left.declaration == right.declaration
        && left.is_unite == right.is_unite
        && left.generics == right.generics
        && left
            .variants
            .iter()
            .map(|variant| {
                (
                    &variant.name,
                    variant.tag,
                    &variant.result_type,
                    variant.fields.iter().map(|field| (&field.name, &field.ty)).collect::<Vec<_>>(),
                )
            })
            .eq(right.variants.iter().map(|variant| {
                (
                    &variant.name,
                    variant.tag,
                    &variant.result_type,
                    variant.fields.iter().map(|field| (&field.name, &field.ty)).collect::<Vec<_>>(),
                )
            }))
}

fn freeze_aggregate_identities(
    consumer: &MirModule,
    dependencies: &[MirModule],
) -> Result<(Vec<MirStruct>, Vec<MirSumDeclaration>, Vec<AggregateRemap>), ParseError> {
    let modules = std::iter::once(consumer).chain(dependencies).collect::<Vec<_>>();
    let mut global_structs: Vec<MirStruct> = Vec::new();
    let mut global_sums: Vec<MirSumDeclaration> = Vec::new();
    let mut remaps = Vec::with_capacity(modules.len());
    // 字段身份只由 HIR 注册阶段生产；链接器只校验跨模块副本一致，不得重新编号。
    let mut field_owners: BTreeMap<FieldId, String> = BTreeMap::new();
    for module in modules {
        let mut remap = AggregateRemap::default();
        for declaration in &module.structs {
            let owner = struct_field_owner(declaration);
            if let Some(existing) =
                global_structs.iter().find(|existing| existing.declaration.is_some() && existing.declaration == declaration.declaration)
            {
                if !same_struct_contract(existing, declaration) {
                    return Err(ParseError::invalid(format!("聚合声明 `{}` 合同冲突", declaration.qualified_name())));
                }
                remap.nominals.insert(declaration.nominal, existing.nominal);
                for (local, global) in declaration.fields.iter().zip(&existing.fields) {
                    if local.id != global.id {
                        return Err(ParseError::invalid(format!(
                            "聚合声明 `{}` 的字段身份不一致；local={}; global={}",
                            declaration.qualified_name(),
                            local.id,
                            global.id
                        )));
                    }
                    claim_field_identity(&mut field_owners, local.id, &owner)?;
                    if remap.fields.insert(local.id, global.id).is_some() {
                        return Err(ParseError::invalid(format!("字段身份 `{}` 在模块 `{}` 中重复", local.id, module.name)));
                    }
                }
            }
            else {
                let nominal =
                    NominalInstanceId::from_index(global_structs.len() as u32).ok_or_else(|| ParseError::invalid("NominalInstanceId 溢出"))?;
                let mut frozen = declaration.clone();
                frozen.nominal = nominal;
                remap.nominals.insert(declaration.nominal, nominal);
                for field in &frozen.fields {
                    claim_field_identity(&mut field_owners, field.id, &owner)?;
                    remap.fields.insert(field.id, field.id);
                }
                global_structs.push(frozen);
            }
        }
        for declaration in &module.sum_types {
            if let Some(existing) =
                global_sums.iter().find(|existing| existing.declaration.is_some() && existing.declaration == declaration.declaration)
            {
                if !same_sum_contract(existing, declaration) {
                    return Err(ParseError::invalid(format!("sum 声明 `{}` 合同冲突", declaration.name)));
                }
                remap.nominals.insert(declaration.nominal, existing.nominal);
                for (local_variant, global_variant) in declaration.variants.iter().zip(&existing.variants) {
                    let owner = sum_field_owner(declaration, local_variant);
                    remap.variants.insert(local_variant.id, global_variant.id);
                    for (local, global) in local_variant.fields.iter().zip(&global_variant.fields) {
                        if local.id != global.id {
                            return Err(ParseError::invalid(format!(
                                "sum 声明 `{}` 变体 `{}` 的字段身份不一致；local={}; global={}",
                                declaration.name, local_variant.name, local.id, global.id
                            )));
                        }
                        claim_field_identity(&mut field_owners, local.id, &owner)?;
                        if remap.fields.insert(local.id, global.id).is_some() {
                            return Err(ParseError::invalid(format!("字段身份 `{}` 在模块 `{}` 中重复", local.id, module.name)));
                        }
                    }
                }
            }
            else {
                let mut frozen = declaration.clone();
                let global_nominal = NominalInstanceId::from_index((global_structs.len() + global_sums.len()) as u32)
                    .ok_or_else(|| ParseError::invalid("NominalInstanceId 溢出"))?;
                remap.nominals.insert(declaration.nominal, global_nominal);
                frozen.nominal = global_nominal;
                for (variant_index, variant) in frozen.variants.iter_mut().enumerate() {
                    let global_variant = nyar_types::VariantId::from_index(
                        global_sums.iter().flat_map(|sum| sum.variants.iter()).count() as u32 + variant_index as u32,
                    )
                    .ok_or_else(|| ParseError::invalid("VariantId 溢出"))?;
                    let owner = sum_field_owner(declaration, variant);
                    remap.variants.insert(variant.id, global_variant);
                    variant.id = global_variant;
                    for field in &variant.fields {
                        claim_field_identity(&mut field_owners, field.id, &owner)?;
                        remap.fields.insert(field.id, field.id);
                    }
                }
                global_sums.push(frozen);
            }
        }
        remaps.push(remap);
    }
    Ok((global_structs, global_sums, remaps))
}

fn struct_field_owner(declaration: &MirStruct) -> String {
    match declaration.declaration {
        Some(identity) => format!("struct#{identity}"),
        None => format!("anonymous:{}", declaration.qualified_name()),
    }
}

fn sum_field_owner(declaration: &MirSumDeclaration, variant: &crate::valkyrie::mir::MirSumVariant) -> String {
    match variant.declaration {
        Some(identity) => format!("variant#{identity}"),
        None => format!("sum:{}::{}", declaration.name, variant.name),
    }
}

fn claim_field_identity(owners: &mut BTreeMap<FieldId, String>, id: FieldId, owner: &str) -> Result<(), ParseError> {
    match owners.get(&id) {
        Some(existing) if existing == owner => Ok(()),
        Some(existing) => Err(ParseError::invalid(format!("字段身份 `{id}` 被 `{owner}` 与 `{existing}` 同时占用"))),
        None => {
            owners.insert(id, owner.to_string());
            Ok(())
        }
    }
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
