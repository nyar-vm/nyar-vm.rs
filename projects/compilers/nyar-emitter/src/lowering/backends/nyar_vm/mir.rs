//! NyarVM bytecode lowering from semantic MIR.

use std::collections::BTreeMap;

use crate::{
    contracts::{EffectKind, ValueOrigin},
    backend_plan_views::{
        ExecutableBlock as MirBlock, ExecutableBlockRef as MirBlockRef, ExecutableConstant as MirConstant, ExecutableFunction as MirFunction,
        ExecutableInstruction as MirInstruction, ExecutableInstructionKind as MirInstructionKind, ExecutableOperand as MirOperand,
        ExecutableTerminator as MirTerminator, ExecutableValueRef as MirValueRef, NyarType,
    },
};
use nyar::NamePath;
use nyar_types::{AggregateLayout, IntrinsicId, ItemInstanceId, LayoutId};
use nyar_bytecode::{
    NyarConstant, NyarExport, NyarExportKind, NyarFunction, NyarHeadCode, NyarImport, NyarImportKind, NyarLayout, NyarModuleData,
    NYAR_VERSION,
};

use super::{
    executable::{ExecutableLoweringContext, block_label, collect_reachable_blocks, slots::ExecutableSlotPlan},
};
use crate::{BackendPrivatePlan, FragmentSubmission};
use miette::Result;

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
pub(crate) fn lower_fragment_mir_to_nyar_module(submission: &FragmentSubmission) -> Result<NyarModuleData> {
    for instance in submission.backend_plan.instances() {
        let Some(view) = submission.backend_plan.get_function(&instance) else { continue; };
        if view.function.blocks.iter().flat_map(|block| &block.instructions).any(|instruction| matches!(
            &instruction.kind,
            MirInstructionKind::SumNew { .. } | MirInstructionKind::SumPayloadGet { .. } | MirInstructionKind::SumVariantIs { .. }
        )) {
            return Err(miette::miette!("Nyar backend requires a typed sum representation plan; semantic sum operation has no target contract"));
        }
    }
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

    let mut function_index_by_instance = BTreeMap::<ItemInstanceId, i32>::new();
    {
        let exec = &submission.backend_plan;
        let operations: Vec<ItemInstanceId> = exec
            .instances()
            .into_iter()
            .filter(|instance| exec.get_function(instance).is_some())
            .collect();
        for (index, instance) in operations.iter().enumerate() {
            function_index_by_instance.insert(*instance, index as i32);
        }
        let function_entry_arities = build_nyar_function_entry_arities(exec.as_ref(), &operations);

        for instance in operations {
            let Some(view) = exec.get_function(&instance)
            else {
                continue;
            };
            let mir_fn = &view.function;
            let export_name = nyar_mir_export_name(submission, instance);
            let code_offset = module.code_bytes.len() as i32;
            let constants_base = module.constants.len() as i32;
            let mut emitter = BytecodeEmitter::new(constants_base, &mut module.imports);

            lower_mir_function_to_bytecode(
                submission,
                mir_fn,
                &function_index_by_instance,
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
            let local_count = ExecutableSlotPlan::plan_nyar(mir_fn).local_types.len() as i32;
            let function_index = module.functions.len() as i32;
            module.functions.push(NyarFunction {
                name: export_name.clone(),
                arity,
                local_count: local_count.max(arity),
                code_offset,
                code_length: module.code_bytes.len() as i32 - code_offset,
            });
            if nyar_should_export_operation(submission, instance) {
                module.exports.push(NyarExport { kind: NyarExportKind::Function, symbol_name: export_name, function_index });
            }
        }
    }

    Ok(module)
}

/// 在降低函数体之前登记全部 operation → 稠密下标，供 `Call` 解析（含前向引用）。
fn build_nyar_function_entry_arities(
    exec: &BackendPrivatePlan,
    operations: &[ItemInstanceId],
) -> BTreeMap<i32, usize> {
    let mut map = BTreeMap::new();
    for (index, instance) in operations.iter().enumerate() {
        let Some(view) = exec.get_function(instance) else {
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

/// 库模式只导出用户 `[export]` / `exported_operations`；闭包内 std 辅助函数保持内部 `Call` 可见性。
fn nyar_should_export_operation(submission: &FragmentSubmission, operation: ItemInstanceId) -> bool {
    if submission.wasm_export_names.contains_key(&operation) {
        return true;
    }
    submission.exported_operations.iter().any(|exported| *exported == operation)
}

fn nyar_mir_export_name(submission: &FragmentSubmission, operation: ItemInstanceId) -> String {
    if let Some(public_name) = submission.wasm_export_names.get(&operation) {
        return public_name.clone();
    }
    submission.backend_plan.abi_name_for_instance(operation)
        .map(|name| name.to_string())
        .unwrap_or_else(|| panic!("函数实例缺少 ABI 标签: {operation:?}"))
}

fn lower_mir_function_to_bytecode(
    submission: &FragmentSubmission,
    mir_fn: &MirFunction,
    function_index_by_instance: &BTreeMap<ItemInstanceId, i32>,
    function_entry_arities: &BTreeMap<i32, usize>,
    emitter: &mut BytecodeEmitter<'_>,
    layouts: &mut Vec<NyarLayout>,
    layout_index_by_id: &mut BTreeMap<LayoutId, i32>,
) {
    let ctx = ExecutableLoweringContext::new(submission);
    let slots = ExecutableSlotPlan::plan_nyar(mir_fn);
    let block_order = collect_reachable_blocks(mir_fn);

    let mut lowerer = NyarMirLowerer {
        submission,
        ctx,
        mir_fn,
        slots,
        emitter,
        function_index_by_instance,
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
    function_index_by_instance: &'a BTreeMap<ItemInstanceId, i32>,
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
            MirInstructionKind::StructNew { nominal, fields } => {
                let layout = self.ctx.layout_by_nominal(*nominal).cloned().unwrap_or_else(|| panic!("Nyar 聚合身份缺少布局合同: {nominal:?}"));
                let layout_index = match &layout {
                    aggregate => self.ensure_nyar_layout(aggregate),
                };
                self.emitter.emit_imm1(NyarHeadCode::ObjectNew, layout_index);
                let output = output.expect("StructNew must produce an output");
                self.store_to_local(output);
                let output_operand = MirOperand::Value(output);
                for (field, value) in fields.iter() {
                    let slot = self.nyar_field_slot(*field);
                    self.emit_field_set_slot(&output_operand, slot, value);
                    self.emitter.emit_plain(NyarHeadCode::Pop);
                }
            }
            MirInstructionKind::TupleNew { layout_id, fields } => {
                let layout = self.ctx.layout_by_id(*layout_id).cloned().unwrap_or_else(|| panic!("Nyar tuple 布局身份缺少布局合同: {layout_id:?}"));
                let layout_index = self.ensure_nyar_layout(&layout);
                self.emitter.emit_imm1(NyarHeadCode::ObjectNew, layout_index);
                let output = output.expect("TupleNew must produce an output");
                self.store_to_local(output);
                let output_operand = MirOperand::Value(output);
                for (index, value) in fields.iter().enumerate() {
                    self.emit_field_set_slot(&output_operand, index as i32, value);
                    self.emitter.emit_plain(NyarHeadCode::Pop);
                }
            }
            MirInstructionKind::ArrayFromElements { layout_id, elements, .. } => {
                let Some(output) = output else {
                    return;
                };
                let layout = self.ctx.layout_by_id(*layout_id).cloned().unwrap_or_else(|| panic!("Nyar fixed-array 布局身份缺少布局合同: {layout_id:?}"));
                let layout_index = self.ensure_nyar_layout(&layout);
                self.emitter.emit_imm1(NyarHeadCode::ObjectNew, layout_index);
                self.store_to_local(output);
                let output_operand = MirOperand::Value(output);
                for (index, value) in elements.iter().enumerate() {
                    self.emit_field_set_slot(&output_operand, index as i32, value);
                    self.emitter.emit_plain(NyarHeadCode::Pop);
                }
            }
            MirInstructionKind::AggregateCopy { layout_id, source, dest } => {
                let layout = self.ctx.layout_by_id(*layout_id).cloned().unwrap_or_else(|| panic!("Nyar aggregate-copy 布局身份缺少布局合同: {layout_id:?}"));
                let field_count = layout.fields.len() as i32;
                let layout_index = self.ensure_nyar_layout(&layout);
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
                let slot = self.nyar_field_slot(*field);
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
                let slot = self.nyar_field_slot(*field);
                self.emit_field_set_slot(object, slot, value);
                self.emitter.emit_plain(NyarHeadCode::Pop);
            }
            MirInstructionKind::Call { callee, arguments } => {
                if let MirOperand::Item(instance) = callee {
                    let index = self.function_index_by_instance.get(instance).copied().unwrap_or_else(|| {
                        panic!("validated callable identity has no Nyar function index: {instance:?}")
                    });
                    let name = self.submission.backend_plan.abi_name_for_instance(*instance).unwrap_or_else(|| {
                        panic!("validated callable identity has no diagnostic name: {instance:?}")
                    });
                    let path = NamePath::new(name.parts().to_vec());
                    self.emit_direct_call(index, &path, arguments, output);
                    return;
                }
                panic!("Semantic MIR 调用未绑定 ItemInstanceId，拒绝进入 Nyar 后端: {callee:?}");
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

    /// Call 实参只能读取已规划的 SSA/local 值。
    fn emit_call_operand(&mut self, operand: &MirOperand) -> bool {
        if self.try_emit_operand(operand) {
            return true;
        }
        false
    }

    fn nyar_field_slot(&self, field: nyar_types::FieldId) -> i32 {
        self.ctx.submission.aggregate_layout_by_field.get(&field).map(|(_, slot)| *slot as i32).unwrap_or_else(|| panic!("Nyar 字段身份缺少布局槽位合同: {field:?}"))
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
            MirOperand::Item(_) => false,
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
