//! NyarVM bytecode lowering from semantic MIR.

use std::collections::BTreeMap;

use crate::{
    contracts::EffectKind,
    executable_provider::{
        ExecutableBlock as MirBlock, ExecutableBlockRef as MirBlockRef, ExecutableConstant as MirConstant, ExecutableFunction as MirFunction,
        ExecutableInstruction as MirInstruction, ExecutableInstructionKind as MirInstructionKind, ExecutableOperand as MirOperand,
        ExecutableTerminator as MirTerminator, ExecutableValueRef as MirValueRef, NyarType,
    },
};
use nyar::QualifiedName;
use nyar_types::{AggregateLayout, LayoutId};
use std_data::binary::nyar_ir::{NyarConstant, NyarExport, NyarExportKind, NyarFunction, NyarHeadCode, NyarModuleData};

use super::{
    executable::{ExecutableLoweringContext, block_label, collect_reachable_blocks, slots::ExecutableSlotPlan},
    intrinsic_opcode::{IntrinsicBinaryOp, IntrinsicBitwiseOp, IntrinsicCompareOp, IntrinsicOpcode},
    nyar_vm::operation_short_name,
    singleton::{augment_nyar_module_with_singletons, nyar_singleton_accessor_export_name, nyar_singleton_method_export_name},
};
use crate::FragmentSubmission;

struct BytecodeEmitter {
    constants: Vec<NyarConstant>,
    code_bytes: Vec<u8>,
    pending_jumps: Vec<(usize, MirBlockRef)>,
    block_starts: BTreeMap<MirBlockRef, usize>,
    constants_base: i32,
}

impl BytecodeEmitter {
    fn new(constants_base: i32) -> Self {
        Self { constants: Vec::new(), code_bytes: Vec::new(), pending_jumps: Vec::new(), block_starts: BTreeMap::new(), constants_base }
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

    fn emit_call_native(&mut self, name: &str, arg_count: i32) {
        let name_index = self.intern_string(name);
        self.code_bytes.push(NyarHeadCode::CallNative as u8);
        self.code_bytes.extend_from_slice(&name_index.to_le_bytes());
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
        version: 1,
        name: format!("{}__{}", super::sanitize_symbol(&submission.module_name), super::sanitize_symbol(submission.fragment_id.as_str())),
        constants: Vec::new(),
        functions: Vec::new(),
        imports: Vec::new(),
        exports: Vec::new(),
        witness_entries: Vec::new(),
        code_bytes: Vec::new(),
        globals: Vec::new(),
        init_function_indices: Vec::new(),
    };

    let function_index_by_name =
        module.functions.iter().enumerate().map(|(index, function)| (function.name.clone(), index as i32)).collect::<BTreeMap<_, _>>();

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
            let mut emitter = Bytecodenyar_emitter::new(constants_base);

            lower_mir_function_to_bytecode(submission, mir_fn, &function_index_by_name, &mut emitter);

            module.constants.extend(emitter.constants);
            module.code_bytes.extend_from_slice(&emitter.code_bytes);

            let arity = mir_fn.param_types.len() as i32;
            let local_count = ExecutableSlotPlan::plan_jvm(&ExecutableLoweringContext::new(submission), mir_fn).local_types.len() as i32;
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
    if operation.parts().len() == 2 {
        let type_name = operation.parts()[0].as_str();
        let method_name = operation.parts()[1].as_str();
        if submission.singleton_instances.iter().any(|plan| plan.name == type_name) {
            return nyar_singleton_method_export_name(type_name, method_name);
        }
    }
    operation_short_name(operation)
}

fn lower_mir_function_to_bytecode(
    submission: &FragmentSubmission,
    mir_fn: &MirFunction,
    function_index_by_name: &BTreeMap<String, i32>,
    emitter: &mut BytecodeEmitter,
) {
    let ctx = ExecutableLoweringContext::new(submission);
    let slots = ExecutableSlotPlan::plan_jvm(&ctx, mir_fn);
    let block_order = collect_reachable_blocks(mir_fn);

    for block_id in &block_order {
        emitter.block_starts.insert(*block_id, emitter.code_bytes.len());
    }

    let mut lowerer = NyarMirLowerer { submission, ctx, mir_fn, slots, emitter, function_index_by_name };
    if let Some(opcode) = mir_fn.intrinsic {
        if mir_fn.blocks.iter().all(|block| block.instructions.is_empty()) {
            let arguments = mir_fn
                .blocks
                .get(mir_fn.entry.0 as usize)
                .map(|block| block.parameters.iter().copied().map(MirOperand::Value).collect::<Vec<_>>())
                .unwrap_or_default();
            lowerer.emit_intrinsic_opcode(opcode, &arguments, None);
            lowerer.emitter.emit_plain(NyarHeadCode::Return);
            lowerer.emitter.patch_pending_jumps();
            return;
        }
    }
    for block_id in block_order {
        if let Some(block) = mir_fn.blocks.get(block_id.0 as usize) {
            lowerer.emit_block(block);
        }
    }
    lowerer.emitter.patch_pending_jumps();
}

struct NyarMirLowerer<'a> {
    submission: &'a FragmentSubmission,
    ctx: ExecutableLoweringContext<'a>,
    mir_fn: &'a MirFunction,
    slots: ExecutableSlotPlan,
    emitter: &'a mut BytecodeEmitter,
    function_index_by_name: &'a BTreeMap<String, i32>,
}

impl<'a> NyarMirLowerer<'a> {
    fn emit_block(&mut self, block: &MirBlock) {
        let _ = block_label(block.id);
        for instruction in &block.instructions {
            self.emit_instruction(instruction);
        }
        self.emit_terminator(block);
    }

    fn emit_instruction(&mut self, instruction: &MirInstruction) {
        match &instruction.kind {
            MirInstructionKind::LoadConstant { constant, .. } => {
                self.emit_load_constant(constant);
                if let Some(output) = instruction.output {
                    self.store_to_local(output);
                }
            }
            MirInstructionKind::StoreVar { name, value, .. } => {
                self.emit_operand(value);
                if let Some(local) = self.slots.var_locals.get(name).copied() {
                    self.emit_store_local(local);
                }
            }
            MirInstructionKind::Copy { source } => {
                self.emit_operand(source);
                if let Some(output) = instruction.output {
                    self.store_to_local(output);
                }
            }
            MirInstructionKind::StructNew { type_name, fields, layout_id, .. } => {
                // NyarVM 把所有聚合体统一表示为堆上 Record；Value 与 Reference storage
                // 在字节码层面共享同一条 `alloc_record` + 逐字段 `record_set` 路径，
                // 仅在前端语义上区分是否需要深拷贝。
                self.emit_alloc_record(type_name);
                let output = instruction.output.expect("StructNew must produce an output");
                self.store_to_local(output);
                let _ = self.resolve_layout(*layout_id, type_name);
                let output_operand = MirOperand::Value(output);
                for (field_name, value) in fields {
                    self.emit_record_set(&output_operand, field_name, value);
                }
            }
            MirInstructionKind::TupleNew { fields, layout_id, .. } => {
                // 元组字段在 layout 中以索引字符串 ("0","1",...) 命名，与
                // `layout_for_tuple` 保持一致；type_name 取自 layout 或回退到 "__tuple"。
                let layout = self.resolve_layout(*layout_id, "__tuple");
                let type_name = layout.as_ref().map(|item| item.name.as_str()).unwrap_or("__tuple");
                self.emit_alloc_record(type_name);
                let output = instruction.output.expect("TupleNew must produce an output");
                self.store_to_local(output);
                let output_operand = MirOperand::Value(output);
                for (index, value) in fields.iter().enumerate() {
                    let owned = layout
                        .as_ref()
                        .and_then(|item| item.fields.get(index))
                        .map(|item| item.name.clone())
                        .unwrap_or_else(|| index.to_string());
                    self.emit_record_set(&output_operand, &owned, value);
                }
            }
            MirInstructionKind::FixedArrayNew { items: fields, layout_id, .. } => {
                // 定长数组与元组同构：layout 字段名为索引字符串。
                let layout = self.resolve_layout(*layout_id, "__fixedarray");
                let type_name = layout.as_ref().map(|item| item.name.as_str()).unwrap_or("__fixedarray");
                self.emit_alloc_record(type_name);
                let output = instruction.output.expect("FixedArrayNew must produce an output");
                self.store_to_local(output);
                let output_operand = MirOperand::Value(output);
                for (index, value) in fields.iter().enumerate() {
                    let owned = layout
                        .as_ref()
                        .and_then(|item| item.fields.get(index))
                        .map(|item| item.name.clone())
                        .unwrap_or_else(|| index.to_string());
                    self.emit_record_set(&output_operand, &owned, value);
                }
            }
            MirInstructionKind::AggregateCopy { source, dest, layout_id } => {
                // 值语义拷贝：在 NyarVM 上必须分配一条新 Record 再逐字段复制，
                // 否则 dest 与 source 会共享同一对象 id，破坏值语义不变式。
                let (type_name, fields) = match self.ctx.layout_by_id(*layout_id) {
                    Some(layout) => (layout.name.clone(), layout.fields.clone()),
                    None => ("aggregate".to_string(), Vec::new()),
                };
                self.emit_alloc_record(&type_name);
                if let MirOperand::Value(dest_value) = dest {
                    self.store_to_local(*dest_value);
                }
                for field in &fields {
                    let field_index = self.emitter.intern_string(&field.name);
                    self.emit_operand(dest);
                    self.emitter.emit_imm1(NyarHeadCode::Const, field_index);
                    self.emit_operand(source);
                    self.emitter.emit_imm1(NyarHeadCode::Const, field_index);
                    self.emitter.emit_call_native("record_get", 2);
                    self.emitter.emit_call_native("record_set", 3);
                    self.emitter.emit_plain(NyarHeadCode::Pop);
                }
            }
            MirInstructionKind::FieldGet { object, field, layout_id, .. } => {
                // NyarVM 中 Value 与 Reference 路径同构：均为堆 Record，复用 record_get。
                let _ = layout_id;
                self.emit_operand(object);
                let field_index = self.emitter.intern_string(field);
                self.emitter.emit_imm1(NyarHeadCode::Const, field_index);
                self.emitter.emit_call_native("record_get", 2);
                if let Some(output) = instruction.output {
                    self.store_to_local(output);
                }
            }
            MirInstructionKind::FieldSet { object, field, value, layout_id, .. } => {
                // NyarVM 中 Value 与 Reference 路径同构：均为堆 Record，复用 record_set。
                let _ = layout_id;
                self.emit_operand(object);
                let field_index = self.emitter.intern_string(field);
                self.emitter.emit_imm1(NyarHeadCode::Const, field_index);
                self.emit_operand(value);
                self.emitter.emit_call_native("record_set", 3);
                // record_set 返回 Null，必须弹出以保持栈平衡。
                self.emitter.emit_plain(NyarHeadCode::Pop);
            }
            MirInstructionKind::Call { callee, arguments, .. } => {
                if let Some(opcode) = self.ctx.resolve_intrinsic_opcode(callee) {
                    self.emit_intrinsic_opcode(opcode, arguments, instruction.output);
                    return;
                }
                for argument in arguments {
                    self.emit_operand(argument);
                }
                if let MirOperand::Symbol(path) = callee {
                    if self.try_emit_singleton_call(path, arguments.len(), instruction.output) {
                        return;
                    }
                    if let Some(index) = self.resolve_function_index(path) {
                        self.emitter.emit_imm1(NyarHeadCode::Call, index);
                        if instruction.output.is_some() {
                            // keep result on stack
                        }
                    }
                }
                if let Some(output) = instruction.output {
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
                            // 无 payload 的 yield：压入 i32(0) 作为 unit 占位，满足 Yield 弹出语义。
                            self.emitter.emit_const_i32(0);
                        }
                        self.emitter.emit_plain(NyarHeadCode::Yield);
                        self.emit_block_argument_copies(*resume_target, &[]);
                        self.emitter.emit_jump_placeholder(NyarHeadCode::Jump, *resume_target);
                    }
                    EffectKind::Raise => {
                        // `raise expr`：将 payload 压栈后发射 `PerformEffect` opcode。
                        // operand1 = 常量池索引（effect 的 method_name 字符串），
                        // VM 用此字符串在 witness_entries 中查找 handler。
                        // handler 可选择 resume（控制流回到 resume_target，resume 值在栈顶）
                        // 或不 resume（控制流不返回此处）。
                        // 若 `Effectful::Resume = !`，resume 路径不可达，VM 在尝试 resume 时报错。
                        if let Some(payload) = payload {
                            self.emit_operand(payload);
                        }
                        else {
                            // 无 payload 的 raise：压入 i32(0) 作为 unit 占位。
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
                            self.emitter.emit_call_native("console_log", 1);
                        }
                        self.emit_block_argument_copies(*resume_target, &[]);
                        self.emitter.emit_jump_placeholder(NyarHeadCode::Jump, *resume_target);
                    }
                }
            }
            MirTerminator::YieldToRuntime { effect, payload, resume_state: _ } => {
                // 状态机重写后的 effect：与 `PerformEffect` 同构地发射对应 opcode。
                // resume_state 由后续 `StateDispatch` 在恢复时读取（暂以 fallthrough 桩处理）。
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
                            self.emitter.emit_call_native("console_log", 1);
                        }
                    }
                }
            }
            MirTerminator::StateDispatch { state, cases, default_target } => {
                // 状态机入口：读取 state local，与每个 case_key 比较，
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

    /// 解析聚合体 layout，优先按 layout_id 查找，缺失时回退到 type_name。
    fn resolve_layout(&self, layout_id: Option<LayoutId>, type_name: &str) -> Option<AggregateLayout> {
        layout_id.and_then(|id| self.ctx.layout_by_id(id).cloned()).or_else(|| self.ctx.layout_by_type_name(type_name).cloned())
    }

    /// 发射 `alloc_record(type_name)` 字节码序列，结果（新 Record 的对象 id）压栈。
    fn emit_alloc_record(&mut self, type_name: &str) {
        let name_index = self.emitter.intern_string(type_name);
        self.emitter.emit_imm1(NyarHeadCode::Const, name_index);
        self.emitter.emit_call_native("alloc_record", 1);
    }

    /// 发射 `record_set(object, field, value)` 字节码序列并弹出返回的 Null。
    fn emit_record_set(&mut self, object: &MirOperand, field: &str, value: &MirOperand) {
        self.emit_operand(object);
        let field_index = self.emitter.intern_string(field);
        self.emitter.emit_imm1(NyarHeadCode::Const, field_index);
        self.emit_operand(value);
        self.emitter.emit_call_native("record_set", 3);
        // record_set 返回 Null，必须弹出以保持栈平衡。
        self.emitter.emit_plain(NyarHeadCode::Pop);
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

    fn emit_intrinsic_opcode(&mut self, opcode: IntrinsicOpcode, arguments: &[MirOperand], output: Option<MirValueRef>) {
        match opcode {
            IntrinsicOpcode::Binary(op) => self.emit_intrinsic_binary(op, arguments, output),
            IntrinsicOpcode::Neg => {
                self.emit_operand(&arguments[0]);
                self.emitter.emit_call_native("i64_neg", 1);
                if let Some(output) = output {
                    self.store_to_local(output);
                }
            }
            IntrinsicOpcode::Compare(op) => {
                self.emit_operand(&arguments[0]);
                self.emit_operand(&arguments[1]);
                match self.infer_numeric_native(&arguments[0], &arguments[1]) {
                    NumericWidth::I64 => {
                        let native = match op {
                            IntrinsicCompareOp::Eq => "i64_eq",
                            IntrinsicCompareOp::Ne => "i64_ne",
                            IntrinsicCompareOp::Lt => "i64_lt",
                            IntrinsicCompareOp::Le => "i64_le",
                            IntrinsicCompareOp::Gt => "i64_gt",
                            IntrinsicCompareOp::Ge => "i64_ge",
                        };
                        self.emitter.emit_call_native(native, 2);
                    }
                    NumericWidth::I32 => {
                        let opcode = match op {
                            IntrinsicCompareOp::Eq => NyarHeadCode::I32Eq,
                            IntrinsicCompareOp::Ne => NyarHeadCode::I32Ne,
                            _ => NyarHeadCode::I32Eq,
                        };
                        self.emitter.emit_plain(opcode);
                    }
                }
                if let Some(output) = output {
                    self.store_to_local(output);
                }
            }
            IntrinsicOpcode::Not => {
                self.emit_operand(&arguments[0]);
                self.emitter.emit_call_native("bool_not", 1);
                if let Some(output) = output {
                    self.store_to_local(output);
                }
            }
            IntrinsicOpcode::Bitwise(op) => match op {
                IntrinsicBitwiseOp::And | IntrinsicBitwiseOp::Or => {
                    self.emit_operand(&arguments[0]);
                    self.emit_operand(&arguments[1]);
                    let native = if matches!(op, IntrinsicBitwiseOp::And) { "bool_and" } else { "bool_or" };
                    self.emitter.emit_call_native(native, 2);
                    if let Some(output) = output {
                        self.store_to_local(output);
                    }
                }
                _ => {}
            },
            IntrinsicOpcode::ArrayGet
            | IntrinsicOpcode::ArraySet
            | IntrinsicOpcode::ArrayLen
            | IntrinsicOpcode::ArrayPush
            | IntrinsicOpcode::Deref
            | IntrinsicOpcode::Utf8ScalarSlice
            | IntrinsicOpcode::Utf8ScalarLength => {}
            IntrinsicOpcode::Utf8ContentEqual
            | IntrinsicOpcode::Utf8ContentNotEqual
            | IntrinsicOpcode::Utf8Trim
            | IntrinsicOpcode::Utf8IndexOf
            | IntrinsicOpcode::Utf8Contains
            | IntrinsicOpcode::Utf8StartsWith
            | IntrinsicOpcode::Utf8EndsWith => {}
            IntrinsicOpcode::SumVariantIs | IntrinsicOpcode::SumStructuralEqual => {}
        }
    }

    fn emit_intrinsic_binary(&mut self, op: IntrinsicBinaryOp, arguments: &[MirOperand], output: Option<MirValueRef>) {
        self.emit_operand(&arguments[0]);
        self.emit_operand(&arguments[1]);
        let native = match self.infer_numeric_native(&arguments[0], &arguments[1]) {
            NumericWidth::I64 => match op {
                IntrinsicBinaryOp::Add => "i64_add",
                IntrinsicBinaryOp::Sub => "i64_sub",
                IntrinsicBinaryOp::Mul => "i64_mul",
                IntrinsicBinaryOp::Div => "i64_div",
                IntrinsicBinaryOp::Rem => "i64_rem",
            },
            NumericWidth::I32 => {
                let opcode = match op {
                    IntrinsicBinaryOp::Add => NyarHeadCode::I32Add,
                    IntrinsicBinaryOp::Sub => NyarHeadCode::I32Sub,
                    IntrinsicBinaryOp::Mul => NyarHeadCode::I32Mul,
                    IntrinsicBinaryOp::Div | IntrinsicBinaryOp::Rem => {
                        self.emitter.emit_call_native("i32_div", 2);
                        if let Some(output) = output {
                            self.store_to_local(output);
                        }
                        return;
                    }
                };
                self.emitter.emit_plain(opcode);
                if let Some(output) = output {
                    self.store_to_local(output);
                }
                return;
            }
        };
        self.emitter.emit_call_native(native, 2);
        if let Some(output) = output {
            self.store_to_local(output);
        }
    }

    fn infer_numeric_native(&self, lhs: &MirOperand, rhs: &MirOperand) -> NumericWidth {
        let ty = self.operand_type(lhs).or_else(|| self.operand_type(rhs)).unwrap_or(NyarType::Integer32 { signed: true });
        match ty {
            NyarType::Integer64 { .. } => NumericWidth::I64,
            _ => NumericWidth::I32,
        }
    }

    fn operand_type(&self, operand: &MirOperand) -> Option<NyarType> {
        match operand {
            MirOperand::Value(value) => self.mir_fn.value_types.get(value).cloned(),
            MirOperand::Constant(constant) => match constant {
                MirConstant::Int(value) if *value >= i32::MIN as i64 && *value <= i32::MAX as i64 => Some(NyarType::Integer32 { signed: true }),
                MirConstant::Int(_) => Some(NyarType::Integer64 { signed: true }),
                MirConstant::Bool(_) => Some(NyarType::Boolean),
                MirConstant::Utf8(_) => Some(NyarType::Utf8),
                _ => None,
            },
            _ => None,
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
        match operand {
            MirOperand::Value(value) => {
                if let Some(index) = self.parameter_index(*value) {
                    self.emitter.emit_imm1(NyarHeadCode::LoadArg, index as i32);
                    return;
                }
                if let Some(local) = self.slots.value_locals.get(value).copied() {
                    self.emit_load_local(local);
                }
            }
            MirOperand::Constant(constant) => self.emit_load_constant(constant),
            MirOperand::Symbol(path) => {
                if let Some(local) = self.slots.var_locals.get(&path.to_string()).copied() {
                    self.emit_load_local(local);
                }
            }
        }
    }

    fn store_to_local(&mut self, value: MirValueRef) {
        if let Some(local) = self.slots.value_locals.get(&value).copied() {
            self.emit_store_local(local);
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
}

enum NumericWidth {
    I32,
    I64,
}

impl BytecodeEmitter {
    fn emit_const_i32(&mut self, value: i32) {
        let index = self.constants.len() as i32 + self.constants_base;
        self.constants.push(NyarConstant::Integer32(value));
        self.emit_imm1(NyarHeadCode::Const, index);
    }

    fn emit_const_i64(&mut self, value: i64) {
        if value >= i32::MIN as i64 && value <= i32::MAX as i64 {
            self.emit_const_i32(value as i32);
            self.emit_call_native("i32_to_i64", 1);
            return;
        }
        let index = self.constants.len() as i32 + self.constants_base;
        self.constants.push(NyarConstant::Integer32(value as i32));
        self.emit_imm1(NyarHeadCode::Const, index);
        self.emit_call_native("i32_to_i64", 1);
        let _ = index;
    }

    fn emit_const_bool(&mut self, value: bool) {
        let index = self.constants.len() as i32 + self.constants_base;
        self.constants.push(NyarConstant::Boolean(value));
        self.emit_imm1(NyarHeadCode::Const, index);
    }
}
