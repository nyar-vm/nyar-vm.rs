//! 将已完成语义解析的 MIR 生产为 CanonicalProgram。

use std::collections::BTreeMap;

use nyar_types::{
    CanonicalArrayInitialization, CanonicalBlock, CanonicalBlockId, CanonicalCallEdge, CanonicalConstant, CanonicalExternalCallEdge, CanonicalFragment, CanonicalFunction, CanonicalInstruction, CanonicalMirError,
    CanonicalCallee, CanonicalOperation, CanonicalPrimitiveType, CanonicalProgram, CanonicalSemanticMir, CanonicalTerminator, CanonicalTypeKind,
    ItemId, ItemInstanceId, ItemInstanceRecord, LinkedSemanticProgram, MirValueId, NominalInstanceId, NominalInstanceRecord, NominalValueSemantics,
    FieldId, FieldRecord, Identifier, ImportCapability, ImportIndex, ImportRecord, QualifiedName, StructuredDiagnosticSet, SubstitutionId, TypeId,
    TypeRecord,
};
use nyar_types::canonical_program::{CanonicalEffectKind, EntryRecord, ExportRecord};

use nyar::SemanticFragment;

use crate::valkyrie::{
    mir::{MirConstant, MirFunction, MirModule, MirOperand, MirOperation, MirTerminator, MirValueOrigin},
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
    let (nominals, fields, field_records) = collect_aggregate_identities(module, &type_values)?;
    let mut linked = LinkedSemanticProgram { module_name: module.name.clone(), ..LinkedSemanticProgram::default() };
    linked.aggregate_layouts = module.aggregate_layouts.clone();
    linked.sum_types = module.sum_types.iter().map(crate::valkyrie::mir::MirSumDeclaration::physical_layout).collect();
    linked.flags_types = module.flags_types.clone();
    linked.singleton_instances = module.singleton_instances.clone();
    for (symbol, instance) in module.functions.iter().filter_map(|function| function.instance.map(|instance| (function.symbol.clone(), instance)))
        .chain(module.external_calls.iter().filter_map(|contract| contract.instance.map(|instance| (contract.symbol.to_string(), instance)))) {
        let parts = symbol
            .split("::")
            .filter(|part| !part.is_empty())
            .map(Identifier::new)
            .collect::<Vec<_>>();
        if parts.is_empty() {
            return Err(error(module, "CAN040", format!("callable `{symbol}` 没有完整限定 identity")));
        }
        let name = QualifiedName::new(parts);
        if let Some(previous) = linked.callable_names.insert(instance, name.clone())
            && previous != name
        {
            return Err(error(module, "CAN041", format!("callable identity `{instance}` 对应多个限定名称")));
        }
    }
    for (ty, id) in &type_values {
        linked.types.insert(*id, TypeRecord { declaration: *id, kind: canonical_type_kind(ty, &type_values)? });
    }
    for (name, (nominal, declaration, semantics)) in &nominals {
        let nominal_fields = field_records.iter().filter_map(|(field, record)| (record.owner == *nominal).then_some(*field)).collect();
        linked.nominal_instances.insert(*nominal, NominalInstanceRecord { declaration: *declaration, ty: *declaration, substitution: SubstitutionId::from_index(0).expect("monomorphic substitution"), semantics: *semantics, fields: nominal_fields });
        let _ = name;
    }
    linked.fields = field_records;
    for function in &module.functions {
        let instance = function.instance.ok_or_else(|| error(module, "CAN034", format!("函数 `{}` 缺少 Compiler callable identity", function.symbol)))?;
        linked.item_instances.insert(instance, ItemInstanceRecord {
            declaration: function.declaration.ok_or_else(|| error(module, "CAN053", format!("函数 `{}` 缺少声明 identity", function.symbol)))?,
            substitution: monomorphic_substitution(function)?,
            parameter_types: function.param_types.iter().map(|ty| type_id(&type_values, ty)).collect::<Result<_, _>>()?,
            return_type: type_id(&type_values, &function.return_type)?,
        });
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
        let instance = contract.instance.ok_or_else(|| error(module, "CAN034", format!("导入 `{}` 缺少 Compiler callable identity", contract.symbol)))?;
        linked.item_instances.insert(instance, ItemInstanceRecord {
            declaration: contract.declaration.ok_or_else(|| error(module, "CAN053", format!("导入 `{}` 缺少声明 identity", contract.symbol)))?,
            substitution: SubstitutionId::from_index(0).expect("monomorphic substitution"),
            parameter_types: contract.parameter_types.iter().map(|ty| type_id(&type_values, ty)).collect::<Result<_, _>>()?,
            return_type: type_id(&type_values, &contract.return_type)?,
        });
        let import = ImportIndex::from_index(offset as u32).ok_or_else(|| error_without_module("CAN031", "import identity 溢出"))?;
        let capability = ImportCapability::new(module.name.clone(), contract.symbol.to_string());
        linked.imports.insert(import, ImportRecord {
            link: contract.link.clone(),
            capability,
            callee: instance,
            parameter_types: contract.parameter_types.iter().map(|ty| type_id(&type_values, ty)).collect::<Result<_, _>>()?,
            return_type: type_id(&type_values, &contract.return_type)?,
        });
    }
    linked.fragments = canonical_fragments(module, &linked, &module.semantic_fragments)?;
    let mut next_instruction = 0u32;
    let functions = module.functions.iter().map(|function| {
        let instance = function.instance.ok_or_else(|| error(module, "CAN034", format!("函数 `{}` 缺少 Compiler callable identity", function.symbol)))?;
        Ok((instance, lower_function(function, instance, &type_values, &nominals, &fields, &mut next_instruction)?))
    }).collect::<Result<BTreeMap<_, _>, StructuredDiagnosticSet>>()?;
    let program = CanonicalProgram { linked, mir: CanonicalSemanticMir { module_name: module.name.clone(), functions } };
    program.validate().map_err(|error| canonical_error(module, error))?;
    Ok(program)
}

fn canonical_fragments(
    module: &MirModule,
    linked: &LinkedSemanticProgram,
    fragments: &[SemanticFragment],
) -> Result<BTreeMap<Identifier, CanonicalFragment>, StructuredDiagnosticSet> {
    let mut result = BTreeMap::new();
    for fragment in fragments {
        let exported_operations = fragment.exported_operations.clone();
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
        let external_imports = BTreeMap::new();
        let internal_call_edges = Vec::new();
        let external_call_edges = Vec::new();
        if result.insert(fragment.id.clone(), CanonicalFragment {
            id: fragment.id.clone(), exported_operations, required_capabilities: fragment.required_capabilities.clone(), entry_operation,
            external_imports, external_call_edges, internal_call_edges, wasm_export_names,
        }).is_some() {
            return Err(error(module, "CAN051", format!("片段 identity `{}` 重复", fragment.id)));
        }
    }
    Ok(result)
}

type AggregateIdentity = (NominalInstanceId, TypeId, NominalValueSemantics);

fn collect_aggregate_identities(module: &MirModule, types: &BTreeMap<ValkyrieType, TypeId>) -> Result<(BTreeMap<String, AggregateIdentity>, BTreeMap<(String, String), FieldId>, BTreeMap<FieldId, FieldRecord>), StructuredDiagnosticSet> {
    let mut nominals = BTreeMap::new();
    let mut fields = BTreeMap::new();
    let mut field_records = BTreeMap::new();
    let mut next_field = 0u32;
    for (index, aggregate) in module.structs.iter().enumerate() {
        let qualified = if aggregate.namespace.is_empty() { aggregate.name.clone() } else { format!("{}.{}", aggregate.namespace, aggregate.name) };
        let ty = ValkyrieType::Named(crate::valkyrie::types::Identifier::new(&qualified));
        let declaration = types.get(&ty).copied().ok_or_else(|| error_without_module("CAN018", format!("聚合 `{qualified}` 缺少类型事实")))?;
        let nominal = NominalInstanceId::from_index(index as u32).ok_or_else(|| error_without_module("CAN019", "nominal identity 溢出"))?;
        let semantics = if aggregate.is_value_type { NominalValueSemantics::Value } else { NominalValueSemantics::Reference };
        if nominals.insert(qualified.clone(), (nominal, declaration, semantics)).is_some() {
            return Err(error_without_module("CAN020", format!("聚合 identity 重复: {qualified}")));
        }
        for field in &aggregate.fields {
            let id = FieldId::from_index(next_field).ok_or_else(|| error_without_module("CAN021", "field identity 溢出"))?;
            next_field += 1;
            if fields.insert((qualified.clone(), field.name.clone()), id).is_some() {
                return Err(error_without_module("CAN022", format!("字段 identity 重复: {qualified}.{}", field.name)));
            }
            field_records.insert(id, FieldRecord { owner: nominal, ty: type_id(types, &field.ty)? });
        }
    }
    Ok((nominals, fields, field_records))
}

fn validate_callable_identities(module: &MirModule) -> Result<(), StructuredDiagnosticSet> {
    let mut instances = BTreeMap::new();
    for function in &module.functions {
        let Some(identity) = function.instance else {
            return Err(error(module, "CAN034", format!("函数 `{}` 缺少 Compiler callable identity", function.symbol)));
        };
        if instances.insert(identity, function.symbol.clone()).is_some() {
            return Err(error(module, "CAN001", format!("callable identity `{identity}` 重复")));
        }
    }
    for contract in &module.external_calls {
        let symbol = contract.symbol.to_string();
        let Some(identity) = contract.instance else {
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
        ValkyrieType::Named(_) => CanonicalTypeKind::Nominal { declaration: ids[ty], arguments: Vec::new() },
        ValkyrieType::Apply(base, args) => CanonicalTypeKind::Nominal { declaration: id(base)?, arguments: args.iter().map(id).collect::<Result<_, _>>()? },
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
        _ => return Err(error_without_module("CAN003", "类型没有无损 canonical 形状")),
    })
}

fn lower_function(function: &MirFunction, instance: ItemInstanceId, ids: &BTreeMap<ValkyrieType, TypeId>, nominals: &BTreeMap<String, AggregateIdentity>, fields: &BTreeMap<(String, String), FieldId>, next_instruction: &mut u32) -> Result<CanonicalFunction, StructuredDiagnosticSet> {
    let value_types = function.value_types.iter().map(|(value, ty)| Ok((MirValueId::from_index(value.0).ok_or_else(|| error_without_module("CAN004", "SSA value identity 溢出"))?, type_id(ids, ty)?))).collect::<Result<BTreeMap<_, _>, StructuredDiagnosticSet>>()?;
    let parameters = canonical_entry_parameters(function, ids)?;
    let blocks = function.blocks.iter().map(|block| {
        let id = CanonicalBlockId(block.id.0);
        let parameters = block.parameters.iter().map(|value| {
            let value = MirValueId::from_index(value.0).ok_or_else(|| error_without_module("CAN004", "SSA value identity 溢出"))?;
            Ok((value, *value_types.get(&value).ok_or_else(|| error_without_module("CAN005", "块参数缺少类型事实"))?))
        }).collect::<Result<_, StructuredDiagnosticSet>>()?;
        let instructions = block.instructions.iter().map(|instruction| {
            let id = nyar_types::InstructionId::from_index(*next_instruction).ok_or_else(|| error_without_module("CAN016", "instruction identity 溢出"))?;
            *next_instruction = (*next_instruction).checked_add(1).ok_or_else(|| error_without_module("CAN016", "instruction identity 溢出"))?;
            Ok(CanonicalInstruction { id, results: instruction.results.iter().map(|value| MirValueId::from_index(value.0).ok_or_else(|| error_without_module("CAN004", "SSA value identity 溢出"))).collect::<Result<_, _>>()?, operation: lower_operation(&instruction.kind, &instruction.results, &function.value_types, ids, nominals, fields)? })
        }).collect::<Result<_, StructuredDiagnosticSet>>()?;
        Ok((id, CanonicalBlock { id, parameters, instructions, terminator: lower_terminator(&block.terminator)? }))
    }).collect::<Result<BTreeMap<_, _>, StructuredDiagnosticSet>>()?;
    Ok(CanonicalFunction { instance, parameters, return_type: type_id(ids, &function.return_type)?, value_types, entry: CanonicalBlockId(function.entry.0), blocks })
}

fn canonical_entry_parameters(function: &MirFunction, ids: &BTreeMap<ValkyrieType, TypeId>) -> Result<Vec<(MirValueId, TypeId)>, StructuredDiagnosticSet> {
    let invalid = || error_without_module("CAN032", format!("函数 `{}` 的声明参数、SSA 参数来源与入口块参数不一致", function.symbol));
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
    function.param_types.iter().enumerate().map(|(index, ty)| {
        let value = origins.get(&index).ok_or_else(invalid)?;
        if entry.parameters.get(index) != Some(value) || function.value_types.get(value) != Some(ty) {
            return Err(invalid());
        }
        Ok((MirValueId::from_index(value.0).ok_or_else(|| error_without_module("CAN004", "SSA value identity 溢出"))?, type_id(ids, ty)?))
    }).collect()
}

fn lower_operation(operation: &MirOperation, results: &[crate::valkyrie::mir::MirValueRef], value_types: &BTreeMap<crate::valkyrie::mir::MirValueRef, ValkyrieType>, ids: &BTreeMap<ValkyrieType, TypeId>, nominals: &BTreeMap<String, AggregateIdentity>, fields: &BTreeMap<(String, String), FieldId>) -> Result<CanonicalOperation, StructuredDiagnosticSet> {
    let value = |operand: &MirOperand| match operand { MirOperand::Value(value) => MirValueId::from_index(value.0).ok_or_else(|| error_without_module("CAN004", "SSA value identity 溢出")), _ => Err(error_without_module("CAN006", "操作数不是已定义 SSA 值")) };
    match operation {
        MirOperation::Call { callee: MirOperand::Callable(identity), arguments } => Ok(CanonicalOperation::Invoke { callee: CanonicalCallee::Item(*identity), arguments: arguments.iter().map(value).collect::<Result<_, _>>()? }),
        MirOperation::Call { callee: MirOperand::Symbol(_), .. } => Err(error_without_module("CAN007", "Semantic MIR 仍包含未冻结的 callable 名称")),
        MirOperation::Call { callee: MirOperand::Value(callee), arguments } => Ok(CanonicalOperation::Invoke { callee: CanonicalCallee::Value(MirValueId::from_index(callee.0).ok_or_else(|| error_without_module("CAN008", "函数值 callee identity 溢出"))?), arguments: arguments.iter().map(value).collect::<Result<_, _>>()? }),
        MirOperation::Copy { source } => Ok(CanonicalOperation::Copy { source: value(source)? }),
        MirOperation::AggregateCopy { source, dest } => Ok(CanonicalOperation::AggregateCopy {
            source: value(source)?,
            destination: value(dest)?,
        }),
        MirOperation::LoadConstant { constant, .. } => Ok(CanonicalOperation::LoadConstant { constant: lower_constant(constant)? }),
        MirOperation::ArrayGet { array, index } => Ok(CanonicalOperation::ArrayGet { array: value(array)?, index: value(index)? }),
        MirOperation::ArraySet { array, index, value: stored } => Ok(CanonicalOperation::ArraySet { array: value(array)?, index: value(index)?, value: value(stored)? }),
        MirOperation::ArrayLength { array } => Ok(CanonicalOperation::ArrayLength { array: value(array)? }),
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
                _ => return Err(error_without_module("CAN017", "元组构造结果缺少完整 Tuple 类型")),
            },
            fields: fields.iter().map(value).collect::<Result<_, _>>()?,
        }),
        MirOperation::StructNew { type_name, fields: values } => {
            let (nominal, _, _) = nominals.get(&type_name.to_string()).copied().ok_or_else(|| error_without_module("CAN023", format!("未解析聚合 owner: {type_name}")))?;
            let fields = values.iter().map(|(field, operand)| {
                let field_id = fields.get(&(type_name.to_string(), field.to_string())).copied().ok_or_else(|| error_without_module("CAN024", format!("未解析字段 owner: {type_name}.{field}")))?;
                Ok((field_id, value(operand)?))
            }).collect::<Result<_, StructuredDiagnosticSet>>()?;
            Ok(CanonicalOperation::StructNew { nominal, fields })
        }
        MirOperation::FieldGet { object, field } => {
            let object_value = match object { MirOperand::Value(value) => value, _ => return Err(error_without_module("CAN025", "字段对象不是 SSA 值")) };
            let owner = aggregate_owner(value_types.get(object_value).ok_or_else(|| error_without_module("CAN026", "字段对象缺少类型事实"))?)
                .ok_or_else(|| error_without_module("CAN026", "字段对象不是已知聚合类型"))?;
            let field_id = fields.get(&(owner, field.to_string())).copied().ok_or_else(|| error_without_module("CAN027", format!("未解析字段 owner: {field}")))?;
            Ok(CanonicalOperation::FieldGet { object: value(object)?, field: field_id })
        }
        MirOperation::FieldSet { object, field, value: stored } => {
            let object_value = match object { MirOperand::Value(value) => value, _ => return Err(error_without_module("CAN028", "字段对象不是 SSA 值")) };
            let owner = aggregate_owner(value_types.get(object_value).ok_or_else(|| error_without_module("CAN029", "字段对象缺少类型事实"))?)
                .ok_or_else(|| error_without_module("CAN029", "字段对象不是已知聚合类型"))?;
            let field_id = fields.get(&(owner, field.to_string())).copied().ok_or_else(|| error_without_module("CAN030", format!("未解析字段 owner: {field}")))?;
            Ok(CanonicalOperation::FieldSet { object: value(object)?, field: field_id, value: value(stored)? })
        }
        _ => Err(error_without_module("CAN009", "操作没有无损 canonical 形状")),
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
    let value = |operand: &MirOperand| match operand { MirOperand::Value(value) => MirValueId::from_index(value.0).ok_or_else(|| error_without_module("CAN004", "SSA value identity 溢出")), _ => Err(error_without_module("CAN010", "终结符操作数不是 SSA 值")) };
    match terminator {
        MirTerminator::Return { value: Some(value_operand) } => Ok(CanonicalTerminator::Return { value: Some(value(value_operand)?) }),
        MirTerminator::Return { value: None } => Ok(CanonicalTerminator::Return { value: None }),
        MirTerminator::Jump { target, arguments } => Ok(CanonicalTerminator::Jump { target: CanonicalBlockId(target.0), arguments: arguments.iter().map(value).collect::<Result<_, _>>()? }),
        MirTerminator::Branch { condition, then_target, else_target } => Ok(CanonicalTerminator::Branch { condition: value(condition)?, then_target: CanonicalBlockId(then_target.0), else_target: CanonicalBlockId(else_target.0) }),
        MirTerminator::PerformEffect { effect, payload, resume_target } => Ok(CanonicalTerminator::PerformEffect {
            effect: lower_effect(*effect), payload: payload.as_ref().map(value).transpose()?, resume_target: CanonicalBlockId(resume_target.0),
        }),
        MirTerminator::StateDispatch { state, cases, default_target } => Ok(CanonicalTerminator::StateDispatch {
            state: MirValueId::from_index(state.0).ok_or_else(|| error_without_module("CAN004", "SSA value identity 溢出"))?,
            cases: cases.iter().map(|(state, target)| (*state, CanonicalBlockId(target.0))).collect(),
            default_target: CanonicalBlockId(default_target.0),
        }),
        MirTerminator::YieldToRuntime { effect, payload, resume_state } => Ok(CanonicalTerminator::YieldToRuntime {
            effect: lower_effect(*effect), payload: payload.as_ref().map(value).transpose()?, resume_state: *resume_state,
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
    match constant { MirConstant::Int(value) => Ok(CanonicalConstant::Int(*value)), MirConstant::Bool(value) => Ok(CanonicalConstant::Bool(*value)), MirConstant::Utf8(value) => Ok(CanonicalConstant::Utf8(value.clone())), MirConstant::Utf16(value) => Ok(CanonicalConstant::Utf16(value.clone())), MirConstant::Unit => Ok(CanonicalConstant::Unit), MirConstant::Float64(_) => Err(error_without_module("CAN012", "浮点常量没有 canonical 形状")) }
}

fn monomorphic_substitution(function: &MirFunction) -> Result<SubstitutionId, StructuredDiagnosticSet> { if function.param_types.iter().chain(std::iter::once(&function.return_type)).any(contains_unresolved_type) { Err(error_without_module("CAN013", "函数仍包含未代入类型参数")) } else { Ok(SubstitutionId::from_index(0).expect("monomorphic substitution")) } }
fn contains_unresolved_type(ty: &ValkyrieType) -> bool { matches!(ty, ValkyrieType::Generic(_) | ValkyrieType::SelfType | ValkyrieType::Associated(_) | ValkyrieType::AutoType) }
fn type_id(ids: &BTreeMap<ValkyrieType, TypeId>, ty: &ValkyrieType) -> Result<TypeId, StructuredDiagnosticSet> { ids.get(ty).copied().ok_or_else(|| error_without_module("CAN014", "类型事实未进入 canonical table")) }
fn item_id(index: u32) -> ItemId { ItemId::from_index(index).expect("item identity overflow") }
fn error_without_module(code: &'static str, message: impl Into<String>) -> StructuredDiagnosticSet { fail_stage::<()>(nyar_types::CompileStage::SemanticMir, code, "", message).err().unwrap() }
fn error(module: &MirModule, code: &'static str, message: impl Into<String>) -> StructuredDiagnosticSet { fail_stage::<()>(nyar_types::CompileStage::SemanticMir, code, &module.name, message).err().unwrap() }
fn canonical_error(module: &MirModule, error: CanonicalMirError) -> StructuredDiagnosticSet { fail_stage::<()>(nyar_types::CompileStage::ValidateMir, "CAN015", &module.name, format!("canonical MIR 校验失败: {error:?}")).err().unwrap() }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::valkyrie::mir::{AggregateLayoutPlan, MirBlock, MirBlockRef, MirExternalCallContract, MirInstruction, MirModule, MirValue, MirValueRef};
    use crate::valkyrie::types::{Identifier, NamePath};
    use std::collections::BTreeMap;

    fn source_surface_module() -> MirModule {
        let hir = crate::ValkyrieCompiler::default().compile_source(
            "[export(name: \"answer\")] [main] micro answer() -> i32 { return 23 }",
        ).expect("源码导出及入口合同");
        crate::MirLowerer::lower_module_semantic(&hir)
    }

    #[test]
    fn source_instances_reach_canonical_without_a_name_binding_table() {
        let mut module = source_surface_module();
        let instance = module.functions[0].instance.expect("Compiler 实例 identity");
        module.callable_identities.clear();
        let program = canonical_program_from_semantic_mir(&module).expect("Canonical 只消费节点实例，不按名称重绑");
        assert!(program.mir.functions.contains_key(&instance));
        assert!(program.linked.exports.contains_key(&instance));
        assert!(program.linked.entries.contains_key(&instance));
    }

    #[test]
    fn source_export_and_entry_diagnostic_names_cannot_rebind_instances() {
        let mut module = source_surface_module();
        let expected = module.functions[0].instance.expect("Compiler 实例 identity");
        module.exports[0].symbol = NamePath::new(vec![Identifier::new("unrelated_export_label")]);
        module.entries[0].symbol = NamePath::new(vec![Identifier::new("unrelated_entry_label")]);
        let program = canonical_program_from_semantic_mir(&module).expect("诊断标签不参与语义绑定");
        assert!(program.linked.exports.contains_key(&expected));
        assert!(program.linked.entries.contains_key(&expected));
    }

    #[test]
    fn source_missing_node_instance_cannot_be_recovered_from_a_name_table() {
        let mut module = source_surface_module();
        assert!(module.functions[0].instance.take().is_some());
        assert!(!module.callable_identities.is_empty());
        let error = canonical_program_from_semantic_mir(&module).expect_err("名称表不能补造缺失节点身份");
        assert_eq!(error.records[0].code, "CAN034");
    }

    fn module_with(operation: MirOperation, return_value: Option<MirValueRef>, value_types: BTreeMap<MirValueRef, ValkyrieType>) -> MirModule {
        let mut module = MirModule {
            name: "demo".into(),
            functions: vec![MirFunction {
                symbol: "demo::main".into(),
                declaration: Some(ItemId::from_index(0).unwrap()),
                instance: Some(ItemInstanceId::from_index(0).unwrap()),
                return_type: return_value.map(|_| ValkyrieType::Boolean).unwrap_or(ValkyrieType::Unit),
                param_types: Vec::new(),
                value_types,
                entry: MirBlockRef(0),
                values: return_value.into_iter().map(|id| MirValue { id, origin: MirValueOrigin::Temporary }).collect(),
                blocks: vec![MirBlock {
                    id: MirBlockRef(0), label: "entry".into(), parameters: Vec::new(),
                    instructions: vec![MirInstruction::from_operation_with_results(operation, return_value.into_iter().collect())],
                    terminator: MirTerminator::Return { value: return_value.map(MirOperand::Value) },
                }],
            }],
            structs: Vec::new(), imports: Vec::new(), external_calls: Vec::new(), exports: Vec::new(), entries: Vec::new(), callable_identities: BTreeMap::from([("demo::main".to_owned(), ItemInstanceId::from_index(0).unwrap())]), type_identities: BTreeMap::new(), aggregate_layouts: AggregateLayoutPlan::default(),
            sum_types: Vec::new(), flags_types: Vec::new(),
            singleton_instances: Vec::new(),
            semantic_fragments: Vec::new(), diagnostics: Vec::new(),
        };
        module
    }

    #[test]
    fn producer_preserves_exact_function_identity_and_type_shape() {
        let value = MirValueRef(0);
        let module = module_with(MirOperation::LoadConstant { constant: MirConstant::Bool(true), ty: Some(ValkyrieType::Boolean) }, Some(value), BTreeMap::from([(value, ValkyrieType::Boolean)]));
        let program = canonical_program_from_semantic_mir(&module).expect("精确单态函数应进入 canonical");
        assert_eq!(program.linked.item_instances.len(), 1);
        assert_eq!(program.mir.functions.len(), 1);
        assert_eq!(program.linked.callable_names.values().map(ToString::to_string).collect::<Vec<_>>(), vec!["demo::main"]);
        assert!(program.linked.types.values().any(|record| matches!(record.kind, CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Bool))));
    }

    #[test]
    fn producer_carries_export_and_entry_contracts_by_callable_identity() {
        let output = crate::ValkyrieCompiler::default()
            .compile_source_to_program(
                "[export(name: \"public_main\")] [main] micro main() -> unit { return }",
            )
            .expect("源码必须完成前端分析");
        let semantic_mir = crate::valkyrie::mir::MirLowerer::lower_module_semantic(&crate::ValkyrieCompiler::default().compile_source("[export(name: \"public_main\")] [main] micro main() -> unit { return }").expect("source for canonical test"));
        let program = canonical_program_from_semantic_mir(&semantic_mir).expect("公开合同必须进入 canonical");
        assert_eq!(program.linked.exports.len(), 1);
        assert_eq!(program.linked.entries.len(), 1);
        let (instance, export) = program.linked.exports.iter().next().unwrap();
        assert_eq!(program.linked.entries.get(instance), Some(&nyar_types::canonical_program::EntryRecord));
        assert_eq!(export.exported_name, "public_main");
    }

    #[test]
    fn source_entry_contract_rejects_missing_reordered_and_invalid_parameters() {
        let output = crate::ValkyrieCompiler::default()
            .compile_source_to_program("micro select(value: bool, other: i32) -> bool { return value }")
            .expect("源码必须完成前端分析");
        let module = crate::valkyrie::mir::MirLowerer::lower_module_semantic(&crate::ValkyrieCompiler::default().compile_source("micro select(value: bool, other: i32) -> bool { return value }").expect("source for canonical test"));
        canonical_program_from_semantic_mir(&module).expect("合法入口必须产生 CanonicalProgram");

        let mut missing = module.clone();
        missing.functions[0].blocks[0].parameters.pop();
        let mut reordered = module.clone();
        reordered.functions[0].blocks[0].parameters.swap(0, 1);
        let mut invalid_origin = module.clone();
        invalid_origin.functions[0].values[0].origin = MirValueOrigin::Parameter { index: usize::MAX, name: "value".into() };
        let mut duplicate_origin = module.clone();
        duplicate_origin.functions[0].values.push(module.functions[0].values[0].clone());
        let mut wrong_type = module.clone();
        let value = wrong_type.functions[0].blocks[0].parameters[0];
        wrong_type.functions[0].value_types.insert(value, ValkyrieType::Unit);
        for invalid in [missing, reordered, invalid_origin, duplicate_origin, wrong_type] {
            let error = canonical_program_from_semantic_mir(&invalid).expect_err("不一致入口不得通过删除参数修复");
            assert_eq!(error.records[0].code, "CAN032");
        }
    }

    #[test]
    fn producer_rejects_unresolved_callable_without_name_fallback() {
        let module = module_with(
            MirOperation::Call { callee: MirOperand::Symbol(NamePath::new(vec![Identifier::new("new")])), arguments: Vec::new() },
            None,
            BTreeMap::new(),
        );
        let error = canonical_program_from_semantic_mir(&module).expect_err("未解析 callable 必须在 producer 失败");
        assert_eq!(error.records[0].code, "CAN007");
    }

    #[test]
    fn producer_rejects_missing_compiler_callable_identity() {
        let mut module = module_with(
            MirOperation::LoadConstant { constant: MirConstant::Unit, ty: Some(ValkyrieType::Unit) },
            None,
            BTreeMap::new(),
        );
        module.callable_identities.clear();
        let error = canonical_program_from_semantic_mir(&module).expect_err("缺失 Compiler callable identity 必须失败");
        assert_eq!(error.records[0].code, "CAN034");
    }

    #[test]
    fn producer_rejects_missing_compiler_type_identity() {
        let mut module = module_with(
            MirOperation::LoadConstant { constant: MirConstant::Unit, ty: Some(ValkyrieType::Unit) },
            None,
            BTreeMap::new(),
        );
        module.type_identities.clear();
        let error = canonical_program_from_semantic_mir(&module).expect_err("缺失 Compiler type identity 必须失败");
        assert_eq!(error.records[0].code, "CAN035");
    }

    #[test]
    fn producer_rejects_semantic_mir_lowering_diagnostics() {
        let mut module = module_with(
            MirOperation::LoadConstant { constant: MirConstant::Unit, ty: Some(ValkyrieType::Unit) },
            None,
            BTreeMap::new(),
        );
        module.diagnostics.push(crate::valkyrie::mir::MirDiagnostic::UnresolvedVariantIdentity {
            sum_type: "Option".into(),
            variant: "Missing".into(),
        });
        let error = canonical_program_from_semantic_mir(&module).expect_err("lowering diagnostic must not enter canonical success");
        assert_eq!(error.records[0].code, "CAN033");
    }

    #[test]
    fn producer_carries_singleton_lifecycle_contract() {
        let output = crate::ValkyrieCompiler::default()
            .compile_source_to_program(
                r#"lazy singleton Counter {
    total: i64 = 0
}"#,
            )
            .expect("singleton 源码必须产生完整 Canonical 合同");
        let plans = &output.canonical().linked.singleton_instances;
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].qualified_name(), "Counter");
        assert!(plans[0].is_lazy);
        assert_eq!(plans[0].instance_field, "INSTANCE");
    }

    #[test]
    fn producer_binds_external_callable_to_import_index_and_signature() {
        let mut module = module_with(
            MirOperation::LoadConstant { constant: MirConstant::Unit, ty: Some(ValkyrieType::Unit) },
            None,
            BTreeMap::new(),
        );
        module.external_calls.push(MirExternalCallContract {
            declaration: Some(ItemId::from_index(1).unwrap()),
            instance: Some(ItemInstanceId::from_index(1).unwrap()),
            symbol: NamePath::new(vec![Identifier::new("std"), Identifier::new("console"), Identifier::new("write")]),
            link: nyar_types::ExternalImportLink::host(None, vec!["std".to_owned(), "console".to_owned(), "write".to_owned()]),
            parameter_types: vec![ValkyrieType::Boolean],
            return_type: ValkyrieType::Unit,
        });

        let program = canonical_program_from_semantic_mir(&module).expect("external declaration has a complete import contract");
        let import = ImportIndex::from_index(0).unwrap();
        let record = program.linked.imports.get(&import).expect("import index");
        assert_eq!(record.callee.index(), 1);
        assert_eq!(record.parameter_types, vec![program.linked.types.iter().find_map(|(id, record)| matches!(record.kind, CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Bool)).then_some(*id)).unwrap()]);
        assert_eq!(record.return_type, program.linked.types.iter().find_map(|(id, record)| matches!(record.kind, CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Unit)).then_some(*id)).unwrap());
    }
}
