use crate::{
    contracts::ValueOrigin as MirValueOrigin,
    executable_provider::{
        ExecutableBlock as MirBlock, ExecutableBlockRef as MirBlockRef, ExecutableConstant as MirConstant,
        ExecutableDispatchKind as MirDispatchKind, ExecutableFunction as MirFunction, ExecutableInstruction as MirInstruction,
        ExecutableInstructionKind as MirInstructionKind, ExecutableOperand as MirOperand, ExecutableReceiverPassingKind as ReceiverPassingKind,
        ExecutableStorageKind as StorageKind, ExecutableTerminator as MirTerminator, ExecutableValueRef as MirValueRef, NyarType,
    },
    nyar_backend_jvm::{JvmCodeBody, JvmInstruction, JvmMethodDescriptor, JvmMethodRef, JvmMethodSignature, JvmTypeDescriptor},
};
use nyar::{ExternalImportLink, Identifier, QualifiedName};
use nyar_types::LayoutId;
use std_data::binary::class::JvmFieldRef;

use super::{
    executable::{
        ExecutableLoweringContext, block_label, collect_reachable_blocks,
        slots::{ExecutableSlotPlan, jvm_type_field_types},
    },
    interop::jvm_host_print_target,
    intrinsic_opcode::{IntrinsicBinaryOp, IntrinsicBitwiseOp, IntrinsicCompareOp, IntrinsicOpcode},
    pattern_matching_contract::validate_pattern_matching_invariants,
    sanitize_jvm_method_symbol, sanitize_symbol,
    singleton::jvm_internal_name,
    witness_abi::{is_injected_runtime_stub_symbol, witness_slot_jvm_descriptor},
};
use crate::FragmentSubmission;

pub(crate) fn lower_mir_function_to_jvm(
    submission: &FragmentSubmission,
    operation: &QualifiedName,
    mir_fn: &MirFunction,
) -> JvmMethodSignature {
    if let Err(error) = validate_pattern_matching_invariants(mir_fn) {
        debug_assert!(false, "pattern matching contract violation in function `{}` ({}): {error:?}", mir_fn.symbol, operation);
    }
    let ctx = ExecutableLoweringContext::new(submission);
    let self_owner = enclosing_type_name_from_operation(operation);
    let mut slots = ExecutableSlotPlan::plan_jvm(&ctx, mir_fn);
    reserve_jvm_parameter_locals(&ctx, mir_fn, &mut slots, self_owner.as_deref());
    let mut lowerer = JvmMirLowerer {
        ctx,
        mir_fn,
        self_owner: self_owner.clone(),
        slots,
        instructions: Vec::new(),
        label_counter: 0,
        var_types: Default::default(),
        value_type_overrides: Default::default(),
        semantic_unite_value_types: Default::default(),
        boxed_value_refs: Default::default(),
        local_kinds: Default::default(),
    };
    lowerer.seed_parameter_value_types();
    lowerer.seed_parameter_local_kinds();
    if let Some(opcode) = mir_fn.intrinsic {
        if mir_fn.blocks.iter().all(|block| block.instructions.is_empty()) {
            let arguments = mir_fn
                .blocks
                .get(mir_fn.entry.0 as usize)
                .map(|block| block.parameters.iter().copied().map(MirOperand::Value).collect::<Vec<_>>())
                .unwrap_or_default();
            lowerer.emit_intrinsic_opcode(opcode, &arguments, None);
            let effective_return_ty = effective_jvm_type(&lowerer.ctx, &concretize_self_type(&mir_fn.return_type, self_owner.as_deref()));
            match effective_return_ty {
                NyarType::Bottom | NyarType::Unit => lowerer.instructions.push(JvmInstruction::Return),
                NyarType::Integer64 { .. } => lowerer.instructions.push(JvmInstruction::LReturn),
                NyarType::Float64 => lowerer.instructions.push(JvmInstruction::DReturn),
                _ if is_jvm_stack_reference(&effective_return_ty) => lowerer.instructions.push(JvmInstruction::AReturn),
                _ => lowerer.instructions.push(JvmInstruction::IReturn),
            }
        }
        else {
            for block_id in collect_reachable_blocks(mir_fn) {
                if let Some(block) = mir_fn.blocks.get(block_id.0 as usize) {
                    lowerer.emit_block(block);
                }
            }
        }
    }
    else {
        for block_id in collect_reachable_blocks(mir_fn) {
            if let Some(block) = mir_fn.blocks.get(block_id.0 as usize) {
                lowerer.emit_block(block);
            }
        }
    }
    let return_ty = concretize_self_type(&mir_fn.return_type, self_owner.as_deref());
    let return_descriptor = if needs_boxing(&lowerer.ctx, &return_ty) {
        jvm_type_descriptor(&return_ty)
    }
    else {
        let effective_return_type = effective_jvm_type(&lowerer.ctx, &return_ty);
        jvm_type_descriptor(&effective_return_type)
    };
    let param_descriptors = mir_fn
        .param_types
        .iter()
        .flat_map(|ty| {
            let ty = concretize_self_type(ty, self_owner.as_deref());
            jvm_type_field_types(&lowerer.ctx, &ty)
                .into_iter()
                .map(|ft| {
                    let effective_ft = effective_jvm_type(&lowerer.ctx, &ft);
                    jvm_parameter_descriptor(&effective_ft)
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let max_locals = required_max_locals(lowerer.slots.local_types.len(), &param_descriptors, &lowerer.instructions);
    let parameter_slots = param_descriptors.iter().fold(0u16, |slots, descriptor| {
        slots.saturating_add(match descriptor {
            JvmTypeDescriptor::Long | JvmTypeDescriptor::Double => 2,
            _ => 1,
        })
    });
    let max_locals = initialize_jvm_locals(operation, parameter_slots, max_locals, &mut lowerer.instructions);
    let max_stack = required_max_stack(&lowerer.instructions);
    JvmMethodSignature {
        name: sanitize_jvm_method_symbol(operation),
        descriptor: JvmMethodDescriptor::new(param_descriptors, return_descriptor),
        access_flags: 0x0001 | 0x0008,
        code: Some(JvmCodeBody { max_stack, max_locals, instructions: lowerer.instructions }),
    }
}

/// JVM 方法参数先占用 local 0..N，普通 SSA/变量 local 必须从参数区之后开始。
///
/// `ExecutableSlotPlan` 同时服务没有显式参数区的后端 lowering，因此它的 JVM
/// 规划默认从 0 分配临时量。如果不在 JVM 边界把非参数 local 整体后移，函数体
/// 的首个 `astore`/`istore` 会覆盖传入参数；后续再以原参数类型加载同一 local，
/// 验证器会报告 `Register N contains wrong type`，或者在分支汇合处把槽位判为
/// uninitialized。这里仅保留真正的 `MirValueOrigin::Parameter` 在 JVM ABI 指定
/// 的参数槽位，其余 value/变量/block 参数全部移到参数区之后。
fn reserve_jvm_parameter_locals(
    ctx: &ExecutableLoweringContext<'_>,
    mir_fn: &MirFunction,
    slots: &mut ExecutableSlotPlan,
    self_owner: Option<&str>,
) {
    if mir_fn.symbol.contains("lower_executable_instruction_kind_to_msil") {
        let entry_parameter_count = mir_fn.blocks.get(mir_fn.entry.0 as usize).map(|block| block.parameters.len()).unwrap_or(0);
        eprintln!(
            "[jvm_param_debug] op={} declared_params={} entry_params={} planned_values={} planned_locals={}",
            mir_fn.symbol,
            mir_fn.param_types.len(),
            entry_parameter_count,
            slots.value_locals.len(),
            slots.local_types.len()
        );
    }
    let parameter_slots = mir_fn
        .param_types
        .iter()
        .flat_map(|ty| {
            let ty = concretize_self_type(ty, self_owner);
            jvm_type_field_types(ctx, &ty)
        })
        .fold(0u16, |count, ty| count.saturating_add(super::executable::jvm_local_slots(&effective_jvm_type(ctx, &ty))));
    if parameter_slots == 0 {
        return;
    }

    let mut parameter_values: Vec<(usize, MirValueRef)> = mir_fn
        .values
        .iter()
        .filter_map(|value| match value.origin {
            MirValueOrigin::Parameter { index, .. } => Some((index, value.id)),
            _ => None,
        })
        .collect();
    // Some frontend paths represent ABI arguments only as entry-block
    // parameters and do not preserve ValueOrigin::Parameter. Keep those
    // values mapped to JVM argument locals as well; otherwise emit_operand
    // falls back to null/zero and corrupts calls such as PE map helpers.
    if let Some(entry) = mir_fn.blocks.get(mir_fn.entry.0 as usize) {
        for (index, value) in entry.parameters.iter().enumerate() {
            if index < mir_fn.param_types.len() && !parameter_values.iter().any(|(i, v)| *i == index || *v == *value) {
                parameter_values.push((index, *value));
            }
        }
    }
    parameter_values.sort_by_key(|(index, _)| *index);
    let parameter_value_set: std::collections::BTreeSet<MirValueRef> = parameter_values.iter().map(|(_, value)| *value).collect();

    for (value, local) in slots.value_locals.iter_mut() {
        if !parameter_value_set.contains(value) {
            *local = local.saturating_add(parameter_slots);
        }
    }
    for local in slots.var_locals.values_mut() {
        *local = local.saturating_add(parameter_slots);
    }
    for local in slots.block_param_locals.values_mut() {
        *local = local.saturating_add(parameter_slots);
    }

    let mut next_parameter_local = 0u16;
    for (index, ty) in mir_fn.param_types.iter().enumerate() {
        if let Some((_, value)) = parameter_values.iter().find(|(parameter_index, _)| *parameter_index == index) {
            slots.value_locals.insert(*value, next_parameter_local);
        }
        let ty = concretize_self_type(ty, self_owner);
        next_parameter_local = next_parameter_local.saturating_add(
            jvm_type_field_types(ctx, &ty)
                .iter()
                .fold(0u16, |count, field_ty| count.saturating_add(super::executable::jvm_local_slots(&effective_jvm_type(ctx, field_ty)))),
        );
    }

    if let Some(fill) = slots.local_types.first().cloned().or_else(|| {
        mir_fn.param_types.first().map(|ty| {
            let ty = concretize_self_type(ty, self_owner);
            crate::lowering::clr_types::nyar_type_to_msil(&ty, ctx.layouts)
        })
    }) {
        slots.local_types.splice(0..0, std::iter::repeat(fill).take(parameter_slots as usize));
    }
}

fn required_max_locals(planned: usize, parameters: &[JvmTypeDescriptor], instructions: &[JvmInstruction]) -> u16 {
    let parameter_slots = parameters.iter().fold(0u16, |slots, descriptor| {
        slots.saturating_add(match descriptor {
            JvmTypeDescriptor::Long | JvmTypeDescriptor::Double => 2,
            _ => 1,
        })
    });
    let instruction_slots = instructions
        .iter()
        .filter_map(|instruction| match instruction {
            JvmInstruction::ALoad(local)
            | JvmInstruction::ILoad(local)
            | JvmInstruction::FLoad(local)
            | JvmInstruction::AStore(local)
            | JvmInstruction::IStore(local)
            | JvmInstruction::FStore(local) => Some(local.saturating_add(1)),
            JvmInstruction::LLoad(local) | JvmInstruction::DLoad(local) | JvmInstruction::LStore(local) | JvmInstruction::DStore(local) => {
                Some(local.saturating_add(2))
            }
            JvmInstruction::ALoad0 => Some(1),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    u16::try_from(planned).unwrap_or(u16::MAX).max(parameter_slots).max(instruction_slots)
}

fn jvm_descriptor_stack_slots(desc: &JvmTypeDescriptor) -> i32 {
    match desc {
        JvmTypeDescriptor::Void => 0,
        JvmTypeDescriptor::Long | JvmTypeDescriptor::Double => 2,
        _ => 1,
    }
}

/// Linear stack-depth scan for `Code.max_stack` (branch joins take running max).
/// Hardcoded 16 blew up on multi-arg `legion.tools` methods (`Stack size too large`).
fn required_max_stack(instructions: &[JvmInstruction]) -> u16 {
    let mut depth: i32 = 0;
    let mut max_depth: i32 = 0;
    for instruction in instructions {
        let (pop, push) = match instruction {
            JvmInstruction::Label(_) | JvmInstruction::Goto(_) => (0, 0),
            JvmInstruction::ALoad0
            | JvmInstruction::ALoad(_)
            | JvmInstruction::ILoad(_)
            | JvmInstruction::FLoad(_)
            | JvmInstruction::AConstNull
            | JvmInstruction::IConst(_)
            | JvmInstruction::FConst0
            | JvmInstruction::LdcString(_)
            | JvmInstruction::New(_) => (0, 1),
            JvmInstruction::LLoad(_)
            | JvmInstruction::DLoad(_)
            | JvmInstruction::LConst0
            | JvmInstruction::LConst1
            | JvmInstruction::LdcLong(_)
            | JvmInstruction::DConst0
            | JvmInstruction::DConst1
            | JvmInstruction::LdcDouble(_) => (0, 2),
            JvmInstruction::AStore(_) | JvmInstruction::IStore(_) | JvmInstruction::FStore(_) | JvmInstruction::Pop => (1, 0),
            JvmInstruction::LStore(_) | JvmInstruction::DStore(_) => (2, 0),
            JvmInstruction::Dup => (1, 2),
            JvmInstruction::Swap => (2, 2),
            JvmInstruction::IALoad | JvmInstruction::BALoad | JvmInstruction::CALoad | JvmInstruction::SALoad | JvmInstruction::AALoad => {
                (2, 1)
            }
            JvmInstruction::IAStore | JvmInstruction::BAStore | JvmInstruction::CAStore | JvmInstruction::SAStore | JvmInstruction::AAStore => {
                (3, 0)
            }
            JvmInstruction::ArrayLength
            | JvmInstruction::CheckCast(_)
            | JvmInstruction::NewArray(_)
            | JvmInstruction::NewIntArray
            | JvmInstruction::ANewArray(_) => (1, 1),
            JvmInstruction::IAdd
            | JvmInstruction::FAdd
            | JvmInstruction::ISub
            | JvmInstruction::FSub
            | JvmInstruction::IMul
            | JvmInstruction::FMul
            | JvmInstruction::IDiv
            | JvmInstruction::FDiv
            | JvmInstruction::IRem
            | JvmInstruction::FRem
            | JvmInstruction::IAnd
            | JvmInstruction::IOr
            | JvmInstruction::IXor
            | JvmInstruction::IShl
            | JvmInstruction::IShr
            | JvmInstruction::IUShr => (2, 1),
            JvmInstruction::LAdd
            | JvmInstruction::DAdd
            | JvmInstruction::LSub
            | JvmInstruction::DSub
            | JvmInstruction::LMul
            | JvmInstruction::DMul
            | JvmInstruction::LDiv
            | JvmInstruction::DDiv
            | JvmInstruction::LRem
            | JvmInstruction::DRem
            | JvmInstruction::LAnd
            | JvmInstruction::LOr
            | JvmInstruction::LXor => (4, 2),
            JvmInstruction::LShl | JvmInstruction::LShr | JvmInstruction::LUShr => (3, 2),
            JvmInstruction::INeg | JvmInstruction::FNeg | JvmInstruction::L2I | JvmInstruction::D2I => (1, 1),
            JvmInstruction::LNeg | JvmInstruction::DNeg => (2, 2),
            JvmInstruction::I2L | JvmInstruction::I2D => (1, 2),
            JvmInstruction::L2D => (2, 2),
            JvmInstruction::LCmp => (4, 1),
            JvmInstruction::FCmpL | JvmInstruction::FCmpG => (2, 1),
            JvmInstruction::DCmpL | JvmInstruction::DCmpG => (4, 1),
            JvmInstruction::IfEq(_)
            | JvmInstruction::IfNe(_)
            | JvmInstruction::IfLt(_)
            | JvmInstruction::IfLe(_)
            | JvmInstruction::IfGt(_)
            | JvmInstruction::IfGe(_)
            | JvmInstruction::IfNull(_)
            | JvmInstruction::IfNonNull(_) => (1, 0),
            JvmInstruction::IfICmpEq(_)
            | JvmInstruction::IfICmpNe(_)
            | JvmInstruction::IfICmpLt(_)
            | JvmInstruction::IfICmpLe(_)
            | JvmInstruction::IfICmpGt(_)
            | JvmInstruction::IfICmpGe(_)
            | JvmInstruction::IfACmpEq(_)
            | JvmInstruction::IfACmpNe(_) => (2, 0),
            JvmInstruction::GetStatic(field) => (0, jvm_descriptor_stack_slots(&field.descriptor)),
            JvmInstruction::PutStatic(field) => (jvm_descriptor_stack_slots(&field.descriptor), 0),
            JvmInstruction::GetField(field) => (1, jvm_descriptor_stack_slots(&field.descriptor)),
            JvmInstruction::PutField(field) => (1 + jvm_descriptor_stack_slots(&field.descriptor), 0),
            JvmInstruction::InvokeStatic(method) => {
                let args: i32 = method.descriptor.parameter_types.iter().map(jvm_descriptor_stack_slots).sum();
                (args, jvm_descriptor_stack_slots(&method.descriptor.return_type))
            }
            JvmInstruction::InvokeVirtual(method) | JvmInstruction::InvokeSpecial(method) => {
                let args: i32 = method.descriptor.parameter_types.iter().map(jvm_descriptor_stack_slots).sum();
                (1 + args, jvm_descriptor_stack_slots(&method.descriptor.return_type))
            }
            JvmInstruction::IReturn | JvmInstruction::FReturn | JvmInstruction::AReturn => (1, 0),
            JvmInstruction::LReturn | JvmInstruction::DReturn => (2, 0),
            JvmInstruction::Return => (0, 0),
        };
        depth -= pop;
        if depth < 0 {
            depth = 0;
        }
        depth += push;
        if depth > max_depth {
            max_depth = depth;
        }
    }
    // Floor of 8 keeps tiny methods honest; clamp to u16.
    u16::try_from(max_depth.max(8)).unwrap_or(u16::MAX)
}

/// JVM verifier 会在所有控制流路径上追踪 local 的验证类型。任何被读取却没有在
/// 方法入口确定类型的槽位，都可能在分支汇合时退化为 uninitialized。后端已经为
/// 参数保留 ABI 槽位；这里对其余 local 依据首次 store/load 指令推断验证类型并在
/// 入口统一写入零值。真实 MIR 写入随后覆盖这些值，语义不变，但每条路径都有稳定
/// 的 local 类型，避免 `Register N wrong type` / `uninitialized register N`。
fn initialize_jvm_locals(operation: &QualifiedName, parameter_slots: u16, max_locals: u16, instructions: &mut Vec<JvmInstruction>) -> u16 {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum LocalKind {
        Int,
        Long,
        Float,
        Double,
        Reference,
    }

    let mut kinds = std::collections::BTreeMap::<u16, LocalKind>::new();
    let mut conflicts: Vec<u16> = Vec::new();
    let mut conflict_details: Vec<(u16, LocalKind, LocalKind, usize)> = Vec::new();
    for (instruction_index, instruction) in instructions.iter().enumerate() {
        let (local, kind) = match instruction {
            JvmInstruction::ILoad(local) | JvmInstruction::IStore(local) => (*local, LocalKind::Int),
            JvmInstruction::LLoad(local) | JvmInstruction::LStore(local) => (*local, LocalKind::Long),
            JvmInstruction::FLoad(local) | JvmInstruction::FStore(local) => (*local, LocalKind::Float),
            JvmInstruction::DLoad(local) | JvmInstruction::DStore(local) => (*local, LocalKind::Double),
            JvmInstruction::ALoad(local) | JvmInstruction::AStore(local) => (*local, LocalKind::Reference),
            _ => continue,
        };
        if local >= parameter_slots {
            match kinds.entry(local) {
                std::collections::btree_map::Entry::Vacant(slot) => {
                    slot.insert(kind);
                }
                std::collections::btree_map::Entry::Occupied(prev) if *prev.get() != kind => {
                    // Dual-kind stores on one local are a lowering bug; keep the first
                    // kind for the zero-init prefix and leave the conflict for the
                    // verifier so spies surface Register N wrong type rather than
                    // silently masking it with a mismatched init.
                    if !conflicts.contains(&local) {
                        conflicts.push(local);
                    }
                    if !conflict_details.iter().any(|(slot, _, _, _)| *slot == local) {
                        conflict_details.push((local, *prev.get(), kind, instruction_index));
                        eprintln!(
                            "[jvm_conflict_detail] operation={operation} local={local} first={:?} current={instruction:?} index={instruction_index}",
                            kinds.get(&local)
                        );
                        let start = instruction_index.saturating_sub(8);
                        let end = (instruction_index + 4).min(instructions.len());
                        for (near_index, near_instruction) in instructions.iter().enumerate().skip(start).take(end - start) {
                            eprintln!(
                                "[jvm_conflict_trace] operation={operation} local={local} index={near_index} instruction={near_instruction:?}"
                            );
                        }
                    }
                }
                std::collections::btree_map::Entry::Occupied(_) => {}
            }
        }
    }
    /*
    if !conflicts.is_empty() {
        "JVM local kind conflict in `{operation}` before initialize_jvm_locals: locals {conflicts:?}, details={conflict_details:?} — emit_store_local must reallocate"
        eprintln!("[jvm_conflict_continue] operation={operation} locals={conflicts:?} details={conflict_details:?}");
    }

    }
    */
    if !conflicts.is_empty() {
        eprintln!("[jvm_conflict_continue] operation={operation} locals={conflicts:?}");
    }
    // A conflicting slot cannot be repaired by zero-initialization: the verifier
    // still sees (for example) astore 400 followed by istore 400. Move all
    // instructions of the later category to an unused slot before adding the
    // initialization prefix. This keeps the generated method verifier-safe even
    // when an older lowering path bypassed emit_store_local's reallocator.
    let mut used_slots = kinds.keys().copied().collect::<std::collections::BTreeSet<_>>();
    let mut replacement_kinds: Vec<(u16, LocalKind)> = Vec::new();
    for instruction in instructions.iter() {
        let slot = match instruction {
            JvmInstruction::ILoad(slot)
            | JvmInstruction::IStore(slot)
            | JvmInstruction::LLoad(slot)
            | JvmInstruction::LStore(slot)
            | JvmInstruction::FLoad(slot)
            | JvmInstruction::FStore(slot)
            | JvmInstruction::DLoad(slot)
            | JvmInstruction::DStore(slot)
            | JvmInstruction::ALoad(slot)
            | JvmInstruction::AStore(slot) => Some(*slot),
            _ => None,
        };
        if let Some(slot) = slot {
            used_slots.insert(slot);
        }
    }
    let mut effective_max_locals = max_locals;
    for local in conflicts.iter().copied() {
        let Some(first_kind) = kinds.get(&local).copied()
        else {
            continue;
        };
        let Some((_, _, second_kind, _)) = conflict_details.iter().find(|(slot, _, _, _)| *slot == local)
        else {
            continue;
        };
        let width = match second_kind {
            LocalKind::Long | LocalKind::Double => 2,
            _ => 1,
        };
        let mut replacement = parameter_slots;
        while replacement.saturating_add(width) <= effective_max_locals
            && (0..width).any(|offset| used_slots.contains(&replacement.saturating_add(offset)))
        {
            replacement = replacement.saturating_add(1);
        }
        // A stale slot plan can report a max_locals smaller than the locals
        // already present in the instruction stream. Never collapse distinct
        // conflict rewrites onto max_locals - 1; grow the Code local table and
        // reserve the complete category-2 width when necessary.
        if replacement.saturating_add(width) > effective_max_locals {
            replacement = effective_max_locals;
            effective_max_locals = effective_max_locals.saturating_add(width);
        }
        for offset in 0..width {
            used_slots.insert(replacement.saturating_add(offset));
        }
        replacement_kinds.push((replacement, *second_kind));
        eprintln!(
            "[jvm_conflict_rewrite] operation={operation} local={local} first={first_kind:?} moved={second_kind:?} replacement={replacement}"
        );
        for instruction in instructions.iter_mut() {
            let matches = match (*second_kind, &*instruction) {
                (LocalKind::Int, JvmInstruction::ILoad(slot) | JvmInstruction::IStore(slot)) => *slot == local,
                (LocalKind::Long, JvmInstruction::LLoad(slot) | JvmInstruction::LStore(slot)) => *slot == local,
                (LocalKind::Float, JvmInstruction::FLoad(slot) | JvmInstruction::FStore(slot)) => *slot == local,
                (LocalKind::Double, JvmInstruction::DLoad(slot) | JvmInstruction::DStore(slot)) => *slot == local,
                (LocalKind::Reference, JvmInstruction::ALoad(slot) | JvmInstruction::AStore(slot)) => *slot == local,
                _ => false,
            };
            if matches {
                match instruction {
                    JvmInstruction::ILoad(slot)
                    | JvmInstruction::IStore(slot)
                    | JvmInstruction::LLoad(slot)
                    | JvmInstruction::LStore(slot)
                    | JvmInstruction::FLoad(slot)
                    | JvmInstruction::FStore(slot)
                    | JvmInstruction::DLoad(slot)
                    | JvmInstruction::DStore(slot)
                    | JvmInstruction::ALoad(slot)
                    | JvmInstruction::AStore(slot) => *slot = replacement,
                    _ => {}
                }
            }
        }
    }
    let mut prefix = Vec::new();
    for (local, kind) in kinds.into_iter().chain(replacement_kinds.into_iter()) {
        // A stale slot plan can put a first-read local exactly at (or above)
        // the planned max.  It still needs an entry definition so every
        // control-flow path reaches a verifier-typed value before its first
        // load; grow the Code local table instead of leaving that path
        // uninitialized.
        let width = match kind {
            LocalKind::Long | LocalKind::Double => 2,
            _ => 1,
        };
        if local.saturating_add(width) > effective_max_locals {
            effective_max_locals = local.saturating_add(width);
        }
        prefix.push(match kind {
            LocalKind::Int => JvmInstruction::IConst(0),
            LocalKind::Long => JvmInstruction::LConst0,
            LocalKind::Float => JvmInstruction::FConst0,
            LocalKind::Double => JvmInstruction::DConst0,
            LocalKind::Reference => JvmInstruction::AConstNull,
        });
        prefix.push(match kind {
            LocalKind::Int => JvmInstruction::IStore(local),
            LocalKind::Long => JvmInstruction::LStore(local),
            LocalKind::Float => JvmInstruction::FStore(local),
            LocalKind::Double => JvmInstruction::DStore(local),
            LocalKind::Reference => JvmInstruction::AStore(local),
        });
    }
    if !prefix.is_empty() {
        prefix.append(instructions);
        *instructions = prefix;
    }
    effective_max_locals
}

/// `Type.method` → `Type`（倒数第二段），与 CLR `enclosing_type_name_from_operation` 对齐。
pub(crate) fn enclosing_type_name_from_operation(operation: &QualifiedName) -> Option<String> {
    let parts = operation.parts();
    if parts.len() < 2 {
        return None;
    }
    Some(parts[parts.len() - 2].as_str().to_string())
}

/// 将 MIR 中未替换的 `Self` 收成方法所属类型名。
///
/// HIR `self: Self` / 返回 `Self` 常残留为 `Named("Self")`；若不替换，
/// [`jvm_type_descriptor`] 会把它擦成 `Ljava/lang/Object;`，而方法体仍对
/// `isize`/`usize` 等原始所有者发射 `ineg`/`iand`/`ireturn`，触发
/// `VerifyError: Expecting to find integer on stack` 或
/// `Expecting to find object/array on stack`（`aload`+`ineg` / `iconst`+`areturn`）。
pub(crate) fn concretize_self_type(ty: &NyarType, self_owner: Option<&str>) -> NyarType {
    let Some(owner) = self_owner
    else {
        return ty.clone();
    };
    match ty {
        NyarType::Named(name) if name.as_str() == "Self" => NyarType::Named(Identifier::new(owner)),
        NyarType::Array(element) => NyarType::Array(Box::new(concretize_self_type(element, self_owner))),
        NyarType::FixedArray { element, length } => {
            NyarType::FixedArray { element: Box::new(concretize_self_type(element, self_owner)), length: length.clone() }
        }
        NyarType::Apply(base, args) => NyarType::Apply(
            Box::new(concretize_self_type(base, self_owner)),
            args.iter().map(|arg| concretize_self_type(arg, self_owner)).collect(),
        ),
        NyarType::Union(arms) => NyarType::Union(arms.iter().map(|arm| concretize_self_type(arm, self_owner)).collect()),
        _ => ty.clone(),
    }
}

/// 将 `core::primitive::*`（或点号形式 `core.primitive.*`）原始类型名映射为 JVM 对应的原始类型。
///
/// Valkyrie 源码中以 `[primitive("core::primitive::usize")] structure usize { }` 形式定义的原始类型，
/// 在 HIR/MIR 中以 `NyarType::Named("usize")`（简单名）或 `NyarType::Named("core.primitive.usize")`（全名）形式出现。
/// 若不在此处映射，它们会被降级为 `Object` 引用类型，导致 `astore`/`aload` 与栈上 `int` 不匹配，
/// 触发 `VerifyError`（"Expecting to find integer on stack" / "Expecting to find object/array on stack"）。
///
/// 与 CLR 后端 [`map_primitive_name_to_msil`](crate::lowering::backends::clr::types::map_primitive_name_to_msil) 对齐：
/// `usize`/`isize` 在 JVM 上折叠为 `Integer32`（JVM 无 native size 类型，使用 `int` 表示）；
/// `i128`/`u128` 折叠为 `Integer64`（JVM `long` 为 64 位上限）。
///
/// 接受三种形式：简单名（`usize`）、点号形式（`core.primitive.usize`）、双冒号形式（`core::primitive::usize`）。
///
/// 返回 `None` 表示该名称不是已知的原始类型名，调用方应继续走 `Named`/`is_value_type_name` 等后续分支。
pub(crate) fn map_primitive_name_to_jvm(name: &str) -> Option<NyarType> {
    let normalized = name.replace("::", ".");
    let stripped = normalized.strip_prefix("core.primitive.").or_else(|| normalized.strip_prefix("primitive.")).unwrap_or(&normalized);
    let mapped = match stripped {
        "bool" => NyarType::Boolean,
        "char" => NyarType::Character,
        "i8" => NyarType::Integer8 { signed: true },
        "u8" => NyarType::Integer8 { signed: false },
        "i16" => NyarType::Integer16 { signed: true },
        "u16" => NyarType::Integer16 { signed: false },
        "i32" => NyarType::Integer32 { signed: true },
        "u32" => NyarType::Integer32 { signed: false },
        "i64" => NyarType::Integer64 { signed: true },
        "u64" => NyarType::Integer64 { signed: false },
        "i128" => NyarType::Integer64 { signed: true },
        "u128" => NyarType::Integer64 { signed: false },
        "f32" => NyarType::Float32,
        "f64" => NyarType::Float64,
        "f128" => NyarType::Float64,
        "usize" => NyarType::Integer32 { signed: false },
        "isize" => NyarType::Integer32 { signed: true },
        // ADT: void=0 (Bottom / uninhabited), unit=1 (Unit / inhabited). Never conflate.
        "void" => NyarType::Bottom,
        "unit" => NyarType::Unit,
        "null" | "any" => return None,
        _ => return None,
    };
    Some(mapped)
}

/// 判断 `NyarType::Union` 是否为 nullable 联合（即包含 `null` 臂）。
///
/// `T?` 在 HIR 中表示为 `Union([T, null])`，concretize 后 `null` 变为
/// `NyarType::Named("null")`。此方法检测该模式，用于将 nullable 联合与
/// 普通联合区分开。
pub(crate) fn is_nullable_union(ty: &NyarType) -> bool {
    matches!(ty, NyarType::Union(arms) if arms.iter().any(|arm| matches!(arm, NyarType::Named(name) if name.as_str() == "null")))
}

/// 提取 nullable 联合 `Union([T, null])` 的 payload 类型 `T`。
///
/// 若联合仅包含一个非 `null` 臂，返回该臂的类型。若包含多个非 `null` 臂，
/// 返回由它们组成的新 `Union`。若不是 nullable 联合，返回 `None`。
pub(crate) fn nullable_union_payload_type(ty: &NyarType) -> Option<NyarType> {
    if let NyarType::Union(arms) = ty {
        let non_null: Vec<_> = arms.iter().filter(|arm| !matches!(arm, NyarType::Named(name) if name.as_str() == "null")).cloned().collect();
        if non_null.is_empty() {
            return None;
        }
        if non_null.len() == 1 {
            return Some(non_null[0].clone());
        }
        return Some(NyarType::Union(non_null));
    }
    None
}

/// 解析 Named 值类型在 JVM 后端的实际表示类型。
///
/// 解析顺序：
/// 1. 若为 nullable 联合（`Union([T, null])`），按 payload 类型 `T` 决定表示：
///    - 引用类型（`Utf8`/`Named` 非值类型/`Array` 等）：保持引用，用
///      `astore`/`aload` + null 表示缺失。返回 payload 类型本身。
///    - 原始类型（`i32`/`bool`/`usize` 等）：保持原始，用 sentinel 值
///      表示 null。返回 payload 的原始类型。
///    - 多字段值类型（`is_value_type_name` && fields > 1）：必须装箱为堆对象
///      引用，否则无法用 null 表示缺失。返回 payload 的 `Named` 类型本身
///      （不折叠为首字段），使 `store_to_value`/`emit_operand` 使用
///      `astore`/`aload`。
///    - 单字段值类型：折叠为首字段类型（与非 nullable 值类型一致）。
///    否则 `Union` 会落入 `jvm_type_descriptor` 的 catch-all `Int`，
///    导致引用类型被 `istore`/`iload` 处理，触发
///    VerifyError: "Register N contains wrong type"。
/// 2. 若为 `core::primitive::*` 原始类型名，先映射为对应的 JVM 原始类型变体
///    （如 `usize` → `Integer32{signed:false}`），避免被当作引用类型处理。
/// 3. 若为 sum type（`enums` 或 `unite`，如 `WasmOpcode`/`WitWasiCoreResultKind`/
///    `VonParseResult`/`Option`/`Result`），映射为 `Integer32`：JVM 后端用 int 句柄
///    表示变体 tag（构造为 `iconst`）。若此处返回 `Named`，方法返回/参数描述符会变成
///    `L<Type>;`，与栈上 `int` 一起走 `checkcast`/`areturn`/`aload`，触发
///    VerifyError: "Expecting to find object/array on stack"。
/// 4. 否则若为值类型（`is_value_type_name`），使用第一个字段的类型作为有效类型，
///    使 load/store/return 指令与方法描述符保持一致。
/// 5. 否则原样返回。
pub(crate) fn effective_jvm_type(ctx: &ExecutableLoweringContext<'_>, ty: &NyarType) -> NyarType {
    match ty {
        NyarType::Union(_) if is_nullable_union(ty) => {
            let payload = nullable_union_payload_type(ty).unwrap_or(NyarType::Unit);
            // 多字段值类型 payload 必须装箱为堆对象引用（nullable 需要用 null
            // 表示缺失，内联值类型无法为 null）。返回 Named 类型本身，
            // 使后续 store/load 使用 astore/aload。
            if needs_boxing(ctx, &payload) {
                return payload;
            }
            effective_jvm_type(ctx, &payload)
        }
        NyarType::Named(name) => {
            if let Some(primitive) = map_primitive_name_to_jvm(name.as_str()) {
                return primitive;
            }
            if ctx.find_sum_type(name.as_str()).is_some() {
                return NyarType::Integer32 { signed: true };
            }
            if ctx.is_value_type_name(name.as_str()) {
                return ctx
                    .layout_by_type_name(name.as_str())
                    .and_then(|layout| layout.fields.first().map(|field| field.ty.clone()))
                    .unwrap_or(NyarType::Unit);
            }
            ty.clone()
        }
        NyarType::Apply(base, args) => {
            if let NyarType::Named(name) = base.as_ref() {
                // Zero-arg Apply on a primitive/value-type name (e.g. Apply(Named("usize"), []))
                // must resolve like Named — otherwise descriptors become Ljava/lang/Object;
                // while bodies still emit irem/iand/ireturn → VerifyError.
                if args.is_empty() {
                    return effective_jvm_type(ctx, &NyarType::Named(name.clone()));
                }
                if ctx.find_sum_type(name.as_str()).is_some() {
                    return NyarType::Integer32 { signed: true };
                }
            }
            ty.clone()
        }
        // 数组元素类型只需折叠 unite sum type 为 Integer32，值类型必须保持
        // Named（数组元素是堆对象引用，不能像 local 那样内联展开为多字段）。
        // 若对值类型也调用 `effective_jvm_type`，`LegionBuildTarget` 会被折叠
        // 为首字段 `Utf8`，数组描述符变成 `[Ljava/lang/String;`，与方法实际
        // 返回的 `[LLegionBuildTarget;` 不匹配，触发 VerifyError:
        // "Incompatible argument to function"。
        // 仅 unite sum type（如 `VonValue`）折叠为 `Integer32`，使数组描述符
        // 为 `[I`，与 `iaload`/`iastore` 和 int 局部一致。
        NyarType::Array(element) => NyarType::Array(Box::new(effective_array_element_type(ctx, element))),
        NyarType::FixedArray { element, length } => {
            NyarType::FixedArray { element: Box::new(effective_array_element_type(ctx, element)), length: length.clone() }
        }
        _ => ty.clone(),
    }
}

/// 计算数组元素的有效 JVM 表示类型。
///
/// 与 [`effective_jvm_type`] 不同，数组元素不会内联展开值类型——它们是堆对象引用，
/// 必须保持 `Named` 类型以生成正确的 `[LType;` 描述符。此函数仅折叠 unite sum type
/// 为 `Integer32`，使 `[VonValue]` 等数组生成 `[I` 描述符，与 `iaload`/`iastore`
/// 和 int 局部句柄一致。
///
/// - `VonValue` / `WasmOpcode`（sum type：unite 或 enums）→ `Integer32`（数组描述符 `[I`）
/// - `LegionBuildTarget`（值类型）→ 保持 `Named`（数组描述符 `[LLegionBuildTarget;`）
/// - `Utf8`/`Utf16` → 保持原样（数组描述符 `[Ljava/lang/String;`）
/// - 原始类型 → 保持原样
pub(crate) fn effective_array_element_type(ctx: &ExecutableLoweringContext<'_>, ty: &NyarType) -> NyarType {
    match ty {
        NyarType::Named(name) => {
            if let Some(primitive) = map_primitive_name_to_jvm(name.as_str()) {
                return primitive;
            }
            if ctx.find_sum_type(name.as_str()).is_some() {
                return NyarType::Integer32 { signed: true };
            }
            ty.clone()
        }
        NyarType::Apply(base, args) => {
            if let NyarType::Named(name) = base.as_ref() {
                if let Some(primitive) = map_primitive_name_to_jvm(name.as_str()) {
                    return primitive;
                }
                if ctx.find_sum_type(name.as_str()).is_some() {
                    return NyarType::Integer32 { signed: true };
                }
                if args.is_empty() {
                    return effective_array_element_type(ctx, &NyarType::Named(name.clone()));
                }
            }
            ty.clone()
        }
        NyarType::Union(_) if is_nullable_union(ty) => {
            let payload = nullable_union_payload_type(ty).unwrap_or(NyarType::Unit);
            if needs_boxing(ctx, &payload) {
                return payload;
            }
            effective_array_element_type(ctx, &payload)
        }
        _ => ty.clone(),
    }
}

/// 判断值类型是否需要装箱为堆对象。
///
/// JVM 方法返回值只能是单个值，无法内联返回多字段值类型。
/// 当值类型拥有多于一个字段时，返回 `true`，表示调用方和被调用方
/// 应将该值类型作为堆对象引用处理（`new` + `putfield` 装箱，
/// `getfield` 拆字段），而非内联展开为连续 local。
pub(crate) fn needs_boxing(ctx: &ExecutableLoweringContext<'_>, ty: &NyarType) -> bool {
    if let NyarType::Named(name) = ty {
        // `structure isize {}` 等 primitive 别名也在 value_type_names 里，但 JVM ABI 是
        // int 句柄，绝不能按多字段值类型装箱为 `L…;`。
        if map_primitive_name_to_jvm(name.as_str()).is_some() {
            return false;
        }
        if ctx.is_value_type_name(name.as_str()) {
            if let Some(layout) = ctx.layout_by_type_name(name.as_str()) {
                return layout.fields.len() > 1;
            }
        }
    }
    false
}

/// 真正的结构体值类型（可 flatten / 可 boxed），排除 `isize`/`bool` 等 primitive 别名。
fn is_structure_value_type(ctx: &ExecutableLoweringContext<'_>, ty: &NyarType) -> bool {
    match ty {
        NyarType::Named(name) => map_primitive_name_to_jvm(name.as_str()).is_none() && ctx.is_value_type_name(name.as_str()),
        _ => false,
    }
}

/// JVM verifier local category — int-handle ABI vs object reference must not share a slot.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JvmLocalKind {
    Int,
    Long,
    Float,
    Double,
    Reference,
}

/// True when a store of `width` JVM slots at `planned` would reuse a local whose
/// verifier category belongs to a different value. Category-2 locals reserve both
/// slots, so their high slot is not an independently allocatable local.
pub fn jvm_local_slot_conflicts(
    local_kinds: &std::collections::BTreeMap<u16, JvmLocalKind>,
    planned: u16,
    kind: JvmLocalKind,
    width: u16,
) -> bool {
    let base = local_kinds.get(&planned).copied();
    if base.is_some_and(|previous| previous != kind) {
        return true;
    }
    if width <= 1 {
        return false;
    }
    match local_kinds.get(&planned.saturating_add(1)).copied() {
        // A consecutive Long/Double pair is a repeated store to the same wide home
        // only when its base is also reserved with that category.
        Some(_) if base.is_none() => true,
        Some(previous) => previous != kind,
        // A base without its required high slot is not a valid existing wide home.
        None => base.is_some(),
    }
}

fn jvm_local_kind_of(ty: &NyarType) -> JvmLocalKind {
    if let NyarType::Named(name) = ty {
        if let Some(primitive) = map_primitive_name_to_jvm(name.as_str()) {
            return jvm_local_kind_of(&primitive);
        }
    }
    match ty {
        NyarType::Float64 => JvmLocalKind::Double,
        NyarType::Float32 => JvmLocalKind::Float,
        NyarType::Integer64 { .. } => JvmLocalKind::Long,
        NyarType::Boolean
        | NyarType::Integer8 { .. }
        | NyarType::Integer16 { .. }
        | NyarType::Integer32 { .. }
        | NyarType::Character
        | NyarType::Bottom
        | NyarType::Unit => JvmLocalKind::Int,
        NyarType::Named(_) | NyarType::Utf8 | NyarType::Utf16 | NyarType::Array(_) | NyarType::FixedArray { .. } => JvmLocalKind::Reference,
        // Non-sum `Apply` left after `effective_jvm_type` is a generic class → reference.
        // Sum `Apply`/`Result`/`Option` must already be folded to `Integer32`.
        NyarType::Apply(_, _) | NyarType::Union(_) => JvmLocalKind::Reference,
        _ => JvmLocalKind::Int,
    }
}

/// Stack category for an **already-effective** Nyar type (post-`effective_jvm_type`).
///
/// Sum/unite/`Result` handles are `Integer32` here and must not take `areturn`/`aload`/
/// `checkcast`. Generic `Apply` / `Union` that were not folded are references.
fn is_jvm_stack_reference(ty: &NyarType) -> bool {
    if let NyarType::Named(name) = ty {
        if map_primitive_name_to_jvm(name.as_str()).is_some() {
            return false;
        }
    }
    matches!(
        ty,
        NyarType::Utf8
            | NyarType::Utf16
            | NyarType::Named(_)
            | NyarType::Array(_)
            | NyarType::FixedArray { .. }
            | NyarType::Apply(_, _)
            | NyarType::Union(_)
    )
}

fn jvm_descriptor_is_reference(desc: &JvmTypeDescriptor) -> bool {
    matches!(desc, JvmTypeDescriptor::Object(_) | JvmTypeDescriptor::Array(_))
}

struct JvmMirLowerer<'a> {
    ctx: ExecutableLoweringContext<'a>,
    mir_fn: &'a MirFunction,
    /// 方法所属类型名（`Type.method` 的 `Type`），用于把 MIR `Self` 收成具体类型。
    self_owner: Option<String>,
    slots: ExecutableSlotPlan,
    instructions: Vec<JvmInstruction>,
    label_counter: u32,
    /// 变量名到其声明类型的映射，用于 `emit_operand` 的 Symbol 路径
    /// 根据 StoreVar 指令的 `ty` 字段或 output 值类型推断。
    var_types: std::collections::BTreeMap<String, NyarType>,
    /// Backend-local physical representation overrides for explicitly typed
    /// call results. This never supplies a missing source type.
    value_type_overrides: std::collections::BTreeMap<MirValueRef, NyarType>,
    /// Source unite names retained after their JVM storage representation is erased to `int`.
    semantic_unite_value_types: std::collections::BTreeMap<MirValueRef, String>,
    /// 被装箱为堆对象引用的 SSA 值集合。
    ///
    /// 当函数返回多字段值类型（`structure` with >1 fields）时，JVM 无法用单个
    /// 返回值表示整个值类型。后端将值类型装箱为堆对象，返回引用。
    /// 此集合标记哪些 SSA 值是装箱引用，使 `emit_operand` / `store_to_value` /
    /// `Copy` / `FieldGet` / `emit_call_argument` 等操作跳过
    /// `effective_jvm_type` 展开，直接用 `aload`/`astore`/`getfield` 处理。
    boxed_value_refs: std::collections::BTreeSet<MirValueRef>,
    /// First store kind per local. Reused slots with a different kind (int handle vs
    /// reference) are reallocated — otherwise `Register N contains wrong type`.
    local_kinds: std::collections::BTreeMap<u16, JvmLocalKind>,
}

impl<'a> JvmMirLowerer<'a> {
    fn seed_parameter_value_types(&mut self) {
        for value in &self.mir_fn.values {
            let MirValueOrigin::Parameter { index, .. } = value.origin
            else {
                continue;
            };
            let Some(ty) = self.mir_fn.param_types.get(index).cloned()
            else {
                continue;
            };
            self.value_type_overrides.insert(value.id, concretize_self_type(&ty, self.self_owner.as_deref()));
        }
    }

    /// Mark JVM ABI parameter slots with their verifier category so later stores of a
    /// different kind (`astore` then `istore` on the same home) reallocate instead of
    /// poisoning the local (`Register N contains wrong type`).
    fn seed_parameter_local_kinds(&mut self) {
        let mut slot = 0u16;
        for ty in &self.mir_fn.param_types {
            let ty = concretize_self_type(ty, self.self_owner.as_deref());
            // The JVM descriptor and reserved locals use flattened value-type
            // fields. Seed each flattened field separately; treating a struct
            // parameter as one reference slot mislabels later stores (notably
            // the high-numbered PE metadata helpers) and creates category
            // conflicts such as local 399 being both AStore and IStore.
            for field_ty in jvm_type_field_types(&self.ctx, &ty) {
                let effective = effective_jvm_type(&self.ctx, &field_ty);
                let kind = jvm_local_kind_of(&effective);
                let width = super::executable::jvm_local_slots(&effective).max(1);
                self.local_kinds.insert(slot, kind);
                if width > 1 {
                    self.local_kinds.insert(slot.saturating_add(1), kind);
                }
                slot = slot.saturating_add(width);
            }
        }
    }

    /// If `planned` was already written with an incompatible JVM category, allocate a
    /// fresh local. A category-2 value occupies *both* `planned` and `planned + 1`:
    /// checking only the base lets `lstore N` overwrite an adjacent reference in
    /// `N + 1`, then a later `aload N + 1` fails verification. Callers must update
    /// `value_locals` / `var_locals` when the returned slot differs from `planned`.
    fn realloc_local_if_kind_conflict(&mut self, planned: u16, kind: JvmLocalKind, ty: &NyarType) -> u16 {
        let width = super::executable::jvm_local_slots(ty).max(1);
        if jvm_local_slot_conflicts(&self.local_kinds, planned, kind, width) {
            // Value-type fields are addressed as `base + offset` in several
            // lowering paths. When one leaf conflicts, move the whole existing
            // contiguous aggregate range so later offsets cannot land back on
            // the old category-mixed homes.
            // `planned` is often a flattened leaf (`base + offset`) of an
            // aggregate.  Moving only the suffix beginning at that leaf leaves
            // the aggregate base and earlier leaves in the old category-mixed
            // range; the next field access then reconstructs the old slot and
            // reintroduces the conflict.  Expand to the complete contiguous
            // occupied range first, then move that range as one unit.
            // Find the SSA aggregate whose planned base owns this leaf.  Do
            // not scan arbitrary adjacent kinds: the global slot plan is
            // densely numbered, so such a scan would swallow every local.
            let mut range_start = planned;
            let mut range_end = planned.saturating_add(width);
            for (&value, &base) in &self.slots.value_locals {
                let ty = self.lookup_value_type(&value);
                let span = super::executable::jvm_local_slots(&ty).max(1);
                if base <= planned && planned.saturating_add(width) <= base.saturating_add(span) {
                    range_start = base;
                    range_end = base.saturating_add(span);
                }
            }
            let alloc_width = range_end.saturating_sub(range_start).max(width);
            // Only aggregates need their earlier instructions and every base map
            // rewritten as a unit. For a scalar reassignment, rewriting the old
            // store moves (for example) the historical `astore` onto the new
            // `istore` home, recreating the verifier-category conflict we just
            // avoided. The scalar caller updates its own current mapping from
            // the returned slot instead.
            let relocates_aggregate_range = range_start != planned || alloc_width != width;
            let next_local = self.slots.local_types.len();
            assert!(
                next_local <= u16::MAX as usize,
                "JVM local slot overflow while relocating aggregate: len={next_local}, range={range_start}..{range_end}, planned={planned}, width={width}"
            );
            let new_local = next_local as u16;
            if planned >= 200 {
                eprintln!(
                    "[jvm_realloc] planned={planned} kind={kind:?} width={width} range={range_start}..{range_end} new={new_local} existing={:?}",
                    self.local_kinds.get(&planned)
                );
            }
            let fill = self
                .slots
                .local_types
                .get(range_start as usize)
                .cloned()
                .or_else(|| self.slots.local_types.last().cloned())
                .unwrap_or(crate::nyar_backend_clr::MsilType::Int32 { signed: true });
            for _ in 0..alloc_width {
                self.slots.local_types.push(fill.clone());
            }
            let delta = new_local.saturating_sub(range_start);
            if relocates_aggregate_range {
                // Keep every aggregate SSA/symbol mapping synchronized even for
                // callers that intentionally ignore the returned slot
                // (field/aggregate helpers). Otherwise a later leaf can keep
                // using the conflicted range.
                for instruction in &mut self.instructions {
                    let local = match instruction {
                        JvmInstruction::ILoad(slot)
                        | JvmInstruction::IStore(slot)
                        | JvmInstruction::LLoad(slot)
                        | JvmInstruction::LStore(slot)
                        | JvmInstruction::FLoad(slot)
                        | JvmInstruction::FStore(slot)
                        | JvmInstruction::DLoad(slot)
                        | JvmInstruction::DStore(slot)
                        | JvmInstruction::ALoad(slot)
                        | JvmInstruction::AStore(slot) => slot,
                        _ => continue,
                    };
                    if *local >= range_start && *local < range_end {
                        *local = local.saturating_add(delta);
                    }
                }
                for local in self.slots.value_locals.values_mut() {
                    if *local >= range_start && *local < range_end {
                        *local = local.saturating_add(delta);
                    }
                }
                for local in self.slots.var_locals.values_mut() {
                    if *local >= range_start && *local < range_end {
                        *local = local.saturating_add(delta);
                    }
                }
                // Block parameters are another source of planned local operands.
                // Keep them aligned with value/variable maps after aggregate
                // relocation; otherwise a later edge copy can re-emit an old
                // IStore/ AStore at the conflicted home (notably local 223).
                for local in self.slots.block_param_locals.values_mut() {
                    if *local >= range_start && *local < range_end {
                        *local = local.saturating_add(delta);
                    }
                }
            }
            for offset in 0..alloc_width {
                let prior = self.local_kinds.get(&range_start.saturating_add(offset)).copied();
                self.local_kinds.insert(
                    new_local.saturating_add(offset),
                    if offset == planned.saturating_sub(range_start) { kind } else { prior.unwrap_or(kind) },
                );
            }
            new_local
        }
        else {
            if planned >= 200 {
                eprintln!("[jvm_store] planned={planned} kind={kind:?} width={width} existing={:?}", self.local_kinds.get(&planned));
            }
            self.local_kinds.insert(planned, kind);
            if width > 1 {
                self.local_kinds.insert(planned.saturating_add(1), kind);
            }
            planned
        }
    }

    fn emit_block(&mut self, block: &MirBlock) {
        self.instructions.push(JvmInstruction::Label(block_label(block.id)));
        for instruction in &block.instructions {
            self.emit_instruction(instruction);
        }
        self.emit_terminator(block);
    }

    /// 生成一个唯一的局部标签名，用于比较运算的分支模式。
    fn next_label(&mut self) -> String {
        let label = format!("__cmp_{}", self.label_counter);
        self.label_counter += 1;
        label
    }

    /// 在 `Jump` 终止符跳转前，将 `arguments` 依次写入目标块的 block 参数 local，
    /// 并把 `value_locals[parameter] = param_local` 注册，使后续 `emit_operand` 能找到它。
    ///
    /// 这对 loop-carried variables 至关重要：两遍 loop lowering 会为循环跨迭代变量
    /// 在 `loop_header` 块上创建 block 参数，后向 `Jump` 的 arguments 携带这些变量
    /// 的当前值。若不填充，block 参数对应的 SSA Value 永远查不到 local，
    /// `emit_operand` 静默不发射指令，导致后续算术操作数栈为空。
    fn emit_block_argument_copies(&mut self, target: MirBlockRef, arguments: &[MirOperand]) {
        let Some(target_block) = self.mir_fn.blocks.get(target.0 as usize)
        else {
            return;
        };
        for (index, parameter) in target_block.parameters.iter().enumerate() {
            let Some(argument) = arguments.get(index)
            else {
                continue;
            };
            let Some(param_local) = self.slots.block_param_locals.get(&(target, index)).copied()
            else {
                continue;
            };
            let ty = self.mir_fn.value_types.get(parameter).cloned().unwrap_or(NyarType::Unit);
            // 如果源参数是 boxed 值类型，直接用 aload/astore 复制引用，
            // 不展开字段。否则会对对象引用 local 发射 iload，触发 VerifyError。
            let source_is_boxed = match argument {
                MirOperand::Value(v) => self.boxed_value_refs.contains(v),
                _ => false,
            };
            if !source_is_boxed {
                if let NyarType::Named(name) = &ty {
                    if self.ctx.is_value_type_name(name.as_str()) {
                        if let MirOperand::Value(source_value) = argument {
                            if let Some(source_local) = self.slots.value_locals.get(source_value).copied() {
                                // 守卫：仅当源值的实际类型与参数类型一致时才走
                                // `copy_value_type_fields`。MIR 生成器可能将不同类型的
                                // 参数与实参配对（例如循环块参数声明为 `LegionWorkspaceManifest`
                                // 但实参是 `usize` 索引），此时按参数类型展开字段会从源
                                // local（int）发射 `aload`，与 `istore` 写入的 int 冲突，
                                // 触发 VerifyError: "Register N contains wrong type"。
                                // 类型不一致时回退到通用 load/store 路径，用源实际类型驱动。
                                let source_ty = self.lookup_value_type(source_value);
                                let source_matches_param =
                                    source_ty == ty || matches!(&source_ty, NyarType::Named(src_name) if src_name.as_str() == name.as_str());
                                if source_matches_param {
                                    let actual_local = self.copy_value_type_fields(source_local, param_local, name.as_str());
                                    self.slots.block_param_locals.insert((target, index), actual_local);
                                    self.slots.value_locals.insert(*parameter, actual_local);
                                    continue;
                                }
                            }
                        }
                    }
                }
            }
            let effective_ty = if source_is_boxed {
                match argument {
                    MirOperand::Value(v) => {
                        let arg_ty = self.lookup_value_type(v);
                        self.value_type_overrides.insert(*parameter, arg_ty.clone());
                        arg_ty
                    }
                    _ => ty.clone(),
                }
            }
            else {
                // 优先使用源实际类型而非参数声明类型：MIR 生成器可能将参数声明为
                // `LegionWorkspaceManifest` 但实参是 `usize`，按参数类型计算
                // `effective_jvm_type` 会得到 `Named`（引用），发射 `astore`，
                // 但 `emit_operand` 按源实际类型 `Integer32` 发射 `iload`，
                // 栈上是 int 与 `astore` 不匹配触发 VerifyError。
                let source_effective_ty = match argument {
                    MirOperand::Value(v) => {
                        let arg_ty = self.lookup_value_type(v);
                        effective_jvm_type(&self.ctx, &arg_ty)
                    }
                    _ => effective_jvm_type(&self.ctx, &ty),
                };
                self.value_type_overrides.insert(*parameter, source_effective_ty.clone());
                source_effective_ty
            };
            self.emit_operand(argument);
            let actual_local = self.emit_store_local(param_local, &effective_ty);
            if actual_local != param_local {
                self.slots.block_param_locals.insert((target, index), actual_local);
            }
            if source_is_boxed {
                self.boxed_value_refs.insert(*parameter);
            }
            self.slots.value_locals.insert(*parameter, actual_local);
        }
    }

    fn emit_instruction(&mut self, instruction: &MirInstruction) {
        match &instruction.kind {
            MirInstructionKind::LoadConstant { constant, ty } => {
                self.emit_load_constant(constant, ty.as_ref());
                if let Some(output) = instruction.output {
                    self.store_to_value(output);
                }
            }
            MirInstructionKind::StoreVar { name, value, ty } => {
                // Store opcode must match the value on the stack. Prefer value inference,
                // then this instruction's `ty`, then a previously recorded var type.
                // The old "first var_types wins" lock kept Utf8 after a later Int assign
                // on the same planned home — either astore of int, or (older emitters)
                // astore then istore → VerifyError "Register N contains wrong type".
                // Category conflicts are handled by `emit_store_local` reallocation.
                let inferred = self.infer_field_type(value);
                let assigned_ty = if !matches!(inferred, NyarType::Unit | NyarType::Bottom) {
                    inferred
                }
                else if let Some(annotated) = ty.clone().filter(|t| !matches!(t, NyarType::Unit | NyarType::Bottom)) {
                    annotated
                }
                else {
                    self.var_types.get(name).cloned().unwrap_or(NyarType::Unit)
                };
                // 多字段值类型必须在 `effective_jvm_type` 折叠为首字段之前判断
                // `needs_boxing`。否则 `PeTokenTables` 被收成 `[LPeAssemblyRefRow;`，
                // 装箱分支跳过，扁平 `astore` 写入相邻 1 槽变量（如 `meta`），
                // 调用处把 `String[]` 当成 `[S` 传参 → VerifyError:
                // "Incompatible argument to function"。
                if needs_boxing(&self.ctx, &assigned_ty) {
                    let is_value_operand = matches!(value, MirOperand::Value(v) if !self.boxed_value_refs.contains(&v));
                    let is_null_symbol =
                        matches!(value, MirOperand::Symbol(path) if path.parts().len() == 1 && path.parts()[0].as_str() == "null");
                    if is_value_operand {
                        if let MirOperand::Value(v) = value {
                            if let Some(base_local) = self.slots.value_locals.get(&v).copied() {
                                let boxed = self.emit_box_value_type_to_stack(&assigned_ty, base_local);
                                if boxed {
                                    if let Some(planned_local) = self.slots.var_locals.get(name).copied() {
                                        let actual = self.emit_store_local(planned_local, &assigned_ty);
                                        if actual != planned_local {
                                            self.slots.var_locals.insert(name.clone(), actual);
                                        }
                                        if let Some(output) = instruction.output {
                                            self.slots.value_locals.insert(output, actual);
                                            self.boxed_value_refs.insert(output);
                                        }
                                    }
                                    self.var_types.insert(name.clone(), assigned_ty.clone());
                                    return;
                                }
                            }
                        }
                    }
                    else if is_null_symbol {
                        self.emit_operand(value);
                        if let Some(planned_local) = self.slots.var_locals.get(name).copied() {
                            let actual = self.emit_store_local(planned_local, &assigned_ty);
                            if actual != planned_local {
                                self.slots.var_locals.insert(name.clone(), actual);
                            }
                            if let Some(output) = instruction.output {
                                self.slots.value_locals.insert(output, actual);
                                self.boxed_value_refs.insert(output);
                            }
                        }
                        self.var_types.insert(name.clone(), assigned_ty.clone());
                        return;
                    }
                    else if matches!(value, MirOperand::Value(v) if self.boxed_value_refs.contains(&v)) {
                        // 已装箱引用：直接 aload/astore，并传播 boxed 标记。
                        self.emit_operand(value);
                        if let Some(planned_local) = self.slots.var_locals.get(name).copied() {
                            let actual = self.emit_store_local(planned_local, &assigned_ty);
                            if actual != planned_local {
                                self.slots.var_locals.insert(name.clone(), actual);
                            }
                            if let Some(output) = instruction.output {
                                self.slots.value_locals.insert(output, actual);
                                self.boxed_value_refs.insert(output);
                            }
                        }
                        self.var_types.insert(name.clone(), assigned_ty.clone());
                        return;
                    }
                }
                let effective_ty = effective_jvm_type(&self.ctx, &assigned_ty);
                self.var_types.insert(name.clone(), effective_ty.clone());
                self.emit_operand(value);
                if let Some(planned_local) = self.slots.var_locals.get(name).copied() {
                    // Kind-aware store: int-handle vs reference must not share a JVM local.
                    // `emit_store_local` reallocates on category conflict; keep var_locals in sync.
                    let target_local = self.emit_store_local(planned_local, &effective_ty);
                    if target_local != planned_local {
                        self.slots.var_locals.insert(name.clone(), target_local);
                    }
                    if let Some(output) = instruction.output {
                        // StoreVar 覆盖 target_local 后，之前映射到同一 local 且
                        // 类型不同的 SSA 值变为死值。若不清除，后续读取这些死值
                        // 时 emit_operand 会生成类型不匹配的 load 指令，触发
                        // VerifyError。仅清除类型不同的映射，同类型复用不影响
                        // 验证器。
                        let dead_values: Vec<MirValueRef> = self
                            .slots
                            .value_locals
                            .iter()
                            .filter(|(_, l)| **l == target_local)
                            .filter(|(v, _)| {
                                let prev_ty = self.lookup_value_type(v);
                                let prev_effective = effective_jvm_type(&self.ctx, &prev_ty);
                                prev_effective != effective_ty
                            })
                            .map(|(v, _)| *v)
                            .collect();
                        for v in dead_values {
                            self.slots.value_locals.remove(&v);
                        }
                        self.slots.value_locals.insert(output, target_local);
                    }
                }
            }
            MirInstructionKind::Copy { source } => {
                if let Some(output) = instruction.output {
                    if let MirOperand::Value(source_value) = source {
                        let ty = self.lookup_value_type(source_value);
                        if let NyarType::Named(name) = &ty {
                            // boxed 值类型（多字段值类型装箱为堆对象引用）必须跳过
                            // copy_value_type_fields 的字段级 iload/istore 路径，
                            // 改走通用 aload/astore 路径。否则会对对象引用发射
                            // iload，栈上类型不匹配触发 VerifyError。
                            if self.ctx.is_value_type_name(name.as_str()) && !self.boxed_value_refs.contains(source_value) {
                                if let (Some(source_local), Some(dest_local)) =
                                    (self.slots.value_locals.get(source_value).copied(), self.slots.value_locals.get(&output).copied())
                                {
                                    let actual_local = self.copy_value_type_fields(source_local, dest_local, name.as_str());
                                    self.slots.value_locals.insert(output, actual_local);
                                    return;
                                }
                            }
                        }
                        // 当 `lookup_value_type(source_value)` 误判源类型时（例如
                        // `value_type_overrides` 有错误条目，或 `mir_fn.value_types`
                        // 缺少源值条目而回退为 `Unit`/`Integer32`），Copy 会落入
                        // 通用路径，仅用 `emit_operand` + `emit_store_local` 复制
                        // 第一个字段。但 slot plan 按 `mir_fn.value_types[output]`
                        // 为 output 分配了多个连续 local（值类型每个字段一个），
                        // 未被写入的字段 local 保持未初始化状态。后续读取 output
                        // 全部字段时（`emit_call_argument` / `copy_value_type_fields`
                        // / `emit_block_argument_copies`）会 `iload` 这些未初始化
                        // local，触发 VerifyError: "Accessing value from
                        // uninitialized register N"。
                        //
                        // 修复：用 output 在 `mir_fn.value_types` 中的声明类型
                        // （slot plan 的权威来源）做二次检测。若 output 是内联值
                        // 类型且源未被装箱，走 `copy_value_type_fields` 复制全部字段。
                        //
                        // 关键守卫：源类型必须也是 Named 值类型。当源是 int 句柄
                        // （如 `tuple_get_N` 返回的 `Integer32`，由
                        // `value_type_overrides` 标记）但 output 在 `value_types`
                        // 中注册为值类型时，源 local 实际只含 1 个 int，而非展开
                        // 的多个字段 local。若调用 `copy_value_type_fields`，它会
                        // 按 output 类型的字段布局读取 source_local + offset，
                        // 对首字段发射 `aload`（若首字段是引用类型），与 `istore`
                        // 写入的 int 冲突，触发 VerifyError:
                        // "Register N contains wrong type"。
                        let output_ty = self.mir_fn.value_types.get(&output).cloned().unwrap_or(NyarType::Unit);
                        if let NyarType::Named(out_name) = &output_ty {
                            if self.ctx.is_value_type_name(out_name.as_str()) && !self.boxed_value_refs.contains(source_value) {
                                let source_ty = self.lookup_value_type(source_value);
                                let source_is_inline_value_type =
                                    matches!(&source_ty, NyarType::Named(src_name) if self.ctx.is_value_type_name(src_name.as_str()));
                                if source_is_inline_value_type {
                                    if let (Some(source_local), Some(dest_local)) =
                                        (self.slots.value_locals.get(source_value).copied(), self.slots.value_locals.get(&output).copied())
                                    {
                                        let actual_local = self.copy_value_type_fields(source_local, dest_local, out_name.as_str());
                                        self.slots.value_locals.insert(output, actual_local);
                                        return;
                                    }
                                }
                            }
                        }
                    }
                    self.emit_operand(source);
                    // 强制用源值的实际类型驱动 store，而非 output 的类型。
                    // 当源 SSA 值在 value_types 中被 HIR resolver 误注册为
                    // Named(_) 引用类型时，store_to_value 会发射 AStore，
                    // 但 emit_operand 按源实际类型发射了 ILoad，栈上是 int，
                    // 两者不匹配触发 VerifyError: "Expecting to find
                    // object/array on stack"。栈上的值已是源类型，故用源类型
                    // 做 emit_store_local 保证 load/store 类型一致。
                    let source_effective_ty = match source {
                        MirOperand::Value(v) => {
                            let ty = self.lookup_value_type(v);
                            if self.boxed_value_refs.contains(v) { ty } else { effective_jvm_type(&self.ctx, &ty) }
                        }
                        _ => {
                            let ty = self.infer_field_type(source);
                            effective_jvm_type(&self.ctx, &ty)
                        }
                    };
                    if let Some(dest_local) = self.slots.value_locals.get(&output).copied() {
                        let actual = self.emit_store_local(dest_local, &source_effective_ty);
                        if actual != dest_local {
                            self.slots.value_locals.insert(output, actual);
                        }
                    }
                    // 将源类型注册到 output 的类型覆盖表，使后续
                    // `emit_operand(output)` / `store_to_value(output)` 通过
                    // `lookup_value_type(output)` 获取与 store 一致的类型。
                    // 若省略此步，当 HIR resolver 在 `value_types` 中将
                    // output 误注册为 Integer32 而源实际为 Named 对象时，
                    // `emit_operand(output)` 会发射 ILoad，但 register 已被
                    // AStore 写为对象引用，触发 VerifyError:
                    // "Register N contains wrong type"。
                    self.value_type_overrides.insert(output, source_effective_ty);
                    if let MirOperand::Value(source_value) = source {
                        if let Some(sum_name) = self.semantic_unite_value_types.get(source_value).cloned() {
                            self.semantic_unite_value_types.insert(output, sum_name);
                        }
                    }
                    // 当源是 boxed 值类型时，output 也必须是 boxed 引用，
                    // 否则后续 emit_operand / FieldGet 会用 effective_jvm_type
                    // 展开，发射 iload 而非 aload，触发 VerifyError。
                    if let MirOperand::Value(v) = source {
                        if self.boxed_value_refs.contains(v) {
                            self.boxed_value_refs.insert(output);
                        }
                    }
                }
            }
            MirInstructionKind::StructNew { type_name, storage, fields, layout_id } => {
                let mut local = instruction.output.and_then(|v| self.slots.value_locals.get(&v).copied()).expect("struct local");
                if *storage == StorageKind::Value {
                    for (field_name, value) in fields {
                        let field_offset = self.ctx.field_slot_index(*layout_id, type_name, field_name);
                        let field_local = local + field_offset;
                        let field_ty = self.ctx.field_type(*layout_id, type_name, field_name).unwrap_or_else(|| self.infer_field_type(value));
                        let unboxed_local = match value {
                            MirOperand::Value(value_ref) if self.boxed_value_refs.contains(value_ref) => self
                                .slots
                                .value_locals
                                .get(value_ref)
                                .copied()
                                .and_then(|source_local| self.emit_unbox_value_type_to_locals(source_local, &field_ty, field_local)),
                            _ => None,
                        };
                        if let Some(actual_local) = unboxed_local {
                            local = actual_local.saturating_sub(field_offset);
                            continue;
                        }
                        // Nested multi-field value type (e.g. VonDiagnostic.span: TextSpan):
                        // copy all flattened slots. A single emit_operand+store only writes
                        // the first leaf and may aload an int slot → VerifyError.
                        if let (MirOperand::Value(source_value), NyarType::Named(field_name_ty)) = (value, &field_ty) {
                            if map_primitive_name_to_jvm(field_name_ty.as_str()).is_none()
                                && self.ctx.is_value_type_name(field_name_ty.as_str())
                                && !self.boxed_value_refs.contains(source_value)
                            {
                                if let Some(source_local) = self.slots.value_locals.get(source_value).copied() {
                                    let actual_local = self.copy_value_type_fields(source_local, field_local, field_name_ty.as_str());
                                    local = actual_local.saturating_sub(field_offset);
                                    continue;
                                }
                            }
                        }
                        self.emit_operand(value);
                        let effective_field_ty = effective_jvm_type(&self.ctx, &field_ty);
                        let actual_local = self.emit_store_local(field_local, &effective_field_ty);
                        local = actual_local.saturating_sub(field_offset);
                    }
                    if let Some(output) = instruction.output {
                        self.slots.value_locals.insert(output, local);
                    }
                }
                else {
                    let jvm_class = type_name.replace('.', "/");
                    self.instructions.push(JvmInstruction::New(jvm_class.clone()));
                    // `new` 后必须 `invokespecial <init>()V`，否则后续 putfield/getfield
                    // 操作未初始化引用，触发 VerifyError: "Expecting to find object/array on stack"。
                    self.instructions.push(JvmInstruction::Dup);
                    self.instructions.push(JvmInstruction::InvokeSpecial(JvmMethodRef {
                        owner: jvm_class.clone(),
                        name: "<init>".to_string(),
                        descriptor: JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Void),
                    }));
                    for (field_name, value) in fields {
                        self.instructions.push(JvmInstruction::Dup);
                        let field_ty = self.ctx.field_type(*layout_id, type_name, field_name).unwrap_or_else(|| self.infer_field_type(value));
                        // 对 Named 值类型字段，先装箱为堆对象引用再 putfield，
                        // 因为 JVM 类字段声明为 `LType;`（对象引用）。
                        // 已装箱引用（boxed_value_refs）只占 1 槽，禁止按扁平槽
                        // 位重新装箱（见 FieldSet 同名检查），直接 emit_operand
                        // aload 引用。
                        let field_boxed = if let MirOperand::Value(vref) = value {
                            if self.boxed_value_refs.contains(vref) {
                                false
                            }
                            else {
                                self.slots
                                    .value_locals
                                    .get(vref)
                                    .copied()
                                    .map(|base| self.emit_box_value_type_to_stack(&field_ty, base))
                                    .unwrap_or(false)
                            }
                        }
                        else {
                            false
                        };
                        if !field_boxed {
                            self.emit_operand(value);
                        }
                        self.emit_field_store_checkcast(&field_ty);
                        self.instructions.push(JvmInstruction::PutField(JvmFieldRef {
                            owner: jvm_class.clone(),
                            name: field_name.clone(),
                            descriptor: self.jvm_field_descriptor_for_class(&field_ty),
                        }));
                    }
                    // Must go through emit_store_local so local_kinds records Reference;
                    // a raw AStore leaves the slot unmarked and a later IStore reuses it.
                    let struct_ty = NyarType::Named(nyar::Identifier::new(type_name.as_str()));
                    let actual = self.emit_store_local(local, &struct_ty);
                    if let Some(output) = instruction.output {
                        if actual != local {
                            self.slots.value_locals.insert(output, actual);
                        }
                        // Reference 存储的 StructNew 已装箱为堆对象引用，标记为
                        // boxed 使后续 emit_operand / FieldGet / emit_boxed_return
                        // 走 aload + getfield 路径，而非展开字段路径。否则
                        // emit_boxed_return 会按 base+offset 读取不存在的字段槽位，
                        // 触发 VerifyError: "Register N contains wrong type"。
                        self.boxed_value_refs.insert(output);
                    }
                }
            }
            /// `ArrayLiteral`：堆数组字面量构造，恒为引用语义。
            ///
            /// 生成 `iconst <n>` / `anewarray <type>` / 逐元素 `dup` / `iconst <i>` /
            /// `<element>` / `aastore`（或 `iastore`）序列，最后 `astore <local>`。
            MirInstructionKind::ArrayLiteral { element_type, items, .. } => {
                self.emit_jvm_array_literal(instruction.output, element_type, items);
            }
            /// `ArrayNew`：按长度构造堆数组，恒为引用语义。
            MirInstructionKind::ArrayNew { element_type, length, .. } => {
                self.emit_jvm_array_new(instruction.output, element_type, length);
            }
            /// `FixedArrayNew`（Reference 语义）：定长数组作为堆引用而非内联值类型，
            /// 走与 `ArrayLiteral` 相同的数组构造路径。
            MirInstructionKind::FixedArrayNew { items, storage, element_type, .. } if *storage == StorageKind::Reference => {
                self.emit_jvm_array_literal(instruction.output, element_type, items);
            }
            MirInstructionKind::TupleNew { fields, layout_id, .. } | MirInstructionKind::FixedArrayNew { items: fields, layout_id, .. } => {
                let mut local = instruction.output.and_then(|v| self.slots.value_locals.get(&v).copied()).expect("aggregate local");
                let context = match &instruction.kind {
                    MirInstructionKind::TupleNew { .. } => "TupleNew",
                    MirInstructionKind::FixedArrayNew { .. } => "FixedArrayNew",
                    _ => "aggregate new",
                };
                let layout_id = Some(ExecutableLoweringContext::require_layout_id(*layout_id, context));
                for (index, value) in fields.iter().enumerate() {
                    let field_name = index.to_string();
                    let field_offset = self.ctx.field_slot_index(layout_id, "", &field_name);
                    let field_local = local + field_offset;
                    let field_ty = match &instruction.kind {
                        MirInstructionKind::TupleNew { element_types, .. } => element_types.get(index).cloned().unwrap_or(NyarType::Unit),
                        MirInstructionKind::FixedArrayNew { element_type, .. } => element_type.clone(),
                        _ => NyarType::Unit,
                    };
                    let unboxed_local = match value {
                        MirOperand::Value(value_ref) if self.boxed_value_refs.contains(value_ref) => self
                            .slots
                            .value_locals
                            .get(value_ref)
                            .copied()
                            .and_then(|source_local| self.emit_unbox_value_type_to_locals(source_local, &field_ty, field_local)),
                        _ => None,
                    };
                    if let Some(actual_local) = unboxed_local {
                        local = actual_local.saturating_sub(field_offset);
                    }
                    else {
                        self.emit_operand(value);
                        let effective_field_ty = effective_jvm_type(&self.ctx, &field_ty);
                        let actual_local = self.emit_store_local(field_local, &effective_field_ty);
                        local = actual_local.saturating_sub(field_offset);
                    }
                }
                if let Some(output) = instruction.output {
                    self.slots.value_locals.insert(output, local);
                }
            }
            MirInstructionKind::AggregateCopy { source, dest, layout_id } => {
                // Kind-conflict cleanup may drop SSA→local maps; fall back to
                // emit_operand rather than panicking mid-bootstrap.
                let source_local = self.operand_local(source);
                let dest_local = self.operand_local(dest);
                if let (Some(source_local), Some(dest_local)) = (source_local, dest_local) {
                    // boxed 值类型（多字段值类型装箱为堆对象引用）必须用
                    // aload/astore 复制引用，而非字段级 iload/istore。否则会对
                    // 对象引用 local 发射 iload，触发 VerifyError:
                    // "Register N contains wrong type"。
                    let source_is_boxed = match source {
                        MirOperand::Value(v) => self.boxed_value_refs.contains(v),
                        _ => false,
                    };
                    if source_is_boxed {
                        let ty = match source {
                            MirOperand::Value(v) => self.lookup_value_type(v),
                            _ => NyarType::Unit,
                        };
                        self.emit_load_local(source_local, &ty);
                        let actual = self.emit_store_local(dest_local, &ty);
                        if let MirOperand::Value(dest_val) = dest {
                            if actual != dest_local {
                                self.slots.value_locals.insert(*dest_val, actual);
                            }
                            if let MirOperand::Value(v) = source {
                                if self.boxed_value_refs.contains(v) {
                                    self.boxed_value_refs.insert(*dest_val);
                                }
                            }
                        }
                    }
                    else {
                        let layout = self.ctx.layout_by_id(*layout_id);
                        let layout_name = layout.map(|item| item.name.clone());
                        let fields: Vec<_> = layout.map(|item| item.fields.clone()).unwrap_or_default();
                        drop(layout);
                        let dest_ty = match dest {
                            MirOperand::Value(v) => self.lookup_value_type(v),
                            _ => layout_name.as_ref().map(|n| NyarType::Named(nyar::Identifier::new(n))).unwrap_or(NyarType::Unit),
                        };
                        // 目标是 needs_boxing 的单槽堆对象：从扁平源装箱，禁止
                        // copy_value_type_fields 写入相邻变量槽。
                        if needs_boxing(&self.ctx, &dest_ty) {
                            let boxed = self.emit_box_value_type_to_stack(&dest_ty, source_local);
                            if boxed {
                                let actual = self.emit_store_local(dest_local, &dest_ty);
                                if let MirOperand::Value(dest_val) = dest {
                                    if actual != dest_local {
                                        self.slots.value_locals.insert(*dest_val, actual);
                                    }
                                    self.boxed_value_refs.insert(*dest_val);
                                    self.value_type_overrides.insert(*dest_val, dest_ty);
                                }
                                return;
                            }
                        }
                        if fields.is_empty() {
                            self.emit_load_local(source_local, &NyarType::Unit);
                            let actual = self.emit_store_local(dest_local, &NyarType::Unit);
                            if actual != dest_local {
                                if let MirOperand::Value(dest_val) = dest {
                                    self.slots.value_locals.insert(*dest_val, actual);
                                }
                            }
                        }
                        else {
                            // AggregateCopy must preserve every recursively flattened leaf.
                            // Copying only direct fields loses nested value-type leaves (for
                            // example `PeStringU16Map.values: [u16]`) and shifts call ABI slots.
                            let layout_name = layout_name.expect("non-empty aggregate layout");
                            let actual_local = self.copy_value_type_fields(source_local, dest_local, &layout_name);
                            if let MirOperand::Value(dest_val) = dest {
                                self.slots.value_locals.insert(*dest_val, actual_local);
                            }
                        }
                    }
                }
                else {
                    self.emit_operand(source);
                    if let MirOperand::Value(dest_val) = dest {
                        let ty = match source {
                            MirOperand::Value(v) => {
                                let raw = self.lookup_value_type(v);
                                if self.boxed_value_refs.contains(v) { raw } else { effective_jvm_type(&self.ctx, &raw) }
                            }
                            _ => effective_jvm_type(&self.ctx, &self.infer_field_type(source)),
                        };
                        if let Some(planned) = self.slots.value_locals.get(dest_val).copied() {
                            let actual = self.emit_store_local(planned, &ty);
                            if actual != planned {
                                self.slots.value_locals.insert(*dest_val, actual);
                            }
                        }
                        else {
                            self.store_to_value(*dest_val);
                        }
                        self.value_type_overrides.insert(*dest_val, ty);
                        if let MirOperand::Value(v) = source {
                            if self.boxed_value_refs.contains(v) {
                                self.boxed_value_refs.insert(*dest_val);
                            }
                        }
                    }
                    else {
                        self.instructions.push(JvmInstruction::Pop);
                    }
                }
            }
            MirInstructionKind::FieldGet { object, field, storage, layout_id } => {
                // 内联字段路径仅适用于：未被装箱 + 已注册为值类型 + slot plan
                // 已展开字段到连续 local 的对象。boxed 值或未注册为值类型的
                // Named 对象必须走 aload + getfield 路径，否则会从对象引用的
                // local 发射 iload（boxed）或访问超出 max_locals 的字段偏移
                // （未注册值类型），触发 VerifyError。
                let use_inline_fields = self.uses_inline_value_fields(object);
                let mut output_stored = false;
                // enums / unite 在 JVM ABI 上是 int 句柄：句柄本身就是 tag。
                // MIR match 仍会发射 `FieldGet { field: "tag" }`；若走 `getfield`，
                // 会在 int 上要对象引用，触发
                // VerifyError: "Expecting to find object/array on stack"
                // （如 `wit_wasi_package_version(I)Ljava/lang/String;`）。
                if field == "tag" && self.is_sum_type_operand(object) {
                    self.emit_operand(object);
                    if let Some(output) = instruction.output {
                        self.value_type_overrides.insert(output, NyarType::Integer32 { signed: true });
                        if let Some(local) = self.slots.value_locals.get(&output).copied() {
                            let actual = self.emit_store_local(local, &NyarType::Integer32 { signed: true });
                            if actual != local {
                                self.slots.value_locals.insert(output, actual);
                            }
                        }
                    }
                    output_stored = true;
                }
                // JVM 把语言 `utf8`/`Utf8Text` 与 `utf16`/`Utf16Text` 擦成
                // `java.lang.String`，没有可 `getfield` 的 `_repr` 槽。
                // 硬约束：utf8 ≠ utf16 — `_repr` 必须分别是 UTF-8 字节 / UTF-16
                // code units，禁止恒等成 String（否则 `ch._repr[0]` 会把宿主
                // String 当成字节数组，或落到错误的 Utf16Text getfield）。
                else if field == "_repr" && self.is_host_utf8_text_operand(object) {
                    self.emit_host_utf8_repr(object, instruction.output);
                    output_stored = true;
                }
                else if field == "_repr" && self.is_host_utf16_text_operand(object) {
                    self.emit_host_utf16_repr(object, instruction.output);
                    output_stored = true;
                }
                // unite sum type（如 `VonParseResult`/`Option`/`Result`）在 JVM 后端
                // 用 int 句柄表示，HIR 的 `lower_extractor_call_operand` 对其发射
                // `FieldGet { field: "payload" }`。但 int 句柄不能用 `getfield`，
                // 必须改走 `tuple_get_0` runtime stub 提取 payload。
                // 此检测必须在内联字段路径和 getfield 路径之前执行，避免对 int
                // 句柄发射 `iload` + `getfield` 触发
                // VerifyError: "Expecting to find object/array on stack"。
                else if field == "payload" && self.is_unite_payload_field_get(object) {
                    let payload_ty =
                        instruction.output.and_then(|output| self.mir_fn.value_types.get(&output).cloned()).unwrap_or(NyarType::Unit);
                    self.emit_operand(object);
                    // Unite Fine payload（如 `VonValue`）在 JVM ABI 中仍是 int 句柄：
                    // `tuple_get_0` 必须返回 `I`，调用方也必须 `istore`/`iload`。
                    // 若返回 `LVonValue;` 再 `astore`，随后传给仍期望 `I` 的
                    // `project_manifest_from_von(VonValue,…)` 会 `aload`→`istore`，
                    // 触发 VerifyError: "Expecting to find integer on stack"。
                    let payload_is_unite = self.is_unite_sum_type(&payload_ty);
                    let return_desc = if payload_is_unite { JvmTypeDescriptor::Int } else { jvm_type_descriptor(&payload_ty) };
                    let descriptor = JvmMethodDescriptor::new(vec![JvmTypeDescriptor::Int], return_desc);
                    let owner = self.current_class_owner();
                    self.instructions.push(JvmInstruction::InvokeStatic(JvmMethodRef { owner, name: "tuple_get_0".to_string(), descriptor }));
                    if let Some(output) = instruction.output {
                        // 非 unite 的 Named/字符串/数组 payload：`tuple_get_0` 返回
                        // `L<Type>;` 堆引用。即使是单字段值类型（如
                        // `LegionWorkspaceManifest`），也必须标记 boxed，否则
                        // `uses_inline_value_fields` 会走内联路径并跳过 `getfield`，
                        // 触发 VerifyError: "Expecting to find array on stack"。
                        let needs_box = !payload_is_unite
                            && (matches!(&payload_ty, NyarType::Named(name) if self.ctx.is_value_type_name(name.as_str()))
                                || matches!(
                                    &payload_ty,
                                    NyarType::Named(_) | NyarType::Utf8 | NyarType::Utf16 | NyarType::Array(_) | NyarType::FixedArray { .. }
                                ));
                        self.value_type_overrides.insert(output, payload_ty.clone());
                        if needs_box {
                            self.boxed_value_refs.insert(output);
                        }
                        if let Some(local) = self.slots.value_locals.get(&output).copied() {
                            let store_ty = if needs_box { payload_ty.clone() } else { effective_jvm_type(&self.ctx, &payload_ty) };
                            let actual = self.emit_store_local(local, &store_ty);
                            if actual != local {
                                self.slots.value_locals.insert(output, actual);
                            }
                        }
                    }
                    output_stored = true;
                }
                else if *storage == StorageKind::Value && use_inline_fields {
                    let object_local = self.operand_local(object).expect("object");
                    let layout_id = Some(ExecutableLoweringContext::require_layout_id(*layout_id, "FieldGet"));
                    let field_local = object_local + self.ctx.field_slot_index(layout_id, "", field);
                    let field_ty = self.ctx.field_type(layout_id, "", field).unwrap_or(NyarType::Unit);
                    // 当字段本身是多字段值类型时，必须将所有字段从源 local
                    // 复制到 output local，而非仅加载首个字段。slot plan 按
                    // output 类型为值类型分配了多个连续 local，若仅写入首字段，
                    // 后续 FieldGet 读取未初始化的字段 local 会触发
                    // VerifyError: "Accessing value from uninitialized register N"。
                    // 必须同时验证 output 在 mir_fn.value_types 中也注册为值类型，
                    // 否则 slot plan 仅分配 1 个 local，copy_value_type_fields
                    // 会写入超出 max_locals 的寄存器，触发
                    // VerifyError: "Illegal local variable number"。
                    if let Some(output) = instruction.output {
                        if let NyarType::Named(field_type_name) = &field_ty {
                            if self.ctx.is_value_type_name(field_type_name.as_str()) && !self.boxed_value_refs.contains(&output) {
                                let output_ty = self.mir_fn.value_types.get(&output).cloned().unwrap_or(NyarType::Unit);
                                let output_is_value_type =
                                    matches!(&output_ty, NyarType::Named(out_name) if self.ctx.is_value_type_name(out_name.as_str()));
                                if output_is_value_type {
                                    if let Some(dest_local) = self.slots.value_locals.get(&output).copied() {
                                        let actual_local = self.copy_value_type_fields(field_local, dest_local, field_type_name.as_str());
                                        self.slots.value_locals.insert(output, actual_local);
                                        output_stored = true;
                                    }
                                }
                            }
                        }
                    }
                    if !output_stored {
                        let effective_field_ty = effective_jvm_type(&self.ctx, &field_ty);
                        self.emit_load_local(field_local, &effective_field_ty);
                    }
                }
                else {
                    self.emit_operand(object);
                    let owner = self
                        .infer_aggregate_name(object)
                        .or_else(|| self.ctx.find_type_name_by_field(field))
                        .unwrap_or_else(|| "java/lang/Object".to_string());
                    let field_ty =
                        self.ctx.field_type(*layout_id, &owner, field).or_else(|| self.ctx.field_type(None, &owner, field)).unwrap_or_else(
                            || instruction.output.and_then(|output| self.mir_fn.value_types.get(&output).cloned()).unwrap_or(NyarType::Unit),
                        );
                    self.instructions.push(JvmInstruction::GetField(JvmFieldRef {
                        owner: owner.replace('.', "/"),
                        name: field.clone(),
                        descriptor: self.jvm_field_descriptor_for_class(&field_ty),
                    }));
                }
                if !output_stored {
                    if let Some(output) = instruction.output {
                        let owner = self
                            .infer_aggregate_name(object)
                            .or_else(|| self.ctx.find_type_name_by_field(field))
                            .unwrap_or_else(|| "java/lang/Object".to_string());
                        if let Some(field_ty) =
                            self.ctx.field_type(*layout_id, &owner, field).or_else(|| self.ctx.field_type(None, &owner, field))
                        {
                            // 仅当类文件字段确实是对象引用（`L…;`）时才标记 boxed。
                            // `usize`/`i32` 等原始字段描述符为 `I`，getfield 压 int；
                            // 若仍按值类型 boxed → astore，会触发
                            // VerifyError: "Expecting to find object/array on stack"。
                            if let NyarType::Named(name) = &field_ty {
                                if self.ctx.is_value_type_name(name.as_str()) && map_primitive_name_to_jvm(name.as_str()).is_none() {
                                    let desc = self.jvm_field_descriptor_for_class(&field_ty);
                                    if matches!(desc, JvmTypeDescriptor::Object(_) | JvmTypeDescriptor::Array(_)) {
                                        self.boxed_value_refs.insert(output);
                                    }
                                }
                            }
                            self.value_type_overrides.insert(output, field_ty);
                        }
                        self.store_to_value(output);
                    }
                }
            }
            MirInstructionKind::FieldSet { object, field, value, storage, layout_id } => {
                let use_inline_fields = self.uses_inline_value_fields(object);
                if *storage == StorageKind::Value && use_inline_fields {
                    let object_local = self.operand_local(object).expect("object");
                    let layout_id = Some(ExecutableLoweringContext::require_layout_id(*layout_id, "FieldSet"));
                    self.emit_operand(value);
                    let field_local = object_local + self.ctx.field_slot_index(layout_id, "", field);
                    let field_ty = self.ctx.field_type(layout_id, "", field).unwrap_or_else(|| self.infer_field_type(value));
                    let effective_field_ty = effective_jvm_type(&self.ctx, &field_ty);
                    self.emit_store_local(field_local, &effective_field_ty);
                }
                else {
                    self.emit_operand(object);
                    let owner = self.infer_aggregate_name(object).unwrap_or_else(|| "java/lang/Object".to_string());
                    let field_ty = self.ctx.field_type(*layout_id, &owner, field).unwrap_or_else(|| self.infer_field_type(value));
                    // 对 Named 值类型字段，先装箱为堆对象引用再 putfield。
                    // 已装箱的引用（如 `meta.user_strings = add_us.heap` 中 getfield
                    // 产物）只占 1 个 local 槽持有对象引用，禁止再按扁平槽位
                    // base..base+n 装箱——那会把对象引用 putfield 进首字段（如
                    // `[I data`）并读取未初始化的相邻槽位，触发
                    // VerifyError: "Bad type in putfield/putstatic"。
                    // 走 emit_operand 直接 aload 引用即可。
                    let field_boxed = if let MirOperand::Value(vref) = value {
                        if self.boxed_value_refs.contains(vref) {
                            false
                        }
                        else {
                            self.slots
                                .value_locals
                                .get(vref)
                                .copied()
                                .map(|base| self.emit_box_value_type_to_stack(&field_ty, base))
                                .unwrap_or(false)
                        }
                    }
                    else {
                        false
                    };
                    if !field_boxed {
                        self.emit_operand(value);
                    }
                    self.emit_field_store_checkcast(&field_ty);
                    self.instructions.push(JvmInstruction::PutField(JvmFieldRef {
                        owner: owner.replace('.', "/"),
                        name: field.clone(),
                        descriptor: self.jvm_field_descriptor_for_class(&field_ty),
                    }));
                }
            }
            MirInstructionKind::Call { callee, arguments, dispatch, witness, receiver_kind, intrinsic_opcode, .. } => {
                let registry_opcode = match callee {
                    MirOperand::Symbol(path) => self
                        .ctx
                        .submission
                        .intrinsics
                        .get(&path.to_string())
                        .or_else(|| path.parts().last().and_then(|name| self.ctx.submission.intrinsics.get(name.as_str())))
                        .copied(),
                    _ => None,
                };
                if let Some(opcode) = intrinsic_opcode.or(registry_opcode) {
                    self.emit_intrinsic_opcode(opcode, arguments, instruction.output);
                    return;
                }
                // MIR 前端对 class（非值类型）字段访问会插入 `deref(handle)`。
                // CLR 用整型句柄；JVM 上 class 已是对象引用，deref 必须是恒等，
                // 否则会生成 `deref(LType;)I` + `iload` + `getfield` 触发 VerifyError。
                if let MirOperand::Symbol(path) = callee {
                    if path.parts().last().is_some_and(|part| part.as_str() == "deref") && arguments.len() == 1 {
                        self.emit_jvm_deref_identity(&arguments[0], instruction.output);
                        return;
                    }
                }
                // 内联 `is_null` / `unwrap_null` runtime stub（与 CLR `emit_clr_is_null` 对齐）。
                // JVM stub 默认返回 0（`iconst_0; ireturn`），无法正确检测 null。
                // nullable 引用类型用 `IfNull` 判空，nullable 原始类型暂返回 false
                // （sentinel 方案待实现），`unwrap_null` 为恒等传递（payload 即值本身）。
                if let MirOperand::Symbol(path) = callee {
                    let parts: Vec<&str> = path.parts().iter().map(|part| part.as_str()).collect();
                    if is_injected_runtime_stub_symbol(&parts) && parts[0] == "is_null" && arguments.len() == 1 {
                        self.emit_jvm_is_null(&arguments[0], instruction.output);
                        return;
                    }
                    if is_injected_runtime_stub_symbol(&parts) && parts[0] == "unwrap_null" && arguments.len() == 1 {
                        self.emit_jvm_unwrap_null(&arguments[0], instruction.output);
                        return;
                    }
                }
                if let MirOperand::Symbol(path) = callee {
                    let host_print = self.find_jvm_host_print_link(path).and_then(jvm_host_print_target).map(|target| {
                        let (field_name, method_name, stream_owner) = if let Some(dot_pos) = target.method_name.rfind('.') {
                            (
                                target.method_name[..dot_pos].to_string(),
                                target.method_name[dot_pos + 1..].to_string(),
                                "java/io/PrintStream".to_string(),
                            )
                        }
                        else {
                            (target.field_name.to_string(), target.method_name.to_string(), target.stream_owner.replace('.', "/"))
                        };
                        (target.field_owner.replace('.', "/"), field_name, stream_owner, method_name)
                    });
                    if let Some((field_owner, field_name, stream_owner, method_name)) = host_print {
                        self.emit_host_print_call(field_owner, field_name, stream_owner, method_name, arguments, receiver_kind);
                        return;
                    }
                }

                let has_by_address_receiver = matches!(receiver_kind, Some(ReceiverPassingKind::ByAddress));
                let arg_start = if has_by_address_receiver { 1 } else { 0 };

                // Runtime stub Calls: only bare injected symbols (`print`/`panic`/…).
                // Stub descriptors take a single Object/Int arg, not expanded value-type
                // fields — use `emit_operand` instead of `emit_call_argument`.
                let is_stub_call = if let MirOperand::Symbol(path) = callee {
                    let parts: Vec<&str> = path.parts().iter().map(|part| part.as_str()).collect();
                    is_injected_runtime_stub_symbol(&parts)
                }
                else {
                    false
                };

                // Resolve callee ABI **before** pushing args so reference params
                // (utf8/`String`, class, arrays) are not fed `iconst_0` from a
                // Unit/int-typed SSA — that triggers
                // VerifyError: Expecting object/array on stack at invokestatic.
                let callee_expected_params: Option<Vec<JvmTypeDescriptor>> = match callee {
                    MirOperand::Symbol(path) if !is_stub_call => self.resolve_callee_jvm_descriptor(path).map(|desc| desc.parameter_types),
                    _ => None,
                };
                let mut expected_param_index = 0usize;

                if has_by_address_receiver {
                    if let Some(receiver) = arguments.first() {
                        // JVM 没有 ldloca 等价物：局部变量表按值传递，无法取地址。
                        // ByAddress 在 JVM 后端退化为按值传递值类型的所有字段，
                        // 与被调用方展开后的方法描述符匹配。
                        if is_stub_call {
                            self.emit_operand(receiver);
                        }
                        else {
                            let expected = callee_expected_params.as_ref().map(|p| p.as_slice());
                            expected_param_index += self.emit_call_argument(receiver, expected, expected_param_index);
                        }
                    }
                }

                for argument in arguments.iter().skip(arg_start) {
                    if is_stub_call {
                        self.emit_operand(argument);
                    }
                    else {
                        let expected = callee_expected_params.as_ref().map(|p| p.as_slice());
                        expected_param_index += self.emit_call_argument(argument, expected, expected_param_index);
                    }
                }

                if let MirOperand::Symbol(path) = callee {
                    if let Some((method_ref, is_static, is_void)) = self.resolve_singleton_call(path, arguments) {
                        self.instructions.push(if is_static {
                            JvmInstruction::InvokeStatic(method_ref)
                        }
                        else {
                            JvmInstruction::InvokeVirtual(method_ref)
                        });
                        if let Some(output) = instruction.output {
                            if !is_void {
                                self.store_to_value(output);
                            }
                        }
                        return;
                    }
                    self.emit_call_invoke(path, *dispatch, witness.as_ref(), arguments, instruction.output);
                }
            }
            // pattern 无法 lowering：extractor 未 resolved 或类型推断失败，
            // 运行期 trap——调用永不返回的 helper 抛出异常。
            MirInstructionKind::PatternMatch { .. } => {
                self.instructions.push(JvmInstruction::InvokeStatic(JvmMethodRef {
                    owner: "nyar/runtime".to_string(),
                    name: "nyar_pattern_match_unreachable".to_string(),
                    descriptor: JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Void),
                }));
            }
            _ => {}
        }
    }

    /// 为 `ArrayLiteral` 和 `FixedArrayNew`（Reference 语义）生成 JVM 数组构造字节码。
    ///
    /// 生成序列：`iconst <n>` → `anewarray <type>`（或 `newarray int`）→
    /// 逐元素 `dup` / `iconst <i>` / `<element>` / `aastore`（或 `iastore`）→
    /// `astore <local>`。同时在 `value_type_overrides` 注册 `Array(element_type)`，
    /// 确保后续 `emit_operand` 使用 `ALoad` 而非 `ILoad` 加载数组引用。
    fn emit_jvm_array_literal(&mut self, output: Option<MirValueRef>, element_type: &NyarType, items: &[MirOperand]) {
        let Some(output) = output
        else {
            return;
        };
        let Some(&local) = self.slots.value_locals.get(&output)
        else {
            return;
        };
        // 使用 `effective_array_element_type` 仅折叠 unite sum type 元素为 Integer32，
        // 值类型保持 Named（数组元素是堆对象引用，不能内联展开）。
        let mut effective_element = effective_array_element_type(&self.ctx, element_type);
        // Some erased/generic array literals arrive with an Integer32
        // fallback element type even though every operand is a reference
        // (notably `[utf8]` payloads passed through `Fine`).  The operand
        // category is authoritative here: emitting `newarray int` plus
        // `iastore` for String values produces an invalid JVM method.
        if jvm_primitive_newarray(&effective_element).is_some() && !items.is_empty() {
            let reference_types: Vec<NyarType> =
                items.iter().map(|item| effective_array_element_type(&self.ctx, &self.infer_field_type(item))).collect();
            if reference_types.iter().all(|ty| jvm_primitive_newarray(ty).is_none()) {
                if let Some(first) = reference_types.first() {
                    effective_element = first.clone();
                }
            }
        }
        let element_desc = jvm_type_descriptor(&effective_element);
        let new_array = jvm_primitive_newarray(&effective_element);
        let store = jvm_primitive_array_store(&effective_element);
        self.instructions.push(JvmInstruction::IConst(items.len() as i32));
        if let Some(new_array) = new_array {
            self.instructions.push(new_array);
        }
        else {
            let class_name = match &element_desc {
                JvmTypeDescriptor::Object(name) => name.clone(),
                _ => "java/lang/Object".to_string(),
            };
            self.instructions.push(JvmInstruction::ANewArray(class_name));
        }
        for (index, item) in items.iter().enumerate() {
            self.instructions.push(JvmInstruction::Dup);
            self.instructions.push(JvmInstruction::IConst(index as i32));
            self.emit_operand(item);
            self.instructions.push(store.clone());
        }
        let array_ty = NyarType::Array(Box::new(effective_element.clone()));
        let actual = self.emit_store_local(local, &array_ty);
        if actual != local {
            self.slots.value_locals.insert(output, actual);
        }
        self.value_type_overrides.insert(output, array_ty);
    }

    /// 为 `ArrayNew` 生成 JVM 数组构造字节码。
    ///
    /// 生成序列：`<length>` → `anewarray <type>`（或 `newarray <atype>`）→
    /// `astore <local>`。数组元素使用默认值（`null` / `0`），由调用方后续填充。
    fn emit_jvm_array_new(&mut self, output: Option<MirValueRef>, element_type: &NyarType, length: &MirOperand) {
        let Some(output) = output
        else {
            return;
        };
        let Some(&local) = self.slots.value_locals.get(&output)
        else {
            return;
        };
        // 与 `emit_jvm_array_literal` 对齐：仅 unite sum type 元素折叠为 Integer32，
        // 值类型保持 Named 以生成 `[LType;` 描述符。
        let effective_element = effective_array_element_type(&self.ctx, element_type);
        let element_desc = jvm_type_descriptor(&effective_element);
        let new_array = jvm_primitive_newarray(&effective_element);
        self.emit_operand(length);
        if let Some(new_array) = new_array {
            self.instructions.push(new_array);
        }
        else {
            let class_name = match &element_desc {
                JvmTypeDescriptor::Object(name) => name.clone(),
                _ => "java/lang/Object".to_string(),
            };
            self.instructions.push(JvmInstruction::ANewArray(class_name));
        }
        let array_ty = NyarType::Array(Box::new(effective_element.clone()));
        let actual = self.emit_store_local(local, &array_ty);
        if actual != local {
            self.slots.value_locals.insert(output, actual);
        }
        self.value_type_overrides.insert(output, array_ty);
    }

    /// 将值类型从 local 槽位装箱为堆对象引用并压入操作数栈。
    ///
    /// 当结构体字段类型为 Named 值类型时，JVM 类字段声明为对象引用
    /// （`LType;`），但值类型在 local 中以展开形式存储（各字段独立 slot）。
    /// 直接 `iload` 压入 int 会导致 `putfield` 期望引用但栈上是 int，
    /// 触发 VerifyError。此函数递归构造堆对象：`new` + `<init>` +
    /// 逐字段 `putfield`，最终在栈上留下一个对象引用。
    ///
    /// 仅对 [`needs_boxing`] 为真的多字段值类型装箱。`usize`/`i32` 等
    /// `core.primitive` 名虽在 layout 表中（常为 0 字段），字段描述符仍是
    /// `I`；若误 `new usize` 再 `putfield …:I`，会触发
    /// VerifyError: "Expecting to find integer on stack"（如
    /// `new_von_diagnostic` → `TextSpan.offset`）。
    ///
    /// 返回 `true` 表示已装箱（引用在栈上），`false` 表示类型非值类型
    /// （调用方应走常规加载路径）。
    fn emit_box_value_type_to_stack(&mut self, field_ty: &NyarType, base_local: u16) -> bool {
        if !needs_boxing(&self.ctx, field_ty) {
            return false;
        }
        let NyarType::Named(name) = field_ty
        else {
            return false;
        };
        let Some(layout) = self.ctx.layout_by_type_name(name.as_str())
        else {
            return false;
        };
        let jvm_class = name.as_str().replace('.', "/");
        let fields = layout.fields.clone();
        self.instructions.push(JvmInstruction::New(jvm_class.clone()));
        self.instructions.push(JvmInstruction::Dup);
        self.instructions.push(JvmInstruction::InvokeSpecial(JvmMethodRef {
            owner: jvm_class.clone(),
            name: "<init>".to_string(),
            descriptor: JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Void),
        }));
        for nested_field in &fields {
            self.instructions.push(JvmInstruction::Dup);
            let offset = self.ctx.field_slot_index(None, name.as_str(), &nested_field.name);
            let nested_boxed = self.emit_box_value_type_to_stack(&nested_field.ty, base_local + offset);
            if !nested_boxed {
                let effective_field_ty = effective_jvm_type(&self.ctx, &nested_field.ty);
                self.emit_load_local(base_local + offset, &effective_field_ty);
            }
            self.emit_field_store_checkcast(&nested_field.ty);
            self.instructions.push(JvmInstruction::PutField(JvmFieldRef {
                owner: jvm_class.clone(),
                name: nested_field.name.clone(),
                descriptor: self.jvm_field_descriptor_for_class(&nested_field.ty),
            }));
        }
        true
    }

    /// 装箱多字段值类型并 `areturn`。
    ///
    /// 从返回值的 base local 读取各字段，`new <class>` + 逐字段 `dup` +
    /// `<load field>` + `putfield` 构造堆对象，最后 `areturn`。
    /// 若无法解析 base local（如返回常量），退回到 `aconst_null`。
    ///
    /// 对值类型字段（如 `LegionBuildTargetOptions`），先递归装箱为堆对象
    /// 再 `putfield`，因为 JVM 类字段声明为对象引用（`LType;`），
    /// 直接用 `iload` 压入 int 会导致 `putfield` 期望引用但栈上是 int，
    /// 触发 VerifyError: "Expecting to find object/array on stack"。
    fn emit_boxed_return(&mut self, value: Option<&MirOperand>) {
        let return_ty = concretize_self_type(&self.mir_fn.return_type, self.self_owner.as_deref());
        let type_name = match &return_ty {
            NyarType::Named(name) => name.as_str(),
            _ => return,
        };
        let jvm_class = type_name.replace('.', "/");
        let Some(layout) = self.ctx.layout_by_type_name(type_name)
        else {
            return;
        };
        let fields = layout.fields.clone();
        let base_local = match value {
            Some(MirOperand::Value(vref)) => self.slots.value_locals.get(vref).copied(),
            _ => None,
        };
        // 如果返回值已经是装箱的堆对象引用（来自 Call 返回多字段值类型，
        // 或 StructNew Reference 路径），直接 aload + areturn。此时 local
        // 只有一个槽位持有引用，不能按展开字段读取 base+offset，否则会
        // 访问无关 local，触发 VerifyError: "Register N contains wrong type"。
        if let Some(MirOperand::Value(vref)) = value {
            if self.boxed_value_refs.contains(vref) {
                if let Some(local) = base_local {
                    self.instructions.push(JvmInstruction::ALoad(local));
                    self.instructions.push(JvmInstruction::AReturn);
                    return;
                }
            }
        }
        self.instructions.push(JvmInstruction::New(jvm_class.clone()));
        // `new` 之后必须调用无参 `<init>()V` 将未初始化引用转为已初始化对象。
        // 占位类由 `build_jvm_placeholder_class` 注入默认无参 `<init>`（转发到
        // `Object.<init>`）。若省略此步，后续 `putfield` 虽能写入字段，但
        // `areturn` 返回的是未初始化引用，JVM 验证器拒绝并报
        // VerifyError: "Expecting to find object/array on stack"。
        // `dup` 复制一份引用供 `invokespecial` 消费，留另一份已初始化引用
        // 供后续 `putfield`/`areturn` 使用。
        self.instructions.push(JvmInstruction::Dup);
        self.instructions.push(JvmInstruction::InvokeSpecial(JvmMethodRef {
            owner: jvm_class.clone(),
            name: "<init>".to_string(),
            descriptor: JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Void),
        }));
        for field in &fields {
            self.instructions.push(JvmInstruction::Dup);
            if let Some(base) = base_local {
                let offset = self.ctx.field_slot_index(None, type_name, &field.name);
                // 对 Named 值类型字段，递归装箱为堆对象引用再 putfield。
                // 值类型字段在 locals 中以展开形式存储（各字段独立 slot），
                // 但 JVM 类字段声明为对象引用（`LType;`），必须先 `new` +
                // 逐字段 `putfield` 构造堆对象，才能与 `putfield` 描述符匹配。
                let field_boxed = self.emit_box_value_type_to_stack(&field.ty, base + offset);
                if !field_boxed {
                    let effective_field_ty = effective_jvm_type(&self.ctx, &field.ty);
                    self.emit_load_local(base + offset, &effective_field_ty);
                }
            }
            else {
                let effective_field_ty = effective_jvm_type(&self.ctx, &field.ty);
                match effective_field_ty {
                    NyarType::Integer64 { .. } => self.instructions.push(JvmInstruction::LConst0),
                    NyarType::Float64 => self.instructions.push(JvmInstruction::DConst0),
                    NyarType::Utf8 | NyarType::Utf16 | NyarType::Named(_) | NyarType::Array(_) | NyarType::FixedArray { .. } => {
                        self.instructions.push(JvmInstruction::AConstNull)
                    }
                    _ => self.instructions.push(JvmInstruction::IConst(0)),
                }
            }
            self.emit_field_store_checkcast(&field.ty);
            self.instructions.push(JvmInstruction::PutField(JvmFieldRef {
                owner: jvm_class.clone(),
                name: field.name.clone(),
                descriptor: self.jvm_field_descriptor_for_class(&field.ty),
            }));
        }
        self.instructions.push(JvmInstruction::AReturn);
    }

    fn emit_terminator(&mut self, block: &MirBlock) {
        match &block.terminator {
            MirTerminator::Return { value } => {
                let return_ty = concretize_self_type(&self.mir_fn.return_type, self.self_owner.as_deref());
                if needs_boxing(&self.ctx, &return_ty) {
                    self.emit_boxed_return(value.as_ref());
                    return;
                }
                let effective_return_ty = effective_jvm_type(&self.ctx, &return_ty);
                match effective_return_ty {
                    NyarType::Integer64 { .. } => {
                        match value {
                            Some(MirOperand::Constant(MirConstant::Int(n))) => {
                                if *n == 0 {
                                    self.instructions.push(JvmInstruction::LConst0);
                                }
                                else if *n == 1 {
                                    self.instructions.push(JvmInstruction::LConst1);
                                }
                                else {
                                    self.instructions.push(JvmInstruction::LdcLong(*n));
                                }
                            }
                            Some(operand) => {
                                let needs_widen = match operand {
                                    MirOperand::Value(vref) => {
                                        // Intrinsics may refine the result type after MIR construction
                                        // (for example bitwise helpers returning a JVM long).  Use the
                                        // same override-aware lookup as operand emission; consulting
                                        // raw value_types here can emit IReturn-shaped values before
                                        // an LReturn and trigger VerifyError.
                                        let ty = self.lookup_value_type(vref);
                                        let eff = effective_jvm_type(&self.ctx, &ty);
                                        !matches!(eff, NyarType::Integer64 { .. })
                                    }
                                    MirOperand::Constant(MirConstant::Int(_)) => false,
                                    _ => true,
                                };
                                self.emit_operand(operand);
                                if needs_widen {
                                    self.instructions.push(JvmInstruction::I2L);
                                }
                            }
                            None => {
                                self.instructions.push(JvmInstruction::LConst0);
                            }
                        }
                        self.instructions.push(JvmInstruction::LReturn);
                    }
                    NyarType::Float64 => {
                        match value {
                            Some(MirOperand::Constant(MirConstant::Float64(_))) => {
                                self.emit_operand(value.as_ref().unwrap());
                            }
                            Some(operand) => {
                                let needs_convert = match operand {
                                    MirOperand::Value(vref) => {
                                        let ty = self.mir_fn.value_types.get(vref).cloned().unwrap_or(NyarType::Unit);
                                        let eff = effective_jvm_type(&self.ctx, &ty);
                                        !matches!(eff, NyarType::Float64)
                                    }
                                    _ => true,
                                };
                                self.emit_operand(operand);
                                if needs_convert {
                                    match operand {
                                        MirOperand::Value(vref) => {
                                            let ty = self.mir_fn.value_types.get(vref).cloned().unwrap_or(NyarType::Unit);
                                            let eff = effective_jvm_type(&self.ctx, &ty);
                                            if matches!(eff, NyarType::Integer64 { .. }) {
                                                self.instructions.push(JvmInstruction::L2D);
                                            }
                                            else {
                                                self.instructions.push(JvmInstruction::I2D);
                                            }
                                        }
                                        _ => {
                                            self.instructions.push(JvmInstruction::I2D);
                                        }
                                    }
                                }
                            }
                            None => {
                                self.instructions.push(JvmInstruction::DConst0);
                            }
                        }
                        self.instructions.push(JvmInstruction::DReturn);
                    }
                    NyarType::Bottom | NyarType::Unit => {
                        self.instructions.push(JvmInstruction::Return);
                    }
                    _ => {
                        let is_reference = is_jvm_stack_reference(&effective_return_ty);
                        if let Some(value) = value {
                            self.emit_operand(value);
                            // 当返回类型是具体引用类型（非 `java/lang/Object`）时，
                            // 在 `areturn` 前插入 `checkcast` 收窄栈上类型。
                            // stub 调用（如 `collect_array`）返回 `Object`，
                            // 但方法声明的返回类型可能是 `[LLegionBuildTarget;` 等具体类型，
                            // 验证器要求栈上类型与声明返回类型匹配。
                            // Sum/unite/`Result` 已折叠为 int：禁止 checkcast/areturn。
                            if is_reference {
                                let return_desc = jvm_type_descriptor(&effective_return_ty);
                                let needs_cast = !matches!(&return_desc, JvmTypeDescriptor::Object(name) if name == "java/lang/Object");
                                if needs_cast {
                                    let class_name = match &return_desc {
                                        JvmTypeDescriptor::Object(name) => name.clone(),
                                        _ => return_desc.to_string(),
                                    };
                                    self.instructions.push(JvmInstruction::CheckCast(class_name));
                                }
                            }
                        }
                        else if is_reference {
                            self.instructions.push(JvmInstruction::AConstNull);
                        }
                        else {
                            self.instructions.push(JvmInstruction::IConst(0));
                        }
                        self.instructions.push(if is_reference { JvmInstruction::AReturn } else { JvmInstruction::IReturn });
                    }
                }
            }
            MirTerminator::Jump { target, arguments } => {
                self.emit_block_argument_copies(*target, arguments);
                self.instructions.push(JvmInstruction::Goto(block_label(*target)));
            }
            MirTerminator::Branch { condition, then_target, else_target } => {
                self.emit_operand(condition);
                let cond_ty = effective_jvm_type(&self.ctx, &self.infer_field_type(condition));
                let else_label = block_label(*else_target);
                let then_label = block_label(*then_target);
                // 引用条件必须用 ifnull/ifnonnull；ifeq 期望 int，对对象会 VerifyError。
                // Sum/unite int 句柄走 ifeq。
                let is_reference = is_jvm_stack_reference(&cond_ty);
                if is_reference {
                    self.instructions.push(JvmInstruction::IfNull(else_label));
                }
                else {
                    self.instructions.push(JvmInstruction::IfEq(else_label));
                }
                self.instructions.push(JvmInstruction::Goto(then_label));
            }
            MirTerminator::StateDispatch { state, cases, default_target } => {
                let state_in_locals = self.slots.value_locals.get(state).copied();
                if let Some(state_local) = state_in_locals {
                    for (case_key, target) in cases {
                        self.emit_load_local(state_local, &NyarType::Integer32 { signed: true });
                        self.instructions.push(JvmInstruction::IConst(*case_key as i32));
                        self.instructions.push(JvmInstruction::ISub);
                        self.instructions.push(JvmInstruction::IfEq(block_label(*target)));
                    }
                }
                else {
                    for (case_key, target) in cases {
                        self.instructions.push(JvmInstruction::IConst(0));
                        self.instructions.push(JvmInstruction::IConst(*case_key as i32));
                        self.instructions.push(JvmInstruction::ISub);
                        self.instructions.push(JvmInstruction::IfEq(block_label(*target)));
                    }
                }
                self.instructions.push(JvmInstruction::Goto(block_label(*default_target)));
            }
            MirTerminator::PerformEffect { .. } | MirTerminator::YieldToRuntime { .. } | MirTerminator::Unreachable => {
                // 这些终止符在状态机改写后理论上不可达（StateDispatch 调度 + YieldToRuntime
                // 退出）。但 JVM 字节码验证要求每条路径以类型匹配的 return 指令结尾，
                // 故按函数返回类型发射默认值 + 匹配的 return 指令。
                let return_ty = concretize_self_type(&self.mir_fn.return_type, self.self_owner.as_deref());
                let effective_return_ty = effective_jvm_type(&self.ctx, &return_ty);
                match effective_return_ty {
                    NyarType::Integer64 { .. } => {
                        self.instructions.push(JvmInstruction::LConst0);
                        self.instructions.push(JvmInstruction::LReturn);
                    }
                    NyarType::Float64 => {
                        self.instructions.push(JvmInstruction::DConst0);
                        self.instructions.push(JvmInstruction::DReturn);
                    }
                    NyarType::Bottom | NyarType::Unit => {
                        self.instructions.push(JvmInstruction::Return);
                    }
                    _ => {
                        if is_jvm_stack_reference(&effective_return_ty) {
                            self.instructions.push(JvmInstruction::AConstNull);
                            self.instructions.push(JvmInstruction::AReturn);
                        }
                        else {
                            self.instructions.push(JvmInstruction::IConst(0));
                            self.instructions.push(JvmInstruction::IReturn);
                        }
                    }
                }
            }
        }
    }

    /// JVM 上 class 载荷已是对象引用：`deref(x)` 恒等复制引用，并用 `aload`/`astore` 保存。
    fn emit_jvm_deref_identity(&mut self, handle: &MirOperand, output: Option<MirValueRef>) {
        self.emit_operand(handle);
        let Some(output) = output
        else {
            self.instructions.push(JvmInstruction::Pop);
            return;
        };
        let ty = self.infer_field_type(handle);
        self.value_type_overrides.insert(output, ty.clone());
        // 强制后续 FieldGet / emit_operand 走引用路径，避免 Unit/Int 元数据把 astore 结果当成 iload。
        self.boxed_value_refs.insert(output);
        self.store_to_value(output);
    }

    /// 内联 `is_null` runtime stub：判断 nullable 值是否为 null。
    ///
    /// 与 CLR `emit_clr_is_null` 对齐。JVM 后端的 nullable 表示：
    /// - 引用类型（`Utf8`/`Named` 非值类型/`Array`/boxed 值类型）：用 null 引用
    ///   表示缺失，通过 `IfNull` 判空。
    /// - 原始类型（`i32`/`bool`/`usize` 等）：暂用 sentinel 值表示 null
    ///   （`nullable.rs` 已定义 `I64_NULL_SENTINEL`），但完整 sentinel 方案待实现，
    ///   当前返回 false（非 null）。
    ///
    /// 生成字节码模式（引用类型）：
    /// ```text
    /// <emit operand>
    /// ifnull is_null_label
    /// iconst_0          // 非 null → false
    /// goto done
    /// is_null_label:
    /// iconst_1          // null → true
    /// done:
    /// istore output
    /// ```
    fn emit_jvm_is_null(&mut self, argument: &MirOperand, output: Option<MirValueRef>) {
        // 对于 Symbol 操作数（变量名），infer_field_type 返回 Unit，
        // 需要从 var_types 获取实际类型来判断是否为引用。
        let ty = match argument {
            MirOperand::Symbol(path) => {
                let key = path.to_string();
                self.var_types.get(&key).cloned().unwrap_or_else(|| self.infer_field_type(argument))
            }
            _ => self.infer_field_type(argument),
        };
        let effective_ty = effective_jvm_type(&self.ctx, &ty);
        let desc = jvm_type_descriptor(&effective_ty);
        let is_reference = matches!(desc, JvmTypeDescriptor::Object(_) | JvmTypeDescriptor::Array(_));
        // boxed 值类型也是引用（堆对象），用 IfNull 判空。
        let is_boxed = matches!(argument, MirOperand::Value(v) if self.boxed_value_refs.contains(v));
        // nullable union 的 payload 为引用类型时，也用 IfNull 判空。
        let is_nullable_ref = is_nullable_union(&ty) && {
            let payload = nullable_union_payload_type(&ty).unwrap_or(NyarType::Unit);
            let payload_effective = effective_jvm_type(&self.ctx, &payload);
            let payload_desc = jvm_type_descriptor(&payload_effective);
            matches!(payload_desc, JvmTypeDescriptor::Object(_) | JvmTypeDescriptor::Array(_)) || needs_boxing(&self.ctx, &payload)
        };

        let Some(output) = output
        else {
            // 无 output：仍需消费栈上值（避免栈不平衡），引用类型 pop，原始类型 pop。
            self.emit_operand(argument);
            self.instructions.push(JvmInstruction::Pop);
            return;
        };

        if is_reference || is_boxed || is_nullable_ref {
            let is_null_label = self.next_label();
            let done_label = self.next_label();
            self.emit_operand(argument);
            self.instructions.push(JvmInstruction::IfNull(is_null_label.clone()));
            self.instructions.push(JvmInstruction::IConst(0));
            self.instructions.push(JvmInstruction::Goto(done_label.clone()));
            self.instructions.push(JvmInstruction::Label(is_null_label));
            self.instructions.push(JvmInstruction::IConst(1));
            self.instructions.push(JvmInstruction::Label(done_label));
            self.value_type_overrides.insert(output, NyarType::Boolean);
            self.store_to_value(output);
        }
        else {
            // 原始类型 nullable：sentinel 方案待实现，当前返回 false（非 null）。
            // 仍需消费 argument 的栈效果（如果 argument 是 Value 且有 local，emit_operand
            // 会发射 load；此处直接用 IConst(0) 跳过 argument 发射，因为原始类型
            // is_null 的结果固定为 false，不需要读取值）。
            self.instructions.push(JvmInstruction::IConst(0));
            self.value_type_overrides.insert(output, NyarType::Boolean);
            self.store_to_value(output);
        }
    }

    /// 内联 `unwrap_null` runtime stub：提取 nullable 值的 payload。
    ///
    /// `unwrap_null` 是恒等传递：nullable 值的 payload 就是值本身
    /// （引用类型的对象引用，或原始类型的值）。调用方应先通过 `is_null`
    /// 确认非 null 再调用 `unwrap_null`。
    ///
    /// 生成字节码模式：
    /// ```text
    /// <emit operand>
    /// store output
    /// ```
    fn emit_jvm_unwrap_null(&mut self, argument: &MirOperand, output: Option<MirValueRef>) {
        let Some(output) = output
        else {
            self.emit_operand(argument);
            self.instructions.push(JvmInstruction::Pop);
            return;
        };
        let ty = self.infer_field_type(argument);
        // 对于 nullable union（`Union([T, null])`），payload 类型为 `T`。
        // unwrap_null 是恒等传递：引用类型的对象引用，或原始类型的值。
        // 但 payload 的语义类型需要正确注册，使后续 FieldGet/emit_operand
        // 能正确判断是否为值类型/引用类型。
        let payload_ty = if is_nullable_union(&ty) { nullable_union_payload_type(&ty).unwrap_or(ty.clone()) } else { ty.clone() };
        let is_boxed = matches!(argument, MirOperand::Value(v) if self.boxed_value_refs.contains(v));
        // 注册 payload 语义类型（非 nullable 载荷类型），使后续 FieldGet
        // 等操作能正确解析字段 layout。
        self.value_type_overrides.insert(output, payload_ty.clone());
        if is_boxed {
            self.boxed_value_refs.insert(output);
        }
        // 如果 payload 是多字段值类型，确保标记为 boxed（nullable 装箱场景）。
        // nullable union 的 payload 在 effective_jvm_type 中已决定装箱为堆对象引用，
        // unwrap_null 传递该引用，output 必须标记为 boxed。
        if needs_boxing(&self.ctx, &payload_ty) {
            self.boxed_value_refs.insert(output);
        }
        self.emit_operand(argument);
        self.store_to_value(output);
    }

    /// 发射 `length`：数组 → `arraylength`；**仅** `Utf16Text` → `String.length()`（code units）。
    /// `Utf8Text.length` 是 Unicode 标量 — 禁止降到 `String.length()`；由 host_provider /
    /// `codePointCount` 路径处理（见 `std.adaptor.jvm.text.Utf8Text`）。
    fn emit_jvm_length_call(&mut self, receiver: &MirOperand, output: Option<MirValueRef>) {
        let ty = self.infer_field_type(receiver);
        let is_utf16 = matches!(ty, NyarType::Utf16);
        let is_array = matches!(ty, NyarType::Array(_) | NyarType::FixedArray { .. });
        self.emit_operand(receiver);
        if is_array {
            self.instructions.push(JvmInstruction::ArrayLength);
        }
        else if is_utf16 {
            self.instructions.push(JvmInstruction::CheckCast("java/lang/String".to_string()));
            self.instructions.push(JvmInstruction::InvokeVirtual(JvmMethodRef {
                owner: "java/lang/String".to_string(),
                name: "length".to_string(),
                descriptor: JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Int),
            }));
        }
        else {
            // Utf8 must not enter here (call site falls through to host_provider).
            // Untyped force-path: leave as code-unit length only as last resort.
            self.instructions.push(JvmInstruction::CheckCast("java/lang/String".to_string()));
            self.instructions.push(JvmInstruction::InvokeVirtual(JvmMethodRef {
                owner: "java/lang/String".to_string(),
                name: "length".to_string(),
                descriptor: JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Int),
            }));
        }
        if let Some(output) = output {
            self.value_type_overrides.insert(output, NyarType::Integer32 { signed: true });
            self.store_to_value(output);
        }
    }

    /// `Utf8ScalarLength` counts Unicode scalar values in the language `utf8` value.
    fn emit_jvm_unicode_scalar_length(&mut self, receiver: &MirOperand, output: Option<MirValueRef>) {
        self.emit_operand(receiver);
        self.instructions.push(JvmInstruction::CheckCast("java/lang/String".to_string()));
        self.instructions.push(JvmInstruction::Dup);
        self.instructions.push(JvmInstruction::IConst(0));
        self.instructions.push(JvmInstruction::Swap);
        self.instructions.push(JvmInstruction::InvokeVirtual(JvmMethodRef {
            owner: "java/lang/String".to_string(),
            name: "length".to_string(),
            descriptor: JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Int),
        }));
        self.instructions.push(JvmInstruction::InvokeVirtual(JvmMethodRef {
            owner: "java/lang/String".to_string(),
            name: "codePointCount".to_string(),
            descriptor: JvmMethodDescriptor::new(vec![JvmTypeDescriptor::Int, JvmTypeDescriptor::Int], JvmTypeDescriptor::Int),
        }));
        if let Some(output) = output {
            self.value_type_overrides.insert(output, NyarType::Integer32 { signed: true });
            self.store_to_value(output);
        }
    }

    /// `String.equals` 实现 utf8 相等/不等比较。
    fn emit_string_equality(&mut self, equal: bool, arguments: &[MirOperand], output: Option<MirValueRef>) {
        self.emit_operand(&arguments[0]);
        self.instructions.push(JvmInstruction::CheckCast("java/lang/String".to_string()));
        self.emit_operand(&arguments[1]);
        self.instructions.push(JvmInstruction::CheckCast("java/lang/String".to_string()));
        self.instructions.push(JvmInstruction::InvokeVirtual(JvmMethodRef {
            owner: "java/lang/String".to_string(),
            name: "equals".to_string(),
            descriptor: JvmMethodDescriptor::new(vec![JvmTypeDescriptor::Object("java/lang/Object".to_string())], JvmTypeDescriptor::Boolean),
        }));
        if !equal {
            let true_label = self.next_label();
            let end_label = self.next_label();
            self.instructions.push(JvmInstruction::IfEq(true_label.clone()));
            self.instructions.push(JvmInstruction::IConst(0));
            self.instructions.push(JvmInstruction::Goto(end_label.clone()));
            self.instructions.push(JvmInstruction::Label(true_label));
            self.instructions.push(JvmInstruction::IConst(1));
            self.instructions.push(JvmInstruction::Label(end_label));
        }
        if let Some(output) = output {
            self.value_type_overrides.insert(output, NyarType::Boolean);
            self.store_to_value(output);
        }
    }

    /// utf8/utf16 序比较 → `String.compareTo` + `if_icmp*`（禁止落到 `isize::infix >=`
    /// 的 `(Object,I)` ABI：实参是两个 String 时会 VerifyError Expecting integer）。
    fn emit_string_ordering(&mut self, op: IntrinsicCompareOp, arguments: &[MirOperand], output: Option<MirValueRef>) {
        self.emit_operand(&arguments[0]);
        self.instructions.push(JvmInstruction::CheckCast("java/lang/String".to_string()));
        self.emit_operand(&arguments[1]);
        self.instructions.push(JvmInstruction::CheckCast("java/lang/String".to_string()));
        self.instructions.push(JvmInstruction::InvokeVirtual(JvmMethodRef {
            owner: "java/lang/String".to_string(),
            name: "compareTo".to_string(),
            descriptor: JvmMethodDescriptor::new(vec![JvmTypeDescriptor::Object("java/lang/String".to_string())], JvmTypeDescriptor::Int),
        }));
        self.instructions.push(JvmInstruction::IConst(0));
        let true_label = self.next_label();
        let end_label = self.next_label();
        self.instructions.push(match op {
            IntrinsicCompareOp::Lt => JvmInstruction::IfICmpLt(true_label.clone()),
            IntrinsicCompareOp::Le => JvmInstruction::IfICmpLe(true_label.clone()),
            IntrinsicCompareOp::Gt => JvmInstruction::IfICmpGt(true_label.clone()),
            IntrinsicCompareOp::Ge => JvmInstruction::IfICmpGe(true_label.clone()),
            IntrinsicCompareOp::Eq => JvmInstruction::IfICmpEq(true_label.clone()),
            IntrinsicCompareOp::Ne => JvmInstruction::IfICmpNe(true_label.clone()),
        });
        self.instructions.push(JvmInstruction::IConst(0));
        self.instructions.push(JvmInstruction::Goto(end_label.clone()));
        self.instructions.push(JvmInstruction::Label(true_label));
        self.instructions.push(JvmInstruction::IConst(1));
        self.instructions.push(JvmInstruction::Label(end_label));
        if let Some(output) = output {
            self.value_type_overrides.insert(output, NyarType::Boolean);
            self.store_to_value(output);
        }
    }

    /// Project structured `Utf8*` intrinsics to the JVM carrier. `java.lang.String`
    /// is not a language text type: this path intentionally excludes `Utf16`.
    fn emit_string_instance_method(&mut self, java_name: &str, arguments: &[MirOperand], output: Option<MirValueRef>) -> bool {
        if arguments.is_empty() {
            return false;
        }
        let receiver_ty = self.infer_field_type(&arguments[0]);
        let receiver_ok = matches!(receiver_ty, NyarType::Utf8);
        if !receiver_ok {
            return false;
        }
        self.emit_operand(&arguments[0]);
        self.instructions.push(JvmInstruction::CheckCast("java/lang/String".to_string()));
        let (param_types, return_ty, return_nyar) = match java_name {
            "startsWith" | "endsWith" | "contains" if arguments.len() >= 2 => {
                self.emit_operand(&arguments[1]);
                self.instructions.push(JvmInstruction::CheckCast("java/lang/String".to_string()));
                (vec![JvmTypeDescriptor::Object("java/lang/String".to_string())], JvmTypeDescriptor::Boolean, NyarType::Boolean)
            }
            "trim" | "isEmpty" => (
                Vec::new(),
                if java_name == "trim" { JvmTypeDescriptor::Object("java/lang/String".to_string()) } else { JvmTypeDescriptor::Boolean },
                if java_name == "trim" { NyarType::Utf8 } else { NyarType::Boolean },
            ),
            _ => return false,
        };
        // contains(CharSequence) — use CharSequence-compatible Object descriptor via String overload in Java 21: contains(CharSequence)
        let param_types =
            if java_name == "contains" { vec![JvmTypeDescriptor::Object("java/lang/CharSequence".to_string())] } else { param_types };
        self.instructions.push(JvmInstruction::InvokeVirtual(JvmMethodRef {
            owner: "java/lang/String".to_string(),
            name: java_name.to_string(),
            descriptor: JvmMethodDescriptor::new(param_types, return_ty.clone()),
        }));
        if let Some(output) = output {
            self.value_type_overrides.insert(output, return_nyar.clone());
            if matches!(return_nyar, NyarType::Utf8) {
                self.boxed_value_refs.insert(output);
            }
            self.store_to_value(output);
        }
        else if !matches!(return_ty, JvmTypeDescriptor::Void) {
            self.instructions.push(JvmInstruction::Pop);
        }
        true
    }

    fn emit_intrinsic_opcode(&mut self, opcode: IntrinsicOpcode, arguments: &[MirOperand], output: Option<MirValueRef>) {
        match opcode {
            IntrinsicOpcode::Binary(op) => self.emit_intrinsic_binary(op, arguments, output),
            IntrinsicOpcode::Neg => self.emit_intrinsic_neg(arguments, output),
            IntrinsicOpcode::ArrayGet => self.emit_intrinsic_array_get(arguments, output),
            IntrinsicOpcode::ArraySet => self.emit_intrinsic_array_set(arguments, output),
            IntrinsicOpcode::ArrayLen => {
                self.emit_jvm_length_call(&arguments[0], output);
            }
            IntrinsicOpcode::Utf8ScalarLength => {
                self.emit_jvm_unicode_scalar_length(&arguments[0], output);
            }
            IntrinsicOpcode::Utf8ContentEqual => self.emit_string_equality(true, arguments, output),
            IntrinsicOpcode::Utf8ContentNotEqual => self.emit_string_equality(false, arguments, output),
            IntrinsicOpcode::Utf8Trim => {
                self.emit_string_instance_method("trim", arguments, output);
            }
            IntrinsicOpcode::Utf8IndexOf => {
                self.emit_string_instance_method("indexOf", arguments, output);
            }
            IntrinsicOpcode::Utf8Contains => {
                self.emit_string_instance_method("contains", arguments, output);
            }
            IntrinsicOpcode::Utf8StartsWith => {
                self.emit_string_instance_method("startsWith", arguments, output);
            }
            IntrinsicOpcode::Utf8EndsWith => {
                self.emit_string_instance_method("endsWith", arguments, output);
            }
            IntrinsicOpcode::SumVariantIs | IntrinsicOpcode::SumStructuralEqual => {
                return;
            }
            IntrinsicOpcode::ArrayPush => self.emit_intrinsic_array_push(arguments, output),
            IntrinsicOpcode::Utf8ScalarSlice => {
                if arguments.len() < 3 {
                    return;
                }
                self.emit_operand(&arguments[0]);
                self.emit_operand(&arguments[1]);
                self.emit_operand(&arguments[2]);
                self.instructions.push(JvmInstruction::InvokeVirtual(JvmMethodRef {
                    owner: "java/lang/String".to_string(),
                    name: "substring".to_string(),
                    descriptor: JvmMethodDescriptor::new(
                        vec![JvmTypeDescriptor::Int, JvmTypeDescriptor::Int],
                        JvmTypeDescriptor::Object("java/lang/String".to_string()),
                    ),
                }));
                if let Some(output) = output {
                    self.value_type_overrides.insert(output, NyarType::Utf8);
                    self.store_to_value(output);
                }
            }
            IntrinsicOpcode::Deref => {
                self.emit_jvm_deref_identity(&arguments[0], output);
            }
            IntrinsicOpcode::Compare(op) => self.emit_intrinsic_compare(op, arguments, output),
            IntrinsicOpcode::Not => {
                let true_label = self.next_label();
                let end_label = self.next_label();
                self.emit_operand(&arguments[0]);
                self.instructions.push(JvmInstruction::IfEq(true_label.clone()));
                self.instructions.push(JvmInstruction::IConst(0));
                self.instructions.push(JvmInstruction::Goto(end_label.clone()));
                self.instructions.push(JvmInstruction::Label(true_label));
                self.instructions.push(JvmInstruction::IConst(1));
                self.instructions.push(JvmInstruction::Label(end_label));
                if let Some(output) = output {
                    // 逻辑非运算结果始终为 Boolean，写入 override 避免 value_types 回退。
                    self.value_type_overrides.insert(output, NyarType::Boolean);
                    self.store_to_value(output);
                }
            }
            IntrinsicOpcode::Bitwise(op) => {
                let ty0 = self.infer_field_type(&arguments[0]);
                let effective_ty0 = effective_jvm_type(&self.ctx, &ty0);
                let is_long = matches!(effective_ty0, NyarType::Integer64 { .. } | NyarType::Integer128 { .. })
                    || self.operand_is_jvm_long(&arguments[0]);
                // Late ABI widening can leave an SSA operand classified as an
                // int even though this function's declared result is a JVM
                // long. The signature is authoritative for the operation's
                // result category; use it as a generic fallback for all
                // bitwise intrinsics.
                let is_long = is_long
                    || matches!(
                        effective_jvm_type(&self.ctx, &self.mir_fn.return_type),
                        NyarType::Integer64 { .. } | NyarType::Integer128 { .. }
                    );
                if is_long {
                    self.emit_operand_as_long(&arguments[0]);
                    if matches!(op, IntrinsicBitwiseOp::Shl | IntrinsicBitwiseOp::Shr) {
                        // The wasm fixed-shift helpers (for example
                        // `wasm_ashr7_i64`) are unary and encode the shift
                        // count in the intrinsic name. JVM LShr/LShl still
                        // require an int count on the stack.
                        if let Some(shift) = arguments.get(1) {
                            self.emit_operand(shift);
                            let shift_ty = self.infer_field_type(shift);
                            let shift_effective = effective_jvm_type(&self.ctx, &shift_ty);
                            if matches!(shift_effective, NyarType::Integer64 { .. } | NyarType::Integer128 { .. })
                                || self.operand_is_jvm_long(shift)
                            {
                                self.instructions.push(JvmInstruction::L2I);
                            }
                        }
                        else {
                            self.instructions.push(JvmInstruction::IConst(7));
                        }
                    }
                    else {
                        self.emit_operand_as_long(&arguments[1]);
                    }
                }
                else {
                    self.emit_operand(&arguments[0]);
                    if let Some(shift) = arguments.get(1) {
                        self.emit_operand(shift);
                    }
                    else if matches!(op, IntrinsicBitwiseOp::Shl | IntrinsicBitwiseOp::Shr) {
                        self.instructions.push(JvmInstruction::IConst(7));
                    }
                }
                match op {
                    IntrinsicBitwiseOp::And if is_long => self.instructions.push(JvmInstruction::LAnd),
                    IntrinsicBitwiseOp::Or if is_long => self.instructions.push(JvmInstruction::LOr),
                    IntrinsicBitwiseOp::Xor if is_long => self.instructions.push(JvmInstruction::LXor),
                    IntrinsicBitwiseOp::And => self.instructions.push(JvmInstruction::IAnd),
                    IntrinsicBitwiseOp::Or => self.instructions.push(JvmInstruction::IOr),
                    IntrinsicBitwiseOp::Xor => self.instructions.push(JvmInstruction::IXor),
                    IntrinsicBitwiseOp::Shl if is_long => self.instructions.push(JvmInstruction::LShl),
                    IntrinsicBitwiseOp::Shr if is_long => self.instructions.push(JvmInstruction::LShr),
                    IntrinsicBitwiseOp::Shl => self.instructions.push(JvmInstruction::IShl),
                    IntrinsicBitwiseOp::Shr => self.instructions.push(JvmInstruction::IShr),
                }
                if let Some(output) = output {
                    // 位运算结果类型与第一操作数相同，写入 override 避免 value_types 回退。
                    self.value_type_overrides.insert(output, ty0);
                    self.store_to_value(output);
                }
            }
        }
    }

    fn emit_intrinsic_binary(&mut self, op: IntrinsicBinaryOp, arguments: &[MirOperand], output: Option<MirValueRef>) {
        let ty0 = self.infer_field_type(&arguments[0]);
        let ty1 = self.infer_field_type(&arguments[1]);
        let effective_ty0 = effective_jvm_type(&self.ctx, &ty0);
        let effective_ty1 = effective_jvm_type(&self.ctx, &ty1);
        let is_float =
            matches!(effective_ty0, NyarType::Float64 | NyarType::Float32) || matches!(effective_ty1, NyarType::Float64 | NyarType::Float32);
        let is_long = !is_float
            && (matches!(effective_ty0, NyarType::Integer64 { .. })
                || matches!(effective_ty1, NyarType::Integer64 { .. })
                || self.operand_is_jvm_long(&arguments[0])
                || self.operand_is_jvm_long(&arguments[1]));
        if is_long {
            self.emit_operand_as_long(&arguments[0]);
            self.emit_operand_as_long(&arguments[1]);
        }
        else {
            self.emit_operand(&arguments[0]);
            self.emit_operand(&arguments[1]);
        }
        self.instructions.push(match op {
            IntrinsicBinaryOp::Add if is_float => JvmInstruction::DAdd,
            IntrinsicBinaryOp::Sub if is_float => JvmInstruction::DSub,
            IntrinsicBinaryOp::Mul if is_float => JvmInstruction::DMul,
            IntrinsicBinaryOp::Div if is_float => JvmInstruction::DDiv,
            IntrinsicBinaryOp::Rem if is_float => JvmInstruction::DRem,
            IntrinsicBinaryOp::Add if is_long => JvmInstruction::LAdd,
            IntrinsicBinaryOp::Sub if is_long => JvmInstruction::LSub,
            IntrinsicBinaryOp::Mul if is_long => JvmInstruction::LMul,
            IntrinsicBinaryOp::Div if is_long => JvmInstruction::LDiv,
            IntrinsicBinaryOp::Rem if is_long => JvmInstruction::LRem,
            IntrinsicBinaryOp::Add => JvmInstruction::IAdd,
            IntrinsicBinaryOp::Sub => JvmInstruction::ISub,
            IntrinsicBinaryOp::Mul => JvmInstruction::IMul,
            IntrinsicBinaryOp::Div => JvmInstruction::IDiv,
            IntrinsicBinaryOp::Rem => JvmInstruction::IRem,
        });
        if let Some(output) = output {
            // 二元算术运算结果类型与第一操作数相同（如 `usize + 1` 结果仍为 `usize`）。
            // 必须在 `store_to_value` 前写入 override，否则 `lookup_value_type` 会回退
            // 到 `mir_fn.value_types`，而 HIR resolver 可能把 `+` 的返回类型误绑为
            // 引用类型（如 `Utf8Text`），导致 `astore` 与栈上 `int` 冲突，
            // 触发 VerifyError: "Expecting to find integer on stack"。
            // i64 路径（含 `0 - n` 左侧常量被推断为 int）结果必须记为 Integer64，
            // 否则后续 `lstore`/`lload` 与 int local 冲突。
            let result_ty = if is_long { NyarType::Integer64 { signed: true } } else { ty0 };
            self.value_type_overrides.insert(output, result_ty);
            self.store_to_value(output);
        }
    }

    fn emit_intrinsic_neg(&mut self, arguments: &[MirOperand], output: Option<MirValueRef>) {
        let ty0 = self.infer_field_type(&arguments[0]);
        let inferred = effective_jvm_type(&self.ctx, &ty0);
        self.emit_operand(&arguments[0]);
        self.instructions.push(match inferred {
            NyarType::Float64 => JvmInstruction::DNeg,
            NyarType::Float32 => JvmInstruction::FNeg,
            NyarType::Integer64 { .. } => JvmInstruction::LNeg,
            _ => JvmInstruction::INeg,
        });
        if let Some(output) = output {
            // 一元负运算结果类型与操作数相同，写入 override 避免类型回退。
            self.value_type_overrides.insert(output, ty0);
            self.store_to_value(output);
        }
    }

    fn emit_intrinsic_array_get(&mut self, arguments: &[MirOperand], output: Option<MirValueRef>) {
        let array_ty = self.infer_field_type(&arguments[0]);
        let raw_element_type = match &array_ty {
            NyarType::Array(item) | NyarType::FixedArray { element: item, .. } => item.as_ref().clone(),
            // Public lowering validates ArrayGet before preparation. A direct
            // internal caller without an array contract is a compiler bug, not
            // evidence that its receiver is text.
            _ => unreachable!("ArrayGet reached JVM preparation without an array Semantic MIR type"),
        };
        // 使用 `effective_array_element_type` 仅折叠 unite sum type 元素为 Integer32，
        // 值类型保持 Named（数组元素是堆对象引用）。这与数组描述符 `[I` / `[LType;`
        // 和 `iaload`/`aaload` 返回类型一致。
        let element_type = effective_array_element_type(&self.ctx, &raw_element_type);
        let load = match &element_type {
            NyarType::Boolean | NyarType::Integer8 { .. } => JvmInstruction::BALoad,
            NyarType::Character => JvmInstruction::CALoad,
            NyarType::Integer16 { .. } => JvmInstruction::SALoad,
            NyarType::Integer32 { .. } => JvmInstruction::IALoad,
            NyarType::Integer64 { .. } | NyarType::Float32 | NyarType::Float64 => {
                // 宽/浮点数组暂未单独降级：按 int 数组路径会 VerifyError，
                // 仍用 IALoad 保持旧行为；后续可补 LALoad/FALoad/DALoad。
                JvmInstruction::IALoad
            }
            _ => JvmInstruction::AALoad,
        };
        let is_reference = matches!(load, JvmInstruction::AALoad);
        self.emit_operand(&arguments[0]);
        self.emit_operand(&arguments[1]);
        self.instructions.push(load);
        if let Some(output) = output {
            // 必须在 store 前写入元素真实类型，否则 `lookup_value_type` 回退 Unit → `istore`，
            // 与 `aaload` 压入的引用冲突（Expecting to find integer on stack）。
            // baload/caload/saload 压 int：索引结果按 i32 存，避免后续 `as i32` 路径类型漂移。
            let store_ty = match &element_type {
                NyarType::Boolean | NyarType::Integer8 { .. } | NyarType::Integer16 { .. } | NyarType::Character => {
                    NyarType::Integer32 { signed: true }
                }
                other => other.clone(),
            };
            self.value_type_overrides.insert(output, store_ty);
            if is_reference {
                self.boxed_value_refs.insert(output);
            }
            self.store_to_value(output);
        }
    }

    fn emit_intrinsic_array_set(&mut self, arguments: &[MirOperand], output: Option<MirValueRef>) {
        let raw_element_type = match self.infer_field_type(&arguments[0]) {
            NyarType::Array(item) => *item,
            other => other,
        };
        // 与 `emit_intrinsic_array_get` 对齐：仅 unite sum type 元素折叠为 Integer32，
        // 使用 `iastore` 与 `[I` 数组描述符一致。
        let element_type = effective_array_element_type(&self.ctx, &raw_element_type);
        let store = match &element_type {
            NyarType::Boolean | NyarType::Integer8 { .. } => JvmInstruction::BAStore,
            NyarType::Character => JvmInstruction::CAStore,
            NyarType::Integer16 { .. } => JvmInstruction::SAStore,
            NyarType::Integer32 { .. } => JvmInstruction::IAStore,
            NyarType::Integer64 { .. } | NyarType::Float32 | NyarType::Float64 => JvmInstruction::IAStore,
            _ => JvmInstruction::AAStore,
        };
        let is_reference = matches!(store, JvmInstruction::AAStore);
        let _ = is_reference;
        self.emit_operand(&arguments[0]);
        self.emit_operand(&arguments[1]);
        self.emit_operand(&arguments[2]);
        self.instructions.push(store);
        // ArraySet is a statement-like intrinsic: `iastore`/`aastore` consumes
        // all three operands and leaves no value on the operand stack.  The
        // MIR may still carry an SSA output (usually Unit) for sequencing, but
        // storing that output would emit an extra istore/astore with an empty
        // stack and makes the class unverifiable (e.g. `iastore; istore N`).
        let _ = output;
    }

    /// `push(array, value)` → 新数组（长度+1），末尾写入 `value`。
    ///
    /// 与 CLR `emit_intrinsic_array_push` 对齐。先前 JVM 为空实现，导致
    /// `out.names = push(out.names, name)` 等赋值不发射任何字节码，后续
    /// 用错 local 做 `aload`/`astore`，触发 `Register N contains wrong type`。
    fn emit_intrinsic_array_push(&mut self, arguments: &[MirOperand], output: Option<MirValueRef>) {
        if arguments.len() < 2 {
            return;
        }
        let array = &arguments[0];
        let value = &arguments[1];
        let array_ty = self.infer_field_type(array);
        let raw_element = match &array_ty {
            NyarType::Array(item) | NyarType::FixedArray { element: item, .. } => item.as_ref().clone(),
            other => other.clone(),
        };
        let element_type = effective_array_element_type(&self.ctx, &raw_element);
        let is_reference = jvm_primitive_newarray(&element_type).is_none();

        let len_local = self.alloc_jvm_temp(&NyarType::Integer32 { signed: true });
        let new_local = self.alloc_jvm_temp(&array_ty);

        // len = array.length
        self.emit_operand(array);
        self.instructions.push(JvmInstruction::ArrayLength);
        self.emit_store_local(len_local, &NyarType::Integer32 { signed: true });

        // newArr = Arrays.copyOf(array, len + 1)
        self.emit_operand(array);
        self.emit_load_local(len_local, &NyarType::Integer32 { signed: true });
        self.instructions.push(JvmInstruction::IConst(1));
        self.instructions.push(JvmInstruction::IAdd);
        let (owner, name, descriptor, result_cast) = arrays_copy_of_method(&element_type, is_reference);
        self.instructions.push(JvmInstruction::InvokeStatic(JvmMethodRef { owner, name, descriptor }));
        if let Some(cast) = result_cast {
            self.instructions.push(JvmInstruction::CheckCast(cast));
        }
        self.emit_store_local(new_local, &array_ty);

        // newArr[len] = value
        self.emit_load_local(new_local, &array_ty);
        self.emit_load_local(len_local, &NyarType::Integer32 { signed: true });
        self.emit_operand(value);
        self.instructions.push(jvm_primitive_array_store(&element_type));

        // 结果写回：有 output 则 store；同时像 CLR 一样更新源数组 local（语句式 push）。
        self.emit_load_local(new_local, &array_ty);
        if let Some(array_local) = self.operand_local(array) {
            self.instructions.push(JvmInstruction::Dup);
            self.emit_store_local(array_local, &array_ty);
        }
        if let Some(output) = output {
            self.value_type_overrides.insert(output, array_ty.clone());
            if is_reference {
                // 数组本身是引用。
            }
            self.store_to_value(output);
        }
        else {
            self.instructions.push(JvmInstruction::Pop);
        }
    }

    /// 分配一个临时 JVM local，并登记 `local_kinds`。
    fn alloc_jvm_temp(&mut self, ty: &NyarType) -> u16 {
        let effective = effective_jvm_type(&self.ctx, ty);
        let kind = jvm_local_kind_of(&effective);
        let width = super::executable::jvm_local_slots(&effective).max(1);
        let new_local = self.slots.local_types.len() as u16;
        let fill = self.slots.local_types.last().cloned().unwrap_or(crate::nyar_backend_clr::MsilType::Int32 { signed: true });
        for _ in 0..width {
            self.slots.local_types.push(fill.clone());
        }
        self.local_kinds.insert(new_local, kind);
        if width > 1 {
            self.local_kinds.insert(new_local.saturating_add(1), kind);
        }
        new_local
    }

    fn emit_intrinsic_compare(&mut self, op: IntrinsicCompareOp, arguments: &[MirOperand], output: Option<MirValueRef>) {
        let ty = self.infer_field_type(&arguments[0]);
        let ty1 = self.infer_field_type(&arguments[1]);
        let effective_ty = effective_jvm_type(&self.ctx, &ty);
        let effective_ty1 = effective_jvm_type(&self.ctx, &ty1);
        let is_float = matches!(effective_ty, NyarType::Float64) || matches!(effective_ty1, NyarType::Float64);
        let is_long = !is_float
            && (matches!(effective_ty, NyarType::Integer64 { .. })
                || matches!(effective_ty1, NyarType::Integer64 { .. })
                || self.operand_is_jvm_long(&arguments[0])
                || self.operand_is_jvm_long(&arguments[1]));
        let is_reference = !is_long && is_jvm_stack_reference(&effective_ty);
        let true_label = self.next_label();
        let end_label = self.next_label();
        if is_long {
            self.emit_operand_as_long(&arguments[0]);
            self.emit_operand_as_long(&arguments[1]);
        }
        else {
            self.emit_operand(&arguments[0]);
            self.emit_operand(&arguments[1]);
        }
        if is_float {
            self.instructions.push(JvmInstruction::DCmpG);
            self.instructions.push(match op {
                IntrinsicCompareOp::Eq => JvmInstruction::IfNe(true_label.clone()),
                IntrinsicCompareOp::Ne => JvmInstruction::IfEq(true_label.clone()),
                IntrinsicCompareOp::Lt => JvmInstruction::IfLt(true_label.clone()),
                IntrinsicCompareOp::Le => JvmInstruction::IfLe(true_label.clone()),
                IntrinsicCompareOp::Gt => JvmInstruction::IfGt(true_label.clone()),
                IntrinsicCompareOp::Ge => JvmInstruction::IfGe(true_label.clone()),
            });
        }
        else if is_long {
            // `lcmp` → int (-1/0/1)，再 `ifeq`/`iflt`…；禁止 `if_icmp*`（期望两个 int，
            // 栈上是两个 long → VerifyError: Expecting to find integer on stack）。
            self.instructions.push(JvmInstruction::LCmp);
            self.instructions.push(match op {
                IntrinsicCompareOp::Eq => JvmInstruction::IfEq(true_label.clone()),
                IntrinsicCompareOp::Ne => JvmInstruction::IfNe(true_label.clone()),
                IntrinsicCompareOp::Lt => JvmInstruction::IfLt(true_label.clone()),
                IntrinsicCompareOp::Le => JvmInstruction::IfLe(true_label.clone()),
                IntrinsicCompareOp::Gt => JvmInstruction::IfGt(true_label.clone()),
                IntrinsicCompareOp::Ge => JvmInstruction::IfGe(true_label.clone()),
            });
        }
        else if is_reference {
            match op {
                IntrinsicCompareOp::Eq => {
                    self.instructions.push(JvmInstruction::IfACmpEq(true_label.clone()));
                }
                IntrinsicCompareOp::Ne => {
                    self.instructions.push(JvmInstruction::IfACmpNe(true_label.clone()));
                }
                // 引用序比较必须走 compareTo；IfICmp* 期望 int，对两个 String 会
                // VerifyError: Expecting to find integer on stack。
                IntrinsicCompareOp::Lt | IntrinsicCompareOp::Le | IntrinsicCompareOp::Gt | IntrinsicCompareOp::Ge => {
                    self.instructions.push(JvmInstruction::CheckCast("java/lang/String".to_string()));
                    // compareTo 需要 (receiver, other)；栈上已是 [a, b]，先 swap 再
                    // checkcast other 不方便——改为 dup_x1 模式不稳。重新：栈顶是 b，
                    // 次顶是 a。InvokeVirtual compareTo 要 a 在下、b 在上，恰好匹配。
                    // 但 a 尚未 checkcast；对 a 需要先 swap → checkcast → swap。
                    self.instructions.push(JvmInstruction::Swap);
                    self.instructions.push(JvmInstruction::CheckCast("java/lang/String".to_string()));
                    self.instructions.push(JvmInstruction::Swap);
                    self.instructions.push(JvmInstruction::InvokeVirtual(JvmMethodRef {
                        owner: "java/lang/String".to_string(),
                        name: "compareTo".to_string(),
                        descriptor: JvmMethodDescriptor::new(
                            vec![JvmTypeDescriptor::Object("java/lang/String".to_string())],
                            JvmTypeDescriptor::Int,
                        ),
                    }));
                    self.instructions.push(JvmInstruction::IConst(0));
                    self.instructions.push(match op {
                        IntrinsicCompareOp::Lt => JvmInstruction::IfICmpLt(true_label.clone()),
                        IntrinsicCompareOp::Le => JvmInstruction::IfICmpLe(true_label.clone()),
                        IntrinsicCompareOp::Gt => JvmInstruction::IfICmpGt(true_label.clone()),
                        IntrinsicCompareOp::Ge => JvmInstruction::IfICmpGe(true_label.clone()),
                        _ => unreachable!(),
                    });
                }
            }
        }
        else {
            self.instructions.push(match op {
                IntrinsicCompareOp::Eq => JvmInstruction::IfICmpEq(true_label.clone()),
                IntrinsicCompareOp::Ne => JvmInstruction::IfICmpNe(true_label.clone()),
                IntrinsicCompareOp::Lt => JvmInstruction::IfICmpLt(true_label.clone()),
                IntrinsicCompareOp::Le => JvmInstruction::IfICmpLe(true_label.clone()),
                IntrinsicCompareOp::Gt => JvmInstruction::IfICmpGt(true_label.clone()),
                IntrinsicCompareOp::Ge => JvmInstruction::IfICmpGe(true_label.clone()),
            });
        }
        self.instructions.push(JvmInstruction::IConst(0));
        self.instructions.push(JvmInstruction::Goto(end_label.clone()));
        self.instructions.push(JvmInstruction::Label(true_label));
        self.instructions.push(JvmInstruction::IConst(1));
        self.instructions.push(JvmInstruction::Label(end_label));
        if let Some(output) = output {
            // 比较运算结果始终为 Boolean，写入 override 避免 value_types 回退。
            self.value_type_overrides.insert(output, NyarType::Boolean);
            self.store_to_value(output);
        }
    }

    fn emit_load_constant(&mut self, constant: &MirConstant, ty: Option<&NyarType>) {
        match constant {
            MirConstant::Int(value) => {
                let is_long =
                    ty.map(|candidate| matches!(effective_jvm_type(&self.ctx, candidate), NyarType::Integer64 { .. })).unwrap_or(false);
                if is_long {
                    if *value == 0 {
                        self.instructions.push(JvmInstruction::LConst0);
                    }
                    else if *value == 1 {
                        self.instructions.push(JvmInstruction::LConst1);
                    }
                    else {
                        self.instructions.push(JvmInstruction::LdcLong(*value));
                    }
                }
                else {
                    self.instructions.push(JvmInstruction::IConst(*value as i32));
                }
            }
            MirConstant::Float64(value) => {
                let bits = value.into_inner().to_bits();
                if bits == 0f64.to_bits() {
                    self.instructions.push(JvmInstruction::DConst0);
                }
                else if bits == 1f64.to_bits() {
                    self.instructions.push(JvmInstruction::DConst1);
                }
                else {
                    self.instructions.push(JvmInstruction::LdcDouble(bits));
                }
            }
            MirConstant::Bool(value) => self.instructions.push(JvmInstruction::IConst(if *value { 1 } else { 0 })),
            // `java.lang.String` is a backend carrier. The source encoding is
            // retained by the MIR type and intrinsic contract, not recovered
            // from this physical representation.
            MirConstant::Utf8(text) | MirConstant::Utf16(text) => self.instructions.push(JvmInstruction::LdcString(text.clone())),
            MirConstant::Unit => self.instructions.push(JvmInstruction::IConst(0)),
        }
    }

    fn emit_operand(&mut self, operand: &MirOperand) {
        match operand {
            MirOperand::Value(value) => {
                if let Some(local) = self.slots.value_locals.get(value).copied() {
                    let ty = self.lookup_value_type(value);
                    let effective_ty = if self.boxed_value_refs.contains(value) { ty.clone() } else { effective_jvm_type(&self.ctx, &ty) };
                    self.emit_load_local(local, &effective_ty);
                }
                else {
                    let ty = self.lookup_value_type(value);
                    let effective_ty = if self.boxed_value_refs.contains(value) { ty } else { effective_jvm_type(&self.ctx, &ty) };
                    match effective_ty {
                        NyarType::Integer64 { .. } => {
                            self.instructions.push(JvmInstruction::LConst0);
                        }
                        NyarType::Float64 => {
                            self.instructions.push(JvmInstruction::DConst0);
                        }
                        NyarType::Utf8
                        | NyarType::Utf16
                        | NyarType::Named(_)
                        | NyarType::Array(_)
                        | NyarType::FixedArray { .. }
                        | NyarType::Apply(_, _)
                        | NyarType::Union(_) => {
                            self.instructions.push(JvmInstruction::AConstNull);
                        }
                        NyarType::Bottom | NyarType::Unit => {
                            // Unit 是 inhabited ADT（栈上 int 0）。无 local 时仍须压栈，
                            // 否则作为 call 实参会空发射，导致
                            // VerifyError: "Expecting to find integer on stack"。
                            self.instructions.push(JvmInstruction::IConst(0));
                        }
                        _ => {
                            self.instructions.push(JvmInstruction::IConst(0));
                        }
                    }
                }
            }
            MirOperand::Constant(constant) => self.emit_load_constant(constant, None),
            MirOperand::Symbol(path) => {
                let key = path.to_string();
                if let Some(local) = self.slots.var_locals.get(&key).copied() {
                    let ty = self.var_types.get(&key).cloned().unwrap_or(NyarType::Unit);
                    self.emit_load_local(local, &ty);
                }
                else if path.parts().len() == 1 && path.parts()[0].as_str() == "null" {
                    // `null` 字面量在 JVM 上为 null 引用。nullable 引用类型
                    // （如 `LegionPublishTarget?`）用 null 表示缺失，必须发射
                    // `aconst_null` 而非 `iconst_0`，否则 `astore` 期望引用但
                    // 栈上是 int，触发 VerifyError: "Type mismatch"。
                    self.instructions.push(JvmInstruction::AConstNull);
                }
                else {
                    self.instructions.push(JvmInstruction::IConst(0));
                }
            }
        }
    }

    /// 为方法调用压栈一个参数。
    ///
    /// 对于值类型参数，按字段顺序依次 push 每个字段的值到操作数栈，
    /// 与被调用方展开后的方法描述符匹配（[`effective_param_descriptors`] /
    /// [`lower_mir_function_to_jvm`] 始终 flatten 多字段值类型参数）。
    /// 对于非值类型参数，行为与 `emit_operand` 完全一致。
    ///
    /// 装箱的多字段值类型（堆对象）也必须按字段展开：callee ABI 是展开的
    /// `I`/`[I`/`Z`/…，不是单个 `LType;`。若此处 `aload` 整个对象再
    /// `checkcast LType;`，而描述符期望展开字段，会触发
    /// `VerifyError: Expecting to find object/array on stack` 或
    /// `Register N contains wrong type`（尤其当 MIR 已把字段 FieldGet 到
    /// 连续 local，boxed 值的 `value_locals` 被污染成首字段槽时）。
    ///
    /// 引用参数在压栈后按声明类型发射 `checkcast`：runtime stub（如
    /// `collect_array`）常以 `[Ljava/lang/Object;` 写入 local，而调用描述符
    /// 仍声明具体数组类型（如 `[Ljava/lang/String;`）。JVM 验证器按 local
    /// 的存储类型追踪栈顶，缺少收窄会触发
    /// `VerifyError: Incompatible argument to function`。
    ///
    /// `expected_params` / `expected_index`：callee 真实 ABI（经
    /// [`resolve_callee_jvm_descriptor`]）。当描述符要引用（utf8/`String`/class/
    /// array）而操作数有效类型是 int 句柄/`Unit` 时，压 `aconst_null` 而非
    /// `iconst_0`，避免 invokestatic 上报
    /// `Expecting to find object/array on stack`（unite/`Result` 臂里未绑定的
    /// utf8 payload 常见）。返回本参数占用的描述符槽位数。
    fn emit_call_argument(&mut self, operand: &MirOperand, expected_params: Option<&[JvmTypeDescriptor]>, expected_index: usize) -> usize {
        let expected_one = expected_params.and_then(|params| params.get(expected_index));

        // Callee wants a reference but the SSA lowers as int/unit → null, not iconst_0.
        if expected_one.is_some_and(jvm_descriptor_is_reference) {
            let ty = match operand {
                MirOperand::Value(v) => {
                    let raw = self.lookup_value_type(v);
                    if self.boxed_value_refs.contains(v) { raw } else { effective_jvm_type(&self.ctx, &raw) }
                }
                _ => effective_jvm_type(&self.ctx, &self.infer_field_type(operand)),
            };
            let operand_is_ref = is_jvm_stack_reference(&ty) || matches!(operand, MirOperand::Value(v) if self.boxed_value_refs.contains(v));
            if !operand_is_ref {
                self.instructions.push(JvmInstruction::AConstNull);
                if let Some(desc) = expected_one {
                    self.emit_checkcast_for_descriptor(desc);
                }
                return 1;
            }
        }

        if let MirOperand::Value(value) = operand {
            if let Some(local) = self.slots.value_locals.get(value).copied() {
                let ty = self.lookup_value_type(value);
                if let NyarType::Named(name) = &ty {
                    // primitive 别名（空 structure isize/bool/…）也在 value_type_names 中；
                    // 必须按 int 句柄单槽压栈，禁止走零字段 flatten（会 return 1 却不压栈）。
                    if map_primitive_name_to_jvm(name.as_str()).is_none()
                        && self.ctx.is_value_type_name(name.as_str())
                        && self.ctx.layout_by_type_name(name.as_str()).is_some()
                    {
                        let is_boxed = self.boxed_value_refs.contains(value);
                        let slot_kind = self.local_kinds.get(&local).copied();
                        // 堆对象仍完好：从对象 getfield 展开，与 callee flatten ABI 对齐。
                        let object_intact = is_boxed && matches!(slot_kind, None | Some(JvmLocalKind::Reference));
                        if object_intact {
                            // 递归展开到叶子，与 `flatten_value_type_slots` /
                            // callee 参数描述符一致（含嵌套值类型字段）。
                            let mut leaves = Vec::new();
                            self.collect_value_type_leaf_paths(name.as_str(), Vec::new(), 0, &mut leaves);
                            if leaves.is_empty() {
                                let effective_ty = effective_jvm_type(&self.ctx, &ty);
                                self.emit_load_local(local, &effective_ty);
                                self.emit_argument_checkcast(&ty);
                                self.emit_widen_int_to_long_if_needed(operand, expected_one);
                                return 1;
                            }
                            for (leaf_i, (path, leaf_ty, _)) in leaves.iter().enumerate() {
                                let leaf_expected = expected_params.and_then(|params| params.get(expected_index + leaf_i));
                                if leaf_expected.is_some_and(jvm_descriptor_is_reference) {
                                    let leaf_eff = effective_jvm_type(&self.ctx, leaf_ty);
                                    if !is_jvm_stack_reference(&leaf_eff) {
                                        self.instructions.push(JvmInstruction::AConstNull);
                                        if let Some(desc) = leaf_expected {
                                            self.emit_checkcast_for_descriptor(desc);
                                        }
                                        continue;
                                    }
                                }
                                self.instructions.push(JvmInstruction::ALoad(local));
                                for field_ref in path {
                                    self.instructions.push(JvmInstruction::GetField(field_ref.clone()));
                                }
                                self.emit_argument_checkcast(leaf_ty);
                                self.emit_widen_int_to_long_if_needed(operand, leaf_expected);
                            }
                            return leaves.len().max(1);
                        }
                        // 内联展开，或 boxed 槽已被 FieldGet 首字段 istore 污染：
                        // MIR 常在调用前把各字段抽到 base..base+n，按 flatten
                        // 叶子 offset 加载。
                        let mut leaves = Vec::new();
                        self.collect_value_type_leaf_paths(name.as_str(), Vec::new(), 0, &mut leaves);
                        if leaves.is_empty() {
                            let effective_ty = effective_jvm_type(&self.ctx, &ty);
                            self.emit_load_local(local, &effective_ty);
                            self.emit_argument_checkcast(&ty);
                            self.emit_widen_int_to_long_if_needed(operand, expected_one);
                            return 1;
                        }
                        for (leaf_i, (_, leaf_ty, offset)) in leaves.iter().enumerate() {
                            let leaf_expected = expected_params.and_then(|params| params.get(expected_index + leaf_i));
                            let effective_field_ty = effective_jvm_type(&self.ctx, leaf_ty);
                            if leaf_expected.is_some_and(jvm_descriptor_is_reference) && !is_jvm_stack_reference(&effective_field_ty) {
                                self.instructions.push(JvmInstruction::AConstNull);
                                if let Some(desc) = leaf_expected {
                                    self.emit_checkcast_for_descriptor(desc);
                                }
                                continue;
                            }
                            self.emit_load_local(local + offset, &effective_field_ty);
                            self.emit_argument_checkcast(leaf_ty);
                            self.emit_widen_int_to_long_if_needed(operand, leaf_expected);
                        }
                        return leaves.len().max(1);
                    }
                }
                // 非值类型的 boxed 引用（或单字段）仍传一个引用。
                if self.boxed_value_refs.contains(value) {
                    self.emit_load_local(local, &ty);
                    self.emit_argument_checkcast(&ty);
                    self.emit_widen_int_to_long_if_needed(operand, expected_one);
                    return 1;
                }
                let effective_ty = effective_jvm_type(&self.ctx, &ty);
                self.emit_load_local(local, &effective_ty);
                self.emit_argument_checkcast(&ty);
                self.emit_widen_int_to_long_if_needed(operand, expected_one);
                return 1;
            }
        }
        self.emit_operand(operand);
        let ty = self.infer_field_type(operand);
        self.emit_argument_checkcast(&ty);
        self.emit_widen_int_to_long_if_needed(operand, expected_one);
        1
    }

    /// 在调用参数压栈后按声明类型发射 `checkcast`，收窄 stub 返回的宽引用。
    fn emit_argument_checkcast(&mut self, declared_ty: &NyarType) {
        self.emit_field_store_checkcast(declared_ty);
    }

    /// Callee 参数为 `J`（long）而栈上是 `int`（u32/`usize`/`as i64` 未拓宽）时插 `i2l`。
    /// 否则 `render_i64_text:(J)` 等会 VerifyError: Expecting to find long on stack。
    fn emit_widen_int_to_long_if_needed(&mut self, operand: &MirOperand, expected: Option<&JvmTypeDescriptor>) {
        // The inverse mismatch also occurs after late ABI widening: a long
        // producer may feed a callee slot declared as JVM int. Narrow it before
        // the invocation so the verifier sees the descriptor's stack category.
        if matches!(expected, Some(JvmTypeDescriptor::Int))
            && self.instructions.last().is_some_and(|instruction| {
                matches!(
                    instruction,
                    JvmInstruction::LLoad(_)
                        | JvmInstruction::LConst0
                        | JvmInstruction::LConst1
                        | JvmInstruction::LdcLong(_)
                        | JvmInstruction::LAdd
                        | JvmInstruction::LSub
                        | JvmInstruction::LMul
                        | JvmInstruction::LDiv
                        | JvmInstruction::LRem
                        | JvmInstruction::LNeg
                )
            })
        {
            self.instructions.push(JvmInstruction::L2I);
            return;
        }
        if !matches!(expected, Some(JvmTypeDescriptor::Long)) {
            return;
        }
        // The operand may already be a long on the JVM stack even when its MIR
        // value/local metadata was widened late. Never emit I2L after a long
        // producer: the verifier rejects it with "Expecting integer on stack".
        if self.instructions.last().is_some_and(|instruction| {
            matches!(
                instruction,
                JvmInstruction::LLoad(_)
                    | JvmInstruction::LConst0
                    | JvmInstruction::LConst1
                    | JvmInstruction::LdcLong(_)
                    | JvmInstruction::LAdd
                    | JvmInstruction::LSub
                    | JvmInstruction::LMul
                    | JvmInstruction::LDiv
                    | JvmInstruction::LRem
                    | JvmInstruction::LNeg
                    | JvmInstruction::I2L
            )
        }) {
            return;
        }
        let stack_is_int = self.instructions.last().is_some_and(|instruction| {
            matches!(
                instruction,
                JvmInstruction::IConst(_)
                    | JvmInstruction::ILoad(_)
                    | JvmInstruction::IAdd
                    | JvmInstruction::ISub
                    | JvmInstruction::IMul
                    | JvmInstruction::IDiv
                    | JvmInstruction::IRem
                    | JvmInstruction::I2L
            )
        });
        if stack_is_int || !self.operand_is_jvm_long(operand) {
            self.instructions.push(JvmInstruction::I2L);
        }
    }

    /// 操作数当前是否已在 JVM 栈/local 上以 `long` 表示。
    fn operand_is_jvm_long(&self, operand: &MirOperand) -> bool {
        match operand {
            MirOperand::Value(v) => {
                if let Some(local) = self.slots.value_locals.get(v).copied() {
                    if matches!(self.local_kinds.get(&local).copied(), Some(JvmLocalKind::Long)) {
                        return true;
                    }
                }
                matches!(effective_jvm_type(&self.ctx, &self.lookup_value_type(v)), NyarType::Integer64 { .. })
            }
            MirOperand::Constant(MirConstant::Int(_)) => false,
            _ => matches!(effective_jvm_type(&self.ctx, &self.infer_field_type(operand)), NyarType::Integer64 { .. }),
        }
    }

    /// 压入 long：已是 long 则直接发射；int 常量/值则 `i2l`（供 `lcmp`/`ladd` 等）。
    fn emit_operand_as_long(&mut self, operand: &MirOperand) {
        if let MirOperand::Constant(MirConstant::Int(value)) = operand {
            self.instructions.push(JvmInstruction::LdcLong(*value));
            return;
        }
        self.emit_operand(operand);
        // The inferred MIR type can say `Integer64` even when emission falls
        // back to an integer zero (for example an unresolved symbol or unit
        // operand).  In that case trusting `operand_is_jvm_long` leaves an
        // `iconst_0` directly before `ldiv`/`lmul`, which the JVM verifier
        // rejects because those instructions require two longs.  A value is
        // safe to leave unconverted only when its planned local is explicitly
        // long, or a symbol resolves to an explicitly long local.
        let known_long = match operand {
            MirOperand::Value(value) => {
                self.slots.value_locals.get(value).and_then(|local| self.local_kinds.get(local)).is_some_and(|kind| *kind == JvmLocalKind::Long)
            }
            MirOperand::Symbol(path) => self
                .slots
                .var_locals
                .get(&path.to_string())
                .and_then(|local| self.local_kinds.get(local))
                .is_some_and(|kind| *kind == JvmLocalKind::Long),
            _ => false,
        };
        if !known_long {
            self.instructions.push(JvmInstruction::I2L);
        }
    }

    fn emit_checkcast_for_descriptor(&mut self, desc: &JvmTypeDescriptor) {
        let needs_cast = !matches!(desc, JvmTypeDescriptor::Object(name) if name == "java/lang/Object");
        if !needs_cast {
            return;
        }
        let class_name = match desc {
            JvmTypeDescriptor::Object(name) => name.clone(),
            JvmTypeDescriptor::Array(_) => desc.to_string(),
            _ => return,
        };
        self.instructions.push(JvmInstruction::CheckCast(class_name));
    }

    fn implicit_member_receiver(&self, path: &nyar::NamePath) -> Option<(Vec<JvmInstruction>, NyarType)> {
        if path.parts().len() < 2 {
            return None;
        }
        let root_name = path.parts().first()?.as_str();
        let parameter = self.mir_fn.values.iter().find_map(|value| match &value.origin {
            MirValueOrigin::Parameter { name, .. } if name == root_name => Some(value.id),
            _ => None,
        });
        let (mut local, mut ty, mut on_stack) = if let Some(value) = parameter {
            (self.slots.value_locals.get(&value).copied()?, self.lookup_value_type(&value), self.boxed_value_refs.contains(&value))
        }
        else {
            (self.slots.var_locals.get(root_name).copied()?, self.var_types.get(root_name).cloned()?, false)
        };
        let mut instructions = Vec::new();
        if on_stack {
            instructions.push(jvm_load_local_instruction(local, &ty));
        }
        for field in &path.parts()[1..path.parts().len() - 1] {
            let NyarType::Named(owner) = &ty
            else {
                return None;
            };
            let owner_name = owner.as_str().to_string();
            let field_ty = self.ctx.field_type(None, &owner_name, field.as_str())?;
            if !on_stack && self.ctx.is_value_type_name(&owner_name) {
                local = local.saturating_add(self.ctx.field_slot_index(None, &owner_name, field.as_str()));
                ty = field_ty;
                continue;
            }
            if !on_stack {
                instructions.push(jvm_load_local_instruction(local, &effective_jvm_type(&self.ctx, &ty)));
                on_stack = true;
            }
            instructions.push(JvmInstruction::GetField(JvmFieldRef {
                owner: owner_name.replace('.', "/"),
                name: field.as_str().to_string(),
                descriptor: self.jvm_field_descriptor_for_class(&field_ty),
            }));
            ty = field_ty;
        }
        if !on_stack {
            instructions.push(jvm_load_local_instruction(local, &effective_jvm_type(&self.ctx, &ty)));
        }
        Some((instructions, ty))
    }

    /// 拷贝值类型的所有字段从一个 local 范围到另一个。
    ///
    /// 用于 `Copy` 指令和 block 参数拷贝，确保值类型的所有字段
    /// 都被正确复制，而非仅复制第一个字段。遍历 `flatten_value_type_slots`
    /// 返回的扁平槽位列表，递归复制嵌套值类型的所有字段，避免仅复制直接字段
    /// 首槽位导致后续 FieldGet 读取未初始化寄存器触发 VerifyError。
    /// Returns the destination aggregate base after any local-kind relocation.
    ///
    /// A field store can move the complete aggregate range when one leaf collides
    /// with a different JVM verifier category. Keep using that new base for the
    /// remaining leaves; otherwise the next `base + offset` writes back into the
    /// abandoned range and recreates the conflict.
    fn copy_value_type_fields(&mut self, source_local: u16, dest_local: u16, type_name: &str) -> u16 {
        let mut leaves = Vec::new();
        self.collect_value_type_leaf_paths(type_name, Vec::new(), 0, &mut leaves);
        let mut actual_dest = dest_local;
        for (_, field_ty, offset) in leaves {
            let effective_field_ty = effective_jvm_type(&self.ctx, &field_ty);
            self.emit_load_local(source_local.saturating_add(offset), &effective_field_ty);
            let actual_field = self.emit_store_local(actual_dest.saturating_add(offset), &effective_field_ty);
            actual_dest = actual_field.saturating_sub(offset);
        }
        actual_dest
    }

    /// Unbox every flattened leaf and return the destination base after relocation.
    fn emit_unbox_value_type_to_locals(&mut self, source_local: u16, ty: &NyarType, dest_local: u16) -> Option<u16> {
        let NyarType::Named(type_name) = ty
        else {
            return None;
        };
        if !self.ctx.is_value_type_name(type_name.as_str()) {
            return None;
        }
        let mut leaves = Vec::new();
        self.collect_value_type_leaf_paths(type_name.as_str(), Vec::new(), 0, &mut leaves);
        if leaves.is_empty() {
            return None;
        }
        let mut actual_dest = dest_local;
        for (path, leaf_ty, offset) in leaves {
            self.instructions.push(JvmInstruction::ALoad(source_local));
            for field in path {
                self.instructions.push(JvmInstruction::GetField(field));
            }
            let effective_ty = effective_jvm_type(&self.ctx, &leaf_ty);
            let actual_field = self.emit_store_local(actual_dest.saturating_add(offset), &effective_ty);
            actual_dest = actual_field.saturating_sub(offset);
        }
        Some(actual_dest)
    }

    fn collect_value_type_leaf_paths(
        &self,
        type_name: &str,
        prefix: Vec<JvmFieldRef>,
        base_offset: u16,
        leaves: &mut Vec<(Vec<JvmFieldRef>, NyarType, u16)>,
    ) {
        let Some(layout) = self.ctx.layout_by_type_name(type_name)
        else {
            return;
        };
        let mut field_offset = 0u16;
        for field in &layout.fields {
            let mut path = prefix.clone();
            path.push(JvmFieldRef {
                owner: type_name.replace('.', "/"),
                name: field.name.clone(),
                descriptor: self.jvm_field_descriptor_for_class(&field.ty),
            });
            // Keep offsets aligned with `flatten_value_type_slots`: field ordinal
            // is insufficient when a preceding value-type field expands to more
            // than one JVM local.  Accumulate the canonical flattened width.
            let offset = base_offset.saturating_add(field_offset);
            // Must stay aligned with [`ExecutableLoweringContext::flatten_value_type_slots`]:
            // - `usize`/`isize`/… are empty `structure` aliases **and** value_type_names;
            //   flatten keeps them as one leaf, but naive recurse into 0 fields drops them.
            // - That mismatch made `Fine(VonParsedValue{value, next_index: usize})` invent
            //   `Fine:(II)I` while only pushing one int →
            //   VerifyError: Expecting to find integer on stack (parse_von_value).
            // - Same for `Fail(VonDiagnostic{message, start: usize, stop: usize})` →
            //   `Fail:(Ljava/lang/String;II)I` with only String on the stack.
            match &field.ty {
                NyarType::Named(nested)
                    if map_primitive_name_to_jvm(nested.as_str()).is_none() && self.ctx.is_value_type_name(nested.as_str()) =>
                {
                    let before = leaves.len();
                    self.collect_value_type_leaf_paths(nested.as_str(), path.clone(), offset, leaves);
                    if leaves.len() == before {
                        leaves.push((path, field.ty.clone(), offset));
                    }
                }
                _ => leaves.push((path, field.ty.clone(), offset)),
            }
            field_offset = field_offset.saturating_add(self.ctx.jvm_flattened_slot_width(&field.ty));
        }
    }

    fn emit_load_local(&mut self, local: u16, ty: &NyarType) {
        let want = jvm_local_kind_of(ty);
        if let Some(have) = self.local_kinds.get(&local).copied() {
            if have != want {
                // Slot was written with a different JVM category (e.g. utf8/Object
                // astore, later read as int-handle / bool). Mismatched iload/aload
                // yields VerifyError: Register N contains wrong type. Push a
                // typed default instead of a cross-category load.
                match want {
                    JvmLocalKind::Reference => self.instructions.push(JvmInstruction::AConstNull),
                    JvmLocalKind::Long => self.instructions.push(JvmInstruction::LConst0),
                    JvmLocalKind::Double => self.instructions.push(JvmInstruction::DConst0),
                    JvmLocalKind::Float => self.instructions.push(JvmInstruction::FConst0),
                    JvmLocalKind::Int => self.instructions.push(JvmInstruction::IConst(0)),
                }
                return;
            }
        }
        self.instructions.push(jvm_load_local_instruction(local, ty));
    }

    /// Store stack top into `local`, reallocating when the slot's JVM category conflicts
    /// (int-handle vs reference). Returns the slot actually written.
    fn emit_store_local(&mut self, local: u16, ty: &NyarType) -> u16 {
        let kind = jvm_local_kind_of(ty);
        let slot = self.realloc_local_if_kind_conflict(local, kind, ty);
        // A value-level integer literal/operation can flow into a widened
        // Integer64 local without passing through the call-argument coercion
        // path.  JVM `lstore` requires a category-2 long on the operand
        // stack, so normalize the common category-1 producers at the store
        // boundary as well.
        if matches!(kind, JvmLocalKind::Long)
            && self.instructions.last().is_some_and(|instruction| {
                matches!(
                    instruction,
                    JvmInstruction::IConst(_)
                        | JvmInstruction::ILoad(_)
                        | JvmInstruction::IAdd
                        | JvmInstruction::ISub
                        | JvmInstruction::IMul
                        | JvmInstruction::IDiv
                        | JvmInstruction::IRem
                        | JvmInstruction::INeg
                )
            })
        {
            self.instructions.push(JvmInstruction::I2L);
        }
        self.instructions.push(match kind {
            JvmLocalKind::Double => JvmInstruction::DStore(slot),
            JvmLocalKind::Float => JvmInstruction::FStore(slot),
            JvmLocalKind::Long => JvmInstruction::LStore(slot),
            JvmLocalKind::Int => JvmInstruction::IStore(slot),
            // Unit 在栈上以 int 0 表示（见 `MirConstant::Unit` / load）；写入 local 用
            // `istore`，禁止 `pop`——当上游未压栈时 `pop` 会直接 VerifyError。
            JvmLocalKind::Reference => JvmInstruction::AStore(slot),
        });
        slot
    }

    fn store_to_value(&mut self, value: MirValueRef) {
        if let Some(local) = self.slots.value_locals.get(&value).copied() {
            let ty = self.lookup_value_type(&value);
            let effective_ty = if self.boxed_value_refs.contains(&value) { ty.clone() } else { effective_jvm_type(&self.ctx, &ty) };
            let actual = self.emit_store_local(local, &effective_ty);
            if actual != local {
                self.slots.value_locals.insert(value, actual);
            }
            // 与 StoreVar 对齐：写入后清掉仍指向同一槽、但 JVM 类别不同的其它 SSA。
            // 否则 FieldGet 把 int 写入曾承载 boxed 引用的槽后，后续仍按引用
            // `aload`/`checkcast`，触发 `Register N contains wrong type` /
            // `Expecting to find object/array on stack`。
            let written = self.slots.value_locals.get(&value).copied().unwrap_or(actual);
            let store_kind = jvm_local_kind_of(&effective_ty);
            let dead: Vec<MirValueRef> = self
                .slots
                .value_locals
                .iter()
                .filter(|(other, slot)| **slot == written && **other != value)
                .filter(|(other, _)| {
                    let other_ty = self.lookup_value_type(other);
                    let other_eff = if self.boxed_value_refs.contains(other) { other_ty } else { effective_jvm_type(&self.ctx, &other_ty) };
                    jvm_local_kind_of(&other_eff) != store_kind
                })
                .map(|(other, _)| *other)
                .collect();
            for other in dead {
                self.slots.value_locals.remove(&other);
            }
        }
    }

    /// Queries the source type of an SSA value, optionally adjusted only for
    /// a backend representation that an explicit intrinsic contract selected.
    fn lookup_value_type(&self, value: &MirValueRef) -> NyarType {
        let raw =
            self.value_type_overrides.get(value).cloned().or_else(|| self.mir_fn.value_types.get(value).cloned()).unwrap_or(NyarType::Unit);
        concretize_self_type(&raw, self.self_owner.as_deref())
    }

    fn operand_local(&self, operand: &MirOperand) -> Option<u16> {
        match operand {
            MirOperand::Value(value) => self.slots.value_locals.get(value).copied(),
            MirOperand::Symbol(path) => self.slots.var_locals.get(&path.to_string()).copied(),
            _ => None,
        }
    }

    fn infer_aggregate_name(&self, operand: &MirOperand) -> Option<String> {
        match operand {
            MirOperand::Value(value) => match self.lookup_value_type(value) {
                NyarType::Named(name) => Some(name.to_string()),
                NyarType::Utf8 => Some("Utf8Text".to_string()),
                NyarType::Utf16 => Some("Utf16Text".to_string()),
                _ => None,
            },
            _ => None,
        }
    }

    /// JVM 宿主：`utf8` / `Utf8Text` 擦成 `java.lang.String`（语义仍是 UTF-8 字节存储契约）。
    fn is_host_utf8_text_operand(&self, operand: &MirOperand) -> bool {
        matches!(self.infer_field_type(operand), NyarType::Utf8)
    }

    /// JVM 宿主：`utf16` / `Utf16Text` 擦成 `java.lang.String`（UTF-16 code units）。
    fn is_host_utf16_text_operand(&self, operand: &MirOperand) -> bool {
        match self.infer_field_type(operand) {
            NyarType::Utf16 => true,
            _ => false,
        }
    }

    /// `utf8._repr` → `String.getBytes("UTF-8")` → `[B`（禁止落到 `Utf16Text._repr`）。
    fn emit_host_utf8_repr(&mut self, object: &MirOperand, output: Option<MirValueRef>) {
        self.emit_operand(object);
        self.instructions.push(JvmInstruction::CheckCast("java/lang/String".to_string()));
        self.instructions.push(JvmInstruction::LdcString("UTF-8".to_string()));
        self.instructions.push(JvmInstruction::InvokeVirtual(JvmMethodRef {
            owner: "java/lang/String".to_string(),
            name: "getBytes".to_string(),
            descriptor: JvmMethodDescriptor::new(
                vec![JvmTypeDescriptor::Object("java/lang/String".to_string())],
                JvmTypeDescriptor::array(JvmTypeDescriptor::Byte),
            ),
        }));
        if let Some(output) = output {
            let array_ty = NyarType::Array(Box::new(NyarType::Integer8 { signed: false }));
            self.value_type_overrides.insert(output, array_ty.clone());
            self.boxed_value_refs.insert(output);
            if let Some(local) = self.slots.value_locals.get(&output).copied() {
                let actual = self.emit_store_local(local, &array_ty);
                if actual != local {
                    self.slots.value_locals.insert(output, actual);
                }
            }
            else {
                self.store_to_value(output);
            }
        }
    }

    /// `utf16._repr` → `String.toCharArray()` → `[C`（code units；禁止与 utf8 字节混用）。
    fn emit_host_utf16_repr(&mut self, object: &MirOperand, output: Option<MirValueRef>) {
        self.emit_operand(object);
        self.instructions.push(JvmInstruction::CheckCast("java/lang/String".to_string()));
        self.instructions.push(JvmInstruction::InvokeVirtual(JvmMethodRef {
            owner: "java/lang/String".to_string(),
            name: "toCharArray".to_string(),
            descriptor: JvmMethodDescriptor::new(vec![], JvmTypeDescriptor::array(JvmTypeDescriptor::Char)),
        }));
        if let Some(output) = output {
            let array_ty = NyarType::Array(Box::new(NyarType::Character));
            self.value_type_overrides.insert(output, array_ty.clone());
            self.boxed_value_refs.insert(output);
            if let Some(local) = self.slots.value_locals.get(&output).copied() {
                let actual = self.emit_store_local(local, &array_ty);
                if actual != local {
                    self.slots.value_locals.insert(output, actual);
                }
            }
            else {
                self.store_to_value(output);
            }
        }
    }

    /// 判断操作数是否使用内联值类型字段存储（连续 local 槽位）。
    ///
    /// 仅当操作数是未被装箱的 SSA 值引用，且其类型已注册为值类型时返回 `true`。
    /// boxed 值类型（堆对象引用）和未注册为值类型的 Named 对象返回 `false`，
    /// 它们必须通过 `aload` + `getfield` / `putfield` 访问字段。
    fn uses_inline_value_fields(&self, operand: &MirOperand) -> bool {
        match operand {
            MirOperand::Value(v) => {
                if self.boxed_value_refs.contains(v) {
                    return false;
                }
                let ty = self.lookup_value_type(v);
                if let NyarType::Named(name) = &ty { self.ctx.is_value_type_name(name.as_str()) } else { false }
            }
            _ => false,
        }
    }

    /// 判断 FieldGet `object.payload` 是否是对 unite sum type 的 payload 提取。
    ///
    /// Unite sum type（如 `VonParseResult<T>`/`Option<T>`/`Result<T,E>`）在 JVM 后端用
    /// int 句柄表示，不能用 `getfield`，必须改走 `tuple_get_0` runtime stub。
    /// 此方法检测操作数的类型是否为已注册的 unite sum type。
    /// 注意：泛型实例化后类型为 `NyarType::Apply(Base, Args)`，需要剥到 base
    /// `NyarType::Named` 再按简单名匹配 sum_types，否则 `VonParseResult<T>`
    /// 会因 Apply 分支被跳过，导致 FieldGet 走 `getfield` 路径在 int 句柄上
    /// 触发 VerifyError: "Expecting to find object/array on stack"。
    ///
    /// 必须查询 HIR 注册的语义类型（`mir_fn.value_types`），而不是
    /// [`lookup_value_type`] 返回的 JVM 表示类型。`emit_call_invoke` 会把
    /// sum type 返回值通过 `effective_jvm_type` 折叠为 `Integer32` 写入
    /// `value_type_overrides`，用于指导 `store_to_value`/`emit_operand` 使用
    /// `istore`/`iload`。但 `is_unite_payload_field_get` 关心的是语义类型
    /// （是否为 sum type），若使用 override 得到的 `Integer32`，会跳过
    /// `tuple_get_0` 路径，直接在 int 句柄上发射 `getfield`，触发
    /// VerifyError: "Expecting to find object/array on stack"。
    fn is_unite_payload_field_get(&self, object: &MirOperand) -> bool {
        self.sum_type_name_of_operand(object).is_some_and(|name| self.ctx.find_unite_sum_type(&name).is_some())
    }

    /// 判断操作数语义类型是否为任意 sum type（`enums` 或 `unite`）。
    ///
    /// JVM ABI 用 int 句柄表示二者；`FieldGet { field: "tag" }` 必须是恒等
    /// （句柄即 tag），不能 `getfield`。
    fn is_sum_type_operand(&self, object: &MirOperand) -> bool {
        self.sum_type_name_of_operand(object).is_some_and(|name| self.ctx.find_sum_type(&name).is_some())
    }

    /// 从 SSA 操作数取 sum type 简单名（支持 `Apply` 剥基类）。
    ///
    /// 语义类型来源（按优先级）：
    /// 1. `mir_fn.value_types`（HIR 注册的 Named/Apply）
    /// 2. `value_type_overrides` 中仍为 Named/Apply 的条目——
    ///    [`seed_parameter_value_types`] 把参数语义类型写在这里；若只查
    ///    `value_types`，参数上的 `FieldGet tag` 会漏判，落到 `iload`+`getfield`
    ///    触发 VerifyError: "Expecting to find object/array on stack"
    ///    （如 `emit_wasm_artifact_from_mir` 的 `WasmHostBoundary` 参数）。
    ///    跳过已折叠为 `Integer32` 的 override（`emit_call_invoke` 写入的 ABI 表示）。
    /// 3. `Parameter` origin → `param_types`
    fn sum_type_name_of_operand(&self, object: &MirOperand) -> Option<String> {
        let MirOperand::Value(value) = object
        else {
            return None;
        };
        if let Some(name) = self.mir_fn.value_types.get(value).and_then(Self::sum_type_base_name) {
            return Some(name);
        }
        if let Some(name) = self.value_type_overrides.get(value).and_then(Self::sum_type_base_name) {
            return Some(name);
        }
        if let Some(name) = self.semantic_unite_value_types.get(value) {
            return Some(name.clone());
        }
        let param_index = self.mir_fn.values.iter().find_map(|entry| match &entry.origin {
            MirValueOrigin::Parameter { index, .. } if entry.id == *value => Some(*index),
            _ => None,
        })?;
        self.mir_fn.param_types.get(param_index).and_then(Self::sum_type_base_name)
    }

    fn sum_type_base_name(ty: &NyarType) -> Option<String> {
        match ty {
            NyarType::Named(name) => Some(name.to_string()),
            NyarType::Apply(base, _) => match base.as_ref() {
                NyarType::Named(name) => Some(name.to_string()),
                _ => None,
            },
            _ => None,
        }
    }

    fn remember_semantic_unite_type(&mut self, value: MirValueRef, ty: &NyarType) {
        let Some(name) = Self::sum_type_base_name(ty)
        else {
            return;
        };
        if self.ctx.find_unite_sum_type(&name).is_some() {
            self.semantic_unite_value_types.insert(value, name);
        }
    }

    /// 判断给定类型是否为 unite sum type。
    ///
    /// Unite sum type（如 `VonValue`/`Option<T>`/`Result<T,E>`）在 JVM 后端用 int
    /// 句柄表示。此方法与 [`is_unite_payload_field_get`] 的判断逻辑对齐，
    /// 但直接接受 [`NyarType`] 参数，用于字段描述符生成等非 SSA 值场景。
    fn is_unite_sum_type(&self, ty: &NyarType) -> bool {
        let name = match ty {
            NyarType::Named(name) => name.as_str(),
            NyarType::Apply(base, _) => match base.as_ref() {
                NyarType::Named(name) => name.as_str(),
                _ => return false,
            },
            _ => return false,
        };
        self.ctx.find_unite_sum_type(name).is_some()
    }

    /// 返回字段在 JVM 类文件中的实际描述符，与 `build_jvm_placeholder_class` 保持一致。
    ///
    /// 对于 sum type 字段（`enums` 如 `WasmOpcode`/`WitWasiCoreResultKind`，或 `unite`
    /// 如 `VonValue`/`Option`/`Result`），JVM 后端用 int 句柄表示，类文件中字段
    /// 描述符为 `I`（见 [`build_jvm_placeholder_class`]）。
    /// 此方法确保 `getfield`/`putfield` 指令引用的字段描述符与类文件中的字段定义
    /// 匹配，避免 `getfield` 返回对象引用但 `istore` 期望 int，触发
    /// VerifyError: "Expecting to find integer on stack"；或反过来
    /// `iconst` + `putfield LName;` / `areturn` 触发
    /// "Expecting to find object/array on stack"。
    fn jvm_field_descriptor_for_class(&self, ty: &NyarType) -> JvmTypeDescriptor {
        match ty {
            NyarType::Named(name) if self.ctx.find_sum_type(name.as_str()).is_some() => JvmTypeDescriptor::Int,
            NyarType::Apply(base, _) => match base.as_ref() {
                NyarType::Named(name) if self.ctx.find_sum_type(name.as_str()).is_some() => JvmTypeDescriptor::Int,
                _ => jvm_field_descriptor(ty),
            },
            // 结构体字段 `[Unite]` 必须与方法参数 ABI 一致折叠为 `[I`。
            // 否则 `WitInterfaceDef.types` 存成 `[LWitTypeDef;`，而
            // `wit_render_interface` 形参是 `[I`，getfield 后调用触发
            // VerifyError: "Incompatible argument to function"。
            NyarType::Array(element) => {
                let effective_elem = effective_array_element_type(&self.ctx, element);
                jvm_field_descriptor(&NyarType::Array(Box::new(effective_elem)))
            }
            NyarType::FixedArray { element, length } => {
                let effective_elem = effective_array_element_type(&self.ctx, element);
                jvm_field_descriptor(&NyarType::FixedArray { element: Box::new(effective_elem), length: length.clone() })
            }
            _ => jvm_field_descriptor(ty),
        }
    }

    fn infer_field_type(&self, operand: &MirOperand) -> NyarType {
        match operand {
            MirOperand::Value(value) => self.lookup_value_type(value),
            MirOperand::Constant(MirConstant::Float64(_)) => NyarType::Float64,
            MirOperand::Constant(MirConstant::Int(_)) => NyarType::Integer32 { signed: true },
            MirOperand::Constant(MirConstant::Bool(_)) => NyarType::Boolean,
            MirOperand::Constant(MirConstant::Utf8(_)) => NyarType::Utf8,
            MirOperand::Constant(MirConstant::Utf16(_)) => NyarType::Utf16,
            MirOperand::Constant(MirConstant::Unit) => NyarType::Unit,
            _ => NyarType::Unit,
        }
    }

    /// 供算术/比较 intrinsic 使用的操作数类型：优先 HIR/value_types，若 local
    /// 已按数值 kind 写入则回退为对应原始类型（修复 unite payload / tuple_get
    /// 抽出的 `u16` 被标成非原始 Named 后误走 `Utf8Text::infix +` 的问题）。
    fn infer_numeric_operand_type(&self, operand: &MirOperand) -> NyarType {
        let ty = self.infer_field_type(operand);
        if nyar_type_is_numeric(&ty) {
            return ty;
        }
        if let NyarType::Named(name) = &ty {
            if let Some(primitive) = map_primitive_name_to_jvm(name.as_str()) {
                return primitive;
            }
        }
        if let Some(local) = self.operand_local(operand) {
            match self.local_kinds.get(&local).copied() {
                Some(JvmLocalKind::Int) => return NyarType::Integer32 { signed: true },
                Some(JvmLocalKind::Long) => return NyarType::Integer64 { signed: true },
                Some(JvmLocalKind::Float) => return NyarType::Float32,
                Some(JvmLocalKind::Double) => return NyarType::Float64,
                _ => {}
            }
        }
        ty
    }

    /// 在 `putfield` 前按字段声明类型发射 `checkcast` 收窄栈上值类型。
    ///
    /// 当值来源于 runtime stub（如 `unwrap` 返回 `Ljava/lang/Object;`）但
    /// 字段声明为具体引用类型（如 `LLegionPublishTarget;`）时，JVM 验证器
    /// 要求栈上类型与 `putfield` 的字段描述符匹配。此方法在字段为引用类型
    /// 且非 `java/lang/Object` 时发射 `checkcast`，将宽类型收窄为字段声明的
    /// 具体类型，与 `areturn` 路径的 checkcast 模式（行 854-867）对齐。
    fn emit_field_store_checkcast(&mut self, field_ty: &NyarType) {
        // 使用 effective 类型判断栈上值是否为引用。Named 值类型可能折叠为
        // 原始类型（如单 Boolean 字段的值类型折叠为 int），此时栈上是 int
        // 而非对象引用，发射 `checkcast` 会触发 VerifyError:
        // "Expecting to find object/array on stack"。
        let effective_ty = effective_jvm_type(&self.ctx, field_ty);
        // Sum/unite/`Result` → Integer32：禁止 checkcast（会 Expecting object/array）。
        if !is_jvm_stack_reference(&effective_ty) {
            return;
        }
        // 描述符必须与类字段 / 方法参数 ABI 一致：`[WitTypeDef]`（unite）→ `[I`。
        // 若仍用名义 `jvm_field_descriptor` 发 `checkcast [LWitTypeDef;`，而
        // getfield 已压 `[I`，会触发 VerifyError: "Incompatible argument to function"。
        let desc = self.jvm_field_descriptor_for_class(field_ty);
        let needs_cast = !matches!(&desc, JvmTypeDescriptor::Object(name) if name == "java/lang/Object");
        if !needs_cast {
            return;
        }
        // 原始数组（`[I`/`[J`/…）上 checkcast 多余且部分校验器更挑剔；跳过。
        if matches!(
            &desc,
            JvmTypeDescriptor::Array(inner)
                if matches!(
                    inner.as_ref(),
                    JvmTypeDescriptor::Int
                        | JvmTypeDescriptor::Long
                        | JvmTypeDescriptor::Float
                        | JvmTypeDescriptor::Double
                        | JvmTypeDescriptor::Byte
                        | JvmTypeDescriptor::Short
                        | JvmTypeDescriptor::Char
                        | JvmTypeDescriptor::Boolean
                )
        ) {
            return;
        }
        let class_name = match &desc {
            JvmTypeDescriptor::Object(name) => name.clone(),
            _ => desc.to_string(),
        };
        self.instructions.push(JvmInstruction::CheckCast(class_name));
    }

    /// 发出调用指令，按 `dispatch` 选择 `invokestatic`/`invokevirtual`。
    ///
    /// Witness 派发通过 witness 表解析实现符号，使用 `invokevirtual` 调用具体实现；
    /// Static 派发解析静态调用符号，使用 `invokestatic`。
    fn emit_call_invoke(
        &mut self,
        path: &nyar::NamePath,
        dispatch: MirDispatchKind,
        witness: Option<&MirOperand>,
        arguments: &[MirOperand],
        output: Option<MirValueRef>,
    ) {
        let method_name = path.parts().last().map(|part| part.as_str()).unwrap_or_default();
        let owner = self.current_class_owner();
        let (method_ref, is_void_return) = match dispatch {
            MirDispatchKind::Witness => {
                let receiver_type = self.receiver_type_for_witness(witness, arguments.first());
                let trait_name = receiver_type.as_ref().and_then(witness_trait_name);
                let impl_name = trait_name
                    .as_ref()
                    .and_then(|trait_name| self.ctx.witness_impl_symbol(trait_name, method_name))
                    .unwrap_or_else(|| sanitize_symbol(method_name));
                let descriptor = self.resolve_witness_call_descriptor(method_name, witness, arguments).unwrap_or_else(|| {
                    // No table slot: refuse name-sniffed ABI; fall back to (Object)Object.
                    let object = JvmTypeDescriptor::Object("java/lang/Object".to_string());
                    JvmMethodDescriptor::new(vec![object.clone()], object)
                });
                let is_void = matches!(descriptor.return_type, JvmTypeDescriptor::Void);
                let method_ref = JvmMethodRef { owner, name: impl_name, descriptor };
                (method_ref, is_void)
            }
            MirDispatchKind::Static | MirDispatchKind::EffectHandler | MirDispatchKind::Indirect => {
                let resolved = self.resolve_static_call_symbol(path);
                let stub_descriptor = runtime_stub_descriptor(path);
                let concrete_output_type = output.and_then(|value| self.mir_fn.value_types.get(&value).cloned());
                if let (Some(output), Some(output_ty)) = (output, concrete_output_type.as_ref()) {
                    self.remember_semantic_unite_type(output, output_ty);
                    if needs_boxing(&self.ctx, output_ty) {
                        self.value_type_overrides.insert(output, output_ty.clone());
                        self.boxed_value_refs.insert(output);
                    }
                }
                // runtime stub：仅对原始类型返回值（Int/Boolean 等）设置 override，
                // 确保 store_to_value 用 IStore 而非 AStore。对 Object 返回值不设
                // override，保留 HIR resolver 注册的具体类型（如 LegionPublishTarget[]），
                // 避免 areturn 时栈上 Object 与方法声明返回类型不匹配。
                if let Some(descriptor) = &stub_descriptor {
                    if matches!(
                        descriptor.return_type,
                        JvmTypeDescriptor::Int
                            | JvmTypeDescriptor::Boolean
                            | JvmTypeDescriptor::Long
                            | JvmTypeDescriptor::Float
                            | JvmTypeDescriptor::Double
                            | JvmTypeDescriptor::Byte
                            | JvmTypeDescriptor::Short
                            | JvmTypeDescriptor::Char
                    ) {
                        if let Some(output) = output {
                            let stub_return_type = nyar_type_from_jvm_descriptor(&descriptor.return_type);
                            self.value_type_overrides.insert(output, stub_return_type);
                        }
                    }
                }
                else {
                    if let Some(output) = output {
                        // Non-intrinsic calls use only the call result's explicit
                        // Semantic MIR SSA type. The backend may choose a physical
                        // representation (including boxing), but never obtains a
                        // source type by searching a callee name or descriptor.
                        let hir_type = self.lookup_value_type(&output);
                        if needs_boxing(&self.ctx, &hir_type) || is_structure_value_type(&self.ctx, &hir_type) {
                            self.value_type_overrides.insert(output, hir_type.clone());
                            self.boxed_value_refs.insert(output);
                        }
                        else {
                            let effective_return_type = effective_jvm_type(&self.ctx, &hir_type);
                            self.value_type_overrides.insert(output, effective_return_type);
                        }
                    }
                }
                // stub 命中时用简单名 + 非展开推断描述符。
                //
                // Call 处理器对 stub 调用通过 `emit_operand` 每个参数压入 1 个有效值
                // （值类型取首字段 effective 类型），因此描述符参数数必须与压栈数一致。
                // 不能使用 `infer_call_descriptor`：它通过 `jvm_type_field_types`
                // 将值类型参数展开为多个字段类型，导致描述符期望 N+k 个参数但栈上
                // 只有 N 个值，触发 VerifyError: "Unable to pop operand off an
                // empty stack"。返回类型仍通过 `lookup_value_type` 推断，
                // 与 `store_to_value` / `value_type_overrides` 保持一致。
                // stub 桩方法由 `ensure_jvm_runtime_stubs` 按同一描述符注入。
                let (method_name_for_ref, descriptor) = match &stub_descriptor {
                    Some(_) => {
                        let param_types = arguments
                            .iter()
                            .map(|arg| {
                                let ty = self.infer_field_type(arg);
                                let effective_ty = effective_jvm_type(&self.ctx, &ty);
                                let desc = jvm_parameter_descriptor(&effective_ty);
                                // stub 桩方法的参数类型不参与语义，使用 Object 统一
                                // 所有引用类型，避免声明类型与运行时实际类型不一致
                                // （如 collect_array 返回 Object[] 但声明为 String[]）
                                // 导致 VerifyError: "Incompatible argument to function"。
                                match desc {
                                    JvmTypeDescriptor::Object(_) | JvmTypeDescriptor::Array(_) => {
                                        JvmTypeDescriptor::Object("java/lang/Object".to_string())
                                    }
                                    other => other,
                                }
                            })
                            .collect::<Vec<_>>();
                        let return_type = match output {
                            Some(value) => {
                                let ty = self.lookup_value_type(&value);
                                let effective = if self.boxed_value_refs.contains(&value) { ty } else { effective_jvm_type(&self.ctx, &ty) };
                                let desc = jvm_type_descriptor(&effective);
                                match desc {
                                    JvmTypeDescriptor::Void => JvmTypeDescriptor::Int,
                                    other => other,
                                }
                            }
                            None => JvmTypeDescriptor::Void,
                        };
                        (method_name.to_string(), JvmMethodDescriptor::new(param_types, return_type))
                    }
                    None => {
                        let descriptor = self
                            .resolve_callee_jvm_descriptor(path)
                            .unwrap_or_else(|| panic!("JVM pre-emission verifier admitted a call without an exact local ABI: {path}"));
                        (resolved, descriptor)
                    }
                };
                let is_void = matches!(descriptor.return_type, JvmTypeDescriptor::Void);
                if method_name.contains("lower_executable_function_to_msil") {
                    eprintln!(
                        "[jvm-call-debug] path={} descriptor={:?} args={:?}",
                        path,
                        descriptor,
                        arguments.iter().map(|arg| self.infer_field_type(arg)).collect::<Vec<_>>()
                    );
                }
                let method_ref = JvmMethodRef { owner, name: method_name_for_ref, descriptor };
                (method_ref, is_void)
            }
        };
        self.instructions.push(match dispatch {
            MirDispatchKind::Witness => JvmInstruction::InvokeVirtual(method_ref),
            MirDispatchKind::Static | MirDispatchKind::EffectHandler | MirDispatchKind::Indirect => JvmInstruction::InvokeStatic(method_ref),
        });
        if let Some(output) = output {
            if !is_void_return {
                self.store_to_value(output);
            }
        }
        else if !is_void_return {
            self.instructions.push(JvmInstruction::Pop);
        }
    }

    /// 解析 callee 路径对应的 JVM host print 外部导入链接。
    ///
    /// Resolve only the complete external import symbol. Host metadata is a
    /// physical mapping and cannot be selected by a short name.
    fn find_jvm_host_print_link(&self, path: &nyar::NamePath) -> Option<&ExternalImportLink> {
        let qualified = QualifiedName::new(path.parts().to_vec());
        self.ctx.submission.external_import_links.get(&qualified).filter(|link| jvm_host_print_target(link).is_some())
    }

    /// 发射 JVM host print 调用（`System.out.println`）的字节码序列。
    ///
    /// 生成 `GetStatic System.out` → 参数压栈 → `InvokeVirtual PrintStream.println`
    /// 三段指令。参数类型从 MIR 操作数推断，返回类型固定为 `Void`（`println` 无返回值）。
    fn emit_host_print_call(
        &mut self,
        field_owner: String,
        field_name: String,
        stream_owner: String,
        method_name: String,
        arguments: &[MirOperand],
        receiver_kind: &Option<ReceiverPassingKind>,
    ) {
        self.instructions.push(JvmInstruction::GetStatic(JvmFieldRef {
            owner: field_owner,
            name: field_name,
            descriptor: JvmTypeDescriptor::Object(stream_owner.clone()),
        }));
        let has_by_address_receiver = matches!(receiver_kind, Some(ReceiverPassingKind::ByAddress));
        let arg_start = if has_by_address_receiver { 1 } else { 0 };
        if has_by_address_receiver {
            if let Some(receiver) = arguments.first() {
                self.emit_call_argument(receiver, None, 0);
            }
        }
        for argument in arguments.iter().skip(arg_start) {
            self.emit_call_argument(argument, None, 0);
        }
        let param_types: Vec<JvmTypeDescriptor> = arguments
            .iter()
            .skip(arg_start)
            .flat_map(|arg| {
                let ty = self.infer_field_type(arg);
                jvm_type_field_types(&self.ctx, &ty)
                    .into_iter()
                    .map(|ft| {
                        let effective_ft = effective_jvm_type(&self.ctx, &ft);
                        jvm_parameter_descriptor(&effective_ft)
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        self.instructions.push(JvmInstruction::InvokeVirtual(JvmMethodRef {
            owner: stream_owner,
            name: method_name,
            descriptor: JvmMethodDescriptor::new(param_types, JvmTypeDescriptor::Void),
        }));
    }

    /// 返回当前片段对应的 JVM 内部类名，作为静态调用的 owner。
    fn current_class_owner(&self) -> String {
        format!("{}/{}", sanitize_symbol(&self.ctx.submission.module_name), sanitize_symbol(self.ctx.submission.fragment_id.as_str()))
    }

    /// Resolve a static call target using its complete semantic symbol only.
    fn resolve_static_call_symbol(&self, path: &nyar::NamePath) -> String {
        let qualified = QualifiedName::new(path.parts().to_vec());
        if let Some(exec) = &self.ctx.submission.executable {
            if let Some(operation) = exec.operations().into_iter().find(|operation| operation == &qualified) {
                return sanitize_jvm_method_symbol(&operation);
            }
        }
        if let Some(operation) = self.ctx.submission.exported_operations.iter().find(|operation| *operation == &qualified) {
            return sanitize_jvm_method_symbol(operation);
        }
        sanitize_symbol(&path.to_string())
    }

    /// Build `(params)return` from the callee MIR body so InvokeStatic matches the emitted method.
    fn resolve_callee_jvm_descriptor(&self, path: &nyar::NamePath) -> Option<JvmMethodDescriptor> {
        let qualified = QualifiedName::new(path.parts().to_vec());
        let exec = self.ctx.submission.executable.as_ref()?;
        let view = exec.get_function(&qualified)?;
        let params =
            effective_param_descriptors(self.ctx.submission, &view.function, enclosing_type_name_from_operation(&qualified).as_deref());
        let return_type =
            effective_return_descriptor(self.ctx.submission, &view.function, enclosing_type_name_from_operation(&qualified).as_deref());
        Some(JvmMethodDescriptor::new(params, return_type))
    }

    /// 解析 singleton 调用，与 CLR `resolve_singleton_call` 对齐。
    ///
    /// 仅处理形如 `Type.method` 的双段路径。accessor 调用（方法名等于 `plan.accessor_method()` 且无参数）
    /// 返回 `InvokeStatic`；实例方法调用从 `mir_functions` 查找 `{type_name}.{method_name}`，
    /// 跳过接收者参数后返回 `InvokeVirtual`。owner 与 `build_jvm_singleton_classes` 生成的 internal_name 一致。
    fn resolve_singleton_call(&self, path: &nyar::NamePath, arguments: &[MirOperand]) -> Option<(JvmMethodRef, bool, bool)> {
        if path.parts().len() != 2 {
            return None;
        }
        let type_name = path.parts()[0].as_str();
        let method_name = path.parts()[1].as_str();
        let plan = self.ctx.submission.singleton_instances.iter().find(|plan| plan.name == type_name)?;
        let owner = jvm_internal_name(plan);
        if method_name == plan.accessor_method() && arguments.is_empty() {
            let descriptor = JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Object(owner.clone()));
            return Some((JvmMethodRef { owner, name: method_name.to_string(), descriptor }, true, false));
        }
        let symbol = format!("{type_name}.{method_name}");
        let mir_fn = self.ctx.submission.executable.as_ref().and_then(|exec| exec.find_by_symbol(&symbol)).map(|view| view.function)?;
        let return_ty = concretize_self_type(&mir_fn.return_type, Some(type_name));
        let effective_return = effective_jvm_type(&self.ctx, &return_ty);
        let return_type = jvm_type_descriptor(&effective_return);
        let is_void = matches!(return_type, JvmTypeDescriptor::Void);
        let param_types = mir_fn
            .param_types
            .iter()
            .skip(1)
            .flat_map(|ty| {
                let ty = concretize_self_type(ty, Some(type_name));
                jvm_type_field_types(&self.ctx, &ty)
                    .into_iter()
                    .map(|ft| {
                        let effective_ft = effective_jvm_type(&self.ctx, &ft);
                        jvm_parameter_descriptor(&effective_ft)
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        let descriptor = JvmMethodDescriptor::new(param_types, return_type);
        Some((JvmMethodRef { owner, name: method_name.to_string(), descriptor }, false, is_void))
    }

    /// Resolve witness Call descriptor from the registered table slot (not bare method name).
    fn resolve_witness_call_descriptor(
        &self,
        method_name: &str,
        witness: Option<&MirOperand>,
        arguments: &[MirOperand],
    ) -> Option<JvmMethodDescriptor> {
        let receiver_ty = self.receiver_type_for_witness(witness, arguments.first())?;
        let receiver_name = match &receiver_ty {
            NyarType::Named(name) => name.as_str(),
            NyarType::Utf8 => "utf8",
            NyarType::Utf16 => "utf16",
            NyarType::Array(_) => "Array",
            NyarType::Apply(base, _) => match base.as_ref() {
                NyarType::Named(name) => name.as_str(),
                _ => return None,
            },
            _ => return None,
        };
        let table = self.ctx.submission.witness_tables.iter().find(|table| table.type_name == receiver_name)?;
        let method = table.methods.iter().find(|method| method.method_name == method_name)?;
        Some(witness_slot_jvm_descriptor(table, method))
    }

    /// 从 witness 或首个参数推断接收者类型，用于 witness 派发的 trait 解析。
    fn receiver_type_for_witness(&self, witness: Option<&MirOperand>, first_argument: Option<&MirOperand>) -> Option<NyarType> {
        witness.or(first_argument).and_then(|operand| match operand {
            MirOperand::Value(value) => self.mir_fn.value_types.get(value).cloned(),
            _ => None,
        })
    }
}

/// 从接收者类型提取 witness 派发所需的 trait 名称，仅识别内建可调度 trait。
fn witness_trait_name(ty: &NyarType) -> Option<String> {
    let name = match ty {
        NyarType::TraitObject(object) => object.trait_path.as_str(),
        NyarType::Apply(base, _) => match base.as_ref() {
            NyarType::Named(name) => name.as_str(),
            _ => return None,
        },
        NyarType::Named(name) => name.as_str(),
        _ => return None,
    };
    matches!(name, "Iterator" | "Future" | "Generator" | "Coroutine" | "Promise").then(|| name.to_string())
}

/// Runtime stub descriptors for **explicitly injected** helpers only.
///
/// Requires a bare symbol path (`print`, not `Foo.print`) that is in
/// [`super::witness_abi::INJECTED_RUNTIME_STUBS`]. Descriptors must match
/// [`super::ensure_jvm_runtime_stubs`].
///
/// JVM uses 32-bit `Int` handles for nullability (`is_null` / `unwrap_null`);
/// CLR uses `Int64`. `print` takes `Object` and returns `Int` (exit code 0).
pub(crate) fn runtime_stub_descriptor(path: &nyar::NamePath) -> Option<JvmMethodDescriptor> {
    let parts: Vec<&str> = path.parts().iter().map(|part| part.as_str()).collect();
    if !is_injected_runtime_stub_symbol(&parts) {
        return None;
    }
    let object = JvmTypeDescriptor::Object("java/lang/Object".to_string());
    let int = JvmTypeDescriptor::Int;
    match parts[0] {
        "print" => Some(JvmMethodDescriptor::new(vec![object], int)),
        "panic" => Some(JvmMethodDescriptor::new(vec![object], JvmTypeDescriptor::Void)),
        "unimplemented" => Some(JvmMethodDescriptor::new(vec![], object)),
        "is_null" => Some(JvmMethodDescriptor::new(vec![int.clone()], JvmTypeDescriptor::Boolean)),
        "unwrap_null" => Some(JvmMethodDescriptor::new(vec![int.clone()], int)),
        _ => None,
    }
}

/// `java.util.Arrays.copyOf` 重载：按元素类型选择描述符。
/// 返回 `(owner, name, descriptor, optional_checkcast_class)`。
fn arrays_copy_of_method(element_type: &NyarType, is_reference: bool) -> (String, String, JvmMethodDescriptor, Option<String>) {
    let owner = "java/util/Arrays".to_string();
    let name = "copyOf".to_string();
    let int_ty = JvmTypeDescriptor::Int;
    if is_reference {
        let elem_desc = jvm_type_descriptor(element_type);
        let array_desc = JvmTypeDescriptor::array(elem_desc.clone());
        // 泛型擦除为 Object[]；对具体元素类型在返回后 checkcast。
        let object_array = JvmTypeDescriptor::array(JvmTypeDescriptor::Object("java/lang/Object".to_string()));
        let cast = match &elem_desc {
            JvmTypeDescriptor::Object(class) => Some(format!("[L{class};")),
            JvmTypeDescriptor::Array(_) => Some(array_desc.to_string()),
            _ => Some(array_desc.to_string()),
        };
        return (owner, name, JvmMethodDescriptor::new(vec![object_array.clone(), int_ty], object_array), cast);
    }
    let (array_desc, _atype_note) = match element_type {
        NyarType::Boolean => (JvmTypeDescriptor::array(JvmTypeDescriptor::Boolean), "Z"),
        NyarType::Integer8 { .. } => (JvmTypeDescriptor::array(JvmTypeDescriptor::Byte), "B"),
        NyarType::Character => (JvmTypeDescriptor::array(JvmTypeDescriptor::Char), "C"),
        NyarType::Integer16 { .. } => (JvmTypeDescriptor::array(JvmTypeDescriptor::Short), "S"),
        NyarType::Integer64 { .. } => (JvmTypeDescriptor::array(JvmTypeDescriptor::Long), "J"),
        NyarType::Float32 => (JvmTypeDescriptor::array(JvmTypeDescriptor::Float), "F"),
        NyarType::Float64 => (JvmTypeDescriptor::array(JvmTypeDescriptor::Double), "D"),
        _ => (JvmTypeDescriptor::array(JvmTypeDescriptor::Int), "I"),
    };
    (owner, name, JvmMethodDescriptor::new(vec![array_desc.clone(), int_ty], array_desc), None)
}

/// JVM `newarray` atype（表 6.5）：4=Z 5=C 6=F 7=D 8=B 9=S 10=I 11=J。
///
/// 引用元素返回 `None`，调用方应发 `anewarray`。
/// 若对 `u16`/`short` 误发 `newarray int`，随后 `putfield …:[S` 会触发
/// `VerifyError: Bad type in putfield/putstatic`。
fn jvm_primitive_newarray(element_type: &NyarType) -> Option<JvmInstruction> {
    let atype = match element_type {
        NyarType::Boolean => 4,
        NyarType::Character => 5,
        NyarType::Float32 => 6,
        NyarType::Float64 => 7,
        NyarType::Integer8 { .. } => 8,
        NyarType::Integer16 { .. } => 9,
        NyarType::Integer32 { .. } => 10,
        NyarType::Integer64 { .. } => 11,
        _ => return None,
    };
    Some(JvmInstruction::NewArray(atype))
}

/// 与 [`jvm_primitive_newarray`] / 字段描述符对齐的 `*astore`。
fn jvm_primitive_array_store(element_type: &NyarType) -> JvmInstruction {
    match element_type {
        NyarType::Boolean | NyarType::Integer8 { .. } => JvmInstruction::BAStore,
        NyarType::Character => JvmInstruction::CAStore,
        NyarType::Integer16 { .. } => JvmInstruction::SAStore,
        NyarType::Integer32 { .. } => JvmInstruction::IAStore,
        // 宽/浮点数组若尚未单独降级，暂用 IAStore 保持旧路径；创建侧已走正确 atype。
        NyarType::Integer64 { .. } | NyarType::Float32 | NyarType::Float64 => JvmInstruction::IAStore,
        _ => JvmInstruction::AAStore,
    }
}

/// 将 [`JvmTypeDescriptor`] 反向映射为 [`NyarType`]，用于从 runtime stub
/// 描述符的返回类型填充 `value_type_overrides`。
///
/// 此映射与 [`jvm_type_descriptor`] 互为逆函数（`Object` 名称中的 `/` 转换为 `.`）。
/// `Void` 映射为 `Unit`，与 [`ExecutableSlotPlan::alloc_local`] 将 void 槽位分配为
/// `Int32` 的行为一致——调用方在 `emit_call_invoke` 中已过滤 `Void` 返回的 stub，
/// 不会为无返回值的调用填充 `value_type_overrides`。
pub(crate) fn nyar_type_from_jvm_descriptor(desc: &JvmTypeDescriptor) -> NyarType {
    match desc {
        JvmTypeDescriptor::Boolean => NyarType::Boolean,
        JvmTypeDescriptor::Byte => NyarType::Integer8 { signed: true },
        JvmTypeDescriptor::Char => NyarType::Character,
        JvmTypeDescriptor::Short => NyarType::Integer16 { signed: true },
        JvmTypeDescriptor::Int => NyarType::Integer32 { signed: true },
        JvmTypeDescriptor::Long => NyarType::Integer64 { signed: true },
        JvmTypeDescriptor::Float => NyarType::Float32,
        JvmTypeDescriptor::Double => NyarType::Float64,
        JvmTypeDescriptor::Void => NyarType::Unit,
        JvmTypeDescriptor::Object(name) => NyarType::Named(nyar::Identifier::new(&name.replace('/', "."))),
        JvmTypeDescriptor::Array(element) => NyarType::Array(Box::new(nyar_type_from_jvm_descriptor(element))),
    }
}

/// 返回 `StringBuilder.append` 重载的参数描述符，与操作数实际类型匹配。
///
/// String 类型走 `append(String)`，其他引用类型走 `append(Object)`，
/// `long`/`double` 分别走 `append(long)`/`append(double)`，其余整数与布尔
/// 统一走 `append(int)`（JVM 栈上 boolean/short/byte/char 均以 int 表示）。

fn jvm_load_local_instruction(local: u16, ty: &NyarType) -> JvmInstruction {
    match jvm_local_kind_of(ty) {
        JvmLocalKind::Double => JvmInstruction::DLoad(local),
        JvmLocalKind::Float => JvmInstruction::FLoad(local),
        JvmLocalKind::Long => JvmInstruction::LLoad(local),
        JvmLocalKind::Int => JvmInstruction::ILoad(local),
        // `Unit`/`Bottom` 仍可能占有已 `istore`/`astore` 的 SSA local（类型元数据丢失或
        // 被折叠时）。必须从 local 读取，不能发射 `iconst_0`：否则 length/比较结果
        // 被写入后，后续分支与 `infix` 操作数变成常量 0，触发空栈或错误控制流。
        JvmLocalKind::Reference => JvmInstruction::ALoad(local),
    }
}

/// 将 Valkyrie 类型映射为 JVM 类型描述符。
///
/// 未解析的泛型类型参数（单大写字母名称如 `T`/`K`/`V`/`U`）擦除为
/// `java/lang/Object`，因为 JVM 使用类型擦除表示泛型，方法/字段描述符中
/// 不允许出现 `LT;` 等对类型参数的直接引用，否则触发 `NoClassDefFoundError`。
pub(crate) fn jvm_type_descriptor(ty: &NyarType) -> JvmTypeDescriptor {
    match ty {
        // Bottom (void ADT=0): uninhabited — JVM return-only `V`.
        // Unit (unit ADT=1): inhabited; method-return ABI still erases to `V` + `return`,
        // while locals use `JvmLocalKind::Int` (see `jvm_local_kind`). Do not treat as the same ADT.
        NyarType::Bottom | NyarType::Unit => JvmTypeDescriptor::Void,
        NyarType::Boolean => JvmTypeDescriptor::Boolean,
        NyarType::Integer8 { .. } => JvmTypeDescriptor::Byte,
        NyarType::Integer16 { .. } => JvmTypeDescriptor::Short,
        NyarType::Integer32 { .. } => JvmTypeDescriptor::Int,
        NyarType::Integer64 { .. } => JvmTypeDescriptor::Long,
        NyarType::Float32 => JvmTypeDescriptor::Float,
        NyarType::Float64 => JvmTypeDescriptor::Double,
        NyarType::Character => JvmTypeDescriptor::Char,
        // ABI: both may live in java/lang/String. Index APIs must still diverge:
        // Utf16 → code units; Utf8 → scalars (see emit_jvm_length_call / jvm Utf8Text adaptor).
        NyarType::Utf8 | NyarType::Utf16 => JvmTypeDescriptor::Object("java/lang/String".to_string()),
        NyarType::Array(element) => {
            let element_desc = jvm_type_descriptor(element);
            match element_desc {
                JvmTypeDescriptor::Void => JvmTypeDescriptor::array(JvmTypeDescriptor::Int),
                _ => JvmTypeDescriptor::array(element_desc),
            }
        }
        NyarType::Named(name) => {
            let s = name.as_str();
            // 原始类型名（如 `usize`/`i32`/`bool`）需先映射为对应的 JVM 原始描述符，
            // 否则会被当作 `Object` 引用，与方法描述符及栈上 `int`/`long` 不匹配，
            // 触发 `VerifyError`。与 CLR 后端的 `map_primitive_name_to_msil` 对齐。
            if let Some(primitive) = map_primitive_name_to_jvm(s) {
                return jvm_type_descriptor(&primitive);
            }
            // Lossy concretize sentinels — not real class owners. Align with CLR
            // `nyar_type_to_msil` so descriptors never emit `L__auto;` etc.
            // (would surface as `NoClassDefFoundError: __auto` at link/verify time).
            if matches!(s, "Self" | "__auto" | "__type_lambda" | "__associated" | "__row" | "__intersection" | "__opaque" | "__generic") {
                return JvmTypeDescriptor::Object("java/lang/Object".to_string());
            }
            let normalized = s.replace('.', "/");
            match normalized.as_str() {
                // JVM 对泛型使用类型擦除：单大写字母名称（T/K/V/U 等）识别为
                // 未解析的泛型类型参数，擦除为 java/lang/Object，
                // 避免方法/字段描述符中出现非法的 LT; 等引用。
                s if s.len() == 1 && s.chars().next().is_some_and(|c| c.is_ascii_uppercase()) => {
                    JvmTypeDescriptor::Object("java/lang/Object".to_string())
                }
                _ => JvmTypeDescriptor::Object(normalized),
            }
        }
        // `NyarType::Union` 用于 nullable `T?`（`Union([T, null])`）和匿名联合。
        // nullable 联合的 payload 类型已由 `effective_jvm_type` 解析为具体类型，
        // 此处仅处理 `Union` 直接到达 `jvm_type_descriptor` 的边界情况。
        // 映射为 `Object`：nullable 引用类型用 null 引用表示缺失，
        // 匿名联合也用 Object 承载（与 CLR `NyarType::Union(_) => MsilType::Object` 对齐）。
        // 避免 `Union` 落入 catch-all `Int`，导致引用类型被 `istore`/`iload` 处理。
        NyarType::Union(_) => JvmTypeDescriptor::Object("java/lang/Object".to_string()),
        // Non-sum `Apply` (generic class): type-erased Object. Callers must run
        // `effective_jvm_type` first so unite/`Result`/`Option` become `Integer32`.
        NyarType::Apply(base, _) => match base.as_ref() {
            NyarType::Utf8 | NyarType::Utf16 => jvm_type_descriptor(base),
            _ => JvmTypeDescriptor::Object("java/lang/Object".to_string()),
        },
        _ => JvmTypeDescriptor::Int,
    }
}

/// 将 Valkyrie 类型转换为 JVM 方法参数描述符。
///
/// JVM 规范禁止在参数列表中使用 `V`（void），因此 `Void`/`Unit` 参数映射为 `Int`，
/// 与 [`ExecutableSlotPlan::alloc_local`] 将 void 槽位分配为 `Int32` 以及
/// [`JvmMirLowerer::emit_load_local`] 对 `Unit` 值发射 `IConst(0)` 的行为保持一致。
pub(crate) fn jvm_parameter_descriptor(ty: &NyarType) -> JvmTypeDescriptor {
    match jvm_type_descriptor(ty) {
        JvmTypeDescriptor::Void => JvmTypeDescriptor::Int,
        other => other,
    }
}

/// 将 Valkyrie 类型转换为 JVM 字段描述符。
///
/// JVM 规范禁止字段使用 `V`（void）描述符，因此 `Void`/`Unit` 映射为 `Int`，
/// 与 [`ExecutableSlotPlan::alloc_local`] 将 void 槽位分配为 `Int32` 以及
/// [`JvmMirLowerer::emit_load_local`] 对 `Unit` 值发射 `IConst(0)` 的行为保持一致。
/// 当 MIR 字段访问无法解析 layout 时，`field_type` 回退到 `Unit`，
/// 此函数确保最终字段描述符合法。
pub(crate) fn jvm_field_descriptor(ty: &NyarType) -> JvmTypeDescriptor {
    match jvm_type_descriptor(ty) {
        JvmTypeDescriptor::Void => JvmTypeDescriptor::Int,
        other => other,
    }
}

/// 计算 MIR 函数的有效返回类型描述符，供外部调用点（如 `main` 入口桩）构建匹配的 InvokeStatic 描述符。
///
/// `self_owner`：方法所属类型名，用于把参数/返回里的 `Self` 收成具体类型（见 [`concretize_self_type`]）。
pub(crate) fn effective_return_descriptor(
    submission: &FragmentSubmission,
    mir_fn: &MirFunction,
    self_owner: Option<&str>,
) -> JvmTypeDescriptor {
    let ctx = ExecutableLoweringContext::new(submission);
    let return_ty = concretize_self_type(&mir_fn.return_type, self_owner);
    if needs_boxing(&ctx, &return_ty) {
        jvm_type_descriptor(&return_ty)
    }
    else {
        let effective = effective_jvm_type(&ctx, &return_ty);
        jvm_type_descriptor(&effective)
    }
}

/// 计算 MIR 函数的有效参数描述符列表，供外部调用点构建匹配的 InvokeStatic 描述符。
///
/// 与 [`lower_mir_function_to_jvm`] 中的参数描述符构造逻辑保持一致，
/// 确保调用方使用的描述符与方法定义端完全匹配。
///
/// `self_owner`：方法所属类型名，用于把 `Self` 收成具体类型。
pub(crate) fn effective_param_descriptors(
    submission: &FragmentSubmission,
    mir_fn: &MirFunction,
    self_owner: Option<&str>,
) -> Vec<JvmTypeDescriptor> {
    let ctx = ExecutableLoweringContext::new(submission);
    mir_fn
        .param_types
        .iter()
        .flat_map(|ty| {
            let ty = concretize_self_type(ty, self_owner);
            jvm_type_field_types(&ctx, &ty)
                .into_iter()
                .map(|ft| {
                    let effective_ft = effective_jvm_type(&ctx, &ft);
                    jvm_parameter_descriptor(&effective_ft)
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn nyar_type_is_numeric(ty: &NyarType) -> bool {
    matches!(
        ty,
        NyarType::Boolean
            | NyarType::Character
            | NyarType::Integer8 { .. }
            | NyarType::Integer16 { .. }
            | NyarType::Integer32 { .. }
            | NyarType::Integer64 { .. }
            | NyarType::Float32
            | NyarType::Float64
            | NyarType::Unit
            | NyarType::Bottom
    )
}

#[cfg(test)]
#[path = "mir/tests.rs"]
mod tests;
