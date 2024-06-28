//! Compiler 成功载荷到目标私有执行计划的唯一边界。

use std::collections::BTreeMap;

use miette::{Result, miette};
use nyar::QualifiedName;
use nyar_types::{
    CanonicalArrayInitialization, CanonicalCallee, CanonicalConstant, CanonicalOperation, CanonicalProgram, CanonicalTerminator,
    CanonicalTypeKind, CompiledProgram, Constant, Instruction, InstructionKind, ItemInstanceId, NyarType, Operand, Terminator, Value,
    ValueOrigin,
    layout_choice::{InvokeLowering, ValueRepresentation},
};

use crate::{
    backend_plan_views::{ExecutableFunction, FunctionView, SuspendMetadataView},
    contracts::{Block, BlockRef, ValueRef},
};

/// 已完成 callable、类型、CFG 和表示合同绑定的目标私有计划。
#[derive(Debug, Clone, Default)]
pub struct TargetExecutionPlan {
    module_name: String,
    fragment_id: nyar::Identifier,
    exported_operations: Vec<ItemInstanceId>,
    required_capabilities: Vec<nyar::CapabilityTag>,
    theory_bundle: nyar::TheoryBundle,
    entry_operation: Option<ItemInstanceId>,
    wasm_export_names: BTreeMap<ItemInstanceId, String>,
    external_import_links: BTreeMap<ItemInstanceId, nyar::ExternalImportLink>,
    external_call_edges: Vec<nyar_types::CanonicalExternalCallEdge>,
    internal_call_edges: Vec<nyar_types::CanonicalCallEdge>,
    aggregate_layouts: nyar_types::AggregateLayoutPlan,
    aggregate_layout_by_nominal: BTreeMap<nyar_types::NominalInstanceId, nyar_types::LayoutId>,
    aggregate_layout_by_field: BTreeMap<nyar_types::FieldId, (nyar_types::LayoutId, u32)>,
    aggregate_layout_by_type: BTreeMap<nyar_types::TypeId, nyar_types::LayoutId>,
    flags_types: Vec<nyar_types::FlagsLayout>,
    singleton_instances: Vec<nyar_types::SingletonInstancePlan>,
    functions: BTreeMap<ItemInstanceId, ExecutableFunction>,
    abi_names: BTreeMap<ItemInstanceId, QualifiedName>,
    imports: BTreeMap<ItemInstanceId, BackendImport>,
    sum_reps: BTreeMap<nyar_types::NominalInstanceId, nyar_types::layout_choice::SumRepresentation>,
}

/// 已由 Compiler 绑定身份、签名和链接合同的导入。
#[derive(Debug, Clone)]
pub struct BackendImport {
    /// Canonical 导入索引。
    pub index: nyar_types::ImportIndex,
    /// 外部链接合同。
    pub link: nyar_types::ExternalImportLink,
    /// 参数类型列表。
    pub parameter_types: Vec<NyarType>,
    /// 返回类型。
    pub return_type: NyarType,
}

impl TargetExecutionPlan {
    pub(crate) fn from_entry_function(function: ExecutableFunction) -> Self {
        let entry = ItemInstanceId::from_index(0).expect("测试入口身份");
        let name = QualifiedName::new(vec![nyar::Identifier::new("main")]);
        Self {
            entry_operation: Some(entry),
            functions: BTreeMap::from([(entry, function)]),
            abi_names: BTreeMap::from([(entry, name)]),
            ..Self::default()
        }
    }

    /// 从 ABI 名称表构造测试用目标私有计划。
    #[doc(hidden)]
    pub fn from_functions(functions: BTreeMap<QualifiedName, ExecutableFunction>) -> Self {
        let mut plan = Self::default();
        for (index, (name, function)) in functions.into_iter().enumerate() {
            let instance = ItemInstanceId::from_index(u32::try_from(index).expect("测试函数身份溢出")).expect("测试函数身份溢出");
            plan.abi_names.insert(instance, name);
            plan.functions.insert(instance, function);
        }
        plan
    }

    /// 返回全部可执行函数表（集成测试专用）。
    #[doc(hidden)]
    pub fn testing_functions(&self) -> &BTreeMap<ItemInstanceId, ExecutableFunction> {
        &self.functions
    }

    /// 返回可写函数表（集成测试专用）。
    #[doc(hidden)]
    pub fn testing_functions_mut(&mut self) -> &mut BTreeMap<ItemInstanceId, ExecutableFunction> {
        &mut self.functions
    }

    /// 返回可写 wasm 导出名表（集成测试专用）。
    #[doc(hidden)]
    pub fn testing_wasm_export_names_mut(&mut self) -> &mut BTreeMap<ItemInstanceId, String> {
        &mut self.wasm_export_names
    }

    /// 返回可写导出实例表（集成测试专用）。
    #[doc(hidden)]
    pub fn testing_exported_operations_mut(&mut self) -> &mut Vec<ItemInstanceId> {
        &mut self.exported_operations
    }

    /// 设置模块名（集成测试专用）。
    #[doc(hidden)]
    pub fn testing_module_name(&mut self, name: impl Into<String>) -> &mut Self {
        self.module_name = name.into();
        self
    }

    /// 设置入口操作（集成测试专用）。
    #[doc(hidden)]
    pub fn testing_entry_operation(&mut self, name: QualifiedName) -> &mut Self {
        if let Some((instance, _)) = self.abi_names.iter().find(|(_, abi)| **abi == name) {
            self.entry_operation = Some(*instance);
        }
        self
    }

    /// 设置聚合布局计划（集成测试专用）。
    #[doc(hidden)]
    pub fn testing_aggregate_layouts(&mut self, layouts: nyar_types::AggregateLayoutPlan) -> &mut Self {
        self.aggregate_layouts = layouts;
        self
    }

    /// 登记 nominal → layout，并写入 `aggregate_layouts`（集成测试专用）。
    #[doc(hidden)]
    pub fn testing_register_nominal_layout(
        &mut self,
        nominal: nyar_types::NominalInstanceId,
        layout: nyar_types::AggregateLayout,
    ) -> &mut Self {
        let layout_id = layout.id;
        if !self.aggregate_layouts.layouts.iter().any(|item| item.id == layout_id) {
            self.aggregate_layouts.layouts.push(layout);
        }
        self.aggregate_layout_by_nominal.insert(nominal, layout_id);
        self
    }

    /// 登记字段布局索引（集成测试专用）。
    #[doc(hidden)]
    pub fn testing_register_field(&mut self, field: nyar_types::FieldId, layout_id: nyar_types::LayoutId, slot: u32) -> &mut Self {
        self.aggregate_layout_by_field.insert(field, (layout_id, slot));
        self
    }

    /// 从完整 `CompiledProgram` 生成闭包；任何无法无损投影的语义都失败。
    pub fn from_compiled_program(
        program: &CompiledProgram,
        fragment_id: &nyar::Identifier,
        theory_bundle: nyar::TheoryBundle,
        roots: &[ItemInstanceId],
    ) -> Result<Self> {
        let canonical = program.canonical();
        let fragment = canonical.linked.fragments.get(fragment_id).ok_or_else(|| miette!("Canonical 片段 `{fragment_id}` 不存在"))?;
        let mut declared_imports = BTreeMap::new();
        for (index, import) in &canonical.linked.imports {
            if canonical.mir.functions.contains_key(&import.callee) {
                return Err(miette!("callable 实例 `{:?}` 同时绑定函数体与导入", import.callee));
            }
            if declared_imports.insert(import.callee, (*index, import)).is_some() {
                return Err(miette!("callable 实例 `{:?}` 绑定多个 ImportIndex", import.callee));
            }
        }
        let mut pending = roots
            .iter()
            .copied()
            .map(|instance| {
                if !canonical.mir.functions.contains_key(&instance) {
                    return Err(miette!("Compiler callable 实例 `{instance:?}` 缺少 canonical 函数体"));
                }
                Ok(instance)
            })
            .collect::<Result<Vec<_>>>()?;
        let mut seen = std::collections::BTreeSet::new();
        let mut functions = BTreeMap::new();
        let mut abi_names = BTreeMap::new();
        let mut imports = BTreeMap::new();
        while let Some(instance) = pending.pop() {
            if !seen.insert(instance) {
                continue;
            }
            if let Some((index, import)) = declared_imports.get(&instance) {
                imports.insert(
                    instance,
                    BackendImport {
                        index: *index,
                        link: import.link.clone(),
                        parameter_types: import.parameter_types.iter().map(|ty| lower_type(canonical, *ty)).collect::<Result<_>>()?,
                        return_type: lower_type(canonical, import.return_type)?,
                    },
                );
                continue;
            }
            let name = canonical.linked.callable_names.get(&instance).ok_or_else(|| miette!("callable 实例 `{instance:?}` 缺少 ABI 名称"))?;
            abi_names.insert(instance, name.clone());
            let function = canonical.mir.functions.get(&instance).ok_or_else(|| miette!("callable `{name}` 缺少 canonical 函数体"))?;
            let (lowered, callees) = lower_function(program, function)?;
            pending.extend(callees);
            functions.insert(instance, lowered);
        }
        Ok(Self {
            module_name: canonical.linked.module_name.clone(),
            fragment_id: fragment.id.clone(),
            exported_operations: fragment.exported_operations.clone(),
            required_capabilities: fragment.required_capabilities.clone(),
            theory_bundle,
            entry_operation: fragment.entry_operation,
            wasm_export_names: fragment.wasm_export_names.clone(),
            external_import_links: fragment.external_imports.clone(),
            external_call_edges: fragment.external_call_edges.clone(),
            internal_call_edges: fragment.internal_call_edges.clone(),
            aggregate_layouts: canonical.linked.aggregate_layouts.clone(),
            aggregate_layout_by_nominal: canonical.linked.aggregate_layout_by_nominal.clone(),
            aggregate_layout_by_field: canonical.linked.aggregate_layout_by_field.clone(),
            aggregate_layout_by_type: canonical.linked.aggregate_layout_by_type.clone(),
            flags_types: canonical.linked.flags_types.clone(),
            singleton_instances: canonical.linked.singleton_instances.clone(),
            functions,
            abi_names,
            imports,
            sum_reps: program.representation().sum_reps.clone(),
        })
    }

    /// 返回模块名。
    pub fn module_name(&self) -> &str {
        &self.module_name
    }

    /// 返回片段标识。
    pub fn fragment_id(&self) -> &nyar::Identifier {
        &self.fragment_id
    }

    /// 返回已导出的 callable 实例列表。
    pub fn exported_operations(&self) -> &[ItemInstanceId] {
        &self.exported_operations
    }

    /// 返回片段所需 capability 标签。
    pub fn required_capabilities(&self) -> &[nyar::CapabilityTag] {
        &self.required_capabilities
    }

    /// 返回理论束合同。
    pub fn theory_bundle(&self) -> &nyar::TheoryBundle {
        &self.theory_bundle
    }

    /// 返回入口 callable 实例（若有）。
    pub fn entry_operation(&self) -> Option<ItemInstanceId> {
        self.entry_operation
    }

    /// 返回 wasm 导出名映射。
    pub fn wasm_export_names(&self) -> &BTreeMap<ItemInstanceId, String> {
        &self.wasm_export_names
    }

    /// 返回外部导入链接映射。
    pub fn external_import_links(&self) -> &BTreeMap<ItemInstanceId, nyar::ExternalImportLink> {
        &self.external_import_links
    }

    /// 返回外部调用边列表。
    pub fn external_call_edges(&self) -> &[nyar_types::CanonicalExternalCallEdge] {
        &self.external_call_edges
    }

    /// 返回内部调用边列表。
    pub fn internal_call_edges(&self) -> &[nyar_types::CanonicalCallEdge] {
        &self.internal_call_edges
    }

    /// 返回聚合布局计划。
    pub fn aggregate_layouts(&self) -> &nyar_types::AggregateLayoutPlan {
        &self.aggregate_layouts
    }

    /// 返回 nominal 实例到布局 ID 的映射。
    pub fn aggregate_layout_by_nominal(&self) -> &BTreeMap<nyar_types::NominalInstanceId, nyar_types::LayoutId> {
        &self.aggregate_layout_by_nominal
    }

    /// 返回字段 ID 到布局槽位的映射。
    pub fn aggregate_layout_by_field(&self) -> &BTreeMap<nyar_types::FieldId, (nyar_types::LayoutId, u32)> {
        &self.aggregate_layout_by_field
    }

    /// 返回类型 ID 到布局 ID 的映射。
    pub fn aggregate_layout_by_type(&self) -> &BTreeMap<nyar_types::TypeId, nyar_types::LayoutId> {
        &self.aggregate_layout_by_type
    }

    /// 返回 flags 类型布局列表。
    pub fn flags_types(&self) -> &[nyar_types::FlagsLayout] {
        &self.flags_types
    }

    /// 返回单例实例计划列表。
    pub fn singleton_instances(&self) -> &[nyar_types::SingletonInstancePlan] {
        &self.singleton_instances
    }

    /// 返回已声明导入映射。
    pub fn imports(&self) -> &BTreeMap<ItemInstanceId, BackendImport> {
        &self.imports
    }

    /// 返回全部 callable 实例 ID。
    pub fn instances(&self) -> Vec<ItemInstanceId> {
        self.functions.keys().copied().collect()
    }

    /// 按实例 ID 查询函数视图。
    pub fn get_function(&self, instance: &ItemInstanceId) -> Option<FunctionView> {
        self.functions.get(instance).cloned().map(|function| FunctionView { function })
    }

    /// 按实例 ID 查询 ABI 名称。
    pub fn abi_name_for_instance(&self, instance: ItemInstanceId) -> Option<QualifiedName> {
        self.abi_names.get(&instance).cloned()
    }

    /// 按实例 ID 查询挂起元数据视图。
    pub fn suspend_metadata(&self, instance: ItemInstanceId) -> Option<SuspendMetadataView> {
        self.functions.get(&instance).and_then(SuspendMetadataView::from_function)
    }

    /// 返回 sum 类型表示合同映射。
    pub fn sum_representations(&self) -> &BTreeMap<nyar_types::NominalInstanceId, nyar_types::layout_choice::SumRepresentation> {
        &self.sum_reps
    }
}

fn lower_function(program: &CompiledProgram, function: &nyar_types::CanonicalFunction) -> Result<(ExecutableFunction, Vec<ItemInstanceId>)> {
    let canonical = program.canonical();
    for value in function.value_types.keys() {
        let identity = nyar_types::ValueIdentity::new(function.instance, *value);
        match program.representation().value_reps.get(&identity) {
            Some(ValueRepresentation::Specialized) => {}
            Some(representation) => {
                return Err(miette!("值 `{identity:?}` 的表示 `{representation:?}` 尚无目标私有载体合同，拒绝按语义类型重新选择表示"));
            }
            None => {
                return Err(miette!("值 `{identity:?}` 缺少 RepresentationPlan 载体合同"));
            }
        }
    }
    for block in function.blocks.values() {
        for instruction in &block.instructions {
            for (nominal, variant) in canonical_sum_operations(instruction) {
                let Some(sum) = program.representation().sum_reps.get(&nominal)
                else {
                    return Err(miette!("sum `{nominal:?}` 缺少 RepresentationPlan 布局合同"));
                };
                if !sum.variants.contains_key(&variant) {
                    return Err(miette!("sum variant `{nominal:?}/{variant:?}` 缺少 RepresentationPlan 布局合同"));
                }
            }
            if let CanonicalOperation::Invoke { callee, .. } = &instruction.operation {
                match (callee, program.representation().invoke_lowerings.get(&instruction.id)) {
                    (CanonicalCallee::Item(_), Some(InvokeLowering::Direct)) => {}
                    (_, None) => {
                        return Err(miette!("调用指令 `{}` 缺少 RepresentationPlan 降低合同", instruction.id.index()));
                    }
                    (_, Some(lowering)) => {
                        return Err(miette!(
                            "调用指令 `{}` 的表示 `{lowering:?}` 与 callee `{callee:?}` 尚无目标私有调用合同，拒绝改用普通调用",
                            instruction.id.index()
                        ));
                    }
                }
            }
        }
    }
    let mut values =
        function.value_types.keys().map(|value| Value { id: ValueRef(value.index()), origin: ValueOrigin::Temporary }).collect::<Vec<_>>();
    for (index, (value, _)) in function.parameters.iter().enumerate() {
        let row = values.iter_mut().find(|row| row.id.0 == value.index()).ok_or_else(|| miette!("入口参数缺少 SSA 值合同"))?;
        row.origin = ValueOrigin::Parameter { index, name: format!("arg{index}") };
    }
    let mut callees = Vec::new();
    let mut blocks = Vec::new();
    for block in function.blocks.values() {
        let instructions = block
            .instructions
            .iter()
            .map(|instruction| {
                Ok(Instruction {
                    id: instruction.id,
                    results: instruction.results.iter().map(|value| ValueRef(value.index())).collect(),
                    kind: lower_operation(canonical, function, instruction, &mut callees)?,
                    provenance: nyar_types::ProvenanceId::from_index(instruction.id.index()).ok_or_else(|| miette!("指令 identity 溢出"))?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        blocks.push(Block {
            id: BlockRef(block.id.0),
            label: format!("block_{}", block.id.0),
            parameters: block.parameters.iter().map(|(value, _)| ValueRef(value.index())).collect(),
            instructions,
            terminator: lower_terminator(block.terminator.clone())?,
        });
    }
    let symbol = canonical.linked.callable_names.get(&function.instance).ok_or_else(|| miette!("函数实例缺少 ABI 名称"))?.to_string();
    let type_of = |id| lower_type(canonical, id);
    let value_layouts = function
        .value_types
        .iter()
        .filter_map(|(value, type_id)| {
            canonical.linked.aggregate_layout_by_type.get(type_id).copied().map(|layout| (ValueRef(value.index()), layout))
        })
        .collect();
    let return_layout = canonical.linked.aggregate_layout_by_type.get(&function.return_type).copied();
    Ok((
        ExecutableFunction {
            symbol,
            return_type: type_of(function.return_type)?,
            param_types: function.parameters.iter().map(|(_, id)| type_of(*id)).collect::<Result<_>>()?,
            value_types: function.value_types.iter().map(|(value, id)| Ok((ValueRef(value.index()), type_of(*id)?))).collect::<Result<_>>()?,
            value_layouts,
            return_layout,
            entry: BlockRef(function.entry.0),
            values,
            suspend_points: Vec::new(),
            frame_layouts: Vec::new(),
            continuations: Vec::new(),
            case_chains: Vec::new(),
            #[allow(deprecated)]
            state_machine: None,
            suspend_plan: None,
            blocks,
            diagnostics: Vec::new(),
        },
        callees,
    ))
}

fn canonical_sum_operations(instruction: &nyar_types::CanonicalInstruction) -> Vec<(nyar_types::NominalInstanceId, nyar_types::VariantId)> {
    match instruction.operation {
        CanonicalOperation::SumNew { nominal, variant, .. }
        | CanonicalOperation::SumPayloadGet { nominal, variant, .. }
        | CanonicalOperation::SumVariantIs { nominal, variant, .. } => vec![(nominal, variant)],
        _ => Vec::new(),
    }
}

fn lower_operation(
    program: &CanonicalProgram,
    function: &nyar_types::CanonicalFunction,
    instruction: &nyar_types::CanonicalInstruction,
    callees: &mut Vec<ItemInstanceId>,
) -> Result<InstructionKind> {
    let value = |id: nyar_types::MirValueId| Operand::Value(ValueRef(id.index()));
    let layout_for_type = |type_id: nyar_types::TypeId, operation: &str| {
        program
            .linked
            .aggregate_layout_by_type
            .get(&type_id)
            .copied()
            .ok_or_else(|| miette!("{operation} 缺少编译器绑定的布局身份: {type_id:?}"))
    };
    let value_type = |value: nyar_types::MirValueId| {
        function.value_types.get(&value).copied().ok_or_else(|| miette!("聚合值 `{value:?}` 缺少 Semantic MIR 类型事实"))
    };
    Ok(match &instruction.operation {
        CanonicalOperation::Invoke { callee: CanonicalCallee::Item(instance), arguments } => {
            if !program.linked.item_instances.contains_key(instance) {
                return Err(miette!("调用 identity `{instance:?}` 未解析"));
            }
            callees.push(*instance);
            InstructionKind::Call { callee: Operand::Item(*instance), arguments: arguments.iter().map(|id| value(*id)).collect() }
        }
        CanonicalOperation::Invoke { callee: CanonicalCallee::Value(callee), arguments } => {
            InstructionKind::Call { callee: value(*callee), arguments: arguments.iter().map(|id| value(*id)).collect() }
        }
        CanonicalOperation::Copy { source } => InstructionKind::Copy { source: value(*source) },
        CanonicalOperation::StoreVar { name, value: stored, ty } => {
            InstructionKind::StoreVar { name: name.clone(), value: value(*stored), ty: ty.map(|id| lower_type(program, id)).transpose()? }
        }
        CanonicalOperation::AggregateCopy { source, destination } => {
            let source_type = value_type(*source)?;
            let destination_type = value_type(*destination)?;
            if source_type != destination_type {
                return Err(miette!("AggregateCopy 源与目标类型不一致: {source_type:?} != {destination_type:?}"));
            }
            InstructionKind::AggregateCopy {
                layout_id: layout_for_type(source_type, "AggregateCopy")?,
                source: value(*source),
                dest: value(*destination),
            }
        }
        CanonicalOperation::LoadConstant { constant } => InstructionKind::LoadConstant { constant: lower_constant(constant), ty: None },
        CanonicalOperation::ArrayGet { array, index } => InstructionKind::ArrayGet { array: value(*array), index: value(*index) },
        CanonicalOperation::ArraySet { array, index, value: stored } => {
            InstructionKind::ArraySet { array: value(*array), index: value(*index), value: value(*stored) }
        }
        CanonicalOperation::ArrayLength { array } => InstructionKind::ArrayLength { array: value(*array) },
        CanonicalOperation::ArrayPush { array, value: pushed } => InstructionKind::ArrayPush { array: value(*array), value: value(*pushed) },
        CanonicalOperation::NullIsNull { value: operand } => InstructionKind::NullIsNull { value: value(*operand) },
        CanonicalOperation::NullUnwrap { value: operand } => InstructionKind::NullUnwrap { value: value(*operand) },
        CanonicalOperation::ArrayNew { array_type, length, initialization } => InstructionKind::ArrayNew {
            array_type: lower_type(program, *array_type)?,
            length: value(*length),
            initialization: match initialization {
                CanonicalArrayInitialization::Default => nyar_types::ArrayInitialization::Default,
                CanonicalArrayInitialization::Fill(fill) => nyar_types::ArrayInitialization::Fill(value(*fill)),
            },
        },
        CanonicalOperation::ArrayFromElements { array_type, elements } => InstructionKind::ArrayFromElements {
            layout_id: layout_for_type(*array_type, "ArrayFromElements")?,
            array_type: lower_type(program, *array_type)?,
            elements: elements.iter().map(|id| value(*id)).collect(),
        },
        CanonicalOperation::TupleNew { fields, element_types } => {
            let result = instruction.results.first().copied().ok_or_else(|| miette!("TupleNew 缺少结果值"))?;
            let result_type = value_type(result)?;
            if !matches!(program.linked.types.get(&result_type).map(|row| &row.kind), Some(nyar_types::CanonicalTypeKind::Tuple(_))) {
                return Err(miette!("TupleNew 结果不是 tuple 类型: {result_type:?}"));
            }
            if fields.len() != element_types.len() {
                return Err(miette!("TupleNew 元素合同长度不一致"));
            }
            InstructionKind::TupleNew {
                layout_id: layout_for_type(result_type, "TupleNew")?,
                fields: fields.iter().map(|id| value(*id)).collect(),
            }
        }
        CanonicalOperation::StructNew { nominal, fields } => {
            InstructionKind::StructNew { nominal: *nominal, fields: fields.iter().map(|(field, id)| (*field, value(*id))).collect() }
        }
        CanonicalOperation::FieldGet { object, field } => InstructionKind::FieldGet { object: value(*object), field: *field },
        CanonicalOperation::FieldSet { object, field, value: stored } => {
            InstructionKind::FieldSet { object: value(*object), field: *field, value: value(*stored) }
        }
        CanonicalOperation::SumNew { nominal, variant, payload } => {
            InstructionKind::SumNew { nominal: *nominal, variant: *variant, payload: payload.map(value) }
        }
        CanonicalOperation::SumPayloadGet { nominal, variant, object } => {
            InstructionKind::SumPayloadGet { nominal: *nominal, variant: *variant, object: value(*object) }
        }
        CanonicalOperation::SumVariantIs { nominal, variant, object } => {
            InstructionKind::SumVariantIs { nominal: *nominal, variant: *variant, object: value(*object) }
        }
        unsupported => return Err(miette!("canonical 操作尚无目标私有合同: {unsupported:?}")),
    })
}

fn lower_constant(constant: &CanonicalConstant) -> Constant {
    match constant {
        CanonicalConstant::Int(v) => Constant::Int(*v),
        CanonicalConstant::Bool(v) => Constant::Bool(*v),
        CanonicalConstant::Utf8(v) => Constant::Utf8(v.clone()),
        CanonicalConstant::Utf16(v) => Constant::Utf16(v.clone()),
        CanonicalConstant::Unit => Constant::Unit,
    }
}

fn lower_type(program: &CanonicalProgram, id: nyar_types::TypeId) -> Result<NyarType> {
    let kind = &program.linked.types.get(&id).ok_or_else(|| miette!("canonical 类型事实缺失"))?.kind;
    Ok(match kind {
        CanonicalTypeKind::Primitive(kind) => match kind {
            nyar_types::CanonicalPrimitiveType::Void => NyarType::Bottom,
            nyar_types::CanonicalPrimitiveType::Unit => NyarType::Unit,
            nyar_types::CanonicalPrimitiveType::Bool => NyarType::Boolean,
            nyar_types::CanonicalPrimitiveType::Integer { bits: 32, signed } => NyarType::Integer32 { signed: *signed },
            nyar_types::CanonicalPrimitiveType::Integer { bits: 64, signed } => NyarType::Integer64 { signed: *signed },
            nyar_types::CanonicalPrimitiveType::Utf8 => NyarType::Utf8,
            nyar_types::CanonicalPrimitiveType::Utf16 => NyarType::Utf16,
            unsupported => return Err(miette!("primitive 尚无目标合同: {unsupported:?}")),
        },
        CanonicalTypeKind::Tuple(items) => NyarType::Tuple(items.iter().map(|id| lower_type(program, *id)).collect::<Result<_>>()?),
        CanonicalTypeKind::Array { element, length: None } => NyarType::Array(Box::new(lower_type(program, *element)?)),
        CanonicalTypeKind::Array { element, length: Some(length) } => NyarType::FixedArray {
            element: Box::new(lower_type(program, *element)?),
            length: usize::try_from(*length).map_err(|_| miette!("固定数组长度溢出"))?,
        },
        CanonicalTypeKind::Nullable(element) => NyarType::Nullable(Box::new(lower_type(program, *element)?)),
        unsupported => return Err(miette!("canonical 类型尚无目标合同: {unsupported:?}")),
    })
}

fn lower_terminator(terminator: CanonicalTerminator) -> Result<Terminator> {
    let value = |id: nyar_types::MirValueId| Operand::Value(ValueRef(id.index()));
    Ok(match terminator {
        CanonicalTerminator::Return { value: result } => Terminator::Return { value: result.map(value) },
        CanonicalTerminator::Jump { target, arguments } => {
            Terminator::Jump { target: BlockRef(target.0), arguments: arguments.into_iter().map(value).collect() }
        }
        CanonicalTerminator::Branch { condition, then_target, else_target } => {
            Terminator::Branch { condition: value(condition), then_target: BlockRef(then_target.0), else_target: BlockRef(else_target.0) }
        }
        CanonicalTerminator::Unreachable => Terminator::Unreachable,
        unsupported => {
            return Err(miette!("canonical effect terminator 尚无目标合同: {unsupported:?}"));
        }
    })
}
