//! NyarVM bytecode lowering from semantic MIR.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    contracts::{EffectKind, ValueOrigin, instruction_primary_result},
    executable_provider::{
        ExecutableBlock as MirBlock, ExecutableBlockRef as MirBlockRef, ExecutableConstant as MirConstant, ExecutableFunction as MirFunction,
        ExecutableInstruction as MirInstruction, ExecutableInstructionKind as MirInstructionKind, ExecutableOperand as MirOperand,
        ExecutableTerminator as MirTerminator, ExecutableValueRef as MirValueRef, NyarType,
    },
};
use nyar::QualifiedName;
use nyar_types::{AggregateLayout, IntrinsicId, LayoutId, builtin_operator};
use nyar_bytecode::{
    NyarConstant, NyarExport, NyarExportKind, NyarFunction, NyarHeadCode, NyarImport, NyarImportKind, NyarLayout, NyarModuleData,
    NYAR_VERSION,
};

use super::{
    executable::{ExecutableLoweringContext, block_label, collect_reachable_blocks, slots::ExecutableSlotPlan},
    nyar_vm::{nyar_public_export_name, operation_short_name},
    singleton::{augment_nyar_module_with_singletons, nyar_singleton_accessor_export_name, nyar_singleton_method_export_name},
};
use crate::{
    FragmentSubmission,
    executable_provider::resolve_static_callee_operation,
};

/// 宿主 builtin 导入模块名（链接名在 imports section；热路径只用下标）。
const HOST_IMPORT_MODULE: &str = "nyar.host";

struct BytecodeEmitter<'a> {
    constants: Vec<NyarConstant>,
    code_bytes: Vec<u8>,
    pending_jumps: Vec<(usize, MirBlockRef)>,
    block_starts: BTreeMap<MirBlockRef, usize>,
    constants_base: i32,
    imports: &'a mut Vec<NyarImport>,
}

impl<'a> BytecodeEmitter<'a> {
    fn new(constants_base: i32, imports: &'a mut Vec<NyarImport>) -> Self {
        Self {
            constants: Vec::new(),
            code_bytes: Vec::new(),
            pending_jumps: Vec::new(),
            block_starts: BTreeMap::new(),
            constants_base,
            imports,
        }
    }

    fn intern_string(&mut self, value: &str) -> i32 {
        let index = self.constants.len() as i32;
        self.constants.push(NyarConstant::String(value.to_string()));
        index + self.constants_base
    }

    fn emit_plain(&mut self, opcode: NyarHeadCode) {
        nyar_bytecode::emit_plain(&mut self.code_bytes, opcode);
    }

    fn emit_imm1(&mut self, opcode: NyarHeadCode, operand: i32) {
        nyar_bytecode::emit_imm1(&mut self.code_bytes, opcode, operand);
    }

    fn ensure_host_import(&mut self, symbol: &str) -> i32 {
        if let Some((index, _)) = self
            .imports
            .iter()
            .enumerate()
            .find(|(_, import)| import.module_name == HOST_IMPORT_MODULE && import.symbol_name == symbol)
        {
            return index as i32;
        }
        let index = self.imports.len() as i32;
        self.imports.push(NyarImport {
            kind: NyarImportKind::Function,
            module_name: HOST_IMPORT_MODULE.to_string(),
            symbol_name: symbol.to_string(),
        });
        index
    }

    /// 按 imports 表下标调用宿主能力；operand1 = import index，operand2 = argc。
    fn emit_call_import(&mut self, symbol: &str, arg_count: i32) {
        let import_index = self.ensure_host_import(symbol);
        nyar_bytecode::emit_imm2(&mut self.code_bytes, NyarHeadCode::CallImport, import_index, arg_count);
    }

    /// 按 [`IntrinsicId`] 稠密下标调用内置；operand1 = bytecode index，operand2 = argc。
    fn emit_call_intrinsic(&mut self, intrinsic: IntrinsicId, arg_count: i32) {
        nyar_bytecode::emit_imm2(
            &mut self.code_bytes,
            NyarHeadCode::CallIntrinsic,
            intrinsic.bytecode_index() as i32,
            arg_count,
        );
    }

    fn emit_jump_placeholder(&mut self, opcode: NyarHeadCode, target: MirBlockRef) -> usize {
        let position = self.code_bytes.len();
        self.emit_imm1(opcode, 0);
        self.pending_jumps.push((position, target));
        position
    }

    fn patch_jump_to_block(&mut self, jump_position: usize, target: MirBlockRef) {
        let target_pos = *self.block_starts.get(&target).expect("block start");
        let offset = (target_pos as i32) - (jump_position as i32);
        self.code_bytes[jump_position + 1..jump_position + 5].copy_from_slice(&offset.to_le_bytes());
    }

    fn patch_pending_jumps(&mut self) {
        let pending = std::mem::take(&mut self.pending_jumps);
        for (position, target) in pending {
            self.patch_jump_to_block(position, target);
        }
    }
}

/// Lower MIR-backed fragment operations into a `.nyar` module.
pub(crate) fn lower_fragment_mir_to_nyar_module(submission: &FragmentSubmission) -> NyarModuleData {
    let mut module = NyarModuleData {
        version: NYAR_VERSION,
        name: format!("{}__{}", super::sanitize_symbol(&submission.module_name), super::sanitize_symbol(submission.fragment_id.as_str())),
        constants: Vec::new(),
        functions: Vec::new(),
        imports: Vec::new(),
        exports: Vec::new(),
        witness_entries: Vec::new(),
        code_bytes: Vec::new(),
        globals: Vec::new(),
        init_function_indices: Vec::new(),
        layouts: Vec::new(),
    };

    let mut layout_index_by_id = BTreeMap::<LayoutId, i32>::new();

    let mut function_index_by_name = BTreeMap::<String, i32>::new();
    if let Some(exec) = &submission.executable {
        let operations: Vec<QualifiedName> = exec
            .operations()
            .into_iter()
            .filter(|operation| exec.get_function(operation).is_some())
            .collect();
        function_index_by_name = build_nyar_function_index_map(submission, exec.as_ref(), &operations);
        let function_entry_arities = build_nyar_function_entry_arities(exec.as_ref(), &operations);

        for operation in operations {
            let Some(view) = exec.get_function(&operation)
            else {
                continue;
            };
            let mir_fn = &view.function;
            let export_name = nyar_mir_export_name(submission, &operation);
            let code_offset = module.code_bytes.len() as i32;
            let constants_base = module.constants.len() as i32;
            let mut emitter = BytecodeEmitter::new(constants_base, &mut module.imports);

            lower_mir_function_to_bytecode(
                submission,
                mir_fn,
                &function_index_by_name,
                &function_entry_arities,
                &mut emitter,
                &mut module.layouts,
                &mut layout_index_by_id,
            );

            module.constants.extend(emitter.constants);
            module.code_bytes.extend_from_slice(&emitter.code_bytes);

            // 与 JVM/CLR 对齐：调用约定 arity 以入口块 SSA 形参为准；`param_types` 可能含已废弃的 ABI 槽。
            let arity = mir_fn
                .blocks
                .get(mir_fn.entry.0 as usize)
                .map(|block| block.parameters.len())
                .unwrap_or(mir_fn.param_types.len()) as i32;
            let local_count = ExecutableSlotPlan::plan_nyar(&ExecutableLoweringContext::new(submission), mir_fn).local_types.len() as i32;
            let function_index = module.functions.len() as i32;
            module.functions.push(NyarFunction {
                name: export_name.clone(),
                arity,
                local_count: local_count.max(arity),
                code_offset,
                code_length: module.code_bytes.len() as i32 - code_offset,
            });
            if nyar_should_export_operation(submission, &operation) {
                module.exports.push(NyarExport { kind: NyarExportKind::Function, symbol_name: export_name, function_index });
            }
        }
    }

    augment_nyar_module_with_singletons(submission, &mut module, &function_index_by_name);

    module
}

/// 在降低函数体之前登记全部 operation → 稠密下标，供 `Call` 解析（含前向引用）。
fn build_nyar_function_entry_arities(
    exec: &dyn crate::executable_provider::ExecutableProvider,
    operations: &[QualifiedName],
) -> BTreeMap<i32, usize> {
    let mut map = BTreeMap::new();
    for (index, operation) in operations.iter().enumerate() {
        let Some(view) = exec.get_function(operation) else {
            continue;
        };
        let entry_arity = view
            .function
            .blocks
            .get(view.function.entry.0 as usize)
            .map(|block| block.parameters.len())
            .unwrap_or(view.function.param_types.len());
        map.insert(index as i32, entry_arity);
    }
    map
}

fn build_nyar_function_index_map(
    _submission: &FragmentSubmission,
    exec: &dyn crate::executable_provider::ExecutableProvider,
    operations: &[QualifiedName],
) -> BTreeMap<String, i32> {
    let mut map = BTreeMap::new();
    for (index, operation) in operations.iter().enumerate() {
        let dense = index as i32;
        map.insert(operation.to_string(), dense);
        if let Some(view) = exec.get_function(operation) {
            let symbol = view.function.symbol;
            if symbol != operation.to_string() {
                map.insert(symbol, dense);
            }
        }
    }
    map
}

/// 库模式只导出用户 `[export]` / `exported_operations`；闭包内 std 辅助函数保持内部 `Call` 可见性。
fn nyar_should_export_operation(submission: &FragmentSubmission, operation: &QualifiedName) -> bool {
    if submission.wasm_export_names.contains_key(operation) {
        return true;
    }
    submission.exported_operations.iter().any(|exported| exported == operation)
}

fn nyar_mir_export_name(submission: &FragmentSubmission, operation: &QualifiedName) -> String {
    if let Some(public_name) = submission.wasm_export_names.get(operation) {
        return public_name.clone();
    }
    if operation.parts().len() == 2 {
        let type_name = operation.parts()[0].as_str();
        let method_name = operation.parts()[1].as_str();
        if submission.singleton_instances.iter().any(|plan| plan.name == type_name) {
            return nyar_singleton_method_export_name(type_name, method_name);
        }
    }
    nyar_public_export_name(submission, operation)
}

fn lower_mir_function_to_bytecode(
    submission: &FragmentSubmission,
    mir_fn: &MirFunction,
    function_index_by_name: &BTreeMap<String, i32>,
    function_entry_arities: &BTreeMap<i32, usize>,
    emitter: &mut BytecodeEmitter<'_>,
    layouts: &mut Vec<NyarLayout>,
    layout_index_by_id: &mut BTreeMap<LayoutId, i32>,
) {
    let ctx = ExecutableLoweringContext::new(submission);
    let slots = ExecutableSlotPlan::plan_nyar(&ctx, mir_fn);
    let block_order = collect_reachable_blocks(mir_fn);

    let mut lowerer = NyarMirLowerer {
        submission,
        ctx,
        mir_fn,
        slots,
        emitter,
        function_index_by_name,
        function_entry_arities,
        layouts,
        layout_index_by_id,
    };
    for block_id in block_order {
        lowerer.emitter.block_starts.insert(block_id, lowerer.emitter.code_bytes.len());
        if let Some(block) = mir_fn.blocks.get(block_id.0 as usize) {
            lowerer.emit_block(block);
        }
    }
    lowerer.emitter.patch_pending_jumps();
}

struct NyarMirLowerer<'a, 'e> {
    submission: &'a FragmentSubmission,
    ctx: ExecutableLoweringContext<'a>,
    mir_fn: &'a MirFunction,
    slots: ExecutableSlotPlan,
    emitter: &'a mut BytecodeEmitter<'e>,
    function_index_by_name: &'a BTreeMap<String, i32>,
    function_entry_arities: &'a BTreeMap<i32, usize>,
    layouts: &'a mut Vec<NyarLayout>,
    layout_index_by_id: &'a mut BTreeMap<LayoutId, i32>,
}

impl<'a, 'e> NyarMirLowerer<'a, 'e> {
    fn emit_block(&mut self, block: &MirBlock) {
        let _ = block_label(block.id);
        for instruction in &block.instructions {
            self.emit_instruction(instruction);
        }
        self.emit_terminator(block);
    }

    fn emit_instruction(&mut self, instruction: &MirInstruction) {
        let output = self.slots.instruction_output(instruction);
        match &instruction.kind {
            MirInstructionKind::LoadConstant { constant, .. } => {
                self.emit_load_constant(constant);
                if let Some(output) = output {
                    self.store_to_local(output);
                }
            }
            MirInstructionKind::StoreVar { name, value, .. } => {
                self.emit_operand(value);
                if let Some(output) = output {
                    self.store_to_local(output);
                }
                else {
                    self.emitter.emit_plain(NyarHeadCode::Pop);
                }
                let _ = name;
            }
            MirInstructionKind::Copy { source } => {
                self.emit_operand(source);
                if let Some(output) = output {
                    self.store_to_local(output);
                }
            }
            MirInstructionKind::StructNew { type_name, fields } => {
                // executable InstructionKind 不携带 layout_id；从 type_name / 字段数闭合 layouts。
                let layout = self.resolve_layout(None, type_name);
                let layout_index = match &layout {
                    Some(aggregate) => self.ensure_nyar_layout(aggregate),
                    None => self.ensure_nyar_layout_count(fields.len() as i32),
                };
                self.emitter.emit_imm1(NyarHeadCode::ObjectNew, layout_index);
                let output = output.expect("StructNew must produce an output");
                self.store_to_local(output);
                let output_operand = MirOperand::Value(output);
                for (index, (field_name, value)) in fields.iter().enumerate() {
                    let slot = layout
                        .as_ref()
                        .and_then(|aggregate| aggregate.fields.iter().position(|entry| entry.name == *field_name))
                        .unwrap_or(index) as i32;
                    self.emit_field_set_slot(&output_operand, slot, value);
                    self.emitter.emit_plain(NyarHeadCode::Pop);
                }
            }
            MirInstructionKind::TupleNew { fields } => {
                let layout = self.resolve_layout(None, "__tuple");
                let layout_index = match &layout {
                    Some(aggregate) => self.ensure_nyar_layout(aggregate),
                    None => self.ensure_nyar_layout_count(fields.len() as i32),
                };
                self.emitter.emit_imm1(NyarHeadCode::ObjectNew, layout_index);
                let output = output.expect("TupleNew must produce an output");
                self.store_to_local(output);
                let output_operand = MirOperand::Value(output);
                for (index, value) in fields.iter().enumerate() {
                    self.emit_field_set_slot(&output_operand, index as i32, value);
                    self.emitter.emit_plain(NyarHeadCode::Pop);
                }
            }
            MirInstructionKind::ArrayFromElements { elements, .. } => {
                let Some(output) = output else {
                    return;
                };
                let layout = self.resolve_layout(None, "__fixedarray");
                let layout_index = match &layout {
                    Some(aggregate) => self.ensure_nyar_layout(aggregate),
                    None => self.ensure_nyar_layout_count(elements.len() as i32),
                };
                self.emitter.emit_imm1(NyarHeadCode::ObjectNew, layout_index);
                self.store_to_local(output);
                let output_operand = MirOperand::Value(output);
                for (index, value) in elements.iter().enumerate() {
                    self.emit_field_set_slot(&output_operand, index as i32, value);
                    self.emitter.emit_plain(NyarHeadCode::Pop);
                }
            }
            MirInstructionKind::AggregateCopy { source, dest } => {
                let layout = self.infer_layout_for_operand(source);
                let field_count = layout.as_ref().map(|item| item.fields.len() as i32).unwrap_or(0);
                let layout_index = match &layout {
                    Some(aggregate) => self.ensure_nyar_layout(aggregate),
                    None => self.ensure_nyar_layout_count(field_count),
                };
                self.emitter.emit_imm1(NyarHeadCode::ObjectNew, layout_index);
                if let MirOperand::Value(dest_value) = dest {
                    self.store_to_local(*dest_value);
                }
                for slot in 0..field_count {
                    self.emit_operand(dest);
                    self.emit_operand(source);
                    self.emitter.emit_imm1(NyarHeadCode::FieldGet, slot);
                    self.emitter.emit_imm1(NyarHeadCode::FieldSet, slot);
                    self.emitter.emit_plain(NyarHeadCode::Pop);
                }
            }
            MirInstructionKind::FieldGet { object, field } => {
                let type_name = self.type_name_for_operand(object);
                let slot = self.nyar_field_slot(None, &type_name, field);
                self.emit_operand(object);
                self.emitter.emit_imm1(NyarHeadCode::FieldGet, slot);
                if let Some(output) = output {
                    self.store_to_local(output);
                }
                else {
                    // 无 SSA 绑定的 FieldGet 不得污染后续 Call 的操作数栈；实参由 `emit_call_operand` 再求值。
                    self.emitter.emit_plain(NyarHeadCode::Pop);
                }
            }
            MirInstructionKind::FieldSet { object, field, value } => {
                let type_name = self.type_name_for_operand(object);
                let slot = self.nyar_field_slot(None, &type_name, field);
                self.emit_field_set_slot(object, slot, value);
                self.emitter.emit_plain(NyarHeadCode::Pop);
            }
            MirInstructionKind::Call { callee, arguments } => {
                if let MirOperand::Symbol(path) = callee {
                    if self.try_emit_language_operator_call(path, arguments, output) {
                        return;
                    }
                    if self.try_emit_host_phase_call(path, arguments, output) {
                        return;
                    }
                    if self.try_emit_array_storage_intrinsic(path, arguments, output) {
                        return;
                    }
                    if self.try_emit_arraylist_element_get(path, arguments, output) {
                        return;
                    }
                }
                if let MirOperand::Symbol(path) = callee {
                    if self.try_emit_singleton_call(path, arguments, output) {
                        return;
                    }
                    if let Some(index) = self.resolve_function_index(path, arguments) {
                        self.emit_direct_call(index, path, arguments, output);
                        return;
                    }
                }
                panic!("unresolved Nyar call in validated Semantic MIR");
            }
            MirInstructionKind::ArrayGet { array, index } => {
                self.emit_call_operand(array);
                self.emit_operand(index);
                self.emitter.emit_call_intrinsic(IntrinsicId::ArrayGet, 2);
                if let Some(output) = output {
                    self.store_to_local(output);
                }
            }
            MirInstructionKind::ArraySet { array, index, value } => {
                self.emit_call_operand(array);
                self.emit_operand(index);
                self.emit_operand(value);
                self.emitter.emit_call_intrinsic(IntrinsicId::ArraySet, 3);
                self.emitter.emit_plain(NyarHeadCode::Pop);
            }
            MirInstructionKind::ArrayLength { array } => {
                // 禁止 Const(0) 回退：会把 ArrayLen 的 array 实参变成 i32，运行时报 expected object。
                if !self.emit_call_operand(array) {
                    panic!(
                        "nyar-vm ArrayLength operand not materializable in `{}`",
                        self.mir_fn.symbol
                    );
                }
                self.emitter.emit_call_intrinsic(IntrinsicId::ArrayLen, 1);
                if let Some(output) = output {
                    self.store_to_local(output);
                }
            }
            MirInstructionKind::SumNew { variant, payload, .. } => {
                // Option / 名义 sum 的最小 ABI：Some(payload) 直接传 payload；None → Null（Const 0 占位，由后续 SumVariantIs 区分前需扩展）。
                // 与 SumPayloadGet 成对：先保证 unwrap 链可读，再演进带 tag 的布局。
                let is_none = variant == "None" || variant.ends_with(".None") || variant.ends_with("::None");
                if is_none {
                    self.emitter.emit_const_i32(0);
                } else if let Some(payload) = payload {
                    self.emit_call_operand(payload);
                } else {
                    self.emitter.emit_const_i32(0);
                }
                if let Some(output) = output {
                    self.store_to_local(output);
                } else {
                    self.emitter.emit_plain(NyarHeadCode::Pop);
                }
            }
            MirInstructionKind::SumPayloadGet { object, .. } => {
                // 与上方 Some 直通 ABI 对齐：payload 即 receiver。
                self.emit_call_operand(object);
                if let Some(output) = output {
                    self.store_to_local(output);
                }
            }
            MirInstructionKind::SumVariantIs { variant, object, .. } => {
                // 直通 ABI：None 为 i32(0)；Some 为非 0 / 对象。`is_some` ≈ 非零。
                let is_none = variant == "None" || variant.ends_with(".None") || variant.ends_with("::None");
                self.emit_call_operand(object);
                self.emitter.emit_const_i32(0);
                if is_none {
                    self.emitter.emit_plain(NyarHeadCode::I32Eq);
                } else {
                    self.emitter.emit_plain(NyarHeadCode::I32Ne);
                }
                if let Some(output) = output {
                    self.store_to_local(output);
                } else {
                    self.emitter.emit_plain(NyarHeadCode::Pop);
                }
            }
            _ => {}
        }
    }

    fn emit_terminator(&mut self, block: &MirBlock) {
        match &block.terminator {
            MirTerminator::Return { value } => {
                if let Some(value) = value {
                    self.emit_operand(value);
                }
                else {
                    // Call 约定固定压回 1 个返回值；unit / void 用 i32(0) 占位，供调用方 Pop。
                    self.emitter.emit_const_i32(0);
                }
                self.emitter.emit_plain(NyarHeadCode::Return);
            }
            MirTerminator::Jump { target, arguments } => {
                self.emit_block_argument_copies(*target, arguments);
                self.emitter.emit_jump_placeholder(NyarHeadCode::Jump, *target);
            }
            MirTerminator::Branch { condition, then_target, else_target } => {
                self.emit_operand(condition);
                self.emitter.emit_jump_placeholder(NyarHeadCode::JumpIfFalse, *else_target);
                self.emit_block_argument_copies(*then_target, &[]);
                self.emitter.emit_jump_placeholder(NyarHeadCode::Jump, *then_target);
                let _ = else_target;
            }
            MirTerminator::PerformEffect { effect, payload, resume_target } => {
                match effect {
                    EffectKind::Yield | EffectKind::DelegateYield => {
                        // `yield expr` / `yield from expr`：把 payload 压栈后发射 `Yield` opcode。
                        // VM 弹出 yielded 值、捕获当前帧为 `CoroutineState`、挂起返回父帧；
                        // 当父帧 `Resume` 时从 `Yield` 之后继续，跳到 resume_target。
                        if let Some(payload) = payload {
                            self.emit_operand(payload);
                        }
                        else {
                            // 无 payload 的 yield：压 i32(0) 作为 unit 占位后再发 Yield。
                            self.emitter.emit_const_i32(0);
                        }
                        self.emitter.emit_plain(NyarHeadCode::Yield);
                        self.emit_block_argument_copies(*resume_target, &[]);
                        self.emitter.emit_jump_placeholder(NyarHeadCode::Jump, *resume_target);
                    }
                    EffectKind::Raise => {
                        // `raise expr`：将 payload 压栈后发射 `PerformEffect` opcode。
                        // operand1 = 常量池索引（effect 的 method_name 字符串）。
                        // VM 用此字符串在 witness_entries 中查找 handler。
                        // handler 可选择 resume（控制流回到 resume_target，resume 值在栈顶）；
                        // 若未 resume 则按未处理效果传播。
                        // 对 `Effectful::Resume = !`：resume 不可达时 VM 不必保留 resume 续体。
                        if let Some(payload) = payload {
                            self.emit_operand(payload);
                        }
                        else {
                            // 无 payload 的 raise：压 i32(0) 作为 unit 占位。
                            self.emitter.emit_const_i32(0);
                        }
                        let effect_name_index = self.emitter.intern_string("raise");
                        self.emitter.emit_imm1(NyarHeadCode::PerformEffect, effect_name_index);
                        self.emit_block_argument_copies(*resume_target, &[]);
                        self.emitter.emit_jump_placeholder(NyarHeadCode::Jump, *resume_target);
                    }
                    EffectKind::Await | EffectKind::AsyncSpawn | EffectKind::AsyncBlock => {
                        // future / async 相关 effect 暂未落地 VM opcode，保留 console_log 调试桩。
                        if let Some(payload) = payload {
                            self.emit_operand(payload);
                            self.emitter.emit_call_import("console_log", 1);
                        }
                        self.emit_block_argument_copies(*resume_target, &[]);
                        self.emitter.emit_jump_placeholder(NyarHeadCode::Jump, *resume_target);
                    }
                }
            }
            MirTerminator::YieldToRuntime { effect, payload, resume_state: _ } => {
                // 状态机重写后的 effect：与 `PerformEffect` 同构地发射对应 opcode。
                // resume_state 由后续 `StateDispatch` 在恢复时读取（暂用 fallthrough 桩处理）。
                match effect {
                    EffectKind::Yield | EffectKind::DelegateYield => {
                        if let Some(payload) = payload {
                            self.emit_operand(payload);
                        }
                        else {
                            self.emitter.emit_const_i32(0);
                        }
                        self.emitter.emit_plain(NyarHeadCode::Yield);
                    }
                    EffectKind::Raise => {
                        if let Some(payload) = payload {
                            self.emit_operand(payload);
                        }
                        else {
                            self.emitter.emit_const_i32(0);
                        }
                        let effect_name_index = self.emitter.intern_string("raise");
                        self.emitter.emit_imm1(NyarHeadCode::PerformEffect, effect_name_index);
                    }
                    EffectKind::Await | EffectKind::AsyncSpawn | EffectKind::AsyncBlock => {
                        // future / async 相关 effect 暂未落地 VM opcode，保留 console_log 调试桩。
                        if let Some(payload) = payload {
                            self.emit_operand(payload);
                            self.emitter.emit_call_import("console_log", 1);
                        }
                    }
                }
            }
            MirTerminator::StateDispatch { state, cases, default_target } => {
                // 状态机入口：读取 state local，与每个 case_key 比较；
                // 匹配则跳到对应 target，否则跳到 default_target。
                for (case_key, target) in cases {
                    self.emit_operand(&MirOperand::Value(*state));
                    self.emitter.emit_const_i32(*case_key as i32);
                    self.emitter.emit_plain(NyarHeadCode::I32Eq);
                    self.emitter.emit_jump_placeholder(NyarHeadCode::JumpIfTrue, *target);
                }
                self.emitter.emit_jump_placeholder(NyarHeadCode::Jump, *default_target);
            }
            MirTerminator::Unreachable => {
                // 不可达：发射 Return 防止 fall-through 到下一函数。
                self.emitter.emit_plain(NyarHeadCode::Return);
            }
        }
    }

    /// 将显式 `begin_phase` / `end_phase` 调用降为 `nyar.host` `CallImport`（无专用 MIR opcode）。
    ///
    /// 符号末段必须精确匹配；参数按宿主合同：`begin_phase` 至少 1 个（阶段名），`end_phase` 1 个。
    fn try_emit_host_phase_call(
        &mut self,
        path: &nyar::NamePath,
        arguments: &[MirOperand],
        output: Option<MirValueRef>,
    ) -> bool {
        let simple = path.parts().last().map(|part| part.as_str()).unwrap_or("");
        let Some((symbol, min_argc)) = host_phase_import(simple)
        else {
            return false;
        };
        if arguments.len() < min_argc {
            return false;
        }
        for argument in arguments {
            self.emit_operand(argument);
        }
        self.emitter.emit_call_import(symbol, arguments.len() as i32);
        if let Some(output) = output {
            self.store_to_local(output);
        }
        else {
            self.emitter.emit_plain(NyarHeadCode::Pop);
        }
        true
    }

    /// 尝试把语言 `Call` 降为 [`OperatorId`] 对应的 Nyar VM 指令 / 宿主 import。
    ///
    /// 先用 `lookup_display_name` 解析为 id，再按 [`OperatorId`] 发射，不按字符串猜语义。
    fn try_emit_language_operator_call(
        &mut self,
        path: &nyar::NamePath,
        arguments: &[MirOperand],
        output: Option<MirValueRef>,
    ) -> bool {
        let simple = path.parts().last().map(|part| part.as_str()).unwrap_or("");
        let Some(op) = builtin_operator::lookup_display_name(simple)
        else {
            return false;
        };
        if op == builtin_operator::prefix_not() {
            if arguments.len() != 1 {
                return false;
            }
            self.emit_operand(&arguments[0]);
            self.emitter.emit_call_import("bool_not", 1);
        }
        else if op == builtin_operator::prefix_neg() {
            if arguments.len() != 1 {
                return false;
            }
            match self.infer_numeric_width(&arguments[0]) {
                NumericWidth::I64 => {
                    self.emit_operand(&arguments[0]);
                    self.emitter.emit_call_import("i64_neg", 1);
                }
                NumericWidth::I32 => {
                    self.emitter.emit_const_i32(0);
                    self.emit_operand(&arguments[0]);
                    self.emitter.emit_plain(NyarHeadCode::I32Sub);
                }
            }
        }
        else if op == builtin_operator::prefix_pos() {
            if arguments.len() != 1 {
                return false;
            }
            self.emit_operand(&arguments[0]);
        }
        else if op == builtin_operator::infix_eq()
            || op == builtin_operator::infix_ne()
            || op == builtin_operator::infix_lt()
            || op == builtin_operator::infix_le()
            || op == builtin_operator::infix_gt()
            || op == builtin_operator::infix_ge()
        {
            if arguments.len() < 2 {
                return false;
            }
            self.emit_operand(&arguments[0]);
            self.emit_operand(&arguments[1]);
            match self.infer_numeric_width_from_pair(&arguments[0], &arguments[1]) {
                NumericWidth::I64 => {
                    let native = if op == builtin_operator::infix_eq() {
                        "i64_eq"
                    }
                    else if op == builtin_operator::infix_ne() {
                        "i64_ne"
                    }
                    else if op == builtin_operator::infix_lt() {
                        "i64_lt"
                    }
                    else if op == builtin_operator::infix_le() {
                        "i64_le"
                    }
                    else if op == builtin_operator::infix_gt() {
                        "i64_gt"
                    }
                    else {
                        "i64_ge"
                    };
                    self.emitter.emit_call_import(native, 2);
                }
                NumericWidth::I32 => {
                    let opcode = if op == builtin_operator::infix_eq() {
                        NyarHeadCode::I32Eq
                    }
                    else if op == builtin_operator::infix_ne() {
                        NyarHeadCode::I32Ne
                    }
                    else if op == builtin_operator::infix_lt() {
                        NyarHeadCode::I32LtS
                    }
                    else if op == builtin_operator::infix_le() {
                        NyarHeadCode::I32LeS
                    }
                    else if op == builtin_operator::infix_gt() {
                        NyarHeadCode::I32GtS
                    }
                    else {
                        NyarHeadCode::I32GeS
                    };
                    self.emitter.emit_plain(opcode);
                }
            }
        }
        else if op == builtin_operator::infix_add()
            || op == builtin_operator::infix_sub()
            || op == builtin_operator::infix_mul()
            || op == builtin_operator::infix_div()
            || op == builtin_operator::infix_rem()
        {
            if arguments.len() < 2 {
                return false;
            }
            self.emit_operand(&arguments[0]);
            self.emit_operand(&arguments[1]);
            match self.infer_numeric_width_from_pair(&arguments[0], &arguments[1]) {
                NumericWidth::I64 => {
                    let native = if op == builtin_operator::infix_add() {
                        "i64_add"
                    }
                    else if op == builtin_operator::infix_sub() {
                        "i64_sub"
                    }
                    else if op == builtin_operator::infix_mul() {
                        "i64_mul"
                    }
                    else if op == builtin_operator::infix_div() {
                        "i64_div"
                    }
                    else {
                        "i64_rem"
                    };
                    self.emitter.emit_call_import(native, 2);
                }
                NumericWidth::I32 => {
                    if op == builtin_operator::infix_div() || op == builtin_operator::infix_rem() {
                        self.emitter.emit_call_import("i32_div", 2);
                    }
                    else {
                        let opcode = if op == builtin_operator::infix_add() {
                            NyarHeadCode::I32Add
                        }
                        else if op == builtin_operator::infix_sub() {
                            NyarHeadCode::I32Sub
                        }
                        else {
                            NyarHeadCode::I32Mul
                        };
                        self.emitter.emit_plain(opcode);
                    }
                }
            }
        }
        else if op == builtin_operator::infix_bit_and()
            || op == builtin_operator::infix_bit_or()
            || op == builtin_operator::infix_and()
            || op == builtin_operator::infix_or()
        {
            if arguments.len() < 2 {
                return false;
            }
            self.emit_operand(&arguments[0]);
            self.emit_operand(&arguments[1]);
            let native = if op == builtin_operator::infix_bit_and() || op == builtin_operator::infix_and() {
                "bool_and"
            }
            else {
                "bool_or"
            };
            self.emitter.emit_call_import(native, 2);
        }
        else {
            return false;
        }
        if let Some(output) = output {
            self.store_to_local(output);
        }
        else {
            self.emitter.emit_plain(NyarHeadCode::Pop);
        }
        true
    }

    fn emit_direct_call(
        &mut self,
        function_index: i32,
        path: &nyar::NamePath,
        arguments: &[MirOperand],
        output: Option<MirValueRef>,
    ) {
        let expected = self.function_entry_arities.get(&function_index).copied().unwrap_or(arguments.len());
        assert_eq!(arguments.len(), expected, "validated call arity differs from function entry: {path:?} index={function_index}");
        for argument in arguments {
            assert!(self.emit_call_operand(argument), "unmaterialized Nyar call operand in `{path:?}`: {argument:?}");
        }
        self.emitter.emit_imm1(NyarHeadCode::Call, function_index);
        if let Some(output) = output {
            self.store_to_local(output);
        }
        else {
            self.emitter.emit_plain(NyarHeadCode::Pop);
        }
    }

    /// Call 实参：优先 local/LoadArg；失败时对定义该 SSA 值的 `FieldGet`/`Copy` 再求值。
    fn emit_call_operand(&mut self, operand: &MirOperand) -> bool {
        if self.try_emit_operand(operand) {
            return true;
        }
        false
    }

    fn find_instruction_producing(&self, value: MirValueRef) -> Option<&MirInstruction> {
        for block in &self.mir_fn.blocks {
            for instruction in &block.instructions {
                if instruction_primary_result(instruction) == Some(value) {
                    return Some(instruction);
                }
            }
        }
        None
    }

    /// 数组存储 intrinsic：`builtin.array.*`，或首参为 `Array`/`FixedArray` 时的 push/len/get/set。
    ///
    /// 阻断 `ArrayList.push` 体内 `push(_items, v)` 被短名解析回 `ArrayList.push` 的递归。
    fn try_emit_array_storage_intrinsic(
        &mut self,
        path: &nyar::NamePath,
        arguments: &[MirOperand],
        output: Option<MirValueRef>,
    ) -> bool {
        let parts: Vec<&str> = path.parts().iter().map(|part| part.as_str()).collect();
        let intrinsic = IntrinsicId::resolve_from_segments(&parts);
        let Some(intrinsic) = intrinsic else {
            return false;
        };
        let argc = match intrinsic {
            IntrinsicId::ArrayPush => 2,
            IntrinsicId::ArrayLen => 1,
            IntrinsicId::ArrayGet => 2,
            IntrinsicId::ArraySet => 3,
            IntrinsicId::RefDeref | IntrinsicId::IsNull | IntrinsicId::UnwrapNull => return false,
        };
        if arguments.len() < argc {
            return false;
        }
        for argument in &arguments[..argc] {
            self.emit_call_operand(argument);
        }
        self.emitter.emit_call_intrinsic(intrinsic, argc as i32);
        if let Some(output) = output {
            self.store_to_local(output);
        }
        else {
            self.emitter.emit_plain(NyarHeadCode::Pop);
        }
        true
    }

    /// `ArrayList.get` / `⁅ ⁆`：`_items` + 1-based ordinal → `ArrayGet`（0-based）。
    ///
    /// 闭包常只拉到 `length`/`push` 而漏 `get`；短名 `get` 还会与 `HashMap.get` 冲突。
    fn try_emit_arraylist_element_get(
        &mut self,
        path: &nyar::NamePath,
        arguments: &[MirOperand],
        output: Option<MirValueRef>,
    ) -> bool {
        if arguments.len() < 2 {
            return false;
        }
        let Some(receiver_ty) = self.call_receiver_nyar_type(arguments) else {
            return false;
        };
        let Some(receiver_name) = nyar_type_layout_name(&receiver_ty) else {
            return false;
        };
        if receiver_name != "ArrayList" && !receiver_name.ends_with(".ArrayList") && !receiver_name.ends_with("::ArrayList") {
            return false;
        }
        let simple = path.parts().last().map(|part| part.as_str()).unwrap_or("");
        let is_cardinal_index = simple.contains('⁅');
        let is_get = simple == "get" || simple.contains("subscript");
        if !is_get && !is_cardinal_index {
            return false;
        }
        // self._items
        if !self.emit_call_operand(&arguments[0]) {
            return false;
        }
        self.emitter.emit_imm1(NyarHeadCode::FieldGet, 0);
        self.emit_call_operand(&arguments[1]);
        // `get(ordinal)` 为 1-based；`⁅cardinal⁆` 已是 0-based（体内 get(cardinal+1)）。
        if is_get && !is_cardinal_index {
            self.emitter.emit_const_i32(1);
            self.emitter.emit_plain(NyarHeadCode::I32Sub);
        }
        self.emitter.emit_call_intrinsic(IntrinsicId::ArrayGet, 2);
        if let Some(output) = output {
            self.store_to_local(output);
        } else {
            self.emitter.emit_plain(NyarHeadCode::Pop);
        }
        true
    }

    fn try_emit_singleton_call(&mut self, path: &nyar::NamePath, arguments: &[MirOperand], output: Option<MirValueRef>) -> bool {
        if path.parts().len() != 2 {
            return false;
        }
        let type_name = path.parts()[0].as_str();
        let method_name = path.parts()[1].as_str();
        let Some(plan) = self.submission.singleton_instances.iter().find(|plan| plan.name == type_name)
        else {
            return false;
        };
        let export_name = if method_name == plan.accessor_method() && arguments.is_empty() {
            nyar_singleton_accessor_export_name(plan)
        }
        else if method_name != plan.accessor_method() {
            nyar_singleton_method_export_name(type_name, method_name)
        }
        else {
            return false;
        };
        let Some(index) = self.function_index_by_name.get(&export_name).copied()
        else {
            return false;
        };
        self.emit_direct_call(index, path, arguments, output);
        true
    }

    fn resolve_function_index(&self, path: &nyar::NamePath, _arguments: &[MirOperand]) -> Option<i32> {
        if let Some(exec) = &self.submission.executable {
            if let Some(operation) = resolve_static_callee_operation(exec.as_ref(), path) {
                return self.function_index_for_operation(&operation);
            }
        if let Some(index) = self.function_index_by_name.get(&path.to_string()) {
            return Some(*index);
        }
        let colon_path = path.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>().join("::");
        if colon_path != path.to_string() {
            if let Some(index) = self.function_index_by_name.get(&colon_path) {
                return Some(*index);
            }
        }
        None
    }

    fn call_receiver_nyar_type(&self, arguments: &[MirOperand]) -> Option<NyarType> {
        let MirOperand::Value(value) = arguments.first()? else {
            return None;
        };
        if let Some(ty) = self.mir_fn.value_types.get(value).cloned() {
            return Some(ty);
        }
        // 与 executable_closure 对齐：FieldGet 结果缺 value_types 时按字段布局反查。
        for block in &self.mir_fn.blocks {
            for instruction in &block.instructions {
                if instruction_primary_result(instruction) != Some(*value) {
                    continue;
                }
                let MirInstructionKind::FieldGet { object, field } = &instruction.kind else {
                    continue;
                };
                let owner = self.type_name_for_operand(object);
                if owner.is_empty() {
                    if let Some(self_ty) = self.mir_fn.param_types.first() {
                        if let Some(name) = nyar_type_layout_name(self_ty) {
                            return self.ctx.field_type(None, name.as_str(), field);
                        }
                    }
                    continue;
                }
                return self.ctx.field_type(None, owner.as_str(), field);
            }
        }
        None
    }

    fn function_index_for_operation(&self, operation: &QualifiedName) -> Option<i32> {
        if let Some(index) = self.function_index_by_name.get(&operation.to_string()) {
            return Some(*index);
        }
        if let Some(exec) = &self.submission.executable {
            if let Some(view) = exec.get_function(operation) {
                if let Some(index) = self.function_index_by_name.get(&view.function.symbol) {
                    return Some(*index);
                }
                let dotted = view.function.symbol.replace('.', "::");
                if let Some(index) = self.function_index_by_name.get(&dotted) {
                    return Some(*index);
                }
            }
        }
        None
    }

    /// 解析聚合 layout：优先 layout_id，否则回退 type_name。
    fn resolve_layout(&self, layout_id: Option<LayoutId>, type_name: &str) -> Option<AggregateLayout> {
        layout_id.and_then(|id| self.ctx.layout_by_id(id).cloned()).or_else(|| self.ctx.layout_by_type_name(type_name).cloned())
    }

    fn infer_layout_for_operand(&self, operand: &MirOperand) -> Option<AggregateLayout> {
        match operand {
            MirOperand::Value(value) => self
                .mir_fn
                .value_types
                .get(value)
                .and_then(|ty| match ty {
                    NyarType::Named(name) => self.ctx.layout_by_type_name(name.as_str()).cloned(),
                    _ => None,
                }),
            _ => None,
        }
    }

    fn type_name_for_operand(&self, operand: &MirOperand) -> String {
        match operand {
            MirOperand::Value(value) => match self.mir_fn.value_types.get(value) {
                Some(NyarType::Named(name)) => name.to_string(),
                _ => String::new(),
            },
            _ => String::new(),
        }
    }

    /// 按布局解析字段槽，供 `FieldGet`/`FieldSet` 使用（与 JVM 槽位合同对齐）。
    fn nyar_field_slot(&self, layout_id: Option<LayoutId>, type_name: &str, field: &str) -> i32 {
        let Some(layout) = self.ctx.layout_for_named_field(layout_id, type_name, field)
        else {
            return 0;
        };
        layout.fields.iter().position(|entry| entry.name == field).unwrap_or(0) as i32
    }

    fn ensure_nyar_layout(&mut self, aggregate: &AggregateLayout) -> i32 {
        if let Some(index) = self.layout_index_by_id.get(&aggregate.id) {
            return *index;
        }
        let index = self.layouts.len() as i32;
        self.layouts.push(NyarLayout { field_count: aggregate.fields.len() as i32 });
        self.layout_index_by_id.insert(aggregate.id, index);
        index
    }

    fn ensure_nyar_layout_count(&mut self, field_count: i32) -> i32 {
        let index = self.layouts.len() as i32;
        self.layouts.push(NyarLayout { field_count });
        index
    }

    /// `FieldSet` 栈效果为 `[obj, value] -> [obj]`；调用方随后 `Pop`。
    fn emit_field_set_slot(&mut self, object: &MirOperand, field_slot: i32, value: &MirOperand) {
        self.emit_operand(object);
        self.emit_operand(value);
        self.emitter.emit_imm1(NyarHeadCode::FieldSet, field_slot);
    }

    fn emit_block_argument_copies(&mut self, target: MirBlockRef, arguments: &[MirOperand]) {
        let target_block = self.mir_fn.blocks.iter().find(|block| block.id == target).expect("validated branch target");
        assert_eq!(arguments.len(), target_block.parameters.len(), "validated branch arity");
        for argument in arguments {
            self.emit_operand(argument);
        }
        for index in (0..arguments.len()).rev() {
            let param_local = self.slots.block_param_locals[&(target, index)];
            self.emit_store_local(param_local);
        }
    }

    fn emit_load_constant(&mut self, constant: &MirConstant) {
        match constant {
            MirConstant::Int(value) if *value >= i32::MIN as i64 && *value <= i32::MAX as i64 => {
                self.emitter.emit_const_i32(*value as i32);
            }
            MirConstant::Int(value) => {
                self.emitter.emit_const_i64(*value);
            }
            MirConstant::Float64(value) => {
                let index = self.emitter.constants.len() as i32 + self.emitter.constants_base;
                self.emitter.constants.push(NyarConstant::Float64(value.into_inner()));
                self.emitter.emit_imm1(NyarHeadCode::Const, index);
            }
            MirConstant::Bool(value) => {
                self.emitter.emit_const_bool(*value);
            }
            MirConstant::Utf8(text) => {
                let index = self.emitter.intern_string(text);
                self.emitter.emit_imm1(NyarHeadCode::Const, index);
            }
            MirConstant::Utf16(_) => panic!("nyar-vm lowering has no explicit UTF-16 text ABI"),
            MirConstant::Unit => {
                self.emitter.emit_const_i32(0);
            }
        }
    }

    fn emit_operand(&mut self, operand: &MirOperand) {
        assert!(self.try_emit_operand(operand), "validated operand has a planned local");
    }

    fn try_emit_operand(&mut self, operand: &MirOperand) -> bool {
        match operand {
            MirOperand::Value(value) => {
                if let Some(local) = self.slots.value_locals.get(value).copied() {
                    self.emit_load_local(local);
                    return true;
                }
                false
            }
            MirOperand::Constant(constant) => {
                self.emit_load_constant(constant);
                true
            }
            MirOperand::Symbol(_) => false,
        }
    }

    fn ensure_value_local(&mut self, value: MirValueRef) -> u16 {
        self.slots.value_locals[&value]
    }

    fn store_to_local(&mut self, value: MirValueRef) {
        let local = self.ensure_value_local(value);
        self.emit_store_local(local);
    }

    fn emit_load_local(&mut self, local: u16) {
        self.emitter.emit_imm1(NyarHeadCode::LoadLocal, local as i32);
    }

    fn emit_store_local(&mut self, local: u16) {
        self.emitter.emit_imm1(NyarHeadCode::StoreLocal, local as i32);
    }

    fn infer_numeric_width_from_pair(&self, lhs: &MirOperand, rhs: &MirOperand) -> NumericWidth {
        self.infer_numeric_width(lhs).max(self.infer_numeric_width(rhs))
    }

    fn infer_numeric_width(&self, operand: &MirOperand) -> NumericWidth {
        match self.operand_type(operand) {
            Some(NyarType::Integer64 { .. }) => NumericWidth::I64,
            _ => NumericWidth::I32,
        }
    }

    fn operand_type(&self, operand: &MirOperand) -> Option<NyarType> {
        match operand {
            MirOperand::Value(value) => self.mir_fn.value_types.get(value).cloned(),
            MirOperand::Constant(constant) => match constant {
                MirConstant::Int(value) if *value >= i32::MIN as i64 && *value <= i32::MAX as i64 => {
                    Some(NyarType::Integer32 { signed: true })
                }
                MirConstant::Int(_) => Some(NyarType::Integer64 { signed: true }),
                MirConstant::Bool(_) => Some(NyarType::Boolean),
                MirConstant::Utf8(_) => Some(NyarType::Utf8),
                _ => None,
            },
            _ => None,
        }
    }
}

fn nyar_type_layout_name(ty: &NyarType) -> Option<String> {
    nyar_types::layout_key_for_nyar_type(ty).or_else(|| match ty {
        NyarType::Named(name) => Some(name.as_str().to_string()),
        NyarType::Apply(base, _) => nyar_type_layout_name(base),
        _ => None,
    })
}

/// 工作负载阶段宿主符号：末段名 → (`nyar.host` 符号, 最小参数个数)。
fn host_phase_import(simple: &str) -> Option<(&'static str, usize)> {
    match simple {
        "begin_phase" => Some(("begin_phase", 1)),
        "end_phase" => Some(("end_phase", 1)),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum NumericWidth {
    I32,
    I64,
}

#[cfg(test)]
mod host_phase_tests {
    use super::host_phase_import;

    #[test]
    fn recognizes_phase_host_symbols() {
        assert_eq!(host_phase_import("begin_phase"), Some(("begin_phase", 1)));
        assert_eq!(host_phase_import("end_phase"), Some(("end_phase", 1)));
        assert_eq!(host_phase_import("console_log"), None);
    }
}

impl BytecodeEmitter<'_> {
    fn emit_const_i32(&mut self, value: i32) {
        let index = self.constants.len() as i32 + self.constants_base;
        self.constants.push(NyarConstant::Integer32(value));
        self.emit_imm1(NyarHeadCode::Const, index);
    }

    fn emit_const_i64(&mut self, value: i64) {
        if value >= i32::MIN as i64 && value <= i32::MAX as i64 {
            self.emit_const_i32(value as i32);
            self.emit_call_import("i32_to_i64", 1);
            return;
        }
        let index = self.constants.len() as i32 + self.constants_base;
        self.constants.push(NyarConstant::Integer32(value as i32));
        self.emit_imm1(NyarHeadCode::Const, index);
        self.emit_call_import("i32_to_i64", 1);
        let _ = index;
    }

    fn emit_const_bool(&mut self, value: bool) {
        let index = self.constants.len() as i32 + self.constants_base;
        self.constants.push(NyarConstant::Boolean(value));
        self.emit_imm1(NyarHeadCode::Const, index);
    }
}
