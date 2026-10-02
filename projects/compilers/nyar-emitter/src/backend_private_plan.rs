//! Compiler 成功载荷到目标私有执行计划的唯一边界。

use std::collections::BTreeMap;

use miette::{Result, miette};
use nyar::QualifiedName;
use nyar_types::{
    CanonicalArrayInitialization, CanonicalCallee, CanonicalConstant, CanonicalOperation, CanonicalProgram, CanonicalTerminator,
    CanonicalTypeKind, CompiledProgram, Constant, Instruction, InstructionKind, NyarType, Operand, Terminator, Value, ValueOrigin,
};

use crate::{
    contracts::{Block, BlockRef, ValueRef},
    executable_provider::{ExecutableFunction, FunctionView, SuspendMetadataView},
};

/// 已完成 callable、类型、CFG 和表示合同绑定的目标私有计划。
#[derive(Debug, Clone, Default)]
pub struct BackendPrivatePlan {
    functions: BTreeMap<QualifiedName, ExecutableFunction>,
}

impl BackendPrivatePlan {
    pub fn from_functions(functions: BTreeMap<QualifiedName, ExecutableFunction>) -> Self {
        Self { functions }
    }

    /// 从完整 `CompiledProgram` 生成闭包；任何无法无损投影的语义都失败。
    pub fn from_compiled_program(program: &CompiledProgram, roots: &[QualifiedName]) -> Result<Self> {
        let canonical = program.canonical();
        let mut pending = roots.to_vec();
        let mut seen = std::collections::BTreeSet::new();
        let mut functions = BTreeMap::new();
        while let Some(name) = pending.pop() {
            if !seen.insert(name.clone()) { continue; }
            let instance = canonical.linked.callable_names.iter()
                .find_map(|(instance, candidate)| (candidate == &name).then_some(*instance))
                .ok_or_else(|| miette!("callable `{name}` 缺少 Compiler identity"))?;
            let function = canonical.mir.functions.get(&instance)
                .ok_or_else(|| miette!("callable `{name}` 缺少 canonical 函数体"))?;
            let (lowered, callees) = lower_function(program, function)?;
            pending.extend(callees);
            functions.insert(name, lowered);
        }
        Ok(Self { functions })
    }

    pub fn operations(&self) -> Vec<QualifiedName> {
        self.functions.keys().cloned().collect()
    }

    pub fn get_function(&self, operation: &QualifiedName) -> Option<FunctionView> {
        self.functions.get(operation).cloned().map(|function| FunctionView { function })
    }

    pub fn suspend_metadata(&self, operation: &QualifiedName) -> Option<SuspendMetadataView> {
        self.functions.get(operation).and_then(SuspendMetadataView::from_function)
    }
}

fn lower_function(program: &CompiledProgram, function: &nyar_types::CanonicalFunction) -> Result<(ExecutableFunction, Vec<QualifiedName>)> {
    let canonical = program.canonical();
    for value in function.value_types.keys() {
        let identity = nyar_types::ValueIdentity::new(function.instance, *value);
        if !program.representation().value_reps.contains_key(&identity) {
            return Err(miette!("值 `{identity:?}` 缺少 RepresentationPlan 载体合同"));
        }
    }
    for block in function.blocks.values() {
        for instruction in &block.instructions {
            if matches!(instruction.operation, CanonicalOperation::Invoke { .. })
                && !program.representation().invoke_lowerings.contains_key(&instruction.id)
            {
                return Err(miette!("调用指令 `{}` 缺少 RepresentationPlan 降低合同", instruction.id.index()));
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

fn lower_operation(program: &CanonicalProgram, instruction: &nyar_types::CanonicalInstruction, callees: &mut Vec<QualifiedName>) -> Result<InstructionKind> {
    let value = |id: nyar_types::MirValueId| Operand::Value(ValueRef(id.index()));
    Ok(match &instruction.operation {
        CanonicalOperation::Invoke { callee: CanonicalCallee::Item(instance), arguments } => {
            let name = program.linked.callable_names.get(instance).ok_or_else(|| miette!("调用 identity 未解析"))?.clone();
            callees.push(name.clone());
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
