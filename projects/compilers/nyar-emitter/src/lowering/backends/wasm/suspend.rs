//! Suspend / witness CPS run-loop lowering for wasm host shells.
use crate::{
    FragmentSubmission,
    nyar_backend_wasi::{WasmBinaryModule, materialize_witness_bytes, plan_witness_table_layout, resolve_witness_call},
};
use nyar::{SuspendFunctionArtifact, SuspendStateArtifact, WitnessSubmission};
use std_data::binary::wasm::{
    BLOCKTYPE_EMPTY, VALTYPE_I32, WasmExternalKind, WasmOpcode, encode_br, encode_br_if, encode_i32_const, encode_i32_eqz, encode_i32_load,
    encode_i32_store, encode_local_get, encode_unreachable,
};

use super::{
    super::{
        suspend_sm::{dispatch_case_keys, has_resolved_witness_metadata, resolve_state_for_case},
        suspend_witness::{
            frame_has_field, primary_witness_binding, resolve_witness_slot, secondary_witness_binding, tertiary_witness_binding,
            witness_receiver_field,
        },
    },
    cabi::{
        CABI_HEAP_DEFAULT_BASE, cabi_heap_base_after_data, cabi_heap_global_section, memory_min_pages_for_heap_base,
        wasm_cabi_realloc_bump_body,
    },
    host_imports::{declared_wasi_host_import_targets, wasm_impl_return_offset_body},
    sections::{
        code_section_bytes, data_section_bytes, elem_section_bytes, encode_sleb128_i32, encode_uleb128, export_section_bytes,
        function_section_bytes, import_section_bytes, memory_section_bytes, table_section_bytes, type_section_bytes, wasm_function_body,
        wasm_function_type,
    },
};

pub(crate) fn lower_suspend_module(
    submission: &FragmentSubmission,
    artifact: &nyar::SuspendFunctionArtifact,
    export_name: &str,
    returns_i32: bool,
) -> WasmBinaryModule {
    if has_resolved_witness_metadata(submission) {
        lower_suspend_witness_wasm_module(submission, artifact, export_name, returns_i32)
    }
    else {
        lower_suspend_wasm_module(submission, artifact, export_name, returns_i32)
    }
}

fn lower_suspend_wasm_module(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    export_name: &str,
    returns_i32: bool,
) -> WasmBinaryModule {
    let _ = submission;
    let mut module = WasmBinaryModule::new();
    if export_name == "_start" && !returns_i32 {
        module.sections.push(type_section_bytes(vec![
            wasm_function_type(&[], &[]),
            wasm_function_type(&[VALTYPE_I32, VALTYPE_I32, VALTYPE_I32, VALTYPE_I32], &[VALTYPE_I32]),
        ]));
        module.sections.push(function_section_bytes(&[0, 0, 0, 1, 0]));
        module.sections.push(memory_section_bytes(memory_min_pages_for_heap_base(CABI_HEAP_DEFAULT_BASE)));
        module.sections.push(cabi_heap_global_section(CABI_HEAP_DEFAULT_BASE));
        module.sections.push(export_section_bytes(&wasi_component_command_exports(0, 1, 2, 3, 4)));
        module.sections.push(code_section_bytes(&[
            wasm_function_body(suspend_run_loop_wasm_bytes(artifact, false, None)),
            wasm_function_body(suspend_run_loop_wasm_bytes(artifact, false, None)),
            wasm_function_body(wasm_noop_body()),
            wasm_function_body(wasm_cabi_realloc_bump_body()),
            wasm_function_body(wasm_noop_body()),
        ]));
        return module;
    }

    let result_types: &[u8] = if returns_i32 { &[VALTYPE_I32] } else { &[] };
    module.sections.push(type_section_bytes(vec![wasm_function_type(&[], result_types)]));
    module.sections.push(function_section_bytes(&[0]));
    module.sections.push(export_section_bytes(&[(export_name, WasmExternalKind::Func.as_u8(), 0)]));
    module.sections.push(code_section_bytes(&[wasm_function_body(suspend_run_loop_wasm_bytes(artifact, returns_i32, None))]));
    module
}

fn lower_suspend_witness_wasm_module(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    export_name: &str,
    returns_i32: bool,
) -> WasmBinaryModule {
    let witness_state = artifact.states.iter().find(|state| primary_witness_binding(state).is_some());
    let binding = witness_state.and_then(|state| primary_witness_binding(state));
    let slot = binding.and_then(|binding| resolve_witness_slot(submission, binding));
    let table = slot
        .as_ref()
        .and_then(|slot| submission.witness_tables.iter().find(|table| table.table_label == slot.table_label))
        .cloned()
        .or_else(|| submission.witness_tables.first().cloned())
        .unwrap_or(WitnessSubmission {
            type_name: "CounterIterator".to_string(),
            trait_name: "Iterator".to_string(),
            table_label: "witness_table".to_string(),
            fat_ptr_label: "witness_fat".to_string(),
            methods: Vec::new(),
            result_literal: String::new(),
        });
    let method_index = slot.as_ref().map(|slot| slot.method_index).unwrap_or_else(|| {
        artifact.states.iter().find_map(|state| primary_witness_binding(state)).map(|binding| binding.method_index).unwrap_or(0)
    });
    let method_names = table.methods.iter().map(|slot| slot.method_name.as_str()).collect::<Vec<_>>();
    let layout = plan_witness_table_layout(&table.type_name, &table.trait_name, &method_names);
    let witness_bytes = materialize_witness_bytes(&layout);
    let witness_offset = 0u32;
    let spill_offset = u32::try_from(witness_bytes.len()).unwrap();
    let witness_type_index = 0u32;
    let start_function_index = u32::try_from(layout.methods.len()).unwrap_or(1);

    let mut data_bytes = witness_bytes;
    data_bytes.extend_from_slice(&0i32.to_le_bytes());

    let mut impl_bodies = Vec::new();
    for method in &table.methods {
        impl_bodies.push(wasm_function_body(wasm_witness_impl_body(&table.trait_name, &method.method_name)));
    }
    let impl_count = impl_bodies.len() as u32;

    let mut module = WasmBinaryModule::new();
    let result_types: &[u8] = if returns_i32 { &[VALTYPE_I32] } else { &[] };
    let mut types = vec![wasm_function_type(&[VALTYPE_I32], &[VALTYPE_I32])];
    types.push(wasm_function_type(&[], result_types));
    if export_name == "_start" && !returns_i32 {
        types.push(wasm_function_type(&[VALTYPE_I32, VALTYPE_I32, VALTYPE_I32, VALTYPE_I32], &[VALTYPE_I32]));
    }
    module.sections.push(type_section_bytes(types));

    let mut function_indices: Vec<u32> = (0..impl_count).collect();
    function_indices.push(start_function_index);

    let mut code_bodies = impl_bodies;
    let start_body = wasm_function_body(suspend_run_loop_wasm_bytes(
        artifact,
        returns_i32,
        Some(WasmWitnessContext {
            witness_offset,
            spill_offset,
            witness_type_index,
            method_index,
            function_index: resolve_witness_call(&layout, method_index).unwrap_or(0),
        }),
    ));

    module.sections.push(table_section_bytes(1));
    let heap_base = cabi_heap_base_after_data(data_bytes.len());
    module.sections.push(memory_section_bytes(memory_min_pages_for_heap_base(heap_base)));

    if export_name == "_start" && !returns_i32 {
        function_indices.extend_from_slice(&[1, 1, 2, 1]);
        module.sections.push(function_section_bytes(&function_indices));
        module.sections.push(cabi_heap_global_section(heap_base));
        module.sections.push(export_section_bytes(&wasi_component_command_exports(
            start_function_index,
            start_function_index + 1,
            start_function_index + 2,
            start_function_index + 3,
            start_function_index + 4,
        )));
        module.sections.push(elem_section_bytes(0, &(0..impl_count).collect::<Vec<_>>()));
        code_bodies.push(start_body.clone());
        code_bodies.push(start_body);
        code_bodies.push(wasm_function_body(wasm_noop_body()));
        code_bodies.push(wasm_function_body(wasm_cabi_realloc_bump_body()));
        code_bodies.push(wasm_function_body(wasm_noop_body()));
    }
    else {
        module.sections.push(function_section_bytes(&function_indices));
        module.sections.push(export_section_bytes(&[(export_name, WasmExternalKind::Func.as_u8(), start_function_index)]));
        module.sections.push(elem_section_bytes(0, &(0..impl_count).collect::<Vec<_>>()));
        code_bodies.push(start_body);
    }

    module.sections.push(code_section_bytes(&code_bodies));
    module.sections.push(data_section_bytes(0, &data_bytes));
    module
}

fn wasi_component_command_exports(
    start_index: u32,
    run_index: u32,
    post_run_index: u32,
    realloc_index: u32,
    initialize_index: u32,
) -> [(&'static str, u8, u32); 6] {
    [
        ("_start", WasmExternalKind::Func.as_u8(), start_index),
        ("run", WasmExternalKind::Func.as_u8(), run_index),
        ("cabi_post_run", WasmExternalKind::Func.as_u8(), post_run_index),
        ("memory", WasmExternalKind::Memory.as_u8(), 0),
        ("cabi_realloc", WasmExternalKind::Func.as_u8(), realloc_index),
        ("_initialize", WasmExternalKind::Func.as_u8(), initialize_index),
    ]
}

fn wasm_noop_body() -> Vec<u8> {
    vec![0, WasmOpcode::End.as_u8()]
}

struct WasmWitnessContext {
    witness_offset: u32,
    spill_offset: u32,
    witness_type_index: u32,
    method_index: u32,
    function_index: u32,
}

/// Witness `next`/`poll` stub: `(i32 receiver_ptr) -> i32` (0 = null/false)。
///
/// 对未实现的 trait/method 组合,发射 `unreachable` trap 而非静默返回 0。
/// 静默返回 0 会导致 poll 永远返回 false(死循环)或调用方得到错误结果。
fn wasm_witness_impl_body(trait_name: &str, method_name: &str) -> Vec<u8> {
    if trait_name == "Future" && method_name == "poll" {
        return wasm_future_poll_impl_bytes();
    }
    if trait_name == "Iterator" && method_name == "next" {
        return wasm_iterator_next_impl_bytes();
    }
    // unreachable trap: local_count=0, unreachable (0x00), end (0x0B)
    vec![0, WasmOpcode::Unreachable.as_u8(), WasmOpcode::End.as_u8()]
}

fn wasm_future_poll_impl_bytes() -> Vec<u8> {
    // Exact historical encoding (locals + poll once then return Ready).
    let mut body = vec![1, 1, VALTYPE_I32];
    body.extend_from_slice(&[
        WasmOpcode::LocalGet.as_u8(),
        0,
        WasmOpcode::I32Eqz.as_u8(),
        WasmOpcode::BrIf.as_u8(),
        1,
        WasmOpcode::LocalGet.as_u8(),
        0,
        WasmOpcode::I32Load.as_u8(),
        2,
        0,
        WasmOpcode::I32Eqz.as_u8(),
        WasmOpcode::BrIf.as_u8(),
        1,
        WasmOpcode::LocalGet.as_u8(),
        0,
        WasmOpcode::I32Const.as_u8(),
        1,
        WasmOpcode::I32Store.as_u8(),
        2,
        0,
        WasmOpcode::I32Const.as_u8(),
        0,
        WasmOpcode::End.as_u8(),
        WasmOpcode::I32Const.as_u8(),
        1,
        WasmOpcode::End.as_u8(),
    ]);
    body
}

fn wasm_iterator_next_impl_bytes() -> Vec<u8> {
    let mut body = vec![1, 1, VALTYPE_I32];
    body.extend_from_slice(&[
        WasmOpcode::LocalGet.as_u8(),
        0,
        WasmOpcode::I32Eqz.as_u8(),
        WasmOpcode::BrIf.as_u8(),
        1,
        WasmOpcode::LocalGet.as_u8(),
        0,
        WasmOpcode::I32Load.as_u8(),
        2,
        0,
        WasmOpcode::I32Const.as_u8(),
        2,
        WasmOpcode::I32LeU.as_u8(),
        WasmOpcode::BrIf.as_u8(),
        1,
        WasmOpcode::LocalGet.as_u8(),
        0,
        WasmOpcode::I32Load.as_u8(),
        2,
        0,
        WasmOpcode::I32Const.as_u8(),
        0,
        WasmOpcode::I32Eq.as_u8(),
        WasmOpcode::BrIf.as_u8(),
        2,
        WasmOpcode::LocalGet.as_u8(),
        0,
        WasmOpcode::I32Load.as_u8(),
        2,
        0,
        WasmOpcode::I32Const.as_u8(),
        1,
        WasmOpcode::I32Eq.as_u8(),
        WasmOpcode::BrIf.as_u8(),
        1,
        WasmOpcode::LocalGet.as_u8(),
        0,
        WasmOpcode::I32Const.as_u8(),
        1,
        WasmOpcode::I32Store.as_u8(),
        2,
        0,
        WasmOpcode::I32Const.as_u8(),
        1,
        WasmOpcode::End.as_u8(),
        WasmOpcode::LocalGet.as_u8(),
        0,
        WasmOpcode::I32Const.as_u8(),
        2,
        WasmOpcode::I32Store.as_u8(),
        2,
        0,
        WasmOpcode::I32Const.as_u8(),
        2,
        WasmOpcode::End.as_u8(),
        WasmOpcode::I32Const.as_u8(),
        0,
        WasmOpcode::End.as_u8(),
    ]);
    body
}

fn suspend_run_loop_wasm_bytes(artifact: &SuspendFunctionArtifact, returns_i32: bool, witness: Option<WasmWitnessContext>) -> Vec<u8> {
    let mut body = Vec::new();
    let local_count = 1 + (witness.is_some() as usize) * 2 + returns_i32 as usize;
    encode_uleb128(u32::try_from(local_count).unwrap(), &mut body);
    body.push(1);
    body.push(VALTYPE_I32);
    if witness.is_some() {
        body.push(1);
        body.push(VALTYPE_I32);
        body.push(1);
        body.push(VALTYPE_I32);
    }
    if returns_i32 {
        body.push(1);
        body.push(VALTYPE_I32);
    }
    emit_i32_const(0, &mut body);
    emit_local_set(0, &mut body);
    if let Some(ctx) = witness.as_ref() {
        emit_i32_const(i32::try_from(ctx.spill_offset).unwrap(), &mut body);
        emit_local_set(1, &mut body);
        emit_i32_const(0, &mut body);
        emit_i32_const(i32::try_from(ctx.spill_offset).unwrap(), &mut body);
        body.extend_from_slice(&[WasmOpcode::I32Store.as_u8(), 2, 0]);
    }
    // loop { block { dispatch } br 0 }
    body.extend_from_slice(&[WasmOpcode::Loop.as_u8(), BLOCKTYPE_EMPTY, WasmOpcode::Block.as_u8(), BLOCKTYPE_EMPTY]);
    emit_suspend_dispatch(artifact, witness.as_ref(), &mut body);
    WasmOpcode::End.encode(&mut body);
    WasmOpcode::Br.encode(&mut body);
    body.push(0);
    WasmOpcode::End.encode(&mut body);
    if returns_i32 {
        emit_i32_const(0, &mut body);
    }
    WasmOpcode::End.encode(&mut body);
    body
}

fn emit_suspend_dispatch(artifact: &SuspendFunctionArtifact, witness: Option<&WasmWitnessContext>, body: &mut Vec<u8>) {
    let case_keys = dispatch_case_keys(artifact);
    if can_use_br_table_dispatch(&case_keys) {
        emit_suspend_br_table_dispatch(artifact, witness, &case_keys, body);
        return;
    }
    emit_suspend_branch_chain_dispatch(artifact, witness, &case_keys, body);
}

fn can_use_br_table_dispatch(case_keys: &[u32]) -> bool {
    if case_keys.is_empty() {
        return false;
    }
    let max = *case_keys.iter().max().unwrap_or(&0);
    case_keys.len() == usize::try_from(max + 1).unwrap_or(usize::MAX) && case_keys.iter().enumerate().all(|(index, key)| *key == index as u32)
}

fn emit_suspend_br_table_dispatch(
    artifact: &SuspendFunctionArtifact,
    witness: Option<&WasmWitnessContext>,
    case_keys: &[u32],
    body: &mut Vec<u8>,
) {
    let case_count = case_keys.len();
    for _ in 0..case_count {
        WasmOpcode::Block.encode(body);
        body.push(BLOCKTYPE_EMPTY);
    }
    emit_local_get(0, body);
    WasmOpcode::BrTable.encode(body);
    encode_uleb128(u32::try_from(case_count).unwrap(), body);
    for index in 0..case_count {
        encode_uleb128(u32::try_from(index).unwrap(), body);
    }
    encode_uleb128(u32::try_from(case_count).unwrap(), body);
    WasmOpcode::End.encode(body);
    emit_suspend_case_body(artifact, witness, case_keys[0], body);
    emit_br_continue_loop_from_br_table_case(body, case_count, 0);
    for (idx, case_key) in case_keys.iter().skip(1).enumerate() {
        WasmOpcode::End.encode(body);
        emit_suspend_case_body(artifact, witness, *case_key, body);
        emit_br_continue_loop_from_br_table_case(body, case_count, idx + 1);
    }
    emit_br_exit_loop(body);
}

fn emit_br_continue_loop(body: &mut Vec<u8>) {
    WasmOpcode::Br.encode(body);
    body.push(1);
}

fn emit_br_exit_loop(body: &mut Vec<u8>) {
    WasmOpcode::Br.encode(body);
    body.push(1);
}

fn emit_br_continue_loop_from_br_table_case(body: &mut Vec<u8>, case_count: usize, case_index: usize) {
    WasmOpcode::Br.encode(body);
    encode_uleb128(u32::try_from(case_count - case_index).unwrap(), body);
}

fn emit_suspend_branch_chain_dispatch(
    artifact: &SuspendFunctionArtifact,
    witness: Option<&WasmWitnessContext>,
    case_keys: &[u32],
    body: &mut Vec<u8>,
) {
    for case_key in case_keys {
        emit_local_get(0, body);
        emit_i32_const(i32::try_from(*case_key).unwrap(), body);
        WasmOpcode::I32Eq.encode(body);
        WasmOpcode::BrIf.encode(body);
        body.push(1);
        emit_suspend_case_body(artifact, witness, *case_key, body);
        emit_br_continue_loop(body);
    }
    emit_br_exit_loop(body);
}

fn emit_suspend_case_body(artifact: &SuspendFunctionArtifact, witness: Option<&WasmWitnessContext>, case_key: u32, body: &mut Vec<u8>) {
    let Some(state) = resolve_state_for_case(artifact, case_key)
    else {
        return;
    };
    match state.effect.as_str() {
        "Yield" if case_key != 0 && case_key == state.resume_case_key => {
            emit_wasm_complete_state(artifact, witness, state, body);
        }
        "Yield" => emit_wasm_yield_case(artifact, state, body),
        "DelegateYield" => emit_wasm_delegate_yield_case(artifact, witness, state, body),
        "Await" => emit_wasm_await_case(artifact, witness, state, body),
        "AsyncSpawn" => emit_wasm_async_spawn_case(artifact, witness, state, body),
        "AsyncBlock" => emit_wasm_async_block_case(artifact, witness, state, body),
        "Raise" if case_key != 0 && case_key == state.resume_case_key => {
            emit_wasm_complete_state(artifact, witness, state, body);
        }
        "Raise" => emit_wasm_yield_case(artifact, state, body),
        _ => {
            // 未知 effect:发射 wasm `unreachable` trap (0x00) 而非静默跳过。
            // 静默跳过会导致 br_table case 体为空,运行时语义错误。
            WasmOpcode::Unreachable.encode(body);
        }
    }
}

fn emit_wasm_yield_case(_artifact: &SuspendFunctionArtifact, state: &SuspendStateArtifact, body: &mut Vec<u8>) {
    emit_i32_const(i32::try_from(state.resume_case_key).unwrap(), body);
    emit_local_set(0, body);
    emit_br_continue_loop(body);
}

fn emit_wasm_delegate_yield_case(
    artifact: &SuspendFunctionArtifact,
    witness: Option<&WasmWitnessContext>,
    state: &SuspendStateArtifact,
    body: &mut Vec<u8>,
) {
    let Some(ctx) = witness
    else {
        emit_wasm_yield_case(artifact, state, body);
        return;
    };
    emit_wasm_witness_receiver_load(artifact, state, witness, body);
    emit_wasm_witness_call(ctx, body);
    WasmOpcode::I32Eqz.encode(body);
    WasmOpcode::If.encode(body);
    body.push(BLOCKTYPE_EMPTY);
    emit_wasm_complete_state(artifact, witness, state, body);
    WasmOpcode::End.encode(body);
    emit_wasm_yield_case(artifact, state, body);
}

fn emit_wasm_await_case(
    artifact: &SuspendFunctionArtifact,
    witness: Option<&WasmWitnessContext>,
    state: &SuspendStateArtifact,
    body: &mut Vec<u8>,
) {
    let Some(ctx) = witness
    else {
        return;
    };
    emit_wasm_witness_receiver_load(artifact, state, witness, body);
    emit_wasm_witness_call(ctx, body);
    WasmOpcode::I32Eqz.encode(body);
    WasmOpcode::If.encode(body);
    body.push(BLOCKTYPE_EMPTY);
    emit_wasm_yield_case(artifact, state, body);
    WasmOpcode::End.encode(body);
    emit_wasm_cancel_check_or_output(artifact, witness, state, body);
    emit_wasm_complete_state(artifact, witness, state, body);
}

fn emit_wasm_async_spawn_case(
    artifact: &SuspendFunctionArtifact,
    witness: Option<&WasmWitnessContext>,
    state: &SuspendStateArtifact,
    body: &mut Vec<u8>,
) {
    let Some(ctx) = witness
    else {
        emit_wasm_complete_state(artifact, witness, state, body);
        return;
    };
    emit_wasm_witness_receiver_load(artifact, state, witness, body);
    emit_wasm_witness_call(ctx, body);
    WasmOpcode::Drop.encode(body);
    emit_wasm_complete_state(artifact, witness, state, body);
}

fn emit_wasm_async_block_case(
    artifact: &SuspendFunctionArtifact,
    witness: Option<&WasmWitnessContext>,
    state: &SuspendStateArtifact,
    body: &mut Vec<u8>,
) {
    let Some(ctx) = witness
    else {
        return;
    };
    emit_wasm_witness_receiver_load(artifact, state, witness, body);
    emit_wasm_witness_call(ctx, body);
    WasmOpcode::I32Eqz.encode(body);
    WasmOpcode::If.encode(body);
    body.push(BLOCKTYPE_EMPTY);
    emit_wasm_yield_case(artifact, state, body);
    WasmOpcode::End.encode(body);
    emit_wasm_cancel_check_or_output(artifact, witness, state, body);
    emit_wasm_complete_state(artifact, witness, state, body);
}

/// 在 `Future::poll` 返回 ready 后调用 `Future::output` 取出恢复值 `T`，并存入 local 2（专用输出槽）。
///
/// spec Task 3.2 要求后端在 `poll` 返回 true 后显式调用 `output` 获取 `T`。WASM 后端
/// 通过 [`secondary_witness_binding`] 获取 `output` 绑定的 `method_index`，从 witness 表的
/// `witness_offset + method_index * 4` 偏移处加载函数索引，发射 `call_indirect`，再通过
/// `local.set 2` 将结果存入 local 2（专用输出槽，避免覆盖 `__state`/local 0 导致状态
/// 分发损坏）。当 `secondary_witness_binding` 返回 `None`（旧单绑定工件）时不发射任何
/// 字节，后端回退到旧有行为，保持向后兼容。
fn emit_wasm_output_call_and_store(
    artifact: &SuspendFunctionArtifact,
    witness: Option<&WasmWitnessContext>,
    state: &SuspendStateArtifact,
    body: &mut Vec<u8>,
) {
    let Some(ctx) = witness
    else {
        return;
    };
    let Some(output_binding) = secondary_witness_binding(state)
    else {
        return;
    };
    emit_wasm_witness_receiver_load(artifact, state, witness, body);
    let output_table_offset = ctx.witness_offset + output_binding.method_index * 4;
    emit_i32_const(i32::try_from(output_table_offset).unwrap(), body);
    body.extend_from_slice(&[WasmOpcode::I32Load.as_u8(), 2, 0]);
    WasmOpcode::CallIndirect.encode(body);
    encode_uleb128(ctx.witness_type_index, body);
    WasmOpcode::Unreachable.encode(body);
    emit_local_set(2, body);
}

/// spec Task 5.2：在 `Future::poll` 返回 ready 后，先检查 `is_cancelled`，若被取消则跳过 `output`
/// 调用，以 `i32.const 0`（null）作为恢复值存入 local 2；否则执行原有 `output` 调用取出 `T`。
///
/// 该辅助在 [`emit_wasm_output_call_and_store`] 外包裹可选的 cancel 检查：
/// - 当 [`tertiary_witness_binding`] 返回 `None`（impl 未声明 `is_cancelled`）时，
///   仅调用 [`emit_wasm_output_call_and_store`]，行为与 Task 3.2 完全一致，保持向后兼容；
/// - 当存在第三条 witness 绑定时，发射
///   `witness receiver load → i32.load → call_indirect (is_cancelled) → if → i32.const 0 → local.set 2 →
///   else → output 调用 → end` 的序列。
///
/// 注意：WASM `if` 弹出栈顶 i32，非零时走 if 分支。`is_cancelled` 返回 1（已取消）时走 if 分支
/// （cancel path），返回 0（未取消）时走 else 分支（output path），因此这里不使用 `i32.eqz`。
fn emit_wasm_cancel_check_or_output(
    artifact: &SuspendFunctionArtifact,
    witness: Option<&WasmWitnessContext>,
    state: &SuspendStateArtifact,
    body: &mut Vec<u8>,
) {
    let Some(ctx) = witness
    else {
        return;
    };
    let Some(cancel_binding) = tertiary_witness_binding(state)
    else {
        emit_wasm_output_call_and_store(artifact, witness, state, body);
        return;
    };
    emit_wasm_witness_receiver_load(artifact, state, witness, body);
    let cancel_table_offset = ctx.witness_offset + cancel_binding.method_index * 4;
    emit_i32_const(i32::try_from(cancel_table_offset).unwrap(), body);
    body.extend_from_slice(&[WasmOpcode::I32Load.as_u8(), 2, 0]);
    WasmOpcode::CallIndirect.encode(body);
    encode_uleb128(ctx.witness_type_index, body);
    WasmOpcode::Unreachable.encode(body);
    WasmOpcode::If.encode(body);
    body.push(BLOCKTYPE_EMPTY);
    emit_i32_const(0, body);
    emit_local_set(2, body);
    WasmOpcode::Else.encode(body);
    emit_wasm_output_call_and_store(artifact, witness, state, body);
    WasmOpcode::End.encode(body);
}

fn emit_wasm_complete_state(
    artifact: &SuspendFunctionArtifact,
    witness: Option<&WasmWitnessContext>,
    state: &SuspendStateArtifact,
    body: &mut Vec<u8>,
) {
    if let Some(next) = artifact.states.iter().find(|candidate| candidate.state_id == state.state_id + 1) {
        match next.effect.as_str() {
            "Yield" => emit_wasm_yield_case(artifact, next, body),
            "DelegateYield" => emit_wasm_delegate_yield_case(artifact, witness, next, body),
            "Await" => emit_wasm_await_case(artifact, witness, next, body),
            "AsyncSpawn" => emit_wasm_async_spawn_case(artifact, witness, next, body),
            "AsyncBlock" => emit_wasm_async_block_case(artifact, witness, next, body),
            "Raise" => emit_wasm_yield_case(artifact, next, body),
            _ => {
                // 未知下一状态 effect:发射 `unreachable` trap 而非静默跳过。
                WasmOpcode::Unreachable.encode(body);
            }
        }
    }
}

fn emit_wasm_witness_receiver_load(
    artifact: &SuspendFunctionArtifact,
    state: &SuspendStateArtifact,
    witness: Option<&WasmWitnessContext>,
    body: &mut Vec<u8>,
) {
    let Some(_ctx) = witness
    else {
        emit_i32_const(0, body);
        return;
    };
    if witness_receiver_field(state).filter(|field| frame_has_field(artifact, field)).is_some() {
        emit_local_get(1, body);
        body.extend_from_slice(&[WasmOpcode::I32Load.as_u8(), 2, 0]);
    }
    else {
        emit_i32_const(0, body);
    }
}

fn emit_wasm_witness_call(ctx: &WasmWitnessContext, body: &mut Vec<u8>) {
    emit_i32_const(i32::try_from(ctx.witness_offset).unwrap(), body);
    body.extend_from_slice(&[WasmOpcode::I32Load.as_u8(), 2, 0]);
    WasmOpcode::CallIndirect.encode(body);
    encode_uleb128(ctx.witness_type_index, body);
    WasmOpcode::Unreachable.encode(body);
}

fn emit_i32_const(value: i32, body: &mut Vec<u8>) {
    WasmOpcode::I32Const.encode(body);
    encode_sleb128_i32(value, body);
}

fn emit_local_get(index: u32, body: &mut Vec<u8>) {
    WasmOpcode::LocalGet.encode(body);
    encode_uleb128(index, body);
}

fn emit_local_set(index: u32, body: &mut Vec<u8>) {
    WasmOpcode::LocalSet.encode(body);
    encode_uleb128(index, body);
}

pub(crate) fn suspend_run_loop_with_witness_wasm_bytes(
    artifact: &SuspendFunctionArtifact,
    witness_offset: u32,
    witness_type_index: u32,
    method_index: u32,
    function_index: u32,
    returns_i32: bool,
) -> Vec<u8> {
    suspend_run_loop_wasm_bytes(
        artifact,
        returns_i32,
        Some(WasmWitnessContext { witness_offset, spill_offset: witness_offset + 4, witness_type_index, method_index, function_index }),
    )
}
