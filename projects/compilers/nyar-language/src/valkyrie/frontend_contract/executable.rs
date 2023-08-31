//! Convert language MIR into backend-private [`nyar_types::ExecutableFunction`] views.

use std::collections::BTreeMap;

use nyar::{NyarType, QualifiedName};
use nyar_types::{
    Block, BlockRef, Constant, EffectKind, ExecutableFunction, Instruction, InstructionKind, Operand, Terminator, Value, ValueOrigin, ValueRef,
};

use crate::{
    MirBlock, MirBlockRef, MirConstant, MirEffectKind, MirFunction, MirInstruction, MirOperand, MirOperation, MirTerminator, MirValue,
    MirValueOrigin, MirValueRef, concretize_type_lossy, mir::ssa::ArrayInitialization,
};

/// Deep-convert a language [`MirFunction`] into a platform [`ExecutableFunction`].
pub fn mir_function_to_executable(function: &MirFunction, sum_types: &[nyar_types::SumTypeLayout]) -> Result<ExecutableFunction, String> {
    let return_type = concretize_type_lossy(&function.return_type);
    let param_types = function.param_types.iter().map(concretize_type_lossy).collect();
    let value_types = function.value_types.iter().map(|(key, ty)| (convert_value_ref(*key), concretize_type_lossy(ty))).collect();

    // Semantic MirFunction 不再携带 suspend/frame/case God 元数据。
    #[allow(deprecated)]
    let state_machine = None;
    let suspend_plan = None;

    Ok(ExecutableFunction {
        symbol: function.symbol.clone(),
        return_type,
        param_types,
        value_types,
        entry: convert_block_ref(function.entry),
        values: function.values.iter().map(convert_value).collect(),
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        state_machine,
        suspend_plan,
        blocks: function.blocks.iter().map(|block| convert_block(block, sum_types)).collect::<Result<Vec<_>, _>>()?,
        diagnostics: Vec::new(),
    })
}

/// Convert many MIR functions keyed by qualified name.
pub fn mir_functions_to_executable_map<'a, I>(functions: I, sum_types: &[nyar_types::SumTypeLayout]) -> Result<BTreeMap<QualifiedName, ExecutableFunction>, String>
where
    I: IntoIterator<Item = (&'a QualifiedName, &'a MirFunction)>,
{
    functions.into_iter().map(|(name, function)| mir_function_to_executable(function, sum_types).map(|value| (name.clone(), value))).collect()
}

fn convert_value_ref(value: MirValueRef) -> ValueRef {
    ValueRef(value.0)
}

fn convert_block_ref(block: MirBlockRef) -> BlockRef {
    BlockRef(block.0)
}

fn convert_effect(effect: MirEffectKind) -> EffectKind {
    match effect {
        MirEffectKind::Raise => EffectKind::Raise,
        MirEffectKind::Yield => EffectKind::Yield,
        MirEffectKind::DelegateYield => EffectKind::DelegateYield,
        MirEffectKind::Await => EffectKind::Await,
        MirEffectKind::AsyncSpawn => EffectKind::AsyncSpawn,
        MirEffectKind::AsyncBlock => EffectKind::AsyncBlock,
    }
}

fn convert_optional_type(ty: &Option<crate::types::hir::ValkyrieType>) -> Option<NyarType> {
    ty.as_ref().map(concretize_type_lossy)
}

fn convert_constant(constant: &MirConstant) -> Constant {
    match constant {
        MirConstant::Int(value) => Constant::Int(*value),
        MirConstant::Float64(value) => Constant::Float64(*value),
        MirConstant::Bool(value) => Constant::Bool(*value),
        MirConstant::Utf8(value) => Constant::Utf8(value.clone()),
        MirConstant::Utf16(value) => Constant::Utf16(value.clone()),
        MirConstant::Unit => Constant::Unit,
    }
}

fn convert_operand(operand: &MirOperand) -> Operand {
    match operand {
        MirOperand::Value(value) => Operand::Value(convert_value_ref(*value)),
        MirOperand::Constant(constant) => Operand::Constant(convert_constant(constant)),
        MirOperand::Symbol(path) => Operand::Symbol(path.clone()),
    }
}

fn convert_value_origin(origin: &MirValueOrigin) -> ValueOrigin {
    match origin {
        MirValueOrigin::Parameter { index, name } => ValueOrigin::Parameter { index: *index, name: name.clone() },
        MirValueOrigin::BlockParameter { block, name } => ValueOrigin::BlockParameter { block: convert_block_ref(*block), name: name.clone() },
        MirValueOrigin::LetBinding { name } => ValueOrigin::LetBinding { name: name.clone() },
        MirValueOrigin::MutRefBinding { name } => ValueOrigin::MutRefBinding { name: name.clone() },
        MirValueOrigin::PinMutRefBinding { name } => ValueOrigin::PinMutRefBinding { name: name.clone() },
        MirValueOrigin::Literal => ValueOrigin::Literal,
        MirValueOrigin::Path => ValueOrigin::Path,
        MirValueOrigin::CallResult => ValueOrigin::CallResult,
        MirValueOrigin::Temporary => ValueOrigin::Temporary,
    }
}

fn convert_value(value: &MirValue) -> Value {
    Value { id: convert_value_ref(value.id), origin: convert_value_origin(&value.origin) }
}

fn convert_instruction_kind(kind: &MirOperation, sum_types: &[nyar_types::SumTypeLayout]) -> Result<InstructionKind, String> {
    match kind {
        MirOperation::LoadConstant { constant, ty } => Ok(InstructionKind::LoadConstant { constant: convert_constant(constant), ty: convert_optional_type(ty) }),
        MirOperation::LoadSymbol { path } => Ok(InstructionKind::LoadSymbol { path: path.clone() }),
        MirOperation::Copy { source } => Ok(InstructionKind::Copy { source: convert_operand(source) }),
        MirOperation::StoreVar { name, value, ty } => {
            Ok(InstructionKind::StoreVar { name: name.clone(), value: convert_operand(value), ty: convert_optional_type(ty) })
        }
        MirOperation::Call { callee, arguments } => {
            Ok(InstructionKind::Call { callee: convert_operand(callee), arguments: arguments.iter().map(convert_operand).collect() })
        }
        MirOperation::StructNew { type_name, fields } => Ok(InstructionKind::StructNew {
            type_name: type_name.to_string(),
            fields: fields.iter().map(|(name, value)| (name.to_string(), convert_operand(value))).collect(),
        }),
        MirOperation::TupleNew { fields, .. } => Ok(InstructionKind::TupleNew { fields: fields.iter().map(convert_operand).collect() }),
        MirOperation::AggregateCopy { source, dest } => {
            Ok(InstructionKind::AggregateCopy { source: convert_operand(source), dest: convert_operand(dest) })
        }
        MirOperation::FieldGet { object, field } => Ok(InstructionKind::FieldGet { object: convert_operand(object), field: field.to_string() }),
        MirOperation::FieldSet { object, field, value } => {
            Ok(InstructionKind::FieldSet { object: convert_operand(object), field: field.to_string(), value: convert_operand(value) })
        }
        MirOperation::SumNew { sum_type, type_args, variant, payload_type, payload } => Ok(InstructionKind::SumNew {
            sum_type: sum_type.clone(),
            type_args: type_args.iter().map(concretize_type_lossy).collect(),
            variant: declared_variant_name(sum_types, sum_type, *variant)?,
            payload_type: payload_type.as_ref().map(concretize_type_lossy),
            payload: payload.as_ref().map(convert_operand),
        }),
        MirOperation::SumPayloadGet { sum_type, type_args, variant, payload_type, object } => Ok(InstructionKind::SumPayloadGet {
            sum_type: sum_type.clone(),
            type_args: type_args.iter().map(concretize_type_lossy).collect(),
            variant: declared_variant_name(sum_types, sum_type, *variant)?,
            payload_type: concretize_type_lossy(payload_type),
            object: convert_operand(object),
        }),
        MirOperation::SumVariantIs { sum_type, type_args, variant, object } => Ok(InstructionKind::SumVariantIs {
            sum_type: sum_type.clone(),
            type_args: type_args.iter().map(concretize_type_lossy).collect(),
            variant: declared_variant_name(sum_types, sum_type, *variant)?,
            object: convert_operand(object),
        }),
        MirOperation::PatternMatch { value, pattern } => {
            Ok(InstructionKind::PatternMatch { value: convert_operand(value), pattern_debug: format!("{pattern:?}") })
        }
        MirOperation::ArrayNew { array_type, length, initialization } => Ok(InstructionKind::ArrayNew {
            array_type: concretize_type_lossy(array_type),
            length: convert_operand(length),
            initialization: match initialization {
                ArrayInitialization::Default => nyar_types::executable::ArrayInitialization::Default,
                ArrayInitialization::Fill(value) => nyar_types::executable::ArrayInitialization::Fill(convert_operand(value)),
            },
        }),
        MirOperation::ArrayFromElements { array_type, elements } => Ok(InstructionKind::ArrayFromElements {
            array_type: concretize_type_lossy(array_type),
            elements: elements.iter().map(convert_operand).collect(),
        }),
        MirOperation::ArrayGet { array, index } => Ok(InstructionKind::ArrayGet { array: convert_operand(array), index: convert_operand(index) }),
        MirOperation::ArraySet { array, index, value } => {
            Ok(InstructionKind::ArraySet { array: convert_operand(array), index: convert_operand(index), value: convert_operand(value) })
        }
        MirOperation::ArrayLength { array } => Ok(InstructionKind::ArrayLength { array: convert_operand(array) }),
    }
}

fn declared_variant_name(sum_types: &[nyar_types::SumTypeLayout], sum_type: &str, variant: nyar_types::VariantId) -> Result<String, String> {
    let mut next = 0u32;
    for sum in sum_types {
        for declared in &sum.variants {
            if next == variant.index() {
                return (sum.name == sum_type)
                    .then(|| declared.name.clone())
                    .ok_or_else(|| format!("variant identity {variant} 属于 `{}`，不是 `{sum_type}`", sum.name));
            }
            next = next.checked_add(1).ok_or_else(|| "variant identity 溢出".to_string())?;
        }
    }
    Err(format!("未声明的 variant identity: {sum_type}::{variant}"))
}

fn convert_instruction(instruction: &MirInstruction, sum_types: &[nyar_types::SumTypeLayout]) -> Result<Instruction, String> {
    Ok(Instruction {
        id: instruction.id,
        results: instruction.results.iter().copied().map(convert_value_ref).collect(),
        kind: convert_instruction_kind(&instruction.kind, sum_types)?,
        provenance: instruction.provenance,
    })
}

fn convert_terminator(terminator: &MirTerminator) -> Terminator {
    match terminator {
        MirTerminator::Return { value } => Terminator::Return { value: value.as_ref().map(convert_operand) },
        MirTerminator::Jump { target, arguments } => {
            Terminator::Jump { target: convert_block_ref(*target), arguments: arguments.iter().map(convert_operand).collect() }
        }
        MirTerminator::Branch { condition, then_target, else_target } => Terminator::Branch {
            condition: convert_operand(condition),
            then_target: convert_block_ref(*then_target),
            else_target: convert_block_ref(*else_target),
        },
        MirTerminator::PerformEffect { effect, payload, resume_target } => Terminator::PerformEffect {
            effect: convert_effect(*effect),
            payload: payload.as_ref().map(convert_operand),
            resume_target: convert_block_ref(*resume_target),
        },
        MirTerminator::StateDispatch { state, cases, default_target } => Terminator::StateDispatch {
            state: convert_value_ref(*state),
            cases: cases.iter().map(|(id, target)| (*id, convert_block_ref(*target))).collect(),
            default_target: convert_block_ref(*default_target),
        },
        MirTerminator::YieldToRuntime { effect, payload, resume_state } => Terminator::YieldToRuntime {
            effect: convert_effect(*effect),
            payload: payload.as_ref().map(convert_operand),
            resume_state: *resume_state,
        },
        MirTerminator::Unreachable => Terminator::Unreachable,
    }
}

fn convert_block(block: &MirBlock, sum_types: &[nyar_types::SumTypeLayout]) -> Result<Block, String> {
    Ok(Block {
        id: convert_block_ref(block.id),
        label: block.label.clone(),
        parameters: block.parameters.iter().copied().map(convert_value_ref).collect(),
        instructions: block.instructions.iter().map(|instruction| convert_instruction(instruction, sum_types)).collect::<Result<Vec<_>, _>>()?,
        terminator: convert_terminator(&block.terminator),
    })
}

// suspend/frame/case/diagnostic God 转换器已随 Semantic MIR 瘦身删除。
// mir_function_to_executable 产出空侧表；不得在此恢复 MirSuspendState / StateMachine。
