//! Compiler 成功载荷到目标私有执行计划的唯一边界。

use std::collections::BTreeMap;

use miette::{Result, miette};
use nyar::QualifiedName;
use nyar_types::{
    CanonicalArrayInitialization, CanonicalCallee, CanonicalConstant, CanonicalOperation, CanonicalProgram, CanonicalTerminator,
    CanonicalTypeKind, CompiledProgram, Constant, Instruction, InstructionKind, ItemInstanceId, NyarType, Operand, Terminator, Value, ValueOrigin,
};
use nyar_types::layout_choice::{InvokeLowering, ValueRepresentation};

use crate::{
    contracts::{Block, BlockRef, ValueRef},
    backend_plan_views::{ExecutableFunction, FunctionView, SuspendMetadataView},
};

/// 已完成 callable、类型、CFG 和表示合同绑定的目标私有计划。
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(Default))]
pub struct BackendPrivatePlan {
    functions: BTreeMap<ItemInstanceId, ExecutableFunction>,
    abi_names: BTreeMap<ItemInstanceId, QualifiedName>,
    imports: BTreeMap<ItemInstanceId, BackendImport>,
    sum_reps: BTreeMap<nyar_types::NominalInstanceId, nyar_types::layout_choice::SumRepresentation>,
}

/// 已由 Compiler 绑定身份、签名和链接合同的导入。
#[derive(Debug, Clone)]
pub struct BackendImport {
    pub index: nyar_types::ImportIndex,
    pub link: nyar_types::ExternalImportLink,
    pub parameter_types: Vec<NyarType>,
    pub return_type: NyarType,
}

impl BackendPrivatePlan {
    #[cfg(test)]
    pub(crate) fn from_functions(functions: BTreeMap<QualifiedName, ExecutableFunction>) -> Self {
        let mut plan = Self::default();
        for (index, (name, function)) in functions.into_iter().enumerate() {
            let instance = ItemInstanceId::from_index(u32::try_from(index).expect("测试函数身份溢出")).expect("测试函数身份溢出");
            plan.abi_names.insert(instance, name);
            plan.functions.insert(instance, function);
        }
        plan
    }

    /// 从完整 `CompiledProgram` 生成闭包；任何无法无损投影的语义都失败。
    pub fn from_compiled_program(program: &CompiledProgram, roots: &[ItemInstanceId]) -> Result<Self> {
        let canonical = program.canonical();
        let mut declared_imports = BTreeMap::new();
        for (index, import) in &canonical.linked.imports {
            if canonical.mir.functions.contains_key(&import.callee) {
                return Err(miette!("callable 实例 `{:?}` 同时绑定函数体与导入", import.callee));
            }
            if declared_imports.insert(import.callee, (*index, import)).is_some() {
                return Err(miette!("callable 实例 `{:?}` 绑定多个 ImportIndex", import.callee));
            }
        }
        let mut pending = roots.iter().copied().map(|instance| {
            if !canonical.mir.functions.contains_key(&instance) {
                return Err(miette!("Compiler callable 实例 `{instance:?}` 缺少 canonical 函数体"));
            }
            Ok(instance)
        }).collect::<Result<Vec<_>>>()?;
        let mut seen = std::collections::BTreeSet::new();
        let mut functions = BTreeMap::new();
        let mut abi_names = BTreeMap::new();
        let mut imports = BTreeMap::new();
        while let Some(instance) = pending.pop() {
            if !seen.insert(instance) { continue; }
            if let Some((index, import)) = declared_imports.get(&instance) {
                imports.insert(instance, BackendImport {
                    index: *index,
                    link: import.link.clone(),
                    parameter_types: import.parameter_types.iter().map(|ty| lower_type(canonical, *ty)).collect::<Result<_>>()?,
                    return_type: lower_type(canonical, import.return_type)?,
                });
                continue;
            }
            let name = canonical.linked.callable_names.get(&instance)
                .ok_or_else(|| miette!("callable 实例 `{instance:?}` 缺少 ABI 名称"))?;
            abi_names.insert(instance, name.clone());
            let function = canonical.mir.functions.get(&instance)
                .ok_or_else(|| miette!("callable `{name}` 缺少 canonical 函数体"))?;
            let (lowered, callees) = lower_function(program, function)?;
            pending.extend(callees);
            functions.insert(instance, lowered);
        }
        Ok(Self { functions, abi_names, imports, sum_reps: program.representation().sum_reps.clone() })
    }

    pub fn imports(&self) -> &BTreeMap<ItemInstanceId, BackendImport> {
        &self.imports
    }

    pub fn instances(&self) -> Vec<ItemInstanceId> {
        self.functions.keys().copied().collect()
    }

    pub fn get_function(&self, instance: &ItemInstanceId) -> Option<FunctionView> {
        self.functions.get(instance).cloned().map(|function| FunctionView { function })
    }

    pub fn abi_name_for_instance(&self, instance: ItemInstanceId) -> Option<QualifiedName> {
        self.abi_names.get(&instance).cloned()
    }

    pub fn suspend_metadata(&self, instance: ItemInstanceId) -> Option<SuspendMetadataView> {
        self.functions.get(&instance).and_then(SuspendMetadataView::from_function)
    }

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
            Some(representation) => return Err(miette!(
                "值 `{identity:?}` 的表示 `{representation:?}` 尚无目标私有载体合同，拒绝按语义类型重新选择表示"
            )),
            None => return Err(miette!("值 `{identity:?}` 缺少 RepresentationPlan 载体合同")),
        }
    }
    for block in function.blocks.values() {
        for instruction in &block.instructions {
            for (nominal, variant) in canonical_sum_operations(instruction) {
                let Some(sum) = program.representation().sum_reps.get(&nominal) else {
                    return Err(miette!("sum `{nominal:?}` 缺少 RepresentationPlan 布局合同"));
                };
                if !sum.variants.contains_key(&variant) {
                    return Err(miette!("sum variant `{nominal:?}/{variant:?}` 缺少 RepresentationPlan 布局合同"));
                }
            }
            if let CanonicalOperation::Invoke { callee, .. } = &instruction.operation {
                match (callee, program.representation().invoke_lowerings.get(&instruction.id)) {
                    (CanonicalCallee::Item(_), Some(InvokeLowering::Direct)) => {}
                    (_, None) => return Err(miette!("调用指令 `{}` 缺少 RepresentationPlan 降低合同", instruction.id.index())),
                    (_, Some(lowering)) => return Err(miette!(
                        "调用指令 `{}` 的表示 `{lowering:?}` 与 callee `{callee:?}` 尚无目标私有调用合同，拒绝改用普通调用",
                        instruction.id.index()
                    )),
                }
            }
        }
    }
    let mut values = function.value_types.keys().map(|value| Value { id: ValueRef(value.index()), origin: ValueOrigin::Temporary }).collect::<Vec<_>>();
    for (index, (value, _)) in function.parameters.iter().enumerate() {
        let row = values.iter_mut().find(|row| row.id.0 == value.index()).ok_or_else(|| miette!("入口参数缺少 SSA 值合同"))?;
        row.origin = ValueOrigin::Parameter { index, name: format!("arg{index}") };
    }
    let mut callees = Vec::new();
    let mut blocks = Vec::new();
    for block in function.blocks.values() {
        let instructions = block.instructions.iter().map(|instruction| {
            Ok(Instruction {
                id: instruction.id,
                results: instruction.results.iter().map(|value| ValueRef(value.index())).collect(),
                kind: lower_operation(canonical, function, instruction, &mut callees)?,
                provenance: nyar_types::ProvenanceId::from_index(instruction.id.index()).ok_or_else(|| miette!("指令 identity 溢出"))?,
            })
        }).collect::<Result<Vec<_>>>()?;
        blocks.push(Block {
            id: BlockRef(block.id.0), label: format!("block_{}", block.id.0),
            parameters: block.parameters.iter().map(|(value, _)| ValueRef(value.index())).collect(),
            instructions, terminator: lower_terminator(block.terminator.clone())?,
        });
    }
    let symbol = canonical.linked.callable_names.get(&function.instance).ok_or_else(|| miette!("函数实例缺少 ABI 名称"))?.to_string();
    let type_of = |id| lower_type(canonical, id);
    let value_layouts = function
        .value_types
        .iter()
        .filter_map(|(value, type_id)| canonical.linked.aggregate_layout_by_type.get(type_id).copied().map(|layout| (ValueRef(value.index()), layout)))
        .collect();
    let return_layout = canonical.linked.aggregate_layout_by_type.get(&function.return_type).copied();
    Ok((ExecutableFunction {
        symbol,
        return_type: type_of(function.return_type)?,
        param_types: function.parameters.iter().map(|(_, id)| type_of(*id)).collect::<Result<_>>()?,
        value_types: function.value_types.iter().map(|(value, id)| Ok((ValueRef(value.index()), type_of(*id)?))).collect::<Result<_>>()?,
        value_layouts,
        return_layout,
        entry: BlockRef(function.entry.0), values, suspend_points: Vec::new(), frame_layouts: Vec::new(), continuations: Vec::new(),
        case_chains: Vec::new(), #[allow(deprecated)] state_machine: None, suspend_plan: None, blocks, diagnostics: Vec::new(),
    }, callees))
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
        function
            .value_types
            .get(&value)
            .copied()
            .ok_or_else(|| miette!("聚合值 `{value:?}` 缺少 Semantic MIR 类型事实"))
    };
    Ok(match &instruction.operation {
        CanonicalOperation::Invoke { callee: CanonicalCallee::Item(instance), arguments } => {
            if !program.linked.item_instances.contains_key(instance) {
                return Err(miette!("调用 identity `{instance:?}` 未解析"));
            }
            callees.push(*instance);
            InstructionKind::Call { callee: Operand::Item(*instance), arguments: arguments.iter().map(|id| value(*id)).collect() }
        }
        CanonicalOperation::Invoke { callee: CanonicalCallee::Value(callee), arguments } => InstructionKind::Call { callee: value(*callee), arguments: arguments.iter().map(|id| value(*id)).collect() },
        CanonicalOperation::Copy { source } => InstructionKind::Copy { source: value(*source) },
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
        CanonicalOperation::ArraySet { array, index, value: stored } => InstructionKind::ArraySet { array: value(*array), index: value(*index), value: value(*stored) },
        CanonicalOperation::ArrayLength { array } => InstructionKind::ArrayLength { array: value(*array) },
        CanonicalOperation::ArrayNew { array_type, length, initialization } => InstructionKind::ArrayNew {
            array_type: lower_type(program, *array_type)?, length: value(*length), initialization: match initialization {
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
        CanonicalOperation::StructNew { nominal, fields } => InstructionKind::StructNew {
            nominal: *nominal,
            fields: fields.iter().map(|(field, id)| (*field, value(*id))).collect(),
        },
        CanonicalOperation::FieldGet { object, field } => InstructionKind::FieldGet { object: value(*object), field: *field },
        CanonicalOperation::FieldSet { object, field, value: stored } => InstructionKind::FieldSet {
            object: value(*object), field: *field, value: value(*stored),
        },
        CanonicalOperation::SumNew { nominal, variant, payload } => InstructionKind::SumNew {
            nominal: *nominal, variant: *variant, payload: payload.map(value),
        },
        CanonicalOperation::SumPayloadGet { nominal, variant, object } => InstructionKind::SumPayloadGet {
            nominal: *nominal, variant: *variant, object: value(*object),
        },
        CanonicalOperation::SumVariantIs { nominal, variant, object } => InstructionKind::SumVariantIs {
            nominal: *nominal, variant: *variant, object: value(*object),
        },
        unsupported => return Err(miette!("canonical 操作尚无目标私有合同: {unsupported:?}")),
    })
}

fn lower_constant(constant: &CanonicalConstant) -> Constant {
    match constant { CanonicalConstant::Int(v) => Constant::Int(*v), CanonicalConstant::Bool(v) => Constant::Bool(*v), CanonicalConstant::Utf8(v) => Constant::Utf8(v.clone()), CanonicalConstant::Utf16(v) => Constant::Utf16(v.clone()), CanonicalConstant::Unit => Constant::Unit }
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
        CanonicalTypeKind::Array { element, length: Some(length) } => NyarType::FixedArray { element: Box::new(lower_type(program, *element)?), length: usize::try_from(*length).map_err(|_| miette!("固定数组长度溢出"))? },
        CanonicalTypeKind::Nullable(element) => NyarType::Nullable(Box::new(lower_type(program, *element)?)),
        unsupported => return Err(miette!("canonical 类型尚无目标合同: {unsupported:?}")),
    })
}

fn lower_terminator(terminator: CanonicalTerminator) -> Result<Terminator> {
    let value = |id: nyar_types::MirValueId| Operand::Value(ValueRef(id.index()));
    Ok(match terminator {
        CanonicalTerminator::Return { value: result } => Terminator::Return { value: result.map(value) },
        CanonicalTerminator::Jump { target, arguments } => Terminator::Jump { target: BlockRef(target.0), arguments: arguments.into_iter().map(value).collect() },
        CanonicalTerminator::Branch { condition, then_target, else_target } => Terminator::Branch { condition: value(condition), then_target: BlockRef(then_target.0), else_target: BlockRef(else_target.0) },
        CanonicalTerminator::Unreachable => Terminator::Unreachable,
        unsupported => return Err(miette!("canonical effect terminator 尚无目标合同: {unsupported:?}")),
    })
}

#[cfg(test)]
mod representation_contract_tests {
    use super::*;

    fn source_program() -> CompiledProgram {
        source_program_from(
            "micro identity(value: i32) -> i32 { return value } \
             micro entry(value: i32) -> i32 { return identity(value) }",
        )
    }

    fn source_program_from(source: &str) -> CompiledProgram {
        use nyar_types::pipeline::RepresentationPlanStage;
        let hir = nyar_language::ValkyrieCompiler::default()
            .compile_source(source)
            .expect("单测源码必须完成 HIR 分析");
        let mir = nyar_language::MirLowerer::lower_module_semantic(&hir);
        let canonical = nyar_language::valkyrie::compile_pipeline::canonical_program_from_semantic_mir(&mir)
            .expect("单测 MIR 必须满足 Canonical 合同");
        let representation = nyar_language::valkyrie::compile_pipeline::CanonicalRepresentationPlanner.plan(&canonical)
            .expect("单测 Canonical 必须形成表示计划");
        CompiledProgram::new(canonical, representation).expect("单测表示输入必须一致，不代表生产流水线验收")
    }

    fn prepare(program: &CompiledProgram) -> Result<BackendPrivatePlan> {
        let roots = program.canonical().mir.functions.keys().copied().collect::<Vec<_>>();
        BackendPrivatePlan::from_compiled_program(program, &roots)
    }

    #[test]
    fn source_specialized_values_and_direct_calls_prepare() {
        let program = source_program();
        let plan = prepare(&program).expect("已有完整标量与直接调用合同必须可表达");
        assert_eq!(plan.functions.len(), program.canonical().mir.functions.len());
    }

    #[test]
    fn unknown_root_identity_fails_before_function_projection() {
        let program = source_program();
        let unknown = ItemInstanceId::from_index(1000).expect("构造闭包外身份");
        assert!(!program.canonical().mir.functions.contains_key(&unknown));
        let error = BackendPrivatePlan::from_compiled_program(&program, &[unknown])
            .expect_err("闭包外根身份必须失败，不能改按名称寻找其他函数");
        assert!(error.to_string().contains("缺少 canonical 函数体"), "{error}");
    }

    #[test]
    fn root_identity_is_not_resolved_again_from_abi_spelling() {
        let source = source_program();
        let root = *source.canonical().mir.functions.keys().next().expect("源码有函数");
        let mut canonical = source.canonical().clone();
        canonical.linked.callable_names.insert(root, QualifiedName::new(vec![nyar::Identifier::new("renamed_boundary")]));
        let program = CompiledProgram::new(canonical, source.representation().clone())
            .expect("ABI 名不改变已经验证的调用与表示身份");
        let plan = BackendPrivatePlan::from_compiled_program(&program, &[root])
            .expect("目标准备必须消费同一根身份");
        assert!(plan.functions.contains_key(&root));
        assert_eq!(plan.functions[&root].symbol, "renamed_boundary");
    }

    #[test]
    fn operator_and_builtin_spellings_cannot_bypass_call_identity_validation() {
        let program = source_program();
        for parts in [vec!["infix +"], vec!["builtin", "array", "push"]] {
            let mut plan = prepare(&program).expect("源码计划必须成功");
            let call = plan.functions.values_mut().flat_map(|function| &mut function.blocks)
                .flat_map(|block| &mut block.instructions).find_map(|instruction| match &mut instruction.kind {
                    InstructionKind::Call { callee, .. } => Some(callee),
                    _ => None,
                }).expect("源码包含调用");
            *call = Operand::Symbol(nyar::NamePath::new(parts.iter().map(|part| nyar::Identifier::new(part)).collect()));
            let submission = crate::FragmentSubmission { backend_plan: std::sync::Arc::new(plan), ..Default::default() };
            let error = crate::lowering::features::semantic_mir_contract::validate_submission(&submission)
                .expect_err("拼写属于 builtin 或 operator 也不能替代实例合同");
            assert_eq!(error.code, "SMIR003");
        }
    }

    #[test]
    fn duplicate_abi_labels_preserve_wasm_export_and_call_targets() {
        let source = source_program_from(
            "micro identity(value: i32) -> i32 { return value } \
             micro entry(value: i32) -> i32 { return identity(17) }",
        );
        let mut canonical = source.canonical().clone();
        let exports = canonical.linked.callable_names.clone();
        for name in canonical.linked.callable_names.values_mut() {
            *name = QualifiedName::new(vec![nyar::Identifier::new("same_label")]);
        }
        let program = CompiledProgram::new(canonical, source.representation().clone()).expect("标签不改变语义合同");
        let submission = crate::FragmentSubmission {
            wasm_export_names: exports.iter().map(|(instance, name)| (*instance, name.parts().last().expect("测试声明名称").as_str().to_owned())).collect(),
            backend_plan: std::sync::Arc::new(prepare(&program).expect("独立实例必须可准备")),
            ..Default::default()
        };
        let (module, _) = crate::lowering::backends::wasm::lower_fragment_to_wasm_module(
            &submission, nyar::HostProjectionBoundary::WasmJsGlue,
        ).expect("Wasm 调用和导出只能沿实例编码");
        let directory = tempfile::tempdir().expect("测试目录");
        let artifact = directory.path().join("identity.wasm");
        std::fs::write(&artifact, module.to_bytes().expect("Wasm 编码成功")).expect("写入测试产物");
        let output = std::process::Command::new("node").args(["--input-type=module", "-e",
            "import {readFileSync} from 'node:fs'; const {instance}=await WebAssembly.instantiate(readFileSync(process.argv[1])); if(instance.exports.identity(41)!==41 || instance.exports.entry(41)!==17) throw new Error('wrong instance target');",
        ]).arg(&artifact).output().expect("Node 必须运行");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    }

    #[test]
    fn scalar_library_exports_do_not_require_collection_name_or_layout_glue() {
        let program = source_program_from("micro identity(value: i64) -> i64 { return value }");
        let exports = program.canonical().linked.callable_names.clone();
        let submission = crate::FragmentSubmission {
            wasm_export_names: exports.iter().map(|(instance, name)| (*instance, name.parts().last().expect("测试声明名称").as_str().to_owned())).collect(),
            backend_plan: std::sync::Arc::new(prepare(&program).expect("标量导出有完整实例合同")),
            ..Default::default()
        };
        let (module, _) = crate::lowering::backends::wasm::lower_fragment_to_wasm_module_for(
            &submission, nyar::HostProjectionBoundary::WasmJsGlue,
            crate::nyar_backend_wasi::WasiPreview::Preview2, crate::nyar_backend_wasi::WasmPackageKind::Library,
        ).expect("标量库不得要求 ArrayList 函数或字段布局");
        let section = module.sections.iter().find(|section| section.name.as_deref() == Some("nyar.library_invoke"))
            .expect("必须提供明确导出 ABI");
        let metadata: serde_json::Value = serde_json::from_slice(&section.bytes).expect("ABI JSON 有效");
        assert_eq!(metadata["exports"]["identity"]["params"], serde_json::json!(["i64"]));
        assert_eq!(metadata["exports"]["identity"]["returns"], "i64");
        assert_eq!(metadata["glue"], serde_json::json!({}));
    }

    #[cfg(feature = "nyar-vm-lane")]
    #[test]
    fn duplicate_abi_labels_do_not_redirect_nyar_calls_or_exports() {
        let source = source_program();
        let mut canonical = source.canonical().clone();
        let instances = canonical.mir.functions.keys().copied().collect::<Vec<_>>();
        assert_eq!(instances.len(), 2);
        for instance in &instances {
            canonical.linked.callable_names.insert(*instance, QualifiedName::new(vec![nyar::Identifier::new("same_label")]));
        }
        let program = CompiledProgram::new(canonical, source.representation().clone()).expect("诊断名不能改变实例身份");
        let plan = prepare(&program).expect("相同标签的函数必须保留独立实例");
        let submission = crate::FragmentSubmission {
            exported_operations: instances.clone(),
            wasm_export_names: instances.iter().enumerate().map(|(index, instance)| (*instance, format!("export_{index}"))).collect(),
            backend_plan: std::sync::Arc::new(plan),
            ..Default::default()
        };
        let module = crate::lowering::backends::nyar_vm::lower_fragment_to_nyar_module(&submission)
            .expect("只允许沿已绑定实例发射");
        assert_eq!(module.functions.len(), 2);
        assert_eq!(module.exports.len(), 2);
        for (index, export) in module.exports.iter().enumerate() {
            assert_eq!(export.symbol_name, format!("export_{index}"));
            assert_eq!(export.function_index, index as i32);
        }
        let mut calls = Vec::new();
        let mut position = 0;
        while position < module.code_bytes.len() {
            let instruction = nyar_bytecode::decode_at(&module.code_bytes, position);
            assert!(instruction.size > 0, "字节码必须可解码");
            if instruction.code == nyar_bytecode::NyarHeadCode::Call {
                calls.push(instruction.operand1);
            }
            position += instruction.size as usize;
        }
        let callee = source.canonical().mir.functions.values().flat_map(|function| function.blocks.values())
            .flat_map(|block| &block.instructions).find_map(|instruction| match &instruction.operation {
                CanonicalOperation::Invoke { callee: CanonicalCallee::Item(instance), .. } => Some(*instance),
                _ => None,
            }).expect("源码包含普通调用");
        assert_eq!(calls, vec![instances.iter().position(|instance| *instance == callee).expect("目标实例存在") as i32]);
    }

    #[test]
    fn unsupported_value_choices_do_not_reuse_specialized_encoding() {
        let source = source_program();
        let identity = *source.representation().value_reps.keys().next().expect("源码应有 SSA 值");
        for representation in [
            ValueRepresentation::CompileTimeIdentity,
            ValueRepresentation::Reified,
            ValueRepresentation::ErasedBoxed,
        ] {
            let mut plan = source.representation().clone();
            plan.value_reps.insert(identity, representation.clone());
            let program = CompiledProgram::new(source.canonical().clone(), plan)
                .expect("表示选择的可表达性属于目标准备边界");
            let error = prepare(&program).expect_err("不得忽略表示选择而重用标量编码");
            assert!(error.to_string().contains("目标私有载体合同"), "{error}");
            assert!(error.to_string().contains(&format!("{representation:?}")), "{error}");
        }
    }

    #[test]
    fn unsupported_call_choices_do_not_reuse_direct_encoding() {
        let source = source_program();
        let instruction = *source.representation().invoke_lowerings.keys().next().expect("源码应有普通调用");
        for lowering in [
            InvokeLowering::TypedWitness,
            InvokeLowering::SharedOperationTable,
            InvokeLowering::Specialized,
            InvokeLowering::TypedReference,
        ] {
            let mut plan = source.representation().clone();
            plan.invoke_lowerings.insert(instruction, lowering.clone());
            let program = CompiledProgram::new(source.canonical().clone(), plan)
                .expect("调用表示的可表达性属于目标准备边界");
            let error = prepare(&program).expect_err("不得忽略调用表示而重用直接调用编码");
            assert!(error.to_string().contains("目标私有调用合同"), "{error}");
            assert!(error.to_string().contains(&format!("{lowering:?}")), "{error}");
        }
    }
}
