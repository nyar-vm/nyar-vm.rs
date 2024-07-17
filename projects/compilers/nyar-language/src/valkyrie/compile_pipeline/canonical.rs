//! 将已完成语义解析的 MIR 生产为 CanonicalProgram。

use std::collections::{BTreeMap, BTreeSet};

use nyar_types::{
    CanonicalArrayInitialization, CanonicalBlock, CanonicalBlockId, CanonicalCallEdge, CanonicalCallee, CanonicalConstant,
    CanonicalExternalCallEdge, CanonicalFragment, CanonicalFunction, CanonicalInstruction, CanonicalMirError, CanonicalOperation,
    CanonicalPrimitiveType, CanonicalProgram, CanonicalSemanticMir, CanonicalTerminator, CanonicalTypeKind, FieldId, FieldRecord, Identifier,
    ImportCapability, ImportIndex, ImportRecord, InstructionId, ItemId, ItemInstanceId, ItemInstanceRecord, LinkedSemanticProgram, MirValueId,
    NominalInstanceId, NominalInstanceRecord, NominalValueSemantics, QualifiedName, StructuredDiagnosticSet, SubstitutionId, TypeId,
    TypeRecord,
    canonical_program::{CanonicalEffectKind, EntryRecord, ExportRecord},
};

use nyar::SemanticFragment;

use crate::valkyrie::{
    mir::{MirConstant, MirFunction, MirModule, MirOperand, MirOperation, MirTerminator, MirValueOrigin},
    semantic_closure::DeclarationKey,
    types::hir::ValkyrieType,
};

use super::diagnostics::fail_stage;

/// 从已完成 HIR/Semantic MIR 合同的模块生成 canonical 成功值。
pub fn canonical_program_from_semantic_mir(module: &MirModule) -> Result<CanonicalProgram, StructuredDiagnosticSet> {
    if !module.diagnostics.is_empty() {
        return Err(error(module, "CAN033", format!("Semantic MIR lowering 失败: {:?}", module.diagnostics)));
    }
    let type_values = collect_types(module)?;
    validate_callable_identities(module)?;
    let (mut nominals, field_records) = collect_aggregate_identities(module, &type_values)?;
    let mut linked = LinkedSemanticProgram { module_name: module.name.clone(), ..LinkedSemanticProgram::default() };
    linked.aggregate_layouts = module.aggregate_layouts.clone();
    for (ty, type_id) in &type_values {
        if let Some(layout_id) = crate::valkyrie::mir::layout_id_for_type(ty, &module.aggregate_layouts) {
            linked.aggregate_layout_by_type.insert(*type_id, layout_id);
        }
    }
    for aggregate in &module.structs {
        let Some(declaration) = aggregate.declaration
        else {
            return Err(error_without_module("CAN023", format!("聚合 `{}` 缺少 declaration identity", aggregate.qualified_name())));
        };
        let Some(layout_id) = linked.aggregate_layouts.declaration_to_layout.get(&declaration).copied()
        else {
            return Err(error_without_module("CAN023", format!("聚合 `{}` 缺少布局绑定", aggregate.qualified_name())));
        };
        linked.aggregate_layout_by_nominal.insert(aggregate.nominal, layout_id);
        let Some(layout) = linked.aggregate_layouts.layouts.iter().find(|layout| layout.id == layout_id)
        else {
            return Err(error_without_module("CAN023", format!("聚合 `{}` 的布局记录缺失", aggregate.qualified_name())));
        };
        if layout.fields.len() != aggregate.fields.len() {
            return Err(error_without_module("CAN023", format!("聚合 `{}` 的字段布局数量不一致", aggregate.qualified_name())));
        }
        for (index, field) in aggregate.fields.iter().enumerate() {
            linked.aggregate_layout_by_field.insert(field.id, (layout_id, index as u32));
        }
    }
    linked.sum_types = module
        .sum_types
        .iter()
        .map(|sum| sum.physical_layout())
        .collect::<Result<_, _>>()
        .map_err(|failure| error(module, "CAN054", format!("sum 物理 payload 类型未完成实例化: {failure}")))?;
    linked.flags_types = module.flags_types.clone();
    linked.singleton_instances = module.singleton_instances.clone();
    for (instance, key, display) in module
        .functions
        .iter()
        .filter_map(|function| function.instance.map(|instance| (instance, function.declaration_key.as_ref(), function.display_symbol.clone())))
        .chain(module.external_calls.iter().filter_map(|contract| {
            contract.instance.map(|instance| (instance, contract.declaration_key.as_ref(), contract.symbol.to_string()))
        }))
    {
        let name = qualified_name_from_declaration_key(key)
            .ok_or_else(|| error(module, "CAN040", format!("callable `{display}` 缺少冻结 declaration_key，无法登记限定名称")))?;
        if let Some(previous) = linked.callable_names.insert(instance, name.clone())
            && previous != name
        {
            return Err(error(module, "CAN041", format!("callable identity `{instance}` 对应多个限定名称")));
        }
    }
    for (ty, id) in &type_values {
        linked.types.insert(*id, TypeRecord { declaration: *id, kind: canonical_type_kind(ty, &type_values)? });
        // `Nominal{usize}` / `Named("usize")` 保持名义 TypeId，不得改写成 Primitive；登记后供
        // ArrayGet/ArrayLength/ArrayNew 合同认整数形状（见 LinkedSemanticProgram::array_index_types）。
        if is_array_index_valkyrie_type(ty, module) {
            linked.array_index_types.insert(*id);
        }
    }
    for (nominal, (declaration, semantics)) in &nominals {
        let nominal_fields = field_records.iter().filter_map(|(field, record)| (record.owner == *nominal).then_some(*field)).collect();
        linked.nominal_instances.insert(
            *nominal,
            NominalInstanceRecord {
                declaration: *declaration,
                ty: *declaration,
                substitution: SubstitutionId::from_index(0).expect("monomorphic substitution"),
                semantics: *semantics,
                fields: nominal_fields,
            },
        );
    }
    let sum_instance_by_type = register_sum_nominal_instances(module, &type_values, &mut linked, &mut nominals)?;
    backfill_nominal_semantics_for_linked_types(&mut linked);
    linked.fields = field_records.clone();
    for function in &module.functions {
        let instance = function
            .instance
            .ok_or_else(|| error(module, "CAN034", format!("函数 `{}` 缺少 Compiler callable identity", function.display_symbol)))?;
        linked.item_instances.insert(
            instance,
            ItemInstanceRecord {
                declaration: function
                    .declaration
                    .ok_or_else(|| error(module, "CAN053", format!("函数 `{}` 缺少声明 identity", function.display_symbol)))?,
                substitution: monomorphic_substitution(function)?,
                parameter_types: function.param_types.iter().map(|ty| type_id(&type_values, ty)).collect::<Result<_, _>>()?,
                return_type: type_id(&type_values, &function.return_type)?,
            },
        );
    }
    for export in &module.exports {
        let symbol = export.symbol.to_string();
        let instance = export.instance.ok_or_else(|| error(module, "CAN036", format!("导出 `{symbol}` 缺少 Compiler callable identity")))?;
        if linked.exports.insert(instance, ExportRecord { exported_name: export.exported_name.clone() }).is_some() {
            return Err(error(module, "CAN037", format!("callable `{symbol}` 存在重复导出合同")));
        }
    }
    for entry in &module.entries {
        let symbol = entry.symbol.to_string();
        let instance = entry.instance.ok_or_else(|| error(module, "CAN038", format!("入口 `{symbol}` 缺少 Compiler callable identity")))?;
        if linked.entries.insert(instance, EntryRecord).is_some() {
            return Err(error(module, "CAN039", format!("callable `{symbol}` 存在重复入口合同")));
        }
    }
    for (offset, contract) in module.external_calls.iter().enumerate() {
        let instance =
            contract.instance.ok_or_else(|| error(module, "CAN034", format!("导入 `{}` 缺少 Compiler callable identity", contract.symbol)))?;
        linked.item_instances.insert(
            instance,
            ItemInstanceRecord {
                declaration: contract
                    .declaration
                    .ok_or_else(|| error(module, "CAN053", format!("导入 `{}` 缺少声明 identity", contract.symbol)))?,
                substitution: SubstitutionId::from_index(0).expect("monomorphic substitution"),
                parameter_types: contract.parameter_types.iter().map(|ty| type_id(&type_values, ty)).collect::<Result<_, _>>()?,
                return_type: type_id(&type_values, &contract.return_type)?,
            },
        );
        let import = ImportIndex::from_index(offset as u32).ok_or_else(|| error_without_module("CAN031", "import identity 溢出"))?;
        let capability = ImportCapability::new(module.name.clone(), contract.symbol.to_string());
        linked.imports.insert(
            import,
            ImportRecord {
                link: contract.link.clone(),
                capability,
                callee: instance,
                parameter_types: contract.parameter_types.iter().map(|ty| type_id(&type_values, ty)).collect::<Result<_, _>>()?,
                return_type: type_id(&type_values, &contract.return_type)?,
            },
        );
    }
    let mut next_instruction = 0u32;
    let functions = module
        .functions
        .iter()
        .map(|function| {
            let instance = function
                .instance
                .ok_or_else(|| error(module, "CAN034", format!("函数 `{}` 缺少 Compiler callable identity", function.display_symbol)))?;
            Ok((
                instance,
                lower_function(function, instance, &type_values, &nominals, &field_records, &sum_instance_by_type, &mut next_instruction)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, StructuredDiagnosticSet>>()?;
    linked.fragments = canonical_fragments(module, &linked, &functions, &module.semantic_fragments)?;
    let program = CanonicalProgram { linked, mir: CanonicalSemanticMir { module_name: module.name.clone(), functions } };
    program.validate().map_err(|error| canonical_error(module, &program, error))?;
    Ok(program)
}

fn canonical_fragments(
    module: &MirModule,
    linked: &LinkedSemanticProgram,
    functions: &BTreeMap<ItemInstanceId, CanonicalFunction>,
    fragments: &[SemanticFragment],
) -> Result<BTreeMap<Identifier, CanonicalFragment>, StructuredDiagnosticSet> {
    let mut result = BTreeMap::new();
    for fragment in fragments {
        let mut exported_operations = fragment.exported_operations.clone();
        for instance in &exported_operations {
            if !linked.item_instances.contains_key(instance) {
                return Err(error(module, "CAN042", format!("片段 {} 的操作缺少 callable 合同", fragment.id)));
            }
        }
        let entry_operation = fragment.entry_operation;
        if let Some(instance) = entry_operation {
            if !linked.item_instances.contains_key(&instance) {
                return Err(error(module, "CAN043", format!("片段 {} 的入口缺少 callable 合同", fragment.id)));
            }
        }
        let wasm_export_names = fragment.wasm_export_names.clone();
        let mut external_imports = BTreeMap::new();
        let mut internal_call_edges = Vec::new();
        let mut external_call_edges = Vec::new();
        let mut imports_by_instance = BTreeMap::new();
        for (index, record) in &linked.imports {
            if imports_by_instance.insert(record.callee, (*index, record)).is_some() {
                return Err(error(module, "CAN049", "导入实例绑定多个 ImportIndex"));
            }
        }
        let mut pending = exported_operations.clone();
        let mut visited = BTreeSet::new();
        while let Some(caller) = pending.pop() {
            if !visited.insert(caller) {
                continue;
            }
            if let Some((_, record)) = imports_by_instance.get(&caller) {
                if functions.contains_key(&caller) {
                    return Err(error(module, "CAN049", "同一实例同时绑定导入与函数体"));
                }
                external_imports.insert(caller, record.link.clone());
                continue;
            }
            let function = functions
                .get(&caller)
                .ok_or_else(|| error(module, "CAN045", format!("片段 {} 的实例 {caller:?} 没有函数体或导入", fragment.id)))?;
            let literals = function
                .blocks
                .values()
                .flat_map(|block| &block.instructions)
                .filter_map(|instruction| {
                    if let CanonicalOperation::LoadConstant { constant: CanonicalConstant::Utf8(text) } = &instruction.operation {
                        instruction.results.first().map(|value| (*value, text.clone()))
                    }
                    else {
                        None
                    }
                })
                .collect::<BTreeMap<_, _>>();
            for instruction in function.blocks.values().flat_map(|block| &block.instructions) {
                let CanonicalOperation::Invoke { callee: CanonicalCallee::Item(callee), arguments } = &instruction.operation
                else {
                    continue;
                };
                if let Some((import, record)) = imports_by_instance.get(callee) {
                    if functions.contains_key(callee) {
                        return Err(error(module, "CAN049", "同一实例同时绑定导入与函数体"));
                    }
                    external_imports.insert(*callee, record.link.clone());
                    external_call_edges.push(CanonicalExternalCallEdge {
                        caller,
                        import: *import,
                        arguments: arguments
                            .iter()
                            .filter_map(|value| literals.get(value).cloned().map(nyar_types::ExternalCallArgument::StringLiteral))
                            .collect(),
                    });
                }
                else {
                    if !functions.contains_key(callee) {
                        return Err(error(module, "CAN046", format!("调用实例 {callee:?} 没有函数体或显式导入")));
                    }
                    internal_call_edges.push(CanonicalCallEdge { caller, callee: *callee });
                    pending.push(*callee);
                    if !exported_operations.contains(callee) {
                        exported_operations.push(*callee);
                    }
                }
            }
        }
        if result
            .insert(
                fragment.id.clone(),
                CanonicalFragment {
                    id: fragment.id.clone(),
                    exported_operations,
                    required_capabilities: fragment.required_capabilities.clone(),
                    entry_operation,
                    external_imports,
                    external_call_edges,
                    internal_call_edges,
                    wasm_export_names,
                },
            )
            .is_some()
        {
            return Err(error(module, "CAN051", format!("片段 identity `{}` 重复", fragment.id)));
        }
    }
    Ok(result)
}

type AggregateIdentity = (TypeId, NominalValueSemantics);

fn collect_aggregate_identities(
    module: &MirModule,
    types: &BTreeMap<ValkyrieType, TypeId>,
) -> Result<(BTreeMap<NominalInstanceId, AggregateIdentity>, BTreeMap<FieldId, FieldRecord>), StructuredDiagnosticSet> {
    let mut nominals = BTreeMap::new();
    let mut field_records = BTreeMap::new();
    for aggregate in &module.structs {
        let qualified =
            if aggregate.namespace.is_empty() { aggregate.name.clone() } else { format!("{}.{}", aggregate.namespace, aggregate.name) };
        let item =
            aggregate.declaration.ok_or_else(|| error_without_module("CAN018", format!("聚合 `{qualified}` 缺少 declaration ItemId")))?;
        let ty = ValkyrieType::Nominal { declaration: item, arguments: Vec::new() };
        let declaration = types.get(&ty).copied().ok_or_else(|| error_without_module("CAN018", format!("聚合 `{qualified}` 缺少类型事实")))?;
        let nominal = aggregate.nominal;
        let semantics = if aggregate.is_value_type { NominalValueSemantics::Value } else { NominalValueSemantics::Reference };
        if nominals.insert(nominal, (declaration, semantics)).is_some() {
            return Err(error_without_module("CAN020", format!("聚合 identity 重复: {qualified}")));
        }
        for field in &aggregate.fields {
            if field_records.insert(field.id, FieldRecord { owner: nominal, ty: type_id(types, &field.ty)? }).is_some() {
                return Err(error_without_module("CAN022", format!("字段 identity 重复: {qualified}.{}", field.name)));
            }
        }
    }
    for sum in &module.sum_types {
        let item = sum.declaration.ok_or_else(|| error_without_module("CAN018", format!("sum `{}` 缺少 declaration ItemId", sum.name)))?;
        let ty = ValkyrieType::Nominal { declaration: item, arguments: Vec::new() };
        let declaration = types.get(&ty).copied().ok_or_else(|| error_without_module("CAN018", format!("sum `{}` 缺少类型事实", sum.name)))?;
        if nominals.insert(sum.nominal, (declaration, NominalValueSemantics::Reference)).is_some() {
            return Err(error_without_module("CAN020", format!("sum identity 重复: {}", sum.name)));
        }
    }
    Ok((nominals, field_records))
}

fn validate_callable_identities(module: &MirModule) -> Result<(), StructuredDiagnosticSet> {
    let mut instances = BTreeMap::new();
    for function in &module.functions {
        let Some(identity) = function.instance
        else {
            return Err(error(module, "CAN034", format!("函数 `{}` 缺少 Compiler callable identity", function.display_symbol)));
        };
        if instances.insert(identity, function.display_symbol.clone()).is_some() {
            return Err(error(module, "CAN001", format!("callable identity `{identity}` 重复")));
        }
    }
    for contract in &module.external_calls {
        let symbol = contract.symbol.to_string();
        let Some(identity) = contract.instance
        else {
            return Err(error(module, "CAN034", format!("导入 `{symbol}` 缺少 Compiler callable identity")));
        };
        if instances.insert(identity, symbol.clone()).is_some() {
            return Err(error(module, "CAN001", format!("callable identity `{identity}` 重复")));
        }
    }
    Ok(())
}

fn collect_types(module: &MirModule) -> Result<BTreeMap<ValkyrieType, TypeId>, StructuredDiagnosticSet> {
    if module.type_identities.is_empty() && (!module.functions.is_empty() || !module.external_calls.is_empty() || !module.structs.is_empty()) {
        return Err(error(module, "CAN035", "Semantic MIR 缺少 Compiler type identity 表"));
    }
    Ok(module.type_identities.clone())
}

/// 语言侧可作为数组下标 / `ArrayLength` 结果的类型：原生整数，或 `usize`/`isize` 等名义原语。
///
/// 登记进 `array_index_types` 后，Canonical 用 TypeId 集合认整数形状，不把 `Nominal{usize}` 改写成 Primitive。
fn is_array_index_valkyrie_type(ty: &ValkyrieType, module: &MirModule) -> bool {
    match ty {
        ValkyrieType::Integer8 { .. }
        | ValkyrieType::Integer16 { .. }
        | ValkyrieType::Integer32 { .. }
        | ValkyrieType::Integer64 { .. }
        | ValkyrieType::Integer128 { .. } => true,
        ValkyrieType::Named(name) => is_array_index_surface_name(name.as_str()),
        ValkyrieType::Nominal { declaration, arguments } if arguments.is_empty() => module
            .structs
            .iter()
            .find(|item| item.declaration == Some(*declaration))
            .is_some_and(|item| is_array_index_surface_name(item.name.as_str())),
        _ => false,
    }
}

fn is_array_index_surface_name(name: &str) -> bool {
    matches!(
        name,
        "usize"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "byte"
            | "sbyte"
            | "core.primitive.usize"
            | "core.primitive.isize"
            | "core::primitive::usize"
            | "core::primitive::isize"
    )
}

fn canonical_type_kind(ty: &ValkyrieType, ids: &BTreeMap<ValkyrieType, TypeId>) -> Result<CanonicalTypeKind, StructuredDiagnosticSet> {
    let id = |value: &ValkyrieType| type_id(ids, value);
    Ok(match ty {
        ValkyrieType::Void => CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Void),
        ValkyrieType::Unit => CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Unit),
        ValkyrieType::Boolean => CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Bool),
        ValkyrieType::Integer8 { signed } => CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { bits: 8, signed: *signed }),
        ValkyrieType::Integer16 { signed } => CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { bits: 16, signed: *signed }),
        ValkyrieType::Integer32 { signed } => CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { bits: 32, signed: *signed }),
        ValkyrieType::Integer64 { signed } => CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { bits: 64, signed: *signed }),
        ValkyrieType::Integer128 { signed } => CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { bits: 128, signed: *signed }),
        ValkyrieType::Float32 => CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Float { bits: 32 }),
        ValkyrieType::Float64 => CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Float { bits: 64 }),
        ValkyrieType::Character => CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Character),
        ValkyrieType::Utf8 => CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Utf8),
        ValkyrieType::Utf16 => CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Utf16),
        ValkyrieType::Nominal { declaration: item, arguments } => {
            let declaration_ty = ValkyrieType::Nominal { declaration: *item, arguments: Vec::new() };
            let declaration = type_id(ids, &declaration_ty)?;
            CanonicalTypeKind::Nominal { declaration, arguments: arguments.iter().map(id).collect::<Result<_, _>>()? }
        }
        ValkyrieType::Named(_) => CanonicalTypeKind::Nominal { declaration: ids[ty], arguments: Vec::new() },
        ValkyrieType::Apply(base, args) => {
            CanonicalTypeKind::Nominal { declaration: id(base)?, arguments: args.iter().map(id).collect::<Result<_, _>>()? }
        }
        ValkyrieType::Tuple(items) => CanonicalTypeKind::Tuple(items.iter().map(id).collect::<Result<_, _>>()?),
        ValkyrieType::Array(element) => CanonicalTypeKind::Array { element: id(element)?, length: None },
        ValkyrieType::FixedArray { element, length } => CanonicalTypeKind::Array { element: id(element)?, length: Some(*length as u64) },
        ValkyrieType::Nullable(element) => CanonicalTypeKind::Nullable(id(element)?),
        ValkyrieType::Union(items) => CanonicalTypeKind::Union(items.iter().map(id).collect::<Result<_, _>>()?),
        ValkyrieType::Intersection(items) => CanonicalTypeKind::Intersection(items.iter().map(id).collect::<Result<_, _>>()?),
        ValkyrieType::Function(function) => CanonicalTypeKind::Function {
            parameters: function.params.iter().map(id).collect::<Result<_, _>>()?,
            return_type: id(&function.return_type)?,
        },
        _ => {
            return Err(error_without_module("CAN003", format!("类型没有无损 canonical 形状: {ty:?}")));
        }
    })
}

/// 为每个具体 `Nominal{ItemId, args}` 注册 sum 实例与已代入 payload；SumNew 按结果类型选取实例。
fn register_sum_nominal_instances(
    module: &MirModule,
    type_values: &BTreeMap<ValkyrieType, TypeId>,
    linked: &mut LinkedSemanticProgram,
    nominals: &mut BTreeMap<NominalInstanceId, AggregateIdentity>,
) -> Result<BTreeMap<TypeId, NominalInstanceId>, StructuredDiagnosticSet> {
    let mut by_type = BTreeMap::new();
    let mut next_nominal = nominals.keys().map(|id| id.index()).max().map(|index| index + 1).unwrap_or(0);
    for sum in &module.sum_types {
        let item = sum.declaration.ok_or_else(|| error_without_module("CAN018", format!("sum `{}` 缺少 declaration ItemId", sum.name)))?;
        let declaration_ty = ValkyrieType::Nominal { declaration: item, arguments: Vec::new() };
        let declaration = type_id(type_values, &declaration_ty)?;
        linked.nominal_instances.entry(sum.nominal).or_insert(NominalInstanceRecord {
            declaration,
            ty: declaration,
            substitution: SubstitutionId::from_index(0).expect("monomorphic substitution"),
            semantics: NominalValueSemantics::Reference,
            fields: Vec::new(),
        });
        // 开放/泛型声明侧不登记未代入 payload；具体实例下方补齐。
        for variant in &sum.variants {
            let payload_type =
                if sum.generics.is_empty() { variant.payload_type().map(|ty| type_id(type_values, &ty)).transpose()? } else { None };
            linked.variants.insert((sum.nominal, variant.id), nyar_types::VariantRecord { payload_type });
        }
        by_type.insert(declaration, sum.nominal);
    }
    for (ty, concrete_ty) in type_values {
        let ValkyrieType::Nominal { declaration: item, arguments } = ty
        else {
            continue;
        };
        if arguments.is_empty() || by_type.contains_key(concrete_ty) {
            continue;
        }
        let Some(sum) = module.sum_types.iter().find(|sum| sum.declaration == Some(*item))
        else {
            continue;
        };
        let declaration = type_id(type_values, &ValkyrieType::Nominal { declaration: *item, arguments: Vec::new() })?;
        let nominal =
            NominalInstanceId::from_index(next_nominal).ok_or_else(|| error_without_module("CAN020", "sum instance identity 溢出"))?;
        next_nominal = next_nominal.checked_add(1).ok_or_else(|| error_without_module("CAN020", "sum instance identity 溢出"))?;
        if nominals.insert(nominal, (declaration, NominalValueSemantics::Reference)).is_some() {
            return Err(error_without_module("CAN020", format!("sum instance identity 重复: {nominal:?}")));
        }
        linked.nominal_instances.insert(
            nominal,
            NominalInstanceRecord {
                declaration,
                ty: *concrete_ty,
                substitution: SubstitutionId::from_index(0).expect("monomorphic substitution"),
                semantics: NominalValueSemantics::Reference,
                fields: Vec::new(),
            },
        );
        let mut layout_variants = Vec::with_capacity(sum.variants.len());
        for variant in &sum.variants {
            let instantiated = sum
                .instantiate_payload(variant, arguments)
                .ok_or_else(|| error_without_module("CAN018", format!("sum `{}` 实例化 payload 失败", sum.name)))?;
            let payload_type = instantiated.as_ref().map(|ty| type_id(type_values, ty)).transpose()?;
            linked.variants.insert((nominal, variant.id), nyar_types::VariantRecord { payload_type });
            // 布局侧仍写 NyarType；语义身份只认上方 VariantRecord 的 TypeId。
            let layout_payload = instantiated
                .as_ref()
                .map(crate::frontend_contract::concretize_type)
                .transpose()
                .map_err(|failure| error_without_module("CAN054", format!("sum `{}` 实例 payload 无法 concretize: {failure}", sum.name)))?;
            layout_variants.push(nyar_types::SumVariantLayout {
                id: variant.id,
                name: variant.name.clone(),
                tag: variant.tag,
                payload_type: layout_payload,
            });
        }
        linked.sum_types.push(nyar_types::SumTypeLayout {
            nominal,
            name: sum.name.clone(),
            is_unite: sum.is_unite,
            tag_width: 4,
            variants: layout_variants,
        });
        by_type.insert(*concrete_ty, nominal);
    }
    Ok(by_type)
}

fn lower_function(
    function: &MirFunction,
    instance: ItemInstanceId,
    ids: &BTreeMap<ValkyrieType, TypeId>,
    nominals: &BTreeMap<NominalInstanceId, AggregateIdentity>,
    fields: &BTreeMap<FieldId, FieldRecord>,
    sum_instance_by_type: &BTreeMap<TypeId, NominalInstanceId>,
    next_instruction: &mut u32,
) -> Result<CanonicalFunction, StructuredDiagnosticSet> {
    let value_types = function
        .value_types
        .iter()
        .map(|(value, ty)| {
            Ok((MirValueId::from_index(value.0).ok_or_else(|| error_without_module("CAN004", "SSA value identity 溢出"))?, type_id(ids, ty)?))
        })
        .collect::<Result<BTreeMap<_, _>, StructuredDiagnosticSet>>()?;
    let parameters = canonical_entry_parameters(function, ids)?;
    let blocks = function
        .blocks
        .iter()
        .map(|block| {
            let id = CanonicalBlockId(block.id.0);
            let parameters = block
                .parameters
                .iter()
                .map(|value| {
                    let value = MirValueId::from_index(value.0).ok_or_else(|| error_without_module("CAN004", "SSA value identity 溢出"))?;
                    Ok((value, *value_types.get(&value).ok_or_else(|| error_without_module("CAN005", "块参数缺少类型事实"))?))
                })
                .collect::<Result<_, StructuredDiagnosticSet>>()?;
            let instructions = block
                .instructions
                .iter()
                .map(|instruction| {
                    let id = nyar_types::InstructionId::from_index(*next_instruction)
                        .ok_or_else(|| error_without_module("CAN016", "instruction identity 溢出"))?;
                    *next_instruction =
                        (*next_instruction).checked_add(1).ok_or_else(|| error_without_module("CAN016", "instruction identity 溢出"))?;
                    Ok(CanonicalInstruction {
                        id,
                        results: instruction
                            .results
                            .iter()
                            .map(|value| {
                                MirValueId::from_index(value.0).ok_or_else(|| error_without_module("CAN004", "SSA value identity 溢出"))
                            })
                            .collect::<Result<_, _>>()?,
                        operation: lower_operation(
                            &instruction.kind,
                            &instruction.results,
                            &function.value_types,
                            ids,
                            nominals,
                            fields,
                            sum_instance_by_type,
                        )?,
                    })
                })
                .collect::<Result<_, StructuredDiagnosticSet>>()?;
            Ok((id, CanonicalBlock { id, parameters, instructions, terminator: lower_terminator(&block.terminator)? }))
        })
        .collect::<Result<BTreeMap<_, _>, StructuredDiagnosticSet>>()?;
    Ok(CanonicalFunction {
        instance,
        parameters,
        return_type: type_id(ids, &function.return_type)?,
        value_types,
        entry: CanonicalBlockId(function.entry.0),
        blocks,
    })
}

fn canonical_entry_parameters(
    function: &MirFunction,
    ids: &BTreeMap<ValkyrieType, TypeId>,
) -> Result<Vec<(MirValueId, TypeId)>, StructuredDiagnosticSet> {
    let invalid = || error_without_module("CAN032", format!("函数 `{}` 的声明参数、SSA 参数来源与入口块参数不一致", function.display_symbol));
    let entry = function.blocks.iter().find(|block| block.id == function.entry).ok_or_else(invalid)?;
    let mut origins = BTreeMap::new();
    for value in &function.values {
        if let MirValueOrigin::Parameter { index, .. } = value.origin {
            if function.param_types.get(index).is_none() || origins.insert(index, value.id).is_some() {
                return Err(invalid());
            }
        }
    }
    if origins.len() != function.param_types.len() || entry.parameters.len() != function.param_types.len() {
        return Err(invalid());
    }
    function
        .param_types
        .iter()
        .enumerate()
        .map(|(index, ty)| {
            let value = origins.get(&index).ok_or_else(invalid)?;
            if entry.parameters.get(index) != Some(value) || function.value_types.get(value) != Some(ty) {
                return Err(invalid());
            }
            Ok((MirValueId::from_index(value.0).ok_or_else(|| error_without_module("CAN004", "SSA value identity 溢出"))?, type_id(ids, ty)?))
        })
        .collect()
}

fn resolve_sum_nominal(
    mir_nominal: NominalInstanceId,
    shape: Option<&ValkyrieType>,
    ids: &BTreeMap<ValkyrieType, TypeId>,
    sum_instance_by_type: &BTreeMap<TypeId, NominalInstanceId>,
) -> Result<NominalInstanceId, StructuredDiagnosticSet> {
    let Some(ty) = shape
    else {
        return Ok(mir_nominal);
    };
    let concrete = type_id(ids, ty)?;
    Ok(sum_instance_by_type.get(&concrete).copied().unwrap_or(mir_nominal))
}

fn lower_operation(
    operation: &MirOperation,
    results: &[crate::valkyrie::mir::MirValueRef],
    value_types: &BTreeMap<crate::valkyrie::mir::MirValueRef, ValkyrieType>,
    ids: &BTreeMap<ValkyrieType, TypeId>,
    nominals: &BTreeMap<NominalInstanceId, AggregateIdentity>,
    fields: &BTreeMap<FieldId, FieldRecord>,
    sum_instance_by_type: &BTreeMap<TypeId, NominalInstanceId>,
) -> Result<CanonicalOperation, StructuredDiagnosticSet> {
    let value = |operand: &MirOperand| match operand {
        MirOperand::Value(value) => MirValueId::from_index(value.0).ok_or_else(|| error_without_module("CAN004", "SSA value identity 溢出")),
        _ => Err(error_without_module("CAN006", "操作数不是已定义 SSA 值")),
    };
    match operation {
        MirOperation::Call { callee: MirOperand::Callable(identity), arguments } => Ok(CanonicalOperation::Invoke {
            callee: CanonicalCallee::Item(*identity),
            arguments: arguments.iter().map(value).collect::<Result<_, _>>()?,
        }),
        MirOperation::Call { callee: MirOperand::Symbol(_), .. } => {
            Err(error_without_module("CAN007", "Semantic MIR 仍包含未冻结的 callable 名称"))
        }
        MirOperation::Call { callee: MirOperand::Value(callee), arguments } => Ok(CanonicalOperation::Invoke {
            callee: CanonicalCallee::Value(
                MirValueId::from_index(callee.0).ok_or_else(|| error_without_module("CAN008", "函数值 callee identity 溢出"))?,
            ),
            arguments: arguments.iter().map(value).collect::<Result<_, _>>()?,
        }),
        MirOperation::StoreVar { name, value: stored, ty } => Ok(CanonicalOperation::StoreVar {
            name: name.clone(),
            value: value(stored)?,
            ty: ty.as_ref().map(|ty| type_id(ids, ty)).transpose()?,
        }),
        MirOperation::Copy { source } => Ok(CanonicalOperation::Copy { source: value(source)? }),
        MirOperation::Cast { source, ty } => Ok(CanonicalOperation::Cast { source: value(source)?, ty: type_id(ids, ty)? }),
        MirOperation::AggregateCopy { source, dest } => {
            Ok(CanonicalOperation::AggregateCopy { source: value(source)?, destination: value(dest)? })
        }
        MirOperation::LoadConstant { constant, .. } => Ok(CanonicalOperation::LoadConstant { constant: lower_constant(constant)? }),
        MirOperation::ArrayGet { array, index } => Ok(CanonicalOperation::ArrayGet { array: value(array)?, index: value(index)? }),
        MirOperation::ArraySet { array, index, value: stored } => {
            Ok(CanonicalOperation::ArraySet { array: value(array)?, index: value(index)?, value: value(stored)? })
        }
        MirOperation::ArrayLength { array } => Ok(CanonicalOperation::ArrayLength { array: value(array)? }),
        MirOperation::ArrayPush { array, value: pushed } => Ok(CanonicalOperation::ArrayPush { array: value(array)?, value: value(pushed)? }),
        MirOperation::NullIsNull { value: operand } => Ok(CanonicalOperation::NullIsNull { value: value(operand)? }),
        MirOperation::NullUnwrap { value: operand } => Ok(CanonicalOperation::NullUnwrap { value: value(operand)? }),
        MirOperation::ArrayNew { array_type, length, initialization } => Ok(CanonicalOperation::ArrayNew {
            array_type: type_id(ids, array_type)?,
            length: value(length)?,
            initialization: match initialization {
                crate::valkyrie::mir::ArrayInitialization::Default => CanonicalArrayInitialization::Default,
                crate::valkyrie::mir::ArrayInitialization::Fill(fill) => CanonicalArrayInitialization::Fill(value(fill)?),
            },
        }),
        MirOperation::ArrayFromElements { array_type, elements } => Ok(CanonicalOperation::ArrayFromElements {
            array_type: type_id(ids, array_type)?,
            elements: elements.iter().map(value).collect::<Result<_, _>>()?,
        }),
        MirOperation::TupleNew { fields } => Ok(CanonicalOperation::TupleNew {
            element_types: match results.first().and_then(|result| value_types.get(result)) {
                Some(ValkyrieType::Tuple(types)) => types.iter().map(|ty| type_id(ids, ty)).collect::<Result<_, _>>()?,
                _ => {
                    return Err(error_without_module("CAN017", "元组构造结果缺少完整 Tuple 类型"));
                }
            },
            fields: fields.iter().map(value).collect::<Result<_, _>>()?,
        }),
        MirOperation::SumNew { nominal, variant, payload_type, payload, .. } => {
            let result_ty = results.first().and_then(|result| value_types.get(result));
            let concrete = resolve_sum_nominal(*nominal, result_ty, ids, sum_instance_by_type)?;
            if !nominals.contains_key(&concrete) {
                return Err(error_without_module("CAN031", format!("未解析 sum nominal identity: {concrete:?}")));
            }
            let _ = payload_type;
            Ok(CanonicalOperation::SumNew { nominal: concrete, variant: *variant, payload: payload.as_ref().map(value).transpose()? })
        }
        MirOperation::SumPayloadGet { nominal, variant, object, .. } => {
            let object_ty = match object {
                MirOperand::Value(value) => value_types.get(value),
                _ => None,
            };
            let concrete = resolve_sum_nominal(*nominal, object_ty, ids, sum_instance_by_type)?;
            if !nominals.contains_key(&concrete) {
                return Err(error_without_module("CAN031", format!("未解析 sum nominal identity: {concrete:?}")));
            }
            Ok(CanonicalOperation::SumPayloadGet { nominal: concrete, variant: *variant, object: value(object)? })
        }
        MirOperation::SumVariantIs { nominal, variant, object, .. } => {
            let object_ty = match object {
                MirOperand::Value(value) => value_types.get(value),
                _ => None,
            };
            let concrete = resolve_sum_nominal(*nominal, object_ty, ids, sum_instance_by_type)?;
            if !nominals.contains_key(&concrete) {
                return Err(error_without_module("CAN031", format!("未解析 sum nominal identity: {concrete:?}")));
            }
            Ok(CanonicalOperation::SumVariantIs { nominal: concrete, variant: *variant, object: value(object)? })
        }
        MirOperation::StructNew { nominal, fields: values } => {
            if !nominals.contains_key(nominal) {
                return Err(error_without_module("CAN023", format!("未解析聚合 identity: {nominal:?}")));
            }
            let fields = values
                .iter()
                .map(|(field, operand)| {
                    if !fields.contains_key(field) {
                        return Err(error_without_module("CAN024", format!("未解析字段 identity: {field:?}")));
                    }
                    Ok((*field, value(operand)?))
                })
                .collect::<Result<_, StructuredDiagnosticSet>>()?;
            Ok(CanonicalOperation::StructNew { nominal: *nominal, fields })
        }
        MirOperation::FieldGet { object, field } => {
            if !fields.contains_key(field) {
                return Err(error_without_module("CAN027", format!("未解析字段 identity: {field:?}")));
            }
            Ok(CanonicalOperation::FieldGet { object: value(object)?, field: *field })
        }
        MirOperation::FieldSet { object, field, value: stored } => {
            if !fields.contains_key(field) {
                return Err(error_without_module("CAN030", format!("未解析字段 identity: {field:?}")));
            }
            Ok(CanonicalOperation::FieldSet { object: value(object)?, field: *field, value: value(stored)? })
        }
        _ => Err(error_without_module("CAN009", format!("操作没有无损 canonical 形状: {operation:?}"))),
    }
}

fn aggregate_owner(ty: &ValkyrieType) -> Option<String> {
    match ty {
        ValkyrieType::Named(name) => Some(name.to_string()),
        ValkyrieType::Apply(base, _) => aggregate_owner(base),
        _ => None,
    }
}

fn lower_terminator(terminator: &MirTerminator) -> Result<CanonicalTerminator, StructuredDiagnosticSet> {
    let value = |operand: &MirOperand| match operand {
        MirOperand::Value(value) => MirValueId::from_index(value.0).ok_or_else(|| error_without_module("CAN004", "SSA value identity 溢出")),
        _ => Err(error_without_module("CAN010", "终结符操作数不是 SSA 值")),
    };
    match terminator {
        MirTerminator::Return { value: Some(value_operand) } => Ok(CanonicalTerminator::Return { value: Some(value(value_operand)?) }),
        MirTerminator::Return { value: None } => Ok(CanonicalTerminator::Return { value: None }),
        MirTerminator::Jump { target, arguments } => Ok(CanonicalTerminator::Jump {
            target: CanonicalBlockId(target.0),
            arguments: arguments.iter().map(value).collect::<Result<_, _>>()?,
        }),
        MirTerminator::Branch { condition, then_target, else_target } => Ok(CanonicalTerminator::Branch {
            condition: value(condition)?,
            then_target: CanonicalBlockId(then_target.0),
            else_target: CanonicalBlockId(else_target.0),
        }),
        MirTerminator::PerformEffect { effect, payload, resume_target } => Ok(CanonicalTerminator::PerformEffect {
            effect: lower_effect(*effect),
            payload: payload.as_ref().map(value).transpose()?,
            resume_target: CanonicalBlockId(resume_target.0),
        }),
        MirTerminator::StateDispatch { state, cases, default_target } => Ok(CanonicalTerminator::StateDispatch {
            state: MirValueId::from_index(state.0).ok_or_else(|| error_without_module("CAN004", "SSA value identity 溢出"))?,
            cases: cases.iter().map(|(state, target)| (*state, CanonicalBlockId(target.0))).collect(),
            default_target: CanonicalBlockId(default_target.0),
        }),
        MirTerminator::YieldToRuntime { effect, payload, resume_state } => Ok(CanonicalTerminator::YieldToRuntime {
            effect: lower_effect(*effect),
            payload: payload.as_ref().map(value).transpose()?,
            resume_state: *resume_state,
        }),
        MirTerminator::Unreachable => Ok(CanonicalTerminator::Unreachable),
    }
}

fn lower_effect(effect: crate::valkyrie::mir::MirEffectKind) -> CanonicalEffectKind {
    use crate::valkyrie::mir::MirEffectKind;
    match effect {
        MirEffectKind::Raise => CanonicalEffectKind::Raise,
        MirEffectKind::Yield => CanonicalEffectKind::Yield,
        MirEffectKind::DelegateYield => CanonicalEffectKind::DelegateYield,
        MirEffectKind::Await => CanonicalEffectKind::Await,
        MirEffectKind::AsyncSpawn => CanonicalEffectKind::AsyncSpawn,
        MirEffectKind::AsyncBlock => CanonicalEffectKind::AsyncBlock,
    }
}

fn lower_constant(constant: &MirConstant) -> Result<CanonicalConstant, StructuredDiagnosticSet> {
    match constant {
        MirConstant::Int(value) => Ok(CanonicalConstant::Int(*value)),
        MirConstant::Bool(value) => Ok(CanonicalConstant::Bool(*value)),
        MirConstant::Utf8(value) => Ok(CanonicalConstant::Utf8(value.clone())),
        MirConstant::Utf16(value) => Ok(CanonicalConstant::Utf16(value.clone())),
        MirConstant::Unit => Ok(CanonicalConstant::Unit),
        MirConstant::Float64(_) => Err(error_without_module("CAN012", "浮点常量没有 canonical 形状")),
    }
}

fn backfill_nominal_semantics_for_linked_types(linked: &mut LinkedSemanticProgram) {
    let mut pending = Vec::new();
    for (type_id, record) in &linked.types {
        if let CanonicalTypeKind::Nominal { declaration, .. } = &record.kind {
            if linked.nominal_instances.values().any(|instance| instance.ty == *type_id) {
                continue;
            }
            pending.push((*type_id, *declaration));
        }
    }
    for (type_id, declaration) in pending {
        let nominal = NominalInstanceId::from_index(linked.nominal_instances.len() as u32).expect("nominal identity overflow");
        linked.nominal_instances.insert(
            nominal,
            NominalInstanceRecord {
                declaration,
                ty: type_id,
                substitution: SubstitutionId::from_index(0).expect("monomorphic substitution"),
                semantics: NominalValueSemantics::Reference,
                fields: Vec::new(),
            },
        );
    }
}

fn monomorphic_substitution(function: &MirFunction) -> Result<SubstitutionId, StructuredDiagnosticSet> {
    if function
        .param_types
        .iter()
        .chain(std::iter::once(&function.return_type))
        .any(crate::valkyrie::mir::ssa::type_contains_non_monomorphic_signature_reference)
    {
        Err(error_without_module("CAN013", format!("函数 `{}` 仍包含未代入类型参数", function.display_symbol)))
    }
    else {
        Ok(SubstitutionId::from_index(0).expect("monomorphic substitution"))
    }
}
fn type_id(ids: &BTreeMap<ValkyrieType, TypeId>, ty: &ValkyrieType) -> Result<TypeId, StructuredDiagnosticSet> {
    ids.get(ty).copied().ok_or_else(|| error_without_module("CAN014", "类型事实未进入 canonical table"))
}
fn item_id(index: u32) -> ItemId {
    ItemId::from_index(index).expect("item identity overflow")
}
fn error_without_module(code: &'static str, message: impl Into<String>) -> StructuredDiagnosticSet {
    fail_stage::<()>(nyar_types::CompileStage::SemanticMir, code, "", message).err().unwrap()
}
fn error(module: &MirModule, code: &'static str, message: impl Into<String>) -> StructuredDiagnosticSet {
    fail_stage::<()>(nyar_types::CompileStage::SemanticMir, code, &module.name, message).err().unwrap()
}
fn canonical_error(module: &MirModule, program: &CanonicalProgram, error: CanonicalMirError) -> StructuredDiagnosticSet {
    let detail = match &error {
        CanonicalMirError::OperationResultTypeMismatch { function, instruction } => {
            format!("canonical MIR 校验失败: {error:?}{}", describe_operation_result_mismatch(module, program, *function, *instruction))
        }
        CanonicalMirError::CallResultTypeMismatch { function, callee, value } => {
            format!("canonical MIR 校验失败: {error:?}{}", describe_call_result_mismatch(module, program, *function, *callee, *value))
        }
        _ => format!("canonical MIR 校验失败: {error:?}"),
    };
    fail_stage::<()>(nyar_types::CompileStage::ValidateMir, "CAN015", &module.name, detail).err().unwrap()
}

fn describe_call_result_mismatch(
    module: &MirModule,
    program: &CanonicalProgram,
    function: ItemInstanceId,
    callee: ItemInstanceId,
    value: MirValueId,
) -> String {
    let caller =
        module.functions.iter().find(|item| item.instance == Some(function)).map(|item| item.display_symbol.as_str()).unwrap_or("<unknown>");
    let callee_symbol = module
        .functions
        .iter()
        .find(|item| item.instance == Some(callee))
        .map(|item| item.display_symbol.clone())
        .or_else(|| program.linked.callable_names.get(&callee).map(|name| name.to_string()))
        .unwrap_or_else(|| "<unknown>".to_string());
    let actual = program
        .mir
        .functions
        .get(&function)
        .and_then(|func| func.value_types.get(&value))
        .and_then(|ty| program.linked.types.get(ty).map(|row| (&row.kind, *ty)));
    let expected = program.linked.item_instances.get(&callee).map(|record| {
        let kind = program.linked.types.get(&record.return_type).map(|row| &row.kind);
        (kind, record.return_type)
    });
    format!(" (caller=`{caller}` callee=`{callee_symbol}` actual={actual:?} expected_return={expected:?})")
}

fn describe_operation_result_mismatch(
    module: &MirModule,
    program: &CanonicalProgram,
    function: ItemInstanceId,
    instruction: InstructionId,
) -> String {
    let symbol = module
        .functions
        .iter()
        .find(|item| item.instance == Some(function))
        .map(|item| item.display_symbol.clone())
        .or_else(|| program.linked.callable_names.get(&function).map(|name| name.to_string()))
        .unwrap_or_else(|| "<unknown>".to_string());
    let Some(func) = program.mir.functions.get(&function)
    else {
        return format!(" (function_symbol=`{symbol}`)");
    };
    for block in func.blocks.values() {
        for instr in &block.instructions {
            if instr.id != instruction {
                continue;
            }
            let describe = |value: &MirValueId| -> String {
                let ty = func.value_types.get(value);
                let kind = ty.and_then(|id| program.linked.types.get(id)).map(|row| &row.kind);
                format!("{value:?}->{ty:?}/{kind:?}")
            };
            let result_types: Vec<_> = instr.results.iter().map(describe).collect();
            let operand_hint = match &instr.operation {
                CanonicalOperation::Copy { source } => format!(" source={}", describe(source)),
                _ => String::new(),
            };
            return format!(" (function_symbol=`{symbol}` op={:?}{operand_hint} results=[{}])", instr.operation, result_types.join(", "));
        }
    }
    format!(" (function_symbol=`{symbol}`)")
}

fn qualified_name_from_declaration_key(key: Option<&DeclarationKey>) -> Option<QualifiedName> {
    let key = key?;
    if key.name.is_empty() {
        return None;
    }
    let parts = key.namespace.iter().map(|segment| Identifier::new(segment)).chain([Identifier::new(&key.name)]).collect();
    Some(QualifiedName::new(parts))
}
