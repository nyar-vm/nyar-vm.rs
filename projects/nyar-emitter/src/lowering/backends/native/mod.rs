use crate::executable_provider::{
    ExecutableBlockRef as MirBlockRef, ExecutableConstant as MirConstant, ExecutableDispatchKind as MirDispatchKind,
    ExecutableFunction as MirFunction, ExecutableInstruction as MirInstruction, ExecutableInstructionKind as MirInstructionKind,
    ExecutableOperand as MirOperand, ExecutableReceiverPassingKind as ReceiverPassingKind, ExecutableStorageKind as MirStorageKind,
    ExecutableStorageKind as StorageKind, ExecutableTerminator as MirTerminator, ExecutableValueRef as MirValueRef, NyarType,
};
use miette::Result;
use nyar::{ExternalCallArgument, ExternalCallEdge, SuspendFunctionArtifact, SuspendStateArtifact};
use nyar_types::{AggregateLayout, Identifier, LayoutId, NamePath};
use std_data::binary::{
    aarch64::{emit_jni_glue_module, merge_jni_and_logic, ret_bytes},
    elf::{NativeElfImageBuilder, SharedElfWriter, SharedObjectExport},
    pe::NativeImageBuilder,
    x86_64::{ConditionCode, MsvcFunctionBuilder, Reg64, SysvFunctionBuilder, X64Instruction},
};

use super::{
    executable::{ExecutableLoweringContext, block_label, collect_reachable_blocks},
    interop::{linux_gnu_host_import_target, win32_host_import_target},
    intrinsic_opcode::{IntrinsicBinaryOp, IntrinsicBitwiseOp, IntrinsicCompareOp, IntrinsicOpcode},
    sanitize_symbol,
    suspend_sm::{dispatch_case_keys, resolve_state_for_case},
    suspend_witness::{
        frame_has_field, primary_witness_binding, resolve_witness_slot, secondary_witness_binding, tertiary_witness_binding,
        witness_receiver_field,
    },
    witness::{
        emit_witness_dispatch_x64_msvc, emit_witness_dispatch_x64_sysv, emit_witness_tables, lower_witness_calls_linux,
        lower_witness_calls_windows,
    },
};
use crate::FragmentSubmission;

/// Native ??????? ABI ?????????
///
/// ??? System V AMD64 ABI ? Windows x64 ABI????????????????????
/// ?? / ????????????????????????? ABI ??????????
const NATIVE_REGISTER_PASS_LIMIT: u32 = 16;

/// Native ????????? `RSP` ????????
///
/// ????? shadow space / ?? spill ?????????????? inline ???
pub(crate) const NATIVE_VALUE_AREA_BASE: i32 = 0x40;

/// ????????????8 ??????? i32/i64/f64/????
const NATIVE_SLOT_SIZE: i32 = 8;

/// Native ???? SSA ????????????
///
/// ??? SSA `MirValueRef` ? `StoreVar` ????????? `RSP` ? 8 ?????
/// ??? `NATIVE_VALUE_AREA_BASE`??? `Jump` ? `arguments` ????????
/// ???? block ?????? loop-carried variable ??????
#[derive(Debug, Clone)]
struct NativeSlotPlan {
    /// SSA value ? ????
    value_offsets: std::collections::BTreeMap<MirValueRef, i32>,
    /// ??? ? ????
    var_offsets: std::collections::BTreeMap<String, i32>,
    /// (???, ????) ? ????
    block_param_offsets: std::collections::BTreeMap<(MirBlockRef, usize), i32>,
    /// ????????
    next_offset: i32,
}

impl NativeSlotPlan {
    /// ?????? SSA value?????????????????? `NATIVE_VALUE_AREA_BASE`?
    fn plan(mir_fn: &MirFunction) -> Self {
        Self::plan_with_base(mir_fn, NATIVE_VALUE_AREA_BASE)
    }

    /// ?????? SSA value?????????????????? `base`?
    ///
    /// ?? MIR ?????? PE/ELF ??????????????
    /// `NATIVE_VALUE_AREA_BASE` ??????callee ??? caller ??????
    /// ?????????? `base` ??????????
    fn plan_with_base(mir_fn: &MirFunction, base: i32) -> Self {
        let mut plan = Self {
            value_offsets: std::collections::BTreeMap::new(),
            var_offsets: std::collections::BTreeMap::new(),
            block_param_offsets: std::collections::BTreeMap::new(),
            next_offset: base,
        };
        for block in &mir_fn.blocks {
            for (index, parameter) in block.parameters.iter().enumerate() {
                let offset = plan.alloc_slot();
                plan.block_param_offsets.insert((block.id, index), offset);
                plan.value_offsets.insert(*parameter, offset);
            }
        }
        for block in &mir_fn.blocks {
            for instruction in &block.instructions {
                plan.collect_instruction(instruction);
            }
        }
        plan
    }

    /// ??? `plan(mir_fn).total_size()`????? BTreeMap?
    /// ? `reserve_stack` ? prologue ??????????
    fn plan_max(mir_fn: &MirFunction) -> u32 {
        Self::plan(mir_fn).total_size()
    }

    fn collect_instruction(&mut self, instruction: &MirInstruction) {
        match &instruction.kind {
            MirInstructionKind::StoreVar { name, .. } => {
                if !self.var_offsets.contains_key(name) {
                    let offset = self.alloc_slot();
                    self.var_offsets.insert(name.clone(), offset);
                }
                // `StoreVar` ?????? `output` SSA ???? `Call` ?????
                // `MirOperand::Value(output)` ??????????????
                // `emit_load_operand_to_reg` ? fallback ? `mov reg, 0`?
                // ?? callee ???????
                if let Some(output) = instruction.output {
                    if !self.value_offsets.contains_key(&output) {
                        let offset = self.alloc_slot();
                        self.value_offsets.insert(output, offset);
                    }
                }
            }
            _ => {
                if let Some(output) = instruction.output {
                    if !self.value_offsets.contains_key(&output) {
                        let offset = self.alloc_slot();
                        self.value_offsets.insert(output, offset);
                    }
                }
            }
        }
    }

    fn alloc_slot(&mut self) -> i32 {
        let offset = self.next_offset;
        self.next_offset += NATIVE_SLOT_SIZE;
        offset
    }

    fn value_offset(&self, value: &MirValueRef) -> Option<i32> {
        self.value_offsets.get(value).copied()
    }

    fn var_offset(&self, name: &str) -> Option<i32> {
        self.var_offsets.get(name).copied()
    }

    fn block_param_offset(&self, block: MirBlockRef, index: usize) -> Option<i32> {
        self.block_param_offsets.get(&(block, index)).copied()
    }

    /// ?????????????????????????????????
    fn total_size(&self) -> u32 {
        (self.next_offset - self.base()).max(0) as u32
    }

    /// ??? plan ????????
    fn base(&self) -> i32 {
        // `value_offsets` / `var_offsets` / `block_param_offsets` ???????? base?
        // ??????? SSA value / ?? / ???????? `NATIVE_VALUE_AREA_BASE`?
        self.value_offsets
            .values()
            .chain(self.var_offsets.values())
            .chain(self.block_param_offsets.values())
            .copied()
            .min()
            .unwrap_or(NATIVE_VALUE_AREA_BASE)
    }
}

/// ?? MSVC / SysV ?????????????
///
/// `MsvcFunctionBuilder` ? `SysvFunctionBuilder` ??? `push(X64Instruction)`?
/// ?????? trait?? trait ???????????????????? ABI ???
/// lowering helper ??????????????? ABI ???????
trait X64Emitter {
    /// ??? `X64Instruction` ????????????
    fn emit(&mut self, instruction: X64Instruction);
    /// ????????????? `Return` terminator ???? `ret`?
    fn stack_reserve(&self) -> u32;
    /// ?? true ?????? SysV `_start` ???????????????
    /// `Return` terminator ??? `ret`???? `exit` syscall ???
    fn is_sysv_entry(&self) -> bool;
    /// ???? ABI ???????????? `emit_call` ??? callee ?? prologue ???
    /// MSVC: `[RCX, RDX, R8, R9]`?SysV: `[RDI, RSI, RDX, RCX, R8, R9]`?
    fn arg_registers(&self) -> &'static [Reg64];
}

impl X64Emitter for MsvcFunctionBuilder {
    fn emit(&mut self, instruction: X64Instruction) {
        self.push(instruction);
    }
    fn stack_reserve(&self) -> u32 {
        self.stack_reserve()
    }
    fn is_sysv_entry(&self) -> bool {
        false
    }
    fn arg_registers(&self) -> &'static [Reg64] {
        &[Reg64::Rcx, Reg64::Rdx, Reg64::R8, Reg64::R9]
    }
}

impl X64Emitter for SysvFunctionBuilder {
    fn emit(&mut self, instruction: X64Instruction) {
        self.push(instruction);
    }
    fn stack_reserve(&self) -> u32 {
        self.stack_reserve()
    }
    fn is_sysv_entry(&self) -> bool {
        true
    }
    fn arg_registers(&self) -> &'static [Reg64] {
        &[Reg64::Rdi, Reg64::Rsi, Reg64::Rdx, Reg64::Rcx, Reg64::R8, Reg64::R9]
    }
}

/// Native ????????? ABI ???
///
/// ?? System V AMD64 ABI ? Windows x64 ABI ??????????????
/// - ??? `NATIVE_REGISTER_PASS_LIMIT`?16??????????????? / ???
/// - ?????????????????????????????
///
/// ???????? / ????????????? `FieldLayout.offset`????????
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeValueAbi {
    /// ??????????
    pub size: u32,
    /// ??????????
    pub align: u32,
    /// ????????????`size <= NATIVE_REGISTER_PASS_LIMIT`??
    pub passes_in_registers: bool,
    /// ????????`size > NATIVE_REGISTER_PASS_LIMIT`??
    pub passes_by_reference: bool,
    /// ???? `RAX` / `RDX` ??????`size <= NATIVE_REGISTER_PASS_LIMIT`??
    pub returns_in_registers: bool,
    /// ?????????????`size > NATIVE_REGISTER_PASS_LIMIT`??
    pub returns_via_hidden_pointer: bool,
}

/// ???????? Native ABI ?? / ?????
///
/// ???? `AggregateLayoutPlan` ?????? `AggregateLayout`???? `NativeValueAbi`
/// ???? Native lowering ??? / ?? slot ?????
pub(crate) fn classify_aggregate_for_abi(layout: &AggregateLayout) -> NativeValueAbi {
    let size = layout.size;
    let align = layout.align;
    let fits_in_registers = size <= NATIVE_REGISTER_PASS_LIMIT;
    NativeValueAbi {
        size,
        align,
        passes_in_registers: fits_in_registers,
        passes_by_reference: !fits_in_registers,
        returns_in_registers: fits_in_registers,
        returns_via_hidden_pointer: !fits_in_registers,
    }
}

/// ?? submission ??? MIR ?????????????????
///
/// ??? `StorageKind::Value` ? `AggregateLayout.size`??? PE / ELF prologue
/// ??????????????????????????????
pub(crate) fn native_value_area_size(submission: &FragmentSubmission) -> u32 {
    let mut total: u32 = 0;
    if let Some(exec) = &submission.executable {
        for operation in exec.operations() {
            let Some(view) = exec.get_function(&operation)
            else {
                continue;
            };
            for block in &view.function.blocks {
                for instruction in &block.instructions {
                    if let Some(layout_id) = value_layout_id_for_instruction(&instruction.kind) {
                        if let Some(layout) = submission.aggregate_layouts.layouts.iter().find(|item| item.id == layout_id) {
                            if layout.storage == StorageKind::Value {
                                total = total.saturating_add(layout.size);
                            }
                        }
                    }
                }
            }
        }
    }
    total
}

/// ?? submission ?? MIR ??? layout?aarch64 / Android ?????
///
/// ??? `mir_functions` ??????? `AggregateLayout` ???
/// `classify_aggregate_for_abi` ?? ABI ????? layout ?????
/// aarch64 ??? codegen ????????
pub(crate) fn consume_mir_value_layouts(submission: &FragmentSubmission) {
    if let Some(exec) = &submission.executable {
        for operation in exec.operations() {
            let Some(view) = exec.get_function(&operation)
            else {
                continue;
            };
            for block in &view.function.blocks {
                for instruction in &block.instructions {
                    if let Some(layout_id) = value_layout_id_for_instruction(&instruction.kind) {
                        if let Some(layout) = submission.aggregate_layouts.layouts.iter().find(|item| item.id == layout_id) {
                            if layout.storage == StorageKind::Value {
                                let _ = classify_aggregate_for_abi(layout);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// ? MIR ??????????? `LayoutId`??????????????
fn value_layout_id_for_instruction(kind: &MirInstructionKind) -> Option<LayoutId> {
    match kind {
        MirInstructionKind::StructNew { layout_id, storage, .. }
        | MirInstructionKind::TupleNew { layout_id, storage, .. }
        | MirInstructionKind::FixedArrayNew { layout_id, storage, .. }
            if *storage == StorageKind::Value =>
        {
            *layout_id
        }
        MirInstructionKind::AggregateCopy { layout_id, .. } => Some(*layout_id),
        MirInstructionKind::FieldGet { layout_id, storage, .. } | MirInstructionKind::FieldSet { layout_id, storage, .. }
            if *storage == StorageKind::Value =>
        {
            *layout_id
        }
        _ => None,
    }
}

/// ????? lowering ?????????PE / ELF??
pub(crate) fn lower_fragment_to_native_executable(submission: &FragmentSubmission, host_flavor: &str) -> Result<(Vec<u8>, String)> {
    match host_flavor {
        "android-native" => lower_android_elf_shared(submission),
        flavor if is_linux_host(flavor) => lower_linux_elf(submission),
        _ => lower_windows_pe(submission),
    }
}

fn is_linux_host(host_flavor: &str) -> bool {
    matches!(host_flavor, "linux-gnu" | "linux")
}

fn lower_android_elf_shared(submission: &FragmentSubmission) -> Result<(Vec<u8>, String)> {
    // aarch64 MIR ??? lowering ?????????????? layout???
    // `mir_functions` / `aggregate_layouts` ?????aarch64 ??? codegen ???????
    consume_mir_value_layouts(submission);

    let mut logic_text = Vec::new();
    let mut logic_exports = Vec::new();

    let invoke_offset = u32::try_from(logic_text.len()).unwrap_or(0);
    logic_text.extend_from_slice(&ret_bytes());
    logic_exports.push(SharedObjectExport { name: "asgard_invoke_export".into(), text_offset: invoke_offset });

    for operation in &submission.exported_operations {
        let micro_name = operation.parts().last().map(|segment| segment.as_str()).unwrap_or("main");
        let export_name = format!("awsl_call_{micro_name}");
        let offset = u32::try_from(logic_text.len()).unwrap_or(0);
        logic_text.extend_from_slice(&ret_bytes());
        logic_exports.push(SharedObjectExport { name: export_name, text_offset: offset });
    }

    if logic_exports.len() == 1 {
        let offset = u32::try_from(logic_text.len()).unwrap_or(0);
        logic_text.extend_from_slice(&ret_bytes());
        logic_exports.push(SharedObjectExport { name: "awsl_call_main".into(), text_offset: offset });
    }

    let jni = emit_jni_glue_module().map_err(|e| miette::miette!("{e}"))?;
    let (image, exports) = merge_jni_and_logic(jni, logic_text, logic_exports).map_err(|e| miette::miette!("{e}"))?;
    let shared = SharedElfWriter::write_aarch64(&image, &exports)?;
    Ok((shared, "asgard_invoke_export".to_string()))
}

fn lower_windows_pe(submission: &FragmentSubmission) -> Result<(Vec<u8>, String)> {
    let entry_symbol = native_entry_symbol(submission);
    let mut builder = NativeImageBuilder::new();
    super::singleton::append_native_singleton_metadata(&mut builder, submission);
    super::singleton::reserve_native_singleton_slots(&mut builder, submission);
    emit_witness_tables(Some(&mut builder), None, submission)?;
    let prints = win32_print_edges(submission);

    builder.push(X64Instruction::Label(entry_symbol.clone()));

    let mut function = MsvcFunctionBuilder::new();
    // ?????????? shadow space ???? inline ????????
    let value_area = native_value_area_size(submission);
    let suspend_extra = suspend_stack_needed_windows(submission);
    // `NativeSlotPlan` ? `NATIVE_VALUE_AREA_BASE = 0x40` ???? SSA value ???
    // ?? 8 ?????`reserve_stack` ??????????? `next_offset`?
    // ?? `MovRspOffsetReg32` ?????????? access violation (0xC0000005)?
    // ?? MIR ????????????base ????????? `total_size` ???
    let slot_area: u32 = submission
        .executable
        .as_ref()
        .map(|exec| {
            exec.operations().into_iter().filter_map(|op| exec.get_function(&op)).map(|view| NativeSlotPlan::plan_max(&view.function)).sum()
        })
        .unwrap_or(0);
    function.reserve_stack((NATIVE_VALUE_AREA_BASE as u32 + slot_area + value_area).max(suspend_extra));
    function.emit_prologue();

    emit_eager_singleton_init_calls_msvc(&mut function, submission);

    lower_witness_calls_windows(submission, &mut function, &mut builder)?;
    lower_suspend_witness_calls_windows(submission, &mut function, &mut builder);
    // Print edges ??? MIR ?????????? MIR Call ??? stub?
    // ??????????print edge ?? console_write ???? WriteFile ???
    // ?? MIR ??????????? main Return ??????? stdout?
    if !prints.is_empty() {
        let get_std_handle = builder.import("kernel32", "GetStdHandle");
        let write_file = builder.import("kernel32", "WriteFile");
        for edge in prints {
            emit_win32_print_edge(&mut function, &mut builder, edge, get_std_handle, write_file);
        }
    }
    // ????? MIR ???????????????
    // ????? fallthrough ???? MIR ????????? `ret` ???
    // ???????? access violation (0xC0000005)?
    if let Some(label) = entry_function_first_block_label(submission) {
        function.emit(X64Instruction::Jmp(label));
    }
    lower_mir_functions_to_native_msvc(submission, &mut function);

    function.emit_return_zero();
    function.emit_epilogue_and_ret();
    emit_singleton_init_stubs_msvc(&mut function, submission);
    dump_native_instructions("windows-msvc", &entry_symbol, function.stack_reserve(), function.encoder().instructions());
    for instruction in function.encoder().instructions() {
        builder.push(instruction.clone());
    }

    let executable = builder.build_executable(&entry_symbol)?;
    Ok((executable, entry_symbol))
}

fn lower_linux_elf(submission: &FragmentSubmission) -> Result<(Vec<u8>, String)> {
    let entry_symbol = "_start".to_string();
    let mut builder = NativeElfImageBuilder::new();
    super::singleton::append_elf_singleton_metadata(&mut builder, submission);
    super::singleton::reserve_elf_singleton_slots(&mut builder, submission);
    emit_witness_tables(None, Some(&mut builder), submission)?;
    let prints = linux_print_edges(submission);

    builder.push(X64Instruction::Label(entry_symbol.clone()));

    let mut function = SysvFunctionBuilder::new();
    // ?????????? 16 ????? SysV ?????? inline ????????
    let value_area = native_value_area_size(submission);
    let suspend_extra = suspend_stack_needed_linux(submission);
    // ? MSVC ????? `NativeSlotPlan` ????????????
    // ?? MIR ????????????????? `total_size` ???
    let slot_area: u32 = submission
        .executable
        .as_ref()
        .map(|exec| {
            exec.operations().into_iter().filter_map(|op| exec.get_function(&op)).map(|view| NativeSlotPlan::plan_max(&view.function)).sum()
        })
        .unwrap_or(0);
    function.reserve_stack((NATIVE_VALUE_AREA_BASE as u32 + slot_area + value_area).max(suspend_extra));
    function.emit_prologue();

    emit_eager_singleton_init_calls_sysv(&mut function, submission);

    lower_witness_calls_linux(submission, &mut function, &mut builder)?;
    lower_suspend_witness_calls_linux(submission, &mut function, &mut builder);
    // Print edges ??? MIR ??????? Windows ?????
    for edge in prints {
        emit_linux_print_edge(&mut function, &mut builder, edge);
    }
    // ????? MIR ???????????????? Windows ??????
    if let Some(label) = entry_function_first_block_label(submission) {
        function.emit(X64Instruction::Jmp(label));
    }
    lower_mir_functions_to_native_sysv(submission, &mut function);

    function.emit_linux_exit(0);
    emit_singleton_init_stubs_sysv(&mut function, submission);
    dump_native_instructions("linux-sysv", &entry_symbol, function.stack_reserve(), function.encoder().instructions());
    for instruction in function.encoder().instructions() {
        builder.push(instruction.clone());
    }

    let executable = builder.build_executable(&entry_symbol)?;
    Ok((executable, entry_symbol))
}

/// ? MSVC ????? eager singleton ?? init ???
///
/// ?? eager singleton ?? `LeaRipRelative` ?? `singleton_init_{name}` ????? `RAX`?
/// ??? `CallReg` ???lazy singleton ???????????????????????
/// ?? init stub ? `ret`?? Native ?? runtime ???????????? + ?????
fn emit_eager_singleton_init_calls_msvc(function: &mut MsvcFunctionBuilder, submission: &FragmentSubmission) {
    for plan in &submission.singleton_instances {
        if plan.is_lazy {
            continue;
        }
        let label = format!("singleton_init_{}", plan.name);
        function.push(X64Instruction::LeaRipRelative { dst: Reg64::Rax, label });
        function.push(X64Instruction::CallReg(Reg64::Rax));
    }
}

/// ? MSVC ??????? singleton init stub ????
///
/// ?? singleton ???? `singleton_init_{name}` ?? + `Ret` ???
/// ??????????runtime ?????????????????????????
fn emit_singleton_init_stubs_msvc(function: &mut MsvcFunctionBuilder, submission: &FragmentSubmission) {
    for plan in &submission.singleton_instances {
        let label = format!("singleton_init_{}", plan.name);
        function.push(X64Instruction::Label(label));
        function.push(X64Instruction::Ret);
    }
}

/// ? SysV ????? eager singleton ?? init ???? MSVC ?????
fn emit_eager_singleton_init_calls_sysv(function: &mut SysvFunctionBuilder, submission: &FragmentSubmission) {
    for plan in &submission.singleton_instances {
        if plan.is_lazy {
            continue;
        }
        let label = format!("singleton_init_{}", plan.name);
        function.push(X64Instruction::LeaRipRelative { dst: Reg64::Rax, label });
        function.push(X64Instruction::CallReg(Reg64::Rax));
    }
}

/// ? SysV ??????? singleton init stub ????? MSVC ?????
fn emit_singleton_init_stubs_sysv(function: &mut SysvFunctionBuilder, submission: &FragmentSubmission) {
    for plan in &submission.singleton_instances {
        let label = format!("singleton_init_{}", plan.name);
        function.push(X64Instruction::Label(label));
        function.push(X64Instruction::Ret);
    }
}

/// ? submission ??? `mir_functions` ? MIR ?? lowering ? MSVC x86_64 ????
///
/// ?? `NativeSlotPlan` ??? SSA value ?????????????? / ?? /
/// ????????????? prologue ??? `reserve_stack` ???
pub(crate) fn lower_mir_functions_to_native_msvc(submission: &FragmentSubmission, function: &mut MsvcFunctionBuilder) {
    lower_mir_functions_to_native(submission, function);
}

/// ? submission ??? `mir_functions` ? MIR ?? lowering ? SysV x86_64 ????
///
/// ? MSVC ?????ABI ?????????????????? `X64Emitter` trait ???
pub(crate) fn lower_mir_functions_to_native_sysv(submission: &FragmentSubmission, function: &mut SysvFunctionBuilder) {
    lower_mir_functions_to_native(submission, function);
}

/// ABI ??? MIR ??? lowering ???
///
/// ??? MIR ????????????`base` ?????? callee ?? caller
/// ??????callee ?? block ???????????????? entry block
/// ?????? `emit_call` ???????? callee ??????
fn lower_mir_functions_to_native<E: X64Emitter>(submission: &FragmentSubmission, emitter: &mut E) {
    let Some(exec) = &submission.executable
    else {
        return;
    };
    let operations = exec.operations();
    if operations.is_empty() {
        return;
    }
    let ctx = ExecutableLoweringContext::new(submission);
    let entry_op = submission.entry_operation.as_ref();
    let arg_regs = emitter.arg_registers();
    // ??? MIR ?????????????? base ???
    // ??????? SSA value / ???????
    let mut base = NATIVE_VALUE_AREA_BASE;
    for operation in operations {
        let Some(view) = exec.get_function(&operation)
        else {
            continue;
        };
        let mir_fn = &view.function;
        let slots = NativeSlotPlan::plan_with_base(mir_fn, base);
        let fn_size = slots.total_size();
        base = base.saturating_add((fn_size as i32).max(NATIVE_SLOT_SIZE));
        let block_order = collect_reachable_blocks(mir_fn);
        let is_entry_function = entry_op
            .and_then(|op| op.parts().last().map(|p| p.as_str()))
            .map(|entry_name| entry_name == mir_symbol_simple_name(&mir_fn.symbol))
            .unwrap_or(false);
        if let Some(opcode) = mir_fn.intrinsic {
            if mir_fn.blocks.iter().all(|block| block.instructions.is_empty()) {
                let entry_id = mir_fn.entry;
                emitter.emit(X64Instruction::Label(native_block_label(mir_fn, entry_id)));
                emit_entry_block_param_prologue(mir_fn, &slots, arg_regs, emitter);
                let arguments = mir_fn
                    .blocks
                    .get(entry_id.0 as usize)
                    .map(|block| block.parameters.iter().copied().map(MirOperand::Value).collect::<Vec<_>>())
                    .unwrap_or_default();
                let synthetic = MirInstruction {
                    output: None,
                    kind: MirInstructionKind::Call {
                        dispatch: MirDispatchKind::Static,
                        callee: MirOperand::Symbol(NamePath::new(vec![Identifier::new("intrinsic")])),
                        arguments,
                        witness: None,
                        effect: None,
                        receiver_kind: None,
                        parameter_types: None,
                        intrinsic_opcode: Some(opcode),
                    },
                };
                emit_intrinsic_opcode_x64(opcode, &synthetic, &slots, emitter);
                emit_terminator_x64(&MirTerminator::Return { value: None }, mir_fn, &slots, is_entry_function, emitter);
                continue;
            }
        }
        for &block_id in &block_order {
            let Some(block) = mir_fn.blocks.get(block_id.0 as usize)
            else {
                continue;
            };
            emitter.emit(X64Instruction::Label(native_block_label(mir_fn, block.id)));
            // entry block prologue?caller ?? arg registers ???
            // callee ???? arg registers ?? entry block ?????
            if block_id == mir_fn.entry {
                emit_entry_block_param_prologue(mir_fn, &slots, arg_regs, emitter);
            }
            for instruction in &block.instructions {
                emit_mir_instruction_x64(&ctx, mir_fn, &slots, instruction, emitter);
            }
            emit_terminator_x64(&block.terminator, mir_fn, &slots, is_entry_function, emitter);
        }
    }
}

/// ? callee ?? block ???? arg registers ???????
///
/// caller ?? `emit_call` ????? `arg_registers`?callee ?? block
/// ???????????????? block ???????????
/// `emit_load_operand_to_reg` ??????
fn emit_entry_block_param_prologue<E: X64Emitter>(mir_fn: &MirFunction, slots: &NativeSlotPlan, arg_regs: &[Reg64], emitter: &mut E) {
    let Some(entry_block) = mir_fn.blocks.get(mir_fn.entry.0 as usize)
    else {
        return;
    };
    for (index, _param) in entry_block.parameters.iter().enumerate() {
        if index >= arg_regs.len() {
            break;
        }
        if let Some(offset) = slots.block_param_offset(mir_fn.entry, index) {
            emitter.emit(X64Instruction::MovRspOffsetReg32 { offset, src: arg_regs[index] });
        }
    }
}

/// ABI ??? MIR ?? lowering ????? `X64Emitter` trait ??? MSVC / SysV?
///
/// ???????`LoadConstant` / `StoreVar` / `Copy` / `Call(builtin)`?
/// ???????? `StructNew` / `TupleNew` / `FixedArrayNew` / `AggregateCopy` /
/// `FieldGet` / `FieldSet`??????????? SSA value ????
fn emit_mir_instruction_x64<E: X64Emitter>(
    ctx: &ExecutableLoweringContext<'_>,
    mir_fn: &MirFunction,
    slots: &NativeSlotPlan,
    instruction: &MirInstruction,
    emitter: &mut E,
) {
    match &instruction.kind {
        MirInstructionKind::StructNew { type_name, storage, layout_id, .. } => {
            emit_value_aggregate_new_x64(ctx, type_name, *storage, *layout_id, emitter);
        }
        MirInstructionKind::TupleNew { element_types, storage, layout_id, .. } => {
            let element_types: Vec<_> = element_types.clone();
            let resolved = resolve_value_layout(ctx, *layout_id, &NyarType::Tuple(element_types));
            emit_value_aggregate_new_by_layout_x64(ctx, *storage, resolved, emitter);
        }
        MirInstructionKind::FixedArrayNew { element_type, length, storage, layout_id, .. } => {
            let element_type = element_type.clone();
            let resolved = resolve_value_layout(ctx, *layout_id, &NyarType::FixedArray { element: Box::new(element_type), length: *length });
            emit_value_aggregate_new_by_layout_x64(ctx, *storage, resolved, emitter);
        }
        MirInstructionKind::AggregateCopy { layout_id, .. } => {
            emit_aggregate_copy_x64(ctx, *layout_id, emitter);
        }
        MirInstructionKind::FieldGet { object: _, field, storage, layout_id } => {
            emit_value_field_get(ctx, *layout_id, field, *storage, emitter);
        }
        MirInstructionKind::FieldSet { object: _, field, value: _, storage, layout_id } => {
            emit_value_field_set(ctx, *layout_id, field, *storage, emitter);
        }
        MirInstructionKind::Call { dispatch, callee, arguments, receiver_kind, .. } => {
            if let Some(opcode) = ctx.resolve_intrinsic_opcode(callee) {
                emit_intrinsic_opcode_x64(opcode, instruction, slots, emitter);
            }
            else {
                emit_call(
                    ctx,
                    mir_fn,
                    *dispatch,
                    callee,
                    arguments.as_slice(),
                    *receiver_kind,
                    instruction,
                    slots,
                    is_suspend_callee(ctx, mir_fn, callee),
                    emitter,
                );
            }
        }
        MirInstructionKind::LoadConstant { constant, ty: _ } => {
            emit_load_constant(constant, emitter);
            if let Some(output) = instruction.output {
                emit_store_value(output, slots, emitter);
            }
        }
        MirInstructionKind::Copy { source } => {
            emit_load_operand_to_reg(Some(source), slots, Reg64::Rax, emitter);
            if let Some(output) = instruction.output {
                emit_store_value(output, slots, emitter);
            }
        }
        MirInstructionKind::StoreVar { name, value, .. } => {
            emit_load_operand_to_reg(Some(value), slots, Reg64::Rax, emitter);
            if let Some(offset) = slots.var_offset(name) {
                emitter.emit(X64Instruction::MovRspOffsetReg32 { offset, src: Reg64::Rax });
            }
            if let Some(output) = instruction.output {
                emit_store_value(output, slots, emitter);
            }
        }
        _ => {
            if let Some(output) = instruction.output {
                emitter.emit(X64Instruction::MovRegImm32(Reg64::Rax, 0));
                emit_store_value(output, slots, emitter);
            }
        }
    }
}

/// ABI ??? terminator lowering ????? `X64Emitter` trait ??? MSVC / SysV?
///
/// - `Return`??????? RAX?entry ??? exit syscall?SysV?????? ret?MSVC??
///   ?????????? emit `ret` ??? caller???? caller ???
/// - `Jump`???? block argument copies?loop-carried variables??? jmp?
/// - `Branch`?????? RAX???? then???? else?
fn emit_terminator_x64<E: X64Emitter>(
    terminator: &MirTerminator,
    mir_fn: &MirFunction,
    slots: &NativeSlotPlan,
    is_entry_function: bool,
    emitter: &mut E,
) {
    match terminator {
        MirTerminator::Return { value } => {
            emit_load_operand_to_reg(value.as_ref(), slots, Reg64::Rax, emitter);
            if is_entry_function && emitter.is_sysv_entry() {
                // SysV `_start`???????? `exit(rax)` syscall ???
                emitter.emit(X64Instruction::MovRegReg { dst: Reg64::Rdi, src: Reg64::Rax });
                emitter.emit(X64Instruction::MovRegImm32(Reg64::Rax, 60));
                emitter.emit(X64Instruction::Syscall);
            }
            else if is_entry_function {
                // MSVC ??????? `ret` ???
                let reserve = emitter.stack_reserve();
                if reserve > 0 {
                    emitter.emit(X64Instruction::AddRsp(reserve));
                }
                emitter.emit(X64Instruction::Ret);
            }
            else {
                // ??????? `ret` ??? caller???????
                emitter.emit(X64Instruction::Ret);
            }
        }
        MirTerminator::Jump { target, arguments } => {
            emit_block_argument_copies(*target, arguments, mir_fn, slots, emitter);
            emitter.emit(X64Instruction::Jmp(native_block_label(mir_fn, *target)));
        }
        MirTerminator::Branch { condition, then_target, else_target } => {
            emit_load_operand_to_reg(Some(condition), slots, Reg64::Rax, emitter);
            emitter.emit(X64Instruction::CmpRegImm32(Reg64::Rax, 0));
            emitter.emit(X64Instruction::Jne(native_block_label(mir_fn, *then_target)));
            emitter.emit(X64Instruction::Jmp(native_block_label(mir_fn, *else_target)));
        }
        MirTerminator::Unreachable
        | MirTerminator::PerformEffect { .. }
        | MirTerminator::StateDispatch { .. }
        | MirTerminator::YieldToRuntime { .. } => {
            if is_entry_function && emitter.is_sysv_entry() {
                emitter.emit(X64Instruction::MovRegImm32(Reg64::Rdi, 0));
                emitter.emit(X64Instruction::MovRegImm32(Reg64::Rax, 60));
                emitter.emit(X64Instruction::Syscall);
            }
            else if is_entry_function {
                let reserve = emitter.stack_reserve();
                if reserve > 0 {
                    emitter.emit(X64Instruction::AddRsp(reserve));
                }
                emitter.emit(X64Instruction::Ret);
            }
            else {
                emitter.emit(X64Instruction::Ret);
            }
        }
    }
}

/// ? `Jump` ???????? `arguments` ???????? block ?????
///
/// ??? `(argument, target_param)` ??? argument ??? RAX?
/// ? `mov [rsp+offset], eax` ???????loop-carried variables
/// ?????????????????
fn emit_block_argument_copies<E: X64Emitter>(
    target: MirBlockRef,
    arguments: &[MirOperand],
    mir_fn: &MirFunction,
    slots: &NativeSlotPlan,
    emitter: &mut E,
) {
    let Some(target_block) = mir_fn.blocks.get(target.0 as usize)
    else {
        return;
    };
    for (index, argument) in arguments.iter().enumerate() {
        if index >= target_block.parameters.len() {
            break;
        }
        let Some(offset) = slots.block_param_offset(target, index)
        else {
            continue;
        };
        emit_load_operand_to_reg(Some(argument), slots, Reg64::Rax, emitter);
        emitter.emit(X64Instruction::MovRspOffsetReg32 { offset, src: Reg64::Rax });
    }
}

/// ? `MirOperand` ?????????
///
/// - `Value(ref)`?????????`MovRegRspOffset`??
/// - `Constant(Int(n))`?`MovRegImm32(reg, n)`?
/// - `Constant(Bool(b))`?`MovRegImm32(reg, 1/0)`?
/// - `Constant(Float64(f))`?`MovRegImm64(reg, bits)`?
/// - `Symbol(name)`?????????
/// - ?? value????? 0?
fn emit_load_operand_to_reg<E: X64Emitter>(operand: Option<&MirOperand>, slots: &NativeSlotPlan, dst: Reg64, emitter: &mut E) {
    match operand {
        Some(MirOperand::Value(value)) => {
            if let Some(offset) = slots.value_offset(value) {
                emitter.emit(X64Instruction::MovRegRspOffset { dst, offset });
            }
            else {
                emitter.emit(X64Instruction::MovRegImm32(dst, 0));
            }
        }
        Some(MirOperand::Constant(MirConstant::Int(n))) => {
            emitter.emit(X64Instruction::MovRegImm32(dst, *n as u32));
        }
        Some(MirOperand::Constant(MirConstant::Bool(b))) => {
            emitter.emit(X64Instruction::MovRegImm32(dst, if *b { 1 } else { 0 }));
        }
        Some(MirOperand::Constant(MirConstant::Float64(f))) => {
            emitter.emit(X64Instruction::MovRegImm64(dst, f.to_bits()));
        }
        Some(MirOperand::Constant(MirConstant::Utf16(_))) => {
            panic!("native lowering has no explicit UTF-16 text ABI")
        }
        Some(MirOperand::Constant(MirConstant::Unit)) | Some(MirOperand::Constant(MirConstant::Utf8(_))) => {
            emitter.emit(X64Instruction::MovRegImm32(dst, 0));
        }
        Some(MirOperand::Symbol(path)) => {
            let key = path.to_string();
            if let Some(offset) = slots.var_offset(&key) {
                emitter.emit(X64Instruction::MovRegRspOffset { dst, offset });
            }
            else {
                emitter.emit(X64Instruction::MovRegImm32(dst, 0));
            }
        }
        None => {
            emitter.emit(X64Instruction::MovRegImm32(dst, 0));
        }
    }
}

/// ? RAX ?? SSA value ??????
fn emit_store_value<E: X64Emitter>(value: MirValueRef, slots: &NativeSlotPlan, emitter: &mut E) {
    if let Some(offset) = slots.value_offset(&value) {
        emitter.emit(X64Instruction::MovRspOffsetReg32 { offset, src: Reg64::Rax });
    }
}

/// ???? lowering?? / ? / ? / ? / ?? / ?? / ?? / ???
///
/// ?????????? RAX ? RCX??????? RAX??? output ???
fn emit_intrinsic_opcode_x64<E: X64Emitter>(opcode: IntrinsicOpcode, instruction: &MirInstruction, slots: &NativeSlotPlan, emitter: &mut E) {
    let arguments = match &instruction.kind {
        MirInstructionKind::Call { arguments, .. } => arguments.as_slice(),
        _ => return,
    };
    match opcode {
        IntrinsicOpcode::Binary(op) => {
            if arguments.len() < 2 {
                return;
            }
            emit_load_operand_to_reg(Some(&arguments[0]), slots, Reg64::Rax, emitter);
            emit_load_operand_to_reg(Some(&arguments[1]), slots, Reg64::Rcx, emitter);
            match op {
                IntrinsicBinaryOp::Add => emitter.emit(X64Instruction::AddRegReg { dst: Reg64::Rax, src: Reg64::Rcx }),
                IntrinsicBinaryOp::Sub => emitter.emit(X64Instruction::SubRegReg { dst: Reg64::Rax, src: Reg64::Rcx }),
                IntrinsicBinaryOp::Mul => emitter.emit(X64Instruction::ImulRegReg { dst: Reg64::Rax, src: Reg64::Rcx }),
                IntrinsicBinaryOp::Div => {
                    emitter.emit(X64Instruction::IdivReg { divisor: Reg64::Rcx });
                }
                IntrinsicBinaryOp::Rem => {
                    emitter.emit(X64Instruction::IdivReg { divisor: Reg64::Rcx });
                    emitter.emit(X64Instruction::MovRegReg { dst: Reg64::Rax, src: Reg64::Rdx });
                }
            }
            if let Some(output) = instruction.output {
                emit_store_value(output, slots, emitter);
            }
        }
        IntrinsicOpcode::Neg => {
            if let Some(arg) = arguments.first() {
                emit_load_operand_to_reg(Some(arg), slots, Reg64::Rax, emitter);
                emitter.emit(X64Instruction::NegReg { dst: Reg64::Rax });
                if let Some(output) = instruction.output {
                    emit_store_value(output, slots, emitter);
                }
            }
        }
        IntrinsicOpcode::Compare(op) => {
            if arguments.len() < 2 {
                return;
            }
            emit_load_operand_to_reg(Some(&arguments[0]), slots, Reg64::Rax, emitter);
            emit_load_operand_to_reg(Some(&arguments[1]), slots, Reg64::Rcx, emitter);
            emitter.emit(X64Instruction::CmpRegReg { dst: Reg64::Rax, src: Reg64::Rcx });
            let cc = match op {
                IntrinsicCompareOp::Eq => ConditionCode::Equal,
                IntrinsicCompareOp::Ne => ConditionCode::NotEqual,
                IntrinsicCompareOp::Lt => ConditionCode::Less,
                IntrinsicCompareOp::Le => ConditionCode::LessEqual,
                IntrinsicCompareOp::Gt => ConditionCode::Greater,
                IntrinsicCompareOp::Ge => ConditionCode::GreaterEqual,
            };
            emitter.emit(X64Instruction::SetccRax { cc });
            if let Some(output) = instruction.output {
                emit_store_value(output, slots, emitter);
            }
        }
        IntrinsicOpcode::Not => {
            if let Some(arg) = arguments.first() {
                emit_load_operand_to_reg(Some(arg), slots, Reg64::Rax, emitter);
                emitter.emit(X64Instruction::CmpRegImm32(Reg64::Rax, 0));
                emitter.emit(X64Instruction::SetccRax { cc: ConditionCode::Equal });
                if let Some(output) = instruction.output {
                    emit_store_value(output, slots, emitter);
                }
            }
        }
        IntrinsicOpcode::ArrayGet
        | IntrinsicOpcode::ArraySet
        | IntrinsicOpcode::ArrayLen
        | IntrinsicOpcode::ArrayPush
        | IntrinsicOpcode::Deref
        | IntrinsicOpcode::Utf8ScalarSlice
        | IntrinsicOpcode::Utf8ScalarLength
        | IntrinsicOpcode::Utf8ContentEqual
        | IntrinsicOpcode::Utf8ContentNotEqual
        | IntrinsicOpcode::Utf8Trim
        | IntrinsicOpcode::Utf8IndexOf
        | IntrinsicOpcode::Utf8Contains
        | IntrinsicOpcode::Utf8StartsWith
        | IntrinsicOpcode::Utf8EndsWith
        | IntrinsicOpcode::SumVariantIs
        | IntrinsicOpcode::SumStructuralEqual
        | IntrinsicOpcode::Bitwise(_) => {}
    }
}

/// ???????? `AggregateLayout`??????? `layout_id`?????????
fn resolve_value_layout<'a>(
    ctx: &ExecutableLoweringContext<'a>,
    layout_id: Option<LayoutId>,
    fallback_ty: &NyarType,
) -> Option<&'a AggregateLayout> {
    // ???? `ctx.layouts`?`&'a AggregateLayoutPlan`?????????????? `'a`?
    // ????? `&self` ??????
    if let Some(id) = layout_id {
        if let Some(layout) = ctx.layouts.layouts.iter().find(|item| item.id == id) {
            return Some(layout);
        }
    }
    let key = nyar_types::layout_key_for_nyar_type(fallback_ty)?;
    let id = ctx.layouts.type_name_to_layout.get(&key)?;
    ctx.layouts.layouts.iter().find(|item| item.id == *id)
}

fn emit_value_aggregate_new_x64<E: X64Emitter>(
    ctx: &ExecutableLoweringContext<'_>,
    type_name: &str,
    storage: MirStorageKind,
    layout_id: Option<LayoutId>,
    emitter: &mut E,
) {
    if storage != StorageKind::Value {
        return;
    }
    let resolved = layout_id.and_then(|id| ctx.layout_by_id(id)).or_else(|| ctx.layout_by_type_name(type_name));
    emit_value_aggregate_new_by_layout_x64(ctx, storage, resolved, emitter);
}

fn emit_value_aggregate_new_by_layout_x64<E: X64Emitter>(
    _ctx: &ExecutableLoweringContext<'_>,
    storage: MirStorageKind,
    layout: Option<&AggregateLayout>,
    emitter: &mut E,
) {
    if storage != StorageKind::Value {
        return;
    }
    let Some(layout) = layout
    else {
        return;
    };
    let _abi = classify_aggregate_for_abi(layout);
    emitter.emit(X64Instruction::LeaRspOffset { dst: Reg64::Rax, offset: NATIVE_VALUE_AREA_BASE });
    for field in &layout.fields {
        let offset = i8::try_from(field.offset).unwrap_or(0);
        emitter.emit(X64Instruction::MovMemRegImm32 { base: Reg64::Rax, offset, value: 0 });
    }
}

fn emit_aggregate_copy_x64<E: X64Emitter>(ctx: &ExecutableLoweringContext<'_>, layout_id: LayoutId, emitter: &mut E) {
    let Some(layout) = ctx.layout_by_id(layout_id)
    else {
        return;
    };
    let _abi = classify_aggregate_for_abi(layout);
    emitter.emit(X64Instruction::MovRegImm32(Reg64::Rcx, layout.size));
}

/// ??? `FieldGet` ?????????????????????? `RAX`?
///
/// ??? `StorageKind::Value`?????????? host GC / ????????
/// ?? Native ????????????????????????? `RAX`?
/// ?? `layout_id` ?? `AggregateLayout`??? `field` ??? `FieldLayout.offset`?
fn emit_value_field_get<E: X64Emitter>(
    ctx: &ExecutableLoweringContext<'_>,
    layout_id: Option<LayoutId>,
    field: &str,
    storage: MirStorageKind,
    emitter: &mut E,
) {
    if storage != StorageKind::Value {
        return;
    }
    let Some(layout_id) = layout_id
    else {
        return;
    };
    let Some(layout) = ctx.layout_by_id(layout_id)
    else {
        return;
    };
    let Some(field_layout) = layout.fields.iter().find(|item| item.name == field)
    else {
        return;
    };
    let _abi = classify_aggregate_for_abi(layout);
    emitter.emit(X64Instruction::LeaRspOffset { dst: Reg64::Rax, offset: NATIVE_VALUE_AREA_BASE });
    let offset = i8::try_from(field_layout.offset).unwrap_or(0);
    emitter.emit(X64Instruction::MovRegMemReg { dst: Reg64::Rax, base: Reg64::Rax, offset });
}

/// ??? `FieldSet` ??????????????????????
///
/// ??? `StorageKind::Value`?????????? 0???????????????
/// ? `value` operand ?????????????????? `FieldLayout.offset` ????
fn emit_value_field_set<E: X64Emitter>(
    ctx: &ExecutableLoweringContext<'_>,
    layout_id: Option<LayoutId>,
    field: &str,
    storage: MirStorageKind,
    emitter: &mut E,
) {
    if storage != StorageKind::Value {
        return;
    }
    let Some(layout_id) = layout_id
    else {
        return;
    };
    let Some(layout) = ctx.layout_by_id(layout_id)
    else {
        return;
    };
    let Some(field_layout) = layout.fields.iter().find(|item| item.name == field)
    else {
        return;
    };
    let _abi = classify_aggregate_for_abi(layout);
    emitter.emit(X64Instruction::LeaRspOffset { dst: Reg64::Rax, offset: NATIVE_VALUE_AREA_BASE });
    let offset = i8::try_from(field_layout.offset).unwrap_or(0);
    emitter.emit(X64Instruction::MovMemRegImm32 { base: Reg64::Rax, offset, value: 0 });
}

/// `LoadConstant` ???? `MirConstant` ??????? `RAX`?
///
/// - `Int(n)` ? `MovRegImm32(Rax, n as u32)`
/// - `Float64(f)` ? `MovRegImm64(Rax, f.to_bits())`
/// - ???`Bool` / `String` / `Unit`????????? 0?
fn emit_load_constant<E: X64Emitter>(constant: &MirConstant, emitter: &mut E) {
    match constant {
        MirConstant::Int(n) => emitter.emit(X64Instruction::MovRegImm32(Reg64::Rax, *n as u32)),
        MirConstant::Float64(f) => emitter.emit(X64Instruction::MovRegImm64(Reg64::Rax, f.to_bits())),
        _ => emitter.emit(X64Instruction::MovRegImm32(Reg64::Rax, 0)),
    }
}

/// `Call` ????? `ByAddress` ????????? `Static` ?????
///
/// ?????? ABI ???????????MSVC: RCX/RDX/R8/R9?SysV: RDI/RSI/...??
/// ? `emitter.arg_registers()` ???? `receiver_kind == Some(ByAddress)` ??
/// ??? arg register ??????????????? index 1 ???
///
/// `dispatch == Static` ? `is_suspend_callee` ? false ???? `CallLabel` ???
/// callee ?? block??? `mir_functions` ???? PE/ELF ?????callee ? `Return`
/// terminator emit `ret` ???? call site????? RAX??? output ???
///
/// `is_suspend_callee` ? true ????????suspend ????????
/// `lower_suspend_witness_calls_windows` / `lower_suspend_witness_calls_linux` ???
/// `Witness` / `EffectHandler` ??????????
fn emit_call<E: X64Emitter>(
    ctx: &ExecutableLoweringContext<'_>,
    mir_fn: &MirFunction,
    dispatch: MirDispatchKind,
    callee: &MirOperand,
    arguments: &[MirOperand],
    receiver_kind: Option<ReceiverPassingKind>,
    instruction: &MirInstruction,
    slots: &NativeSlotPlan,
    is_suspend_callee: bool,
    emitter: &mut E,
) {
    let arg_regs = emitter.arg_registers();
    // `ByAddress` ????????????????? arg register?
    let receiver_start = if receiver_kind == Some(ReceiverPassingKind::ByAddress) {
        if let Some(first) = arg_regs.first() {
            emitter.emit(X64Instruction::LeaRspOffset { dst: *first, offset: NATIVE_VALUE_AREA_BASE });
        }
        1
    }
    else {
        0
    };
    if dispatch == MirDispatchKind::Static {
        if let MirOperand::Symbol(_) = callee {
            if is_suspend_callee {
                return;
            }
            // ??????? arg registers??? ByAddress ???????????????
            for (i, arg) in arguments.iter().enumerate() {
                let reg_idx = receiver_start + i;
                if reg_idx >= arg_regs.len() {
                    break;
                }
                emit_load_operand_to_reg(Some(arg), slots, arg_regs[reg_idx], emitter);
            }
            // ?? mir_functions ??? PE/ELF ???????callee ?? block ?
            // ? `native_block_label` ????? `CallLabel` ?????
            // callee ? `Return` terminator emit `ret` ???? call site?
            if let Some(label) = find_callee_entry_label(ctx, callee) {
                emitter.emit(X64Instruction::CallLabel(label));
            }
            else {
                // ????? print-edge / ?? host lowering ???
                // ???????? `CallReg(Rax)`?RAX ?????????? SIGSEGV?
                if let Some(output) = instruction.output {
                    emitter.emit(X64Instruction::MovRegImm32(Reg64::Rax, 0));
                    emit_store_value(output, slots, emitter);
                }
                return;
            }
            // ?????RAX??? output ???
            if let Some(output) = instruction.output {
                emit_store_value(output, slots, emitter);
            }
        }
    }
}

/// ?? `MirOperand::Symbol(path)` ?? callee ??? block label?
///
/// ? path ???? `mir_fn.symbol` ????????????
fn find_callee_entry_label(ctx: &ExecutableLoweringContext<'_>, callee: &MirOperand) -> Option<String> {
    let MirOperand::Symbol(path) = callee
    else {
        return None;
    };
    let target = path.parts().last()?;
    let target_str = target.as_str();
    let exec = ctx.submission.executable.as_ref()?;
    for operation in exec.operations() {
        let Some(view) = exec.get_function(&operation)
        else {
            continue;
        };
        if mir_symbol_simple_name(&view.function.symbol) == target_str {
            return Some(native_block_label(&view.function, view.function.entry));
        }
    }
    None
}

/// ?? callee ??? suspend ???????? lowering ??????? suspend ???
///
/// ?? callee ? `MirOperand::Symbol` ????? `submission.control_flow.functions`
/// ??? artifact ????????**??**???? `mir_fn` ??????
/// `control_flow.functions` ???? true?sync ???? suspend ????? false?
/// ?? sync ????????????????? `CallReg`?
fn is_suspend_callee(ctx: &ExecutableLoweringContext<'_>, mir_fn: &MirFunction, callee: &MirOperand) -> bool {
    let Some(payload) = ctx.submission.control_flow.as_ref()
    else {
        return false;
    };
    let caller_is_suspend =
        payload.functions.iter().any(|artifact| artifact.symbol.parts().last().map(|part| part.as_str()) == Some(mir_fn.symbol.as_str()));
    if !caller_is_suspend {
        return false;
    }
    let MirOperand::Symbol(path) = callee
    else {
        return false;
    };
    let Some(simple) = path.parts().last().map(|part| part.as_str())
    else {
        return false;
    };
    payload.functions.iter().any(|artifact| artifact.symbol.parts().last().map(|part| part.as_str()) == Some(simple))
}

fn emit_win32_print_edge(
    function: &mut MsvcFunctionBuilder,
    builder: &mut NativeImageBuilder,
    edge: &ExternalCallEdge,
    get_std_handle: usize,
    write_file: usize,
) {
    let Some(message) = edge.arguments.iter().find_map(string_literal_argument)
    else {
        return;
    };
    let label = format!("msg_{}", sanitize_symbol(&edge.caller.to_string()));
    builder.add_rdata(&label, message.as_bytes());

    function.push(X64Instruction::MovRegImm32(Reg64::Rcx, 0xFFFF_FFF5));
    function.push(X64Instruction::CallImport { slot: get_std_handle });
    function.push(X64Instruction::MovRegReg { dst: Reg64::Rcx, src: Reg64::Rax });
    function.push(X64Instruction::LeaRipRelative { dst: Reg64::Rdx, label: label.clone() });
    function.push(X64Instruction::MovRegImm32(Reg64::R8, u32::try_from(message.len()).unwrap_or(0)));
    function.push(X64Instruction::LeaRspOffset { dst: Reg64::R9, offset: 0x28 });
    function.push(X64Instruction::MovStackArgQword { value: 0 });
    function.push(X64Instruction::CallImport { slot: write_file });
}

fn emit_linux_print_edge(function: &mut SysvFunctionBuilder, builder: &mut NativeElfImageBuilder, edge: &ExternalCallEdge) {
    let Some(message) = edge.arguments.iter().find_map(string_literal_argument)
    else {
        return;
    };
    let label = format!("msg_{}", sanitize_symbol(&edge.caller.to_string()));
    builder.add_rodata(&label, message.as_bytes());

    function.push(X64Instruction::MovRegImm32(Reg64::Rax, 1));
    function.push(X64Instruction::MovRegImm32(Reg64::Rdi, 1));
    function.push(X64Instruction::LeaRipRelative { dst: Reg64::Rsi, label });
    function.push(X64Instruction::MovRegImm32(Reg64::Rdx, u32::try_from(message.len()).unwrap_or(0)));
    function.push(X64Instruction::Syscall);
}

fn emit_linux_print_buffer(function: &mut SysvFunctionBuilder, builder: &mut NativeElfImageBuilder, label: &str, len: u32) {
    builder.add_rodata(label, b"");
    function.push(X64Instruction::MovRegImm32(Reg64::Rax, 1));
    function.push(X64Instruction::MovRegImm32(Reg64::Rdi, 1));
    function.push(X64Instruction::MovRegReg { dst: Reg64::Rsi, src: Reg64::Rax });
    function.push(X64Instruction::LeaRipRelative { dst: Reg64::Rsi, label: label.to_string() });
    function.push(X64Instruction::MovRegImm32(Reg64::Rdx, len));
    function.push(X64Instruction::Syscall);
}

fn win32_print_edges(submission: &FragmentSubmission) -> Vec<&ExternalCallEdge> {
    submission
        .external_call_edges
        .iter()
        .filter(|edge| {
            submission.external_import_links.get(&edge.callee_symbol).is_some_and(|link| win32_host_import_target(link).is_some())
                && edge.arguments.iter().any(|argument| matches!(argument, ExternalCallArgument::StringLiteral(_)))
        })
        .collect()
}

fn linux_print_edges(submission: &FragmentSubmission) -> Vec<&ExternalCallEdge> {
    submission
        .external_call_edges
        .iter()
        .filter(|edge| {
            submission.external_import_links.get(&edge.callee_symbol).is_some_and(|link| linux_gnu_host_import_target(link).is_some())
                && edge.arguments.iter().any(|argument| matches!(argument, ExternalCallArgument::StringLiteral(_)))
        })
        .collect()
}

fn string_literal_argument(argument: &ExternalCallArgument) -> Option<String> {
    match argument {
        ExternalCallArgument::StringLiteral(value) => Some(value.clone()),
    }
}

fn native_entry_symbol(submission: &FragmentSubmission) -> String {
    submission
        .entry_operation
        .as_ref()
        .and_then(|entry| entry.parts().last().map(|part| sanitize_symbol(part.as_str())))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "main".to_string())
}

/// ? `MirFunction.symbol` ????????????????
///
/// `mir_fn.symbol` ?? `{module}::{function_name}`???????
/// `{singleton}.{method}`???????????? `::` ?? `.` ???
/// ???????????????? block ??? callee ???
fn mir_symbol_simple_name(symbol: &str) -> &str {
    symbol.rsplit("::").next().unwrap_or(symbol).rsplit('.').next().unwrap_or(symbol)
}

/// ? Native ???? MIR ??? block ???? `.text` ???
///
/// ?? `mir_functions` ???? PE/ELF ???????? `block_label(id)` ?
/// ? block ???????????????? block_0???????????
/// prefix ???? `emit_call` ??? `CallLabel` ??? callee ?? block?
fn native_block_label(mir_fn: &MirFunction, block_id: MirBlockRef) -> String {
    let symbol = sanitize_symbol(mir_symbol_simple_name(&mir_fn.symbol));
    format!("n_{symbol}_b{}", block_id.0)
}

/// ???? MIR ??? entry block ???
///
/// ?????? `submission.entry_operation` ? `mir_fn.symbol` ???????
/// ???????? prologue/print ????????? MIR ????????
/// ????????? fallthrough ??? `call` ??????? `ret` ???????
fn entry_function_first_block_label(submission: &FragmentSubmission) -> Option<String> {
    let exec = submission.executable.as_ref()?;
    let entry_op = submission.entry_operation.as_ref()?;
    let entry_name = entry_op.parts().last()?;
    for operation in exec.operations() {
        let view = exec.get_function(&operation)?;
        if mir_symbol_simple_name(&view.function.symbol) == entry_name.as_str() {
            return Some(native_block_label(&view.function, view.function.entry));
        }
    }
    None
}

/// ? native lowering ??? `X64Instruction` ?? dump ? JSON ???
///
/// ????? `VALKYRIE_NATIVE_DUMP=<path>` ????????dump ???
/// ?? `legion spy native <path>` ?????????? access violation
/// ? native ????????????? `Err`?dump ?????????
/// stderr?????????
fn dump_native_instructions(host_flavor: &str, entry_symbol: &str, stack_reserve: u32, instructions: &[X64Instruction]) {
    let Some(path) = std::env::var_os("VALKYRIE_NATIVE_DUMP")
    else {
        return;
    };
    let dump = serde_json::json!({
        "host_flavor": host_flavor,
        "entry_symbol": entry_symbol,
        "stack_reserve": stack_reserve,
        "instructions": instructions,
    });
    let serialized = match serde_json::to_string_pretty(&dump) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("VALKYRIE_NATIVE_DUMP: JSON ??????{e}");
            return;
        }
    };
    if let Err(e) = std::fs::write(&path, serialized) {
        eprintln!("VALKYRIE_NATIVE_DUMP: ?? {} ???{e}", path.to_string_lossy());
    }
}

pub(crate) fn lower_suspend_witness_calls_windows(
    submission: &FragmentSubmission,
    function: &mut MsvcFunctionBuilder,
    builder: &mut NativeImageBuilder,
) {
    let _ = builder;
    let Some(payload) = submission.control_flow.as_ref()
    else {
        return;
    };
    if payload.functions.is_empty() {
        return;
    }
    for artifact in &payload.functions {
        lower_suspend_artifact_windows(submission, artifact, function);
    }
}

pub(crate) fn lower_suspend_witness_calls_linux(
    submission: &FragmentSubmission,
    function: &mut SysvFunctionBuilder,
    builder: &mut NativeElfImageBuilder,
) {
    let _ = builder;
    let Some(payload) = submission.control_flow.as_ref()
    else {
        return;
    };
    if payload.functions.is_empty() {
        return;
    }
    for artifact in &payload.functions {
        lower_suspend_artifact_linux(submission, artifact, function);
    }
}

/// ?? Windows ????? suspend ???????extra??? shadow space??
///
/// suspend ??? `SUSPEND_STATE_RSP_OFFSET`(0x28) ? `SUSPEND_SPILL_RSP_OFFSET`(0x30)
/// ?? RSP ????????? 0x38 ???????? 0x40 ???????
fn suspend_stack_needed_windows(submission: &FragmentSubmission) -> u32 {
    let Some(payload) = submission.control_flow.as_ref()
    else {
        return 0;
    };
    if payload.functions.is_empty() {
        return 0;
    }
    0x40
}

/// ?? Linux ????? suspend ???????
///
/// ? Windows ?????? SysV ? shadow space????? 0x30 ???
fn suspend_stack_needed_linux(submission: &FragmentSubmission) -> u32 {
    let Some(payload) = submission.control_flow.as_ref()
    else {
        return 0;
    };
    if payload.functions.is_empty() {
        return 0;
    }
    0x30
}

const SUSPEND_STATE_RSP_OFFSET: i32 = 0x28;
pub(crate) const SUSPEND_SPILL_RSP_OFFSET: i32 = 0x30;

fn lower_suspend_artifact_windows(submission: &FragmentSubmission, artifact: &SuspendFunctionArtifact, function: &mut MsvcFunctionBuilder) {
    init_suspend_frame_windows(function, artifact);
    function.push(X64Instruction::Label("suspend_run_loop".to_string()));
    emit_suspend_dispatch_windows(submission, artifact, function);
    function.push(X64Instruction::Label("suspend_run_done".to_string()));
}

fn lower_suspend_artifact_linux(submission: &FragmentSubmission, artifact: &SuspendFunctionArtifact, function: &mut SysvFunctionBuilder) {
    init_suspend_frame_linux(function, artifact);
    function.push(X64Instruction::Label("suspend_run_loop".to_string()));
    emit_suspend_dispatch_linux(submission, artifact, function);
    function.push(X64Instruction::Label("suspend_run_done".to_string()));
}

fn init_suspend_frame_windows(function: &mut MsvcFunctionBuilder, artifact: &SuspendFunctionArtifact) {
    function.push(X64Instruction::MovRspOffsetImm32 { offset: SUSPEND_STATE_RSP_OFFSET, value: 0 });
    if artifact_needs_spill_slot(artifact) {
        function.push(X64Instruction::MovRspOffsetImm32 { offset: SUSPEND_SPILL_RSP_OFFSET, value: 1 });
    }
}

fn init_suspend_frame_linux(function: &mut SysvFunctionBuilder, artifact: &SuspendFunctionArtifact) {
    function.push(X64Instruction::MovRspOffsetImm32 { offset: SUSPEND_STATE_RSP_OFFSET, value: 0 });
    if artifact_needs_spill_slot(artifact) {
        function.push(X64Instruction::MovRspOffsetImm32 { offset: SUSPEND_SPILL_RSP_OFFSET, value: 1 });
    }
}

fn artifact_needs_spill_slot(artifact: &SuspendFunctionArtifact) -> bool {
    artifact.frame_fields.iter().any(|field| field.starts_with("__witness_payload_"))
        || artifact.states.iter().any(|state| witness_receiver_field(state).is_some())
}

fn emit_suspend_dispatch_windows(submission: &FragmentSubmission, artifact: &SuspendFunctionArtifact, function: &mut MsvcFunctionBuilder) {
    let case_keys = dispatch_case_keys(artifact);
    function.push(X64Instruction::MovRegRspOffset { dst: Reg64::Rax, offset: SUSPEND_STATE_RSP_OFFSET });
    for case_key in &case_keys {
        let label = format!("suspend_case_{case_key}");
        function.push(X64Instruction::CmpRegImm32(Reg64::Rax, *case_key));
        function.push(X64Instruction::Je(label.clone()));
    }
    function.push(X64Instruction::Jmp("suspend_run_done".to_string()));
    for case_key in &case_keys {
        let label = format!("suspend_case_{case_key}");
        function.push(X64Instruction::Label(label));
        emit_suspend_case_windows(submission, artifact, *case_key, function);
        function.push(X64Instruction::Jmp("suspend_run_loop".to_string()));
    }
}

fn emit_suspend_dispatch_linux(submission: &FragmentSubmission, artifact: &SuspendFunctionArtifact, function: &mut SysvFunctionBuilder) {
    let case_keys = dispatch_case_keys(artifact);
    function.push(X64Instruction::MovRegRspOffset { dst: Reg64::Rax, offset: SUSPEND_STATE_RSP_OFFSET });
    for case_key in &case_keys {
        let label = format!("suspend_case_{case_key}");
        function.push(X64Instruction::CmpRegImm32(Reg64::Rax, *case_key));
        function.push(X64Instruction::Je(label.clone()));
    }
    function.push(X64Instruction::Jmp("suspend_run_done".to_string()));
    for case_key in &case_keys {
        let label = format!("suspend_case_{case_key}");
        function.push(X64Instruction::Label(label));
        emit_suspend_case_linux(submission, artifact, *case_key, function);
        function.push(X64Instruction::Jmp("suspend_run_loop".to_string()));
    }
}

fn emit_suspend_case_windows(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    case_key: u32,
    function: &mut MsvcFunctionBuilder,
) {
    let Some(state) = resolve_state_for_case(artifact, case_key)
    else {
        return;
    };
    match state.effect.as_str() {
        "Yield" if case_key != 0 && case_key == state.resume_case_key => {
            emit_native_complete_state_windows(submission, artifact, state, function);
        }
        "Yield" => emit_native_yield_case_windows(artifact, state, function),
        "DelegateYield" => emit_native_delegate_yield_case_windows(submission, artifact, state, function),
        "Await" => emit_native_await_case_windows(submission, artifact, state, function),
        "AsyncSpawn" => emit_native_async_spawn_case_windows(submission, artifact, state, function),
        "AsyncBlock" => emit_native_async_block_case_windows(submission, artifact, state, function),
        "Raise" if case_key != 0 && case_key == state.resume_case_key => {
            emit_native_complete_state_windows(submission, artifact, state, function);
        }
        "Raise" => emit_native_raise_case_windows(artifact, state, function),
        _ => {}
    }
}

fn emit_suspend_case_linux(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    case_key: u32,
    function: &mut SysvFunctionBuilder,
) {
    let Some(state) = resolve_state_for_case(artifact, case_key)
    else {
        return;
    };
    match state.effect.as_str() {
        "Yield" if case_key != 0 && case_key == state.resume_case_key => {
            emit_native_complete_state_linux(submission, artifact, state, function);
        }
        "Yield" => emit_native_yield_case_linux(artifact, state, function),
        "DelegateYield" => emit_native_delegate_yield_case_linux(submission, artifact, state, function),
        "Await" => emit_native_await_case_linux(submission, artifact, state, function),
        "AsyncSpawn" => emit_native_async_spawn_case_linux(submission, artifact, state, function),
        "AsyncBlock" => emit_native_async_block_case_linux(submission, artifact, state, function),
        "Raise" if case_key != 0 && case_key == state.resume_case_key => {
            emit_native_complete_state_linux(submission, artifact, state, function);
        }
        "Raise" => emit_native_raise_case_linux(artifact, state, function),
        _ => {}
    }
}

fn emit_native_yield_case_windows(_artifact: &SuspendFunctionArtifact, state: &SuspendStateArtifact, function: &mut MsvcFunctionBuilder) {
    function.push(X64Instruction::MovRspOffsetImm32 { offset: SUSPEND_STATE_RSP_OFFSET, value: state.resume_case_key });
    function.push(X64Instruction::Jmp("suspend_run_loop".to_string()));
}

fn emit_native_yield_case_linux(artifact: &SuspendFunctionArtifact, state: &SuspendStateArtifact, function: &mut SysvFunctionBuilder) {
    let _ = artifact;
    function.push(X64Instruction::MovRspOffsetImm32 { offset: SUSPEND_STATE_RSP_OFFSET, value: state.resume_case_key });
    function.push(X64Instruction::Jmp("suspend_run_loop".to_string()));
}

/// `Raise` ?????Windows / MSVC??
///
/// ???? `raise` ??????? `Yield` ????? raise ??????????
/// ??????? `resume_case_key` ??? `suspend_run_loop` ???????????
/// ????? `Effectful::Resume` ???? resume?? `Resume = !` ? resume ??
/// ????`resume_case_key` ??????????
fn emit_native_raise_case_windows(_artifact: &SuspendFunctionArtifact, state: &SuspendStateArtifact, function: &mut MsvcFunctionBuilder) {
    function.push(X64Instruction::MovRspOffsetImm32 { offset: SUSPEND_STATE_RSP_OFFSET, value: state.resume_case_key });
    function.push(X64Instruction::Jmp("suspend_run_loop".to_string()));
}

/// `Raise` ?????Linux / SysV??? Windows ?????
fn emit_native_raise_case_linux(_artifact: &SuspendFunctionArtifact, state: &SuspendStateArtifact, function: &mut SysvFunctionBuilder) {
    function.push(X64Instruction::MovRspOffsetImm32 { offset: SUSPEND_STATE_RSP_OFFSET, value: state.resume_case_key });
    function.push(X64Instruction::Jmp("suspend_run_loop".to_string()));
}

fn emit_native_delegate_yield_case_windows(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    state: &SuspendStateArtifact,
    function: &mut MsvcFunctionBuilder,
) {
    let Some((table_label, method_index)) = resolve_suspend_witness_dispatch(submission, state)
    else {
        emit_native_yield_case_windows(artifact, state, function);
        return;
    };
    let exhausted = format!("suspend_delegate_exhausted_{}", state.state_id);
    emit_native_witness_receiver_load_windows(artifact, state, function);
    emit_witness_dispatch_x64_msvc(function, &table_label, method_index);
    function.push(X64Instruction::TestRegReg { dst: Reg64::Rax, src: Reg64::Rax });
    function.push(X64Instruction::Je(exhausted.clone()));
    emit_native_yield_case_windows(artifact, state, function);
    function.push(X64Instruction::Label(exhausted));
    emit_native_complete_state_windows(submission, artifact, state, function);
}

fn emit_native_delegate_yield_case_linux(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    state: &SuspendStateArtifact,
    function: &mut SysvFunctionBuilder,
) {
    let Some((table_label, method_index)) = resolve_suspend_witness_dispatch(submission, state)
    else {
        emit_native_yield_case_linux(artifact, state, function);
        return;
    };
    let exhausted = format!("suspend_delegate_exhausted_{}", state.state_id);
    emit_native_witness_receiver_load_linux(artifact, state, function);
    emit_witness_dispatch_x64_sysv(function, &table_label, method_index);
    function.push(X64Instruction::TestRegReg { dst: Reg64::Rax, src: Reg64::Rax });
    function.push(X64Instruction::Je(exhausted.clone()));
    emit_native_yield_case_linux(artifact, state, function);
    function.push(X64Instruction::Label(exhausted));
    emit_native_complete_state_linux(submission, artifact, state, function);
}

fn emit_native_await_case_windows(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    state: &SuspendStateArtifact,
    function: &mut MsvcFunctionBuilder,
) {
    let Some((table_label, method_index)) = resolve_suspend_witness_dispatch(submission, state)
    else {
        return;
    };
    emit_native_witness_receiver_load_windows(artifact, state, function);
    emit_witness_dispatch_x64_msvc(function, &table_label, method_index);
    function.push(X64Instruction::TestRegReg { dst: Reg64::Rax, src: Reg64::Rax });
    function.push(X64Instruction::Je(format!("suspend_await_yield_{}", state.state_id)));
    if let Some((cancel_table, cancel_index)) = resolve_tertiary_suspend_witness_dispatch(submission, state) {
        let not_cancelled_label = format!("suspend_await_not_cancelled_{}", state.state_id);
        let skip_output_label = format!("suspend_await_skip_output_{}", state.state_id);
        emit_native_witness_receiver_load_windows(artifact, state, function);
        emit_witness_dispatch_x64_msvc(function, &cancel_table, cancel_index);
        function.push(X64Instruction::TestRegReg { dst: Reg64::Rax, src: Reg64::Rax });
        function.push(X64Instruction::Je(not_cancelled_label.clone()));
        function.push(X64Instruction::XorReg { dst: Reg64::Rax, src: Reg64::Rax });
        function.push(X64Instruction::Jmp(skip_output_label.clone()));
        function.push(X64Instruction::Label(not_cancelled_label));
        if let Some((output_table, output_index)) = resolve_secondary_suspend_witness_dispatch(submission, state) {
            emit_native_witness_receiver_load_windows(artifact, state, function);
            emit_witness_dispatch_x64_msvc(function, &output_table, output_index);
        }
        function.push(X64Instruction::Label(skip_output_label));
    }
    else if let Some((output_table, output_index)) = resolve_secondary_suspend_witness_dispatch(submission, state) {
        emit_native_witness_receiver_load_windows(artifact, state, function);
        emit_witness_dispatch_x64_msvc(function, &output_table, output_index);
    }
    emit_native_complete_state_windows(submission, artifact, state, function);
    function.push(X64Instruction::Label(format!("suspend_await_yield_{}", state.state_id)));
    emit_native_yield_case_windows(artifact, state, function);
}

fn emit_native_await_case_linux(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    state: &SuspendStateArtifact,
    function: &mut SysvFunctionBuilder,
) {
    let Some((table_label, method_index)) = resolve_suspend_witness_dispatch(submission, state)
    else {
        return;
    };
    emit_native_witness_receiver_load_linux(artifact, state, function);
    emit_witness_dispatch_x64_sysv(function, &table_label, method_index);
    function.push(X64Instruction::TestRegReg { dst: Reg64::Rax, src: Reg64::Rax });
    function.push(X64Instruction::Je(format!("suspend_await_yield_{}", state.state_id)));
    if let Some((cancel_table, cancel_index)) = resolve_tertiary_suspend_witness_dispatch(submission, state) {
        let not_cancelled_label = format!("suspend_await_not_cancelled_{}", state.state_id);
        let skip_output_label = format!("suspend_await_skip_output_{}", state.state_id);
        emit_native_witness_receiver_load_linux(artifact, state, function);
        emit_witness_dispatch_x64_sysv(function, &cancel_table, cancel_index);
        function.push(X64Instruction::TestRegReg { dst: Reg64::Rax, src: Reg64::Rax });
        function.push(X64Instruction::Je(not_cancelled_label.clone()));
        function.push(X64Instruction::XorReg { dst: Reg64::Rax, src: Reg64::Rax });
        function.push(X64Instruction::Jmp(skip_output_label.clone()));
        function.push(X64Instruction::Label(not_cancelled_label));
        if let Some((output_table, output_index)) = resolve_secondary_suspend_witness_dispatch(submission, state) {
            emit_native_witness_receiver_load_linux(artifact, state, function);
            emit_witness_dispatch_x64_sysv(function, &output_table, output_index);
        }
        function.push(X64Instruction::Label(skip_output_label));
    }
    else if let Some((output_table, output_index)) = resolve_secondary_suspend_witness_dispatch(submission, state) {
        emit_native_witness_receiver_load_linux(artifact, state, function);
        emit_witness_dispatch_x64_sysv(function, &output_table, output_index);
    }
    emit_native_complete_state_linux(submission, artifact, state, function);
    function.push(X64Instruction::Label(format!("suspend_await_yield_{}", state.state_id)));
    emit_native_yield_case_linux(artifact, state, function);
}

fn emit_native_async_spawn_case_windows(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    state: &SuspendStateArtifact,
    function: &mut MsvcFunctionBuilder,
) {
    if let Some((table_label, method_index)) = resolve_suspend_witness_dispatch(submission, state) {
        emit_native_witness_receiver_load_windows(artifact, state, function);
        emit_witness_dispatch_x64_msvc(function, &table_label, method_index);
    }
    emit_native_complete_state_windows(submission, artifact, state, function);
}

fn emit_native_async_spawn_case_linux(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    state: &SuspendStateArtifact,
    function: &mut SysvFunctionBuilder,
) {
    if let Some((table_label, method_index)) = resolve_suspend_witness_dispatch(submission, state) {
        emit_native_witness_receiver_load_linux(artifact, state, function);
        emit_witness_dispatch_x64_sysv(function, &table_label, method_index);
    }
    emit_native_complete_state_linux(submission, artifact, state, function);
}

fn emit_native_async_block_case_windows(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    state: &SuspendStateArtifact,
    function: &mut MsvcFunctionBuilder,
) {
    let Some((table_label, method_index)) = resolve_suspend_witness_dispatch(submission, state)
    else {
        return;
    };
    let poll_label = format!("suspend_block_poll_{}", state.state_id);
    function.push(X64Instruction::Label(poll_label.clone()));
    emit_native_witness_receiver_load_windows(artifact, state, function);
    emit_witness_dispatch_x64_msvc(function, &table_label, method_index);
    function.push(X64Instruction::TestRegReg { dst: Reg64::Rax, src: Reg64::Rax });
    function.push(X64Instruction::Je(poll_label));
    if let Some((cancel_table, cancel_index)) = resolve_tertiary_suspend_witness_dispatch(submission, state) {
        let not_cancelled_label = format!("suspend_block_not_cancelled_{}", state.state_id);
        let skip_output_label = format!("suspend_block_skip_output_{}", state.state_id);
        emit_native_witness_receiver_load_windows(artifact, state, function);
        emit_witness_dispatch_x64_msvc(function, &cancel_table, cancel_index);
        function.push(X64Instruction::TestRegReg { dst: Reg64::Rax, src: Reg64::Rax });
        function.push(X64Instruction::Je(not_cancelled_label.clone()));
        function.push(X64Instruction::XorReg { dst: Reg64::Rax, src: Reg64::Rax });
        function.push(X64Instruction::Jmp(skip_output_label.clone()));
        function.push(X64Instruction::Label(not_cancelled_label));
        if let Some((output_table, output_index)) = resolve_secondary_suspend_witness_dispatch(submission, state) {
            emit_native_witness_receiver_load_windows(artifact, state, function);
            emit_witness_dispatch_x64_msvc(function, &output_table, output_index);
        }
        function.push(X64Instruction::Label(skip_output_label));
    }
    else if let Some((output_table, output_index)) = resolve_secondary_suspend_witness_dispatch(submission, state) {
        emit_native_witness_receiver_load_windows(artifact, state, function);
        emit_witness_dispatch_x64_msvc(function, &output_table, output_index);
    }
    emit_native_complete_state_windows(submission, artifact, state, function);
}

fn emit_native_async_block_case_linux(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    state: &SuspendStateArtifact,
    function: &mut SysvFunctionBuilder,
) {
    let Some((table_label, method_index)) = resolve_suspend_witness_dispatch(submission, state)
    else {
        return;
    };
    let poll_label = format!("suspend_block_poll_{}", state.state_id);
    function.push(X64Instruction::Label(poll_label.clone()));
    emit_native_witness_receiver_load_linux(artifact, state, function);
    emit_witness_dispatch_x64_sysv(function, &table_label, method_index);
    function.push(X64Instruction::TestRegReg { dst: Reg64::Rax, src: Reg64::Rax });
    function.push(X64Instruction::Je(poll_label));
    if let Some((cancel_table, cancel_index)) = resolve_tertiary_suspend_witness_dispatch(submission, state) {
        let not_cancelled_label = format!("suspend_block_not_cancelled_{}", state.state_id);
        let skip_output_label = format!("suspend_block_skip_output_{}", state.state_id);
        emit_native_witness_receiver_load_linux(artifact, state, function);
        emit_witness_dispatch_x64_sysv(function, &cancel_table, cancel_index);
        function.push(X64Instruction::TestRegReg { dst: Reg64::Rax, src: Reg64::Rax });
        function.push(X64Instruction::Je(not_cancelled_label.clone()));
        function.push(X64Instruction::XorReg { dst: Reg64::Rax, src: Reg64::Rax });
        function.push(X64Instruction::Jmp(skip_output_label.clone()));
        function.push(X64Instruction::Label(not_cancelled_label));
        if let Some((output_table, output_index)) = resolve_secondary_suspend_witness_dispatch(submission, state) {
            emit_native_witness_receiver_load_linux(artifact, state, function);
            emit_witness_dispatch_x64_sysv(function, &output_table, output_index);
        }
        function.push(X64Instruction::Label(skip_output_label));
    }
    else if let Some((output_table, output_index)) = resolve_secondary_suspend_witness_dispatch(submission, state) {
        emit_native_witness_receiver_load_linux(artifact, state, function);
        emit_witness_dispatch_x64_sysv(function, &output_table, output_index);
    }
    emit_native_complete_state_linux(submission, artifact, state, function);
}

fn emit_native_effect_for_state_windows(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    state: &SuspendStateArtifact,
    function: &mut MsvcFunctionBuilder,
) {
    match state.effect.as_str() {
        "Yield" => emit_native_yield_case_windows(artifact, state, function),
        "DelegateYield" => emit_native_delegate_yield_case_windows(submission, artifact, state, function),
        "Await" => emit_native_await_case_windows(submission, artifact, state, function),
        _ => function.push(X64Instruction::Jmp("suspend_run_done".to_string())),
    }
}

fn emit_native_effect_for_state_linux(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    state: &SuspendStateArtifact,
    function: &mut SysvFunctionBuilder,
) {
    match state.effect.as_str() {
        "Yield" => emit_native_yield_case_linux(artifact, state, function),
        "DelegateYield" => emit_native_delegate_yield_case_linux(submission, artifact, state, function),
        "Await" => emit_native_await_case_linux(submission, artifact, state, function),
        _ => function.push(X64Instruction::Jmp("suspend_run_done".to_string())),
    }
}

fn emit_native_complete_state_windows(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    state: &SuspendStateArtifact,
    function: &mut MsvcFunctionBuilder,
) {
    if let Some(next) = artifact.states.iter().find(|candidate| candidate.state_id == state.state_id + 1) {
        emit_native_effect_for_state_windows(submission, artifact, next, function);
    }
    else {
        function.push(X64Instruction::Jmp("suspend_run_done".to_string()));
    }
}

fn emit_native_complete_state_linux(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    state: &SuspendStateArtifact,
    function: &mut SysvFunctionBuilder,
) {
    if let Some(next) = artifact.states.iter().find(|candidate| candidate.state_id == state.state_id + 1) {
        emit_native_effect_for_state_linux(submission, artifact, next, function);
    }
    else {
        function.push(X64Instruction::Jmp("suspend_run_done".to_string()));
    }
}

fn emit_native_witness_receiver_load_windows(
    artifact: &SuspendFunctionArtifact,
    state: &SuspendStateArtifact,
    function: &mut MsvcFunctionBuilder,
) {
    if witness_receiver_field(state).filter(|field| frame_has_field(artifact, field)).is_some() {
        function.push(X64Instruction::LeaRspOffset { dst: Reg64::Rcx, offset: SUSPEND_SPILL_RSP_OFFSET });
    }
    else {
        function.push(X64Instruction::XorReg { dst: Reg64::Rcx, src: Reg64::Rcx });
    }
}

fn emit_native_witness_receiver_load_linux(
    artifact: &SuspendFunctionArtifact,
    state: &SuspendStateArtifact,
    function: &mut SysvFunctionBuilder,
) {
    if witness_receiver_field(state).filter(|field| frame_has_field(artifact, field)).is_some() {
        function.push(X64Instruction::LeaRspOffset { dst: Reg64::Rdi, offset: SUSPEND_SPILL_RSP_OFFSET });
    }
    else {
        function.push(X64Instruction::XorReg { dst: Reg64::Rdi, src: Reg64::Rdi });
    }
}

fn resolve_suspend_witness_dispatch(submission: &FragmentSubmission, state: &SuspendStateArtifact) -> Option<(String, u32)> {
    let binding = primary_witness_binding(state)?;
    let slot = resolve_witness_slot(submission, binding)?;
    let table = submission.witness_tables.iter().find(|table| table.table_label == slot.table_label)?;
    Some((table.table_label.clone(), slot.method_index))
}

/// ?? suspend ?????? witness ???`Future::output`???? witness ?? method index?
///
/// spec Task 3.2 ????? `poll` ?? ready ????? `output` ????? `T`?Native
/// ????????? `secondary_witness_binding` ??? `output` ?????
/// `emit_witness_dispatch_x64_msvc` / `emit_witness_dispatch_x64_sysv` ?? witness ?????
///
/// ???Native ??? `X64Instruction` IR ??????????????????
/// ?`MovRspOffsetImm32` ????????`MovMemRegImm32` ??????????
/// ?? `output` ??????? `RAX` ???????? `RSP+spill_offset`???
/// Native ?? `Yield` ??????? yielded ??????????????????
/// ? `std-data` ? `X64Instruction` ??? `MovRspOffsetReg64` ?????????????
fn resolve_secondary_suspend_witness_dispatch(submission: &FragmentSubmission, state: &SuspendStateArtifact) -> Option<(String, u32)> {
    let binding = secondary_witness_binding(state)?;
    let slot = resolve_witness_slot(submission, binding)?;
    let table = submission.witness_tables.iter().find(|table| table.table_label == slot.table_label)?;
    Some((table.table_label.clone(), slot.method_index))
}

/// ?? suspend ?????? witness ???`Future::is_cancelled`???? witness ?? method index?
///
/// spec Task 5.2 ????? `Future::poll` ?? ready ???? `output` ????? `is_cancelled`?
/// ??? true ??? `output` ???? null?`Xor Rax, Rax`??????????? complete?
/// Native ????????? [`tertiary_witness_binding`] ??? `is_cancelled` ???
/// ?? `emit_witness_dispatch_x64_msvc` / `emit_witness_dispatch_x64_sysv` ?? witness ?????
///
/// ? [`resolve_secondary_suspend_witness_dispatch`] ???????? `RAX` ?
/// ?Native `X64Instruction` IR ????? RAX ?????? Task 3.2 ??????
fn resolve_tertiary_suspend_witness_dispatch(submission: &FragmentSubmission, state: &SuspendStateArtifact) -> Option<(String, u32)> {
    let binding = tertiary_witness_binding(state)?;
    let slot = resolve_witness_slot(submission, binding)?;
    let table = submission.witness_tables.iter().find(|table| table.table_label == slot.table_label)?;
    Some((table.table_label.clone(), slot.method_index))
}
