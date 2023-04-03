//! NyarVM bytecode lowering from semantic MIR.

use std::collections::BTreeMap;

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
use crate::FragmentSubmission;

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
        self.code_bytes.push(opcode as u8);
    }

    fn emit_imm1(&mut self, opcode: NyarHeadCode, operand: i32) {
        self.code_bytes.push(opcode as u8);
        self.code_bytes.extend_from_slice(&operand.to_le_bytes());
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
        self.code_bytes.push(NyarHeadCode::CallImport as u8);
        self.code_bytes.extend_from_slice(&import_index.to_le_bytes());
        self.code_bytes.extend_from_slice(&arg_count.to_le_bytes());
    }

    /// 按 [`IntrinsicId`] 稠密下标调用内置；operand1 = bytecode index，operand2 = argc。
    fn emit_call_intrinsic(&mut self, intrinsic: IntrinsicId, arg_count: i32) {
        self.code_bytes.push(NyarHeadCode::CallIntrinsic as u8);
        self.code_bytes.extend_from_slice(&(intrinsic.bytecode_index() as i32).to_le_bytes());
        self.code_bytes.extend_from_slice(&arg_count.to_le_bytes());
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

    let function_index_by_name =
        module.functions.iter().enumerate().map(|(index, function)| (function.name.clone(), index as i32)).collect::<BTreeMap<_, _>>();
    let mut layout_index_by_id = BTreeMap::<LayoutId, i32>::new();

    if let Some(exec) = &submission.executable {
        for operation in exec.operations() {
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
                &mut emitter,
                &mut module.layouts,
                &mut layout_index_by_id,
            );

            module.constants.extend(emitter.constants);
            module.code_bytes.extend_from_slice(&emitter.code_bytes);

            let arity = mir_fn.param_types.len() as i32;
            let local_count = ExecutableSlotPlan::plan_nyar(&ExecutableLoweringContext::new(submission), mir_fn).local_types.len() as i32;
            let function_index = module.functions.len() as i32;
            module.functions.push(NyarFunction {
                name: export_name.clone(),
                arity,
                local_count: local_count.max(arity),
                code_offset,
                code_length: module.code_bytes.len() as i32 - code_offset,
            });
            module.exports.push(NyarExport { kind: NyarExportKind::Function, symbol_name: export_name, function_index });
        }
    }

    let function_index_by_name =
        module.functions.iter().enumerate().map(|(index, function)| (function.name.clone(), index as i32)).collect::<BTreeMap<_, _>>();
    augment_nyar_module_with_singletons(submission, &mut module, &function_index_by_name);

    module
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
                if let Some(local) = self.slots.var_locals.get(name).copied() {
                    self.emit_store_local(local);
                    if let MirOperand::Value(source) = value {
                        self.slots.value_locals.insert(*source, local);
                    }
                    if let Some(output) = output {
                        self.slots.value_locals.insert(output, local);
                    }
                }
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
                let layout = self.resolve_layout(None, "__fixedarray");
                let layout_index = match &layout {
                    Some(aggregate) => self.ensure_nyar_layout(aggregate),
                    None => self.ensure_nyar_layout_count(elements.len() as i32),
                };
                self.emitter.emit_imm1(NyarHeadCode::ObjectNew, layout_index);
                let output = output.expect("ArrayFromElements must produce an output");
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
                }
                for argument in arguments {
                    self.emit_operand(argument);
                }
                if let MirOperand::Symbol(path) = callee {
                    if self.try_emit_singleton_call(path, arguments.len(), output) {
                        return;
                    }
                    if let Some(index) = self.resolve_function_index(path) {
                        self.emitter.emit_imm1(NyarHeadCode::Call, index);
                        if let Some(output) = output {
                            self.store_to_local(output);
                        }
                        return;
                    }
                }
                for _ in 0..arguments.len() {
                    self.emitter.emit_plain(NyarHeadCode::Pop);
                }
            }
            MirInstructionKind::ArrayGet { array, index } => {
                self.emit_operand(array);
                self.emit_operand(index);
                self.emitter.emit_call_intrinsic(IntrinsicId::ArrayGet, 2);
                if let Some(output) = output {
                    self.store_to_local(output);
                }
            }
            MirInstructionKind::ArraySet { array, index, value } => {
                self.emit_operand(array);
                self.emit_operand(index);
                self.emit_operand(value);
                self.emitter.emit_call_intrinsic(IntrinsicId::ArraySet, 3);
                self.emitter.emit_plain(NyarHeadCode::Pop);
            }
            MirInstructionKind::ArrayLength { array } => {
                self.emit_operand(array);
                self.emitter.emit_call_intrinsic(IntrinsicId::ArrayLen, 1);
                if let Some(output) = output {
                    self.store_to_local(output);
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
        true
    }

    fn try_emit_singleton_call(&mut self, path: &nyar::NamePath, arg_count: usize, output: Option<MirValueRef>) -> bool {
        if path.parts().len() != 2 {
            return false;
        }
        let type_name = path.parts()[0].as_str();
        let method_name = path.parts()[1].as_str();
        let Some(plan) = self.submission.singleton_instances.iter().find(|plan| plan.name == type_name)
        else {
            return false;
        };
        let export_name = if method_name == plan.accessor_method() && arg_count == 0 {
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
        self.emitter.emit_imm1(NyarHeadCode::Call, index);
        if let Some(output) = output {
            self.store_to_local(output);
        }
        true
    }

    fn resolve_function_index(&self, path: &nyar::NamePath) -> Option<i32> {
        if path.parts().len() == 2 {
            let export_name = nyar_singleton_method_export_name(path.parts()[0].as_str(), path.parts()[1].as_str());
            if let Some(index) = self.function_index_by_name.get(&export_name) {
                return Some(*index);
            }
        }
        let simple = path.parts().last().map(|part| part.as_str()).unwrap_or_default();
        self.function_index_by_name.iter().find(|(name, _)| name.ends_with(simple) || name.contains(simple)).map(|(_, index)| *index)
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
            self.emit_operand(argument);
            self.emit_store_local(param_local);
            self.slots.value_locals.insert(*parameter, param_local);
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
        self.try_emit_operand(operand);
    }

    fn try_emit_operand(&mut self, operand: &MirOperand) -> bool {
        match operand {
            MirOperand::Value(value) => {
                if let Some(name) = self.value_binding_name(*value) {
                    if let Some(local) = self.slots.var_locals.get(name).copied() {
                        self.emit_load_local(local);
                        return true;
                    }
                }
                if let Some(local) = self.slots.value_locals.get(value).copied() {
                    self.emit_load_local(local);
                    return true;
                }
                if let Some(index) = self.parameter_index(*value) {
                    self.emitter.emit_imm1(NyarHeadCode::LoadArg, index as i32);
                    return true;
                }
                if let Some(local) = self.block_parameter_local(*value) {
                    self.emit_load_local(local);
                    return true;
                }
                if let Some(name) = self.value_binding_name(*value) {
                    if let Some(named_value) = self.find_named_value(name) {
                        if let Some(local) = self.slots.value_locals.get(&named_value).copied() {
                            self.emit_load_local(local);
                            return true;
                        }
                    }
                }
                false
            }
            MirOperand::Constant(constant) => {
                self.emit_load_constant(constant);
                true
            }
            MirOperand::Symbol(path) => {
                if let Some(local) = self.slots.var_locals.get(&path.to_string()).copied() {
                    self.emit_load_local(local);
                    return true;
                }
                if let Some(name) = path.parts().last().map(|part| part.as_str()) {
                    if let Some(value) = self.find_named_value(name) {
                        if let Some(local) = self.slots.value_locals.get(&value).copied() {
                            self.emit_load_local(local);
                            return true;
                        }
                    }
                }
                false
            }
        }
    }

    fn value_binding_name(&self, value: MirValueRef) -> Option<&str> {
        self.mir_fn.values.iter().find(|candidate| candidate.id == value).and_then(|candidate| match &candidate.origin {
            ValueOrigin::Parameter { name, .. }
            | ValueOrigin::LetBinding { name }
            | ValueOrigin::BlockParameter { name, .. }
            | ValueOrigin::MutRefBinding { name }
            | ValueOrigin::PinMutRefBinding { name } => Some(name.as_str()),
            _ => None,
        })
    }

    fn find_named_value(&self, name: &str) -> Option<MirValueRef> {
        self.mir_fn.values.iter().find_map(|value| match &value.origin {
            ValueOrigin::Parameter { name: binding, .. }
            | ValueOrigin::LetBinding { name: binding }
            | ValueOrigin::BlockParameter { name: binding, .. }
            | ValueOrigin::MutRefBinding { name: binding }
            | ValueOrigin::PinMutRefBinding { name: binding } if binding == name => Some(value.id),
            _ => None,
        })
    }

    fn store_to_local(&mut self, value: MirValueRef) {
        if let Some(local) = self.slots.value_locals.get(&value).copied() {
            self.emit_store_local(local);
        }
        else {
            self.emitter.emit_plain(NyarHeadCode::Pop);
        }
    }

    fn emit_load_local(&mut self, local: u16) {
        self.emitter.emit_imm1(NyarHeadCode::LoadLocal, local as i32);
    }

    fn emit_store_local(&mut self, local: u16) {
        self.emitter.emit_imm1(NyarHeadCode::StoreLocal, local as i32);
    }

    fn parameter_index(&self, value: MirValueRef) -> Option<usize> {
        let entry = self.mir_fn.blocks.get(self.mir_fn.entry.0 as usize)?;
        entry.parameters.iter().position(|parameter| *parameter == value)
    }

    fn block_parameter_local(&self, value: MirValueRef) -> Option<u16> {
        for block in &self.mir_fn.blocks {
            for (index, parameter) in block.parameters.iter().enumerate() {
                if *parameter == value {
                    return self.slots.block_param_locals.get(&(block.id, index)).copied();
                }
            }
        }
        None
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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum NumericWidth {
    I32,
    I64,
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
