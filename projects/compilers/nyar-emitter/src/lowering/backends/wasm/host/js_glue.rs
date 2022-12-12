//! WasmJsGlue host shell: emit_byte string interop and empty/suspend entrypoints.
use crate::{FragmentSubmission, nyar_backend_wasi::WasmBinaryModule};
use std_data::binary::wasm::{VALTYPE_I32, WasmExternalKind, WasmOpcode, encode_call, encode_i32_const};

use super::super::{
    super::suspend_sm::first_suspend_function,
    host_imports::{collect_string_output_for_wasm_host, first_wasm_host_import_target},
    sections::{
        code_section_bytes, encode_sleb128_i32, encode_uleb128, export_section_bytes, function_section_bytes, import_section_bytes,
        type_section_bytes, wasm_function_body, wasm_function_type,
    },
    suspend::lower_suspend_module,
};

pub(super) fn lower_fragment_to_js_glue_module(submission: &FragmentSubmission) -> (WasmBinaryModule, Vec<(String, String)>) {
    let mut module = WasmBinaryModule::new();
    let mut output = collect_string_output_for_wasm_host(submission);
    if !output.is_empty() {
        output.push(b'\n');
    }

    if output.is_empty() {
        if let Some(artifact) = first_suspend_function(submission) {
            return (lower_suspend_module(submission, artifact, "main", true), Vec::new());
        }
        module.sections.push(type_section_bytes(vec![wasm_function_type(&[], &[VALTYPE_I32])]));
        module.sections.push(function_section_bytes(&[0]));
        module.sections.push(export_section_bytes(&[("main", WasmExternalKind::Func.as_u8(), 0)]));
        module.sections.push(code_section_bytes(&[wasm_function_body(js_glue_main_body(&[]))]));
        return (module, Vec::new());
    }

    let import_target = first_wasm_host_import_target(submission).unwrap_or(("env".to_string(), "emit_byte".to_string()));
    module.sections.push(type_section_bytes(vec![wasm_function_type(&[VALTYPE_I32], &[]), wasm_function_type(&[], &[VALTYPE_I32])]));
    module.sections.push(import_section_bytes(&[(&import_target.0, &import_target.1, 0)]));
    module.sections.push(function_section_bytes(&[1]));
    module.sections.push(export_section_bytes(&[("main", WasmExternalKind::Func.as_u8(), 1)]));
    module.sections.push(code_section_bytes(&[wasm_function_body(js_glue_main_body(&output))]));
    (module, vec![import_target])
}

fn js_glue_main_body(output: &[u8]) -> Vec<u8> {
    let mut body = vec![0];
    for byte in output {
        encode_i32_const(i32::from(*byte), &mut body);
        encode_call(0, &mut body);
    }
    encode_i32_const(0, &mut body);
    WasmOpcode::End.encode(&mut body);
    body
}
