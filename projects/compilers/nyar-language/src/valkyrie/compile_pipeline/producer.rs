//! 将已完成语义解析的 MIR 生产为 CanonicalProgram。

use std::collections::{BTreeMap, BTreeSet};

use nyar_types::{
    CanonicalArrayInitialization, CanonicalBlock, CanonicalBlockId, CanonicalConstant, CanonicalFunction, CanonicalInstruction, CanonicalMirError,
    CanonicalOperation, CanonicalPrimitiveType, CanonicalProgram, CanonicalSemanticMir, CanonicalTerminator, CanonicalTypeKind,
    ItemId, ItemInstanceId, ItemInstanceRecord, LinkedSemanticProgram, MirValueId, NominalInstanceId, NominalInstanceRecord,
    FieldId, FieldRecord, StructuredDiagnosticSet, SubstitutionId, TypeId, TypeRecord,
};

use crate::valkyrie::{
    mir::{MirConstant, MirFunction, MirModule, MirOperand, MirOperation, MirTerminator, MirValueOrigin},
    types::hir::ValkyrieType,
};

use super::diagnostics::fail_stage;

/// 从已完成 HIR/Semantic MIR 合同的模块生成 canonical 成功值。
pub fn canonical_program_from_semantic_mir(module: &MirModule) -> Result<CanonicalProgram, StructuredDiagnosticSet> {
    let type_values = collect_types(module)?;
    let symbols = collect_symbols(module)?;
    let (nominals, fields, field_records) = collect_aggregate_identities(module, &type_values)?;
    let mut linked = LinkedSemanticProgram { module_name: module.name.clone(), ..LinkedSemanticProgram::default() };
    for (ty, id) in &type_values {
        linked.types.insert(*id, TypeRecord { declaration: *id, kind: canonical_type_kind(ty, &type_values)? });
    }
    for (name, (nominal, declaration)) in &nominals {
        let nominal_fields = field_records.iter().filter_map(|(field, record)| (record.owner == *nominal).then_some(*field)).collect();
        linked.nominal_instances.insert(*nominal, NominalInstanceRecord { declaration: *declaration, substitution: SubstitutionId::from_index(0).expect("monomorphic substitution"), fields: nominal_fields });
        let _ = name;
    }
    linked.fields = field_records;
    for (index, function) in module.functions.iter().enumerate() {
        let instance = item_instance(index as u32);
        linked.item_instances.insert(instance, ItemInstanceRecord {
            declaration: item_id(index as u32),
            substitution: monomorphic_substitution(function)?,
            parameter_types: function.param_types.iter().map(|ty| type_id(&type_values, ty)).collect::<Result<_, _>>()?,
            return_type: type_id(&type_values, &function.return_type)?,
        });
    }
    let external_start = module.functions.len() as u32;
    for (offset, contract) in module.external_calls.iter().enumerate() {
        let instance = item_instance(external_start + offset as u32);
        linked.item_instances.insert(instance, ItemInstanceRecord {
            declaration: item_id(external_start + offset as u32),
            substitution: SubstitutionId::from_index(0).expect("monomorphic substitution"),
            parameter_types: contract.parameter_types.iter().map(|ty| type_id(&type_values, ty)).collect::<Result<_, _>>()?,
            return_type: type_id(&type_values, &contract.return_type)?,
        });
    }
    let mut next_instruction = 0u32;
    let functions = module.functions.iter().enumerate().map(|(index, function)| {
        let instance = item_instance(index as u32);
        Ok((instance, lower_function(function, instance, &symbols, &type_values, &nominals, &fields, &mut next_instruction)?))
    }).collect::<Result<BTreeMap<_, _>, StructuredDiagnosticSet>>()?;
    let program = CanonicalProgram { linked, mir: CanonicalSemanticMir { module_name: module.name.clone(), functions } };
    program.validate().map_err(|error| canonical_error(module, error))?;
    Ok(program)
}

type AggregateIdentity = (NominalInstanceId, TypeId);

fn collect_aggregate_identities(module: &MirModule, types: &BTreeMap<ValkyrieType, TypeId>) -> Result<(BTreeMap<String, AggregateIdentity>, BTreeMap<(String, String), FieldId>, BTreeMap<FieldId, FieldRecord>), StructuredDiagnosticSet> {
    let mut nominals = BTreeMap::new();
    let mut fields = BTreeMap::new();
    let mut field_records = BTreeMap::new();
    let mut next_field = 0u32;
    for (index, aggregate) in module.structs.iter().enumerate() {
        let qualified = if aggregate.namespace.is_empty() { aggregate.name.clone() } else { format!("{}.{}", aggregate.namespace, aggregate.name) };
        let ty = ValkyrieType::Named(crate::valkyrie::types::Identifier::new(&aggregate.name));
        let declaration = types.get(&ty).copied().ok_or_else(|| error_without_module("CAN018", format!("聚合 `{qualified}` 缺少类型事实")))?;
        let nominal = NominalInstanceId::from_index(index as u32).ok_or_else(|| error_without_module("CAN019", "nominal identity 溢出"))?;
        if nominals.insert(qualified.clone(), (nominal, declaration)).is_some() {
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

fn collect_symbols(module: &MirModule) -> Result<BTreeMap<String, ItemInstanceId>, StructuredDiagnosticSet> {
    let mut symbols = BTreeMap::new();
    for (index, function) in module.functions.iter().enumerate() {
        insert_symbol(&mut symbols, function.symbol.clone(), item_instance(index as u32), module)?;
    }
    let external_start = module.functions.len() as u32;
    for (offset, contract) in module.external_calls.iter().enumerate() {
        insert_symbol(&mut symbols, contract.symbol.to_string(), item_instance(external_start + offset as u32), module)?;
    }
    Ok(symbols)
}

fn insert_symbol(symbols: &mut BTreeMap<String, ItemInstanceId>, symbol: String, id: ItemInstanceId, module: &MirModule) -> Result<(), StructuredDiagnosticSet> {
    if symbols.insert(symbol.clone(), id).is_some() {
        Err(error(module, "CAN001", format!("callable identity 重复: {symbol}")))
    } else {
        Ok(())
    }
}

fn collect_types(module: &MirModule) -> Result<BTreeMap<ValkyrieType, TypeId>, StructuredDiagnosticSet> {
    let mut types = BTreeSet::new();
    for function in &module.functions {
        collect_type(&mut types, &function.return_type)?;
        for ty in &function.param_types { collect_type(&mut types, ty)?; }
        for ty in function.value_types.values() { collect_type(&mut types, ty)?; }
    }
    for contract in &module.external_calls {
        collect_type_list(&mut types, &contract.parameter_types)?;
        collect_type(&mut types, &contract.return_type)?;
    }
    Ok(types.into_iter().enumerate().map(|(index, ty)| (ty, TypeId::from_index(index as u32).expect("type identity overflow"))).collect())
}

fn collect_type(types: &mut BTreeSet<ValkyrieType>, ty: &ValkyrieType) -> Result<(), StructuredDiagnosticSet> {
    match ty {
        ValkyrieType::Apply(base, args) => { collect_type(types, base)?; collect_type_list(types, args)?; }
        ValkyrieType::Tuple(items) | ValkyrieType::Union(items) | ValkyrieType::Intersection(items) => collect_type_list(types, items)?,
        ValkyrieType::Array(element) | ValkyrieType::Nullable(element) => collect_type(types, element)?,
        ValkyrieType::FixedArray { element, .. } => collect_type(types, element)?,
        ValkyrieType::Generic(_) | ValkyrieType::SelfType | ValkyrieType::Associated(_) | ValkyrieType::AutoType => {
            return Err(error_without_module("CAN002", "类型尚未完成泛型/关联类型代入"));
        }
        _ => {}
    }
    types.insert(ty.clone());
    Ok(())
}

fn collect_type_list(types: &mut BTreeSet<ValkyrieType>, values: &[ValkyrieType]) -> Result<(), StructuredDiagnosticSet> {
    for value in values { collect_type(types, value)?; }
    Ok(())
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
        _ => return Err(error_without_module("CAN003", "类型没有无损 canonical 形状")),
    })
}

fn lower_function(function: &MirFunction, instance: ItemInstanceId, symbols: &BTreeMap<String, ItemInstanceId>, ids: &BTreeMap<ValkyrieType, TypeId>, nominals: &BTreeMap<String, AggregateIdentity>, fields: &BTreeMap<(String, String), FieldId>, next_instruction: &mut u32) -> Result<CanonicalFunction, StructuredDiagnosticSet> {
    let value_types = function.value_types.iter().map(|(value, ty)| Ok((MirValueId::from_index(value.0).ok_or_else(|| error_without_module("CAN004", "SSA value identity 溢出"))?, type_id(ids, ty)?))).collect::<Result<BTreeMap<_, _>, StructuredDiagnosticSet>>()?;
    let parameters = function.values.iter().filter_map(|value| match value.origin { MirValueOrigin::Parameter { index, .. } => Some((index, value.id)), _ => None }).map(|(index, value)| Ok((MirValueId::from_index(value.0).ok_or_else(|| error_without_module("CAN004", "SSA value identity 溢出"))?, type_id(ids, &function.param_types[index])?))).collect::<Result<Vec<_>, StructuredDiagnosticSet>>()?;
    let blocks = function.blocks.iter().map(|block| {
        let id = CanonicalBlockId(block.id.0);
        let parameters = block.parameters.iter().map(|value| { let value = MirValueId::from_index(value.0).ok_or_else(|| error_without_module("CAN004", "SSA value identity 溢出"))?; Ok((value, *value_types.get(&value).ok_or_else(|| error_without_module("CAN005", "块参数缺少类型事实"))?)) }).collect::<Result<_, StructuredDiagnosticSet>>()?;
        let instructions = block.instructions.iter().map(|instruction| {
            let id = nyar_types::InstructionId::from_index(*next_instruction).ok_or_else(|| error_without_module("CAN016", "instruction identity 溢出"))?;
            *next_instruction = (*next_instruction).checked_add(1).ok_or_else(|| error_without_module("CAN016", "instruction identity 溢出"))?;
            Ok(CanonicalInstruction { id, results: instruction.results.iter().map(|value| MirValueId::from_index(value.0).ok_or_else(|| error_without_module("CAN004", "SSA value identity 溢出"))).collect::<Result<_, _>>()?, operation: lower_operation(&instruction.kind, &instruction.results, &function.value_types, symbols, ids, nominals, fields)? })
        }).collect::<Result<_, StructuredDiagnosticSet>>()?;
        Ok((id, CanonicalBlock { id, parameters, instructions, terminator: lower_terminator(&block.terminator)? }))
    }).collect::<Result<BTreeMap<_, _>, StructuredDiagnosticSet>>()?;
    Ok(CanonicalFunction { instance, parameters, return_type: type_id(ids, &function.return_type)?, value_types, entry: CanonicalBlockId(function.entry.0), blocks })
}

fn lower_operation(operation: &MirOperation, results: &[crate::valkyrie::mir::MirValueRef], value_types: &BTreeMap<crate::valkyrie::mir::MirValueRef, ValkyrieType>, symbols: &BTreeMap<String, ItemInstanceId>, ids: &BTreeMap<ValkyrieType, TypeId>, nominals: &BTreeMap<String, AggregateIdentity>, fields: &BTreeMap<(String, String), FieldId>) -> Result<CanonicalOperation, StructuredDiagnosticSet> {
    let value = |operand: &MirOperand| match operand { MirOperand::Value(value) => MirValueId::from_index(value.0).ok_or_else(|| error_without_module("CAN004", "SSA value identity 溢出")), _ => Err(error_without_module("CAN006", "操作数不是已定义 SSA 值")) };
    match operation {
        MirOperation::Call { callee: MirOperand::Symbol(symbol), arguments } => Ok(CanonicalOperation::Invoke { callee: *symbols.get(&symbol.to_string()).ok_or_else(|| error_without_module("CAN007", "调用身份未解析"))?, arguments: arguments.iter().map(value).collect::<Result<_, _>>()? }),
        MirOperation::Copy { source } => Ok(CanonicalOperation::Copy { source: value(source)? }),
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
            let (nominal, _) = nominals.get(&type_name.to_string()).copied().ok_or_else(|| error_without_module("CAN023", format!("未解析聚合 owner: {type_name}")))?;
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
        MirTerminator::Unreachable => Ok(CanonicalTerminator::Unreachable),
        _ => Err(error_without_module("CAN011", "终结符没有无损 canonical 形状")),
    }
}

fn lower_constant(constant: &MirConstant) -> Result<CanonicalConstant, StructuredDiagnosticSet> {
    match constant { MirConstant::Int(value) => Ok(CanonicalConstant::Int(*value)), MirConstant::Bool(value) => Ok(CanonicalConstant::Bool(*value)), MirConstant::Utf8(value) => Ok(CanonicalConstant::Utf8(value.clone())), MirConstant::Utf16(value) => Ok(CanonicalConstant::Utf16(value.clone())), MirConstant::Unit => Ok(CanonicalConstant::Unit), MirConstant::Float64(_) => Err(error_without_module("CAN012", "浮点常量没有 canonical 形状")) }
}

fn monomorphic_substitution(function: &MirFunction) -> Result<SubstitutionId, StructuredDiagnosticSet> { if function.param_types.iter().chain(std::iter::once(&function.return_type)).any(contains_unresolved_type) { Err(error_without_module("CAN013", "函数仍包含未代入类型参数")) } else { Ok(SubstitutionId::from_index(0).expect("monomorphic substitution")) } }
fn contains_unresolved_type(ty: &ValkyrieType) -> bool { matches!(ty, ValkyrieType::Generic(_) | ValkyrieType::SelfType | ValkyrieType::Associated(_) | ValkyrieType::AutoType) }
fn type_id(ids: &BTreeMap<ValkyrieType, TypeId>, ty: &ValkyrieType) -> Result<TypeId, StructuredDiagnosticSet> { ids.get(ty).copied().ok_or_else(|| error_without_module("CAN014", "类型事实未进入 canonical table")) }
fn item_id(index: u32) -> ItemId { ItemId::from_index(index).expect("item identity overflow") }
fn item_instance(index: u32) -> ItemInstanceId { ItemInstanceId::from_index(index).expect("item instance overflow") }
fn error_without_module(code: &'static str, message: impl Into<String>) -> StructuredDiagnosticSet { fail_stage::<()>(nyar_types::CompileStage::SemanticMir, code, "", message).err().unwrap() }
fn error(module: &MirModule, code: &'static str, message: impl Into<String>) -> StructuredDiagnosticSet { fail_stage::<()>(nyar_types::CompileStage::SemanticMir, code, &module.name, message).err().unwrap() }
fn canonical_error(module: &MirModule, error: CanonicalMirError) -> StructuredDiagnosticSet { fail_stage::<()>(nyar_types::CompileStage::ValidateMir, "CAN015", &module.name, format!("canonical MIR 校验失败: {error:?}")).err().unwrap() }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::valkyrie::mir::{AggregateLayoutPlan, MirBlock, MirBlockRef, MirInstruction, MirModule, MirValue, MirValueRef};
    use crate::valkyrie::types::{Identifier, NamePath};
    use std::collections::BTreeMap;

    fn module_with(operation: MirOperation, return_value: Option<MirValueRef>, value_types: BTreeMap<MirValueRef, ValkyrieType>) -> MirModule {
        MirModule {
            name: "demo".into(),
            functions: vec![MirFunction {
                symbol: "demo::main".into(),
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
            structs: Vec::new(), imports: Vec::new(), external_calls: Vec::new(), aggregate_layouts: AggregateLayoutPlan::default(),
            sum_types: Vec::new(), diagnostics: Vec::new(),
        }
    }

    #[test]
    fn producer_preserves_exact_function_identity_and_type_shape() {
        let value = MirValueRef(0);
        let module = module_with(MirOperation::LoadConstant { constant: MirConstant::Bool(true), ty: Some(ValkyrieType::Boolean) }, Some(value), BTreeMap::from([(value, ValkyrieType::Boolean)]));
        let program = canonical_program_from_semantic_mir(&module).expect("精确单态函数应进入 canonical");
        assert_eq!(program.linked.item_instances.len(), 1);
        assert_eq!(program.mir.functions.len(), 1);
        assert!(program.linked.types.values().any(|record| matches!(record.kind, CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Bool))));
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
}

