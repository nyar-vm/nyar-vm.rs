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
    abi_names: BTreeMap<QualifiedName, ItemInstanceId>,
}

impl BackendPrivatePlan {
    #[cfg(test)]
    pub(crate) fn from_functions(functions: BTreeMap<QualifiedName, ExecutableFunction>) -> Self {
        let mut plan = Self::default();
        for (index, (name, function)) in functions.into_iter().enumerate() {
            let instance = ItemInstanceId::from_index(u32::try_from(index).expect("测试函数身份溢出")).expect("测试函数身份溢出");
            plan.abi_names.insert(name, instance);
            plan.functions.insert(instance, function);
        }
        plan
    }

    /// 从完整 `CompiledProgram` 生成闭包；任何无法无损投影的语义都失败。
    pub fn from_compiled_program(program: &CompiledProgram, roots: &[QualifiedName]) -> Result<Self> {
        let canonical = program.canonical();
        let mut pending = roots.iter().map(|name| {
            let mut candidates = canonical.linked.callable_names.iter().filter(|(_, candidate)| *candidate == name);
            let instance = candidates.next().map(|(instance, _)| *instance)
                .ok_or_else(|| miette!("callable `{name}` 缺少 Compiler identity"))?;
            if candidates.next().is_some() {
                return Err(miette!("callable ABI 名称 `{name}` 对应多个实例，拒绝选择第一个实例"));
            }
            Ok(instance)
        }).collect::<Result<Vec<_>>>()?;
        let mut seen = std::collections::BTreeSet::new();
        let mut functions = BTreeMap::new();
        let mut abi_names = BTreeMap::new();
        while let Some(instance) = pending.pop() {
            if !seen.insert(instance) { continue; }
            let name = canonical.linked.callable_names.get(&instance)
                .ok_or_else(|| miette!("callable 实例 `{instance:?}` 缺少 ABI 名称"))?;
            if let Some(previous) = abi_names.insert(name.clone(), instance) {
                return Err(miette!("callable 实例 `{previous:?}` 与 `{instance:?}` 共享 ABI 名称 `{name}`，拒绝覆盖函数体"));
            }
            let function = canonical.mir.functions.get(&instance)
                .ok_or_else(|| miette!("callable `{name}` 缺少 canonical 函数体"))?;
            let (lowered, callees) = lower_function(program, function)?;
            pending.extend(callees);
            functions.insert(instance, lowered);
        }
        Ok(Self { functions, abi_names })
    }

    pub fn operations(&self) -> Vec<QualifiedName> {
        self.abi_names.keys().cloned().collect()
    }

    pub fn get_function(&self, operation: &QualifiedName) -> Option<FunctionView> {
        let instance = self.abi_names.get(operation)?;
        self.functions.get(instance).cloned().map(|function| FunctionView { function })
    }

    pub fn suspend_metadata(&self, operation: &QualifiedName) -> Option<SuspendMetadataView> {
        let instance = self.abi_names.get(operation)?;
        self.functions.get(instance).and_then(SuspendMetadataView::from_function)
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
                kind: lower_operation(canonical, instruction, &mut callees)?,
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
    Ok((ExecutableFunction {
        symbol,
        return_type: type_of(function.return_type)?,
        param_types: function.parameters.iter().map(|(_, id)| type_of(*id)).collect::<Result<_>>()?,
        value_types: function.value_types.iter().map(|(value, id)| Ok((ValueRef(value.index()), type_of(*id)?))).collect::<Result<_>>()?,
        entry: BlockRef(function.entry.0), values, suspend_points: Vec::new(), frame_layouts: Vec::new(), continuations: Vec::new(),
        case_chains: Vec::new(), #[allow(deprecated)] state_machine: None, suspend_plan: None, blocks, diagnostics: Vec::new(),
    }, callees))
}

fn lower_operation(program: &CanonicalProgram, instruction: &nyar_types::CanonicalInstruction, callees: &mut Vec<ItemInstanceId>) -> Result<InstructionKind> {
    let value = |id: nyar_types::MirValueId| Operand::Value(ValueRef(id.index()));
    Ok(match &instruction.operation {
        CanonicalOperation::Invoke { callee: CanonicalCallee::Item(instance), arguments } => {
            let name = program.linked.callable_names.get(instance).ok_or_else(|| miette!("调用 identity 未解析"))?.clone();
            callees.push(*instance);
            InstructionKind::Call { callee: Operand::Symbol(nyar_types::NamePath::new(name.parts().to_vec())), arguments: arguments.iter().map(|id| value(*id)).collect() }
        }
        CanonicalOperation::Invoke { callee: CanonicalCallee::Value(callee), arguments } => InstructionKind::Call { callee: value(*callee), arguments: arguments.iter().map(|id| value(*id)).collect() },
        CanonicalOperation::Copy { source } => InstructionKind::Copy { source: value(*source) },
        CanonicalOperation::AggregateCopy { source, destination } => InstructionKind::AggregateCopy { source: value(*source), dest: value(*destination) },
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
        CanonicalOperation::ArrayFromElements { array_type, elements } => InstructionKind::ArrayFromElements { array_type: lower_type(program, *array_type)?, elements: elements.iter().map(|id| value(*id)).collect() },
        CanonicalOperation::TupleNew { fields, .. } => InstructionKind::TupleNew { fields: fields.iter().map(|id| value(*id)).collect() },
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
        nyar_language::ValkyrieCompiler::default()
            .compile_source_to_build_output(
                "micro identity(value: i32) -> i32 { return value } \
                 micro entry(value: i32) -> i32 { return identity(value) }",
            )
            .expect("当前源码必须产生已验证的编译合同")
            .compiled_program()
            .clone()
    }

    fn prepare(program: &CompiledProgram) -> Result<BackendPrivatePlan> {
        let roots = program.canonical().linked.callable_names.values().cloned().collect::<Vec<_>>();
        BackendPrivatePlan::from_compiled_program(program, &roots)
    }

    #[test]
    fn source_specialized_values_and_direct_calls_prepare() {
        let program = source_program();
        let plan = prepare(&program).expect("已有完整标量与直接调用合同必须可表达");
        assert_eq!(plan.functions.len(), program.canonical().mir.functions.len());
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
