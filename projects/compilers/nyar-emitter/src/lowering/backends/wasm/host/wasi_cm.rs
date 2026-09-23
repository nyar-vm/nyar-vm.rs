//! Lowering for the `WasiComponent` host boundary.
//!
//! This path never synthesizes a legacy default import target. Any host import
//! must come from the source declaration and is forwarded through
//! `wasi_host_import_target`.
//!
//! WASI 轨禁止再偷偷补 legacy argv/stdout 导入，也禁止用 preview1 `fd_write`/
//! `fd_read` ABI 伪装写出口。源码声明的 imports 只进入返回的 import 列表
//! （供 WIT/component 包装）；core module 只发最小 `_start` 壳或
//! witness/suspend 真路径。

use nyar::WitnessSubmission;
use std_data::binary::wasm::{VALTYPE_I32, VALTYPE_I64, WasmExternalKind, WasmOpcode, encode_i32_const};

use crate::{
    FragmentSubmission,
    nyar_backend_wasi::{WasiPreview, WasmBinaryModule, materialize_witness_bytes, plan_witness_table_layout},
};

use super::super::{
    super::suspend_sm::first_suspend_function,
    cabi::{
        CABI_HEAP_DEFAULT_BASE, cabi_heap_base_after_data, cabi_heap_global_section, memory_min_pages_for_heap_base,
        wasm_cabi_realloc_bump_body,
    },
    host_imports::{declared_wasi_host_import_targets_for, wasm_impl_return_offset_body},
    sections::{
        code_section_bytes, data_section_bytes, export_section_bytes, function_section_bytes, memory_section_bytes, type_section_bytes,
        wasm_function_body, wasm_function_type,
    },
    suspend::lower_suspend_module,
};

/// Lower a fragment for the `WasiComponent` host boundary (Preview2 package train).
pub(super) fn lower_fragment_to_wasi_cm_module(submission: &FragmentSubmission) -> (WasmBinaryModule, Vec<(String, String)>) {
    lower_fragment_to_wasi_cm_module_for(submission, WasiPreview::Preview2)
}

/// Lower a fragment for the `WasiComponent` host boundary with an explicit package train.
pub(super) fn lower_fragment_to_wasi_cm_module_for(
    submission: &FragmentSubmission,
    preview: WasiPreview,
) -> (WasmBinaryModule, Vec<(String, String)>) {
    let declared_imports = declared_wasi_host_import_targets_for(submission, preview);
    for (module, _) in &declared_imports {
        if module == "wasi_snapshot_preview1" {
            panic!("wasi_snapshot_preview1 is strictly forbidden. Please use wasip2 or wasip3.");
        }
    }

    if !submission.witness_calls.is_empty() {
        return lower_witness_wasi_cm_module_for(submission, preview);
    }
    lower_plain_wasi_cm_module_for(submission, preview)
}

/// Lower a non-witness WASI component fragment.
fn lower_plain_wasi_cm_module_for(submission: &FragmentSubmission, preview: WasiPreview) -> (WasmBinaryModule, Vec<(String, String)>) {
    let declared_imports = declared_wasi_host_import_targets_for(submission, preview);

    if let Some(artifact) = first_suspend_function(submission) {
        return (lower_suspend_module(submission, artifact, "_start", false), declared_imports);
    }

    let mut module = WasmBinaryModule::new();
    emit_minimal_start_module(&mut module, preview, &declared_imports);
    (module, declared_imports)
}

/// Lower a witness-based WASI component fragment.
fn lower_witness_wasi_cm_module_for(submission: &FragmentSubmission, preview: WasiPreview) -> (WasmBinaryModule, Vec<(String, String)>) {
    let declared_imports = declared_wasi_host_import_targets_for(submission, preview);
    let table = submission.witness_tables.first().cloned().unwrap_or(WitnessSubmission {
        type_name: "Dog".to_string(),
        trait_name: "Animal".to_string(),
        table_label: "witness_table".to_string(),
        fat_ptr_label: "witness_fat".to_string(),
        methods: Vec::new(),
        result_literal: "woof".to_string(),
    });
    let method_names = table.methods.iter().map(|slot| slot.method_name.as_str()).collect::<Vec<_>>();
    let layout = plan_witness_table_layout(&table.type_name, &table.trait_name, &method_names);
    let witness_bytes = materialize_witness_bytes(&layout);
    let text = table.result_literal.as_bytes();
    let text_offset = u32::try_from(witness_bytes.len()).unwrap();

    let mut data_bytes = witness_bytes;
    data_bytes.extend_from_slice(text);

    let mut module = WasmBinaryModule::new();
    emit_witness_module_without_host_io(&mut module, text_offset, &data_bytes, preview);
    (module, declared_imports)
}

fn emit_witness_module_without_host_io(module: &mut WasmBinaryModule, text_offset: u32, data_bytes: &[u8], preview: WasiPreview) {
    module.sections.push(type_section_bytes(vec![
        wasm_function_type(&[], &[VALTYPE_I32]),
        wasm_function_type(&[], &[]),
        wasm_function_type(&[VALTYPE_I32, VALTYPE_I32, VALTYPE_I32, VALTYPE_I32], &[VALTYPE_I32]),
    ]));
    // funcs: text, start, run, post_run, realloc, initialize, cli_run_result
    module.sections.push(function_section_bytes(&[0, 1, 1, 1, 2, 1, 0]));
    let heap_base = cabi_heap_base_after_data(data_bytes.len());
    module.sections.push(memory_section_bytes(memory_min_pages_for_heap_base(heap_base)));
    module.sections.push(cabi_heap_global_section(heap_base));
    let exports = component_command_exports(preview, 1, 2, 3, 4, 5, 6);
    module.sections.push(export_section_bytes(&exports.iter().map(|(n, k, i)| (n.as_str(), *k, *i)).collect::<Vec<_>>()));
    module.sections.push(code_section_bytes(&[
        wasm_function_body(wasm_impl_return_offset_body(text_offset)),
        wasm_function_body(vec![0, WasmOpcode::End.as_u8()]),
        wasm_function_body(vec![0, WasmOpcode::End.as_u8()]),
        wasm_function_body(wasm_noop_body()),
        wasm_function_body(wasm_cabi_realloc_bump_body()),
        wasm_function_body(wasm_noop_body()),
        wasm_function_body(wasm_cli_run_result_body()),
    ]));
    module.sections.push(data_section_bytes(0, data_bytes));
}

fn emit_minimal_start_module(module: &mut WasmBinaryModule, preview: WasiPreview, imports: &[(String, String)]) {
    let mut types = vec![
        wasm_function_type(&[], &[]),
        wasm_function_type(&[VALTYPE_I32, VALTYPE_I32, VALTYPE_I32, VALTYPE_I32], &[VALTYPE_I32]),
        wasm_function_type(&[], &[VALTYPE_I32]),
    ];
    let mut import_specs = Vec::new();
    for (module_name, field) in imports {
        let (canonical_module, canonical_field, params, results) = canonical_import(module_name, field, preview)
            .unwrap_or_else(|| panic!("WASI canonical ABI metadata missing for import {module_name}::{field}"));
        let type_index = types.len() as u32;
        types.push(wasm_function_type(params, results));
        import_specs.push((canonical_module, canonical_field, type_index));
    }
    module.sections.push(type_section_bytes(types));
    if !import_specs.is_empty() {
        module.sections.push(super::super::sections::import_section_bytes(&import_specs));
    }
    // funcs: start, run, post_run, realloc, initialize, cli_run_result
    module.sections.push(function_section_bytes(&[0, 0, 0, 1, 0, 2]));
    module.sections.push(memory_section_bytes(memory_min_pages_for_heap_base(CABI_HEAP_DEFAULT_BASE)));
    module.sections.push(cabi_heap_global_section(CABI_HEAP_DEFAULT_BASE));
    let offset = imports.len() as u32;
    let exports = component_command_exports(preview, offset, offset + 1, offset + 2, offset + 3, offset + 4, offset + 5);
    module.sections.push(export_section_bytes(&exports.iter().map(|(n, k, i)| (n.as_str(), *k, *i)).collect::<Vec<_>>()));
    module.sections.push(code_section_bytes(&[
        wasm_function_body(wasm_noop_body()),
        wasm_function_body(wasm_noop_body()),
        wasm_function_body(wasm_noop_body()),
        wasm_function_body(wasm_cabi_realloc_bump_body()),
        wasm_function_body(wasm_noop_body()),
        wasm_function_body(wasm_cli_run_result_body()),
    ]));
}

/// The component model owns the core import spelling and flat ABI.  These
/// descriptors are intentionally explicit; source import names are never
/// passed through as core wasm names and unknown signatures are rejected.
fn canonical_import(module: &str, field: &str, preview: WasiPreview) -> Option<(&'static str, &'static str, &'static [u8], &'static [u8])> {
    match (preview, module, field) {
        (WasiPreview::Preview2, "wasi:clocks/monotonic-clock", "now") => {
            Some(("cm32p2|wasi:clocks/monotonic-clock@0.2", "now", &[], &[VALTYPE_I64]))
        }
        (WasiPreview::Preview2, "wasi:clocks/monotonic-clock", "resolution") => {
            Some(("cm32p2|wasi:clocks/monotonic-clock@0.2", "resolution", &[], &[VALTYPE_I64]))
        }
        (WasiPreview::Preview2, "wasi:clocks/monotonic-clock", "get-resolution") => {
            Some(("cm32p2|wasi:clocks/monotonic-clock@0.2", "get-resolution", &[], &[VALTYPE_I64]))
        }
        (WasiPreview::Preview2, "wasi:io/streams", "blocking-write-and-flush") => {
            Some(("cm32p2|wasi:io/streams@0.2", "blocking-write-and-flush", &[VALTYPE_I32], &[VALTYPE_I32]))
        }
        _ => None,
    }
}

fn component_command_exports(
    preview: WasiPreview,
    start_index: u32,
    run_index: u32,
    post_run_index: u32,
    realloc_index: u32,
    initialize_index: u32,
    cli_run_index: u32,
) -> Vec<(String, u8, u32)> {
    vec![
        ("_start".to_string(), WasmExternalKind::Func.as_u8(), start_index),
        ("run".to_string(), WasmExternalKind::Func.as_u8(), run_index),
        ("cabi_post_run".to_string(), WasmExternalKind::Func.as_u8(), post_run_index),
        ("memory".to_string(), WasmExternalKind::Memory.as_u8(), 0),
        ("cabi_realloc".to_string(), WasmExternalKind::Func.as_u8(), realloc_index),
        ("_initialize".to_string(), WasmExternalKind::Func.as_u8(), initialize_index),
        (crate::nyar_backend_wasi::wasi_cli_run_export_name_for(preview), WasmExternalKind::Func.as_u8(), cli_run_index),
    ]
}

fn wasm_noop_body() -> Vec<u8> {
    vec![0, WasmOpcode::End.as_u8()]
}

fn wasm_cli_run_result_body() -> Vec<u8> {
    let mut body = vec![0];
    encode_i32_const(0, &mut body);
    WasmOpcode::End.encode(&mut body);
    body
}
