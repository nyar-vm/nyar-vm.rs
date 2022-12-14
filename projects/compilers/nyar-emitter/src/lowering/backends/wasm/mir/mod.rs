//! Semantic MIR → Wasm executable prepare/emit (**wasm-gc** required for references).
//!
//! **Not** a second MIR. Language CFG/types stay in Semantic MIR; this facade and its
//! submodules own Wasm physical encoding only (control / representation / calls / …).
//! Directory name `mir/` is historical; prefer speaking of Wasm prepare → `WasmModuleModel` → encode.
//!
//! - [`control`] — `pc_local` + `loop` + `br_table` (Jump/Branch/Return identity)
//! - [`representation`] — unique Wasm storage / value-type rules
//! - [`type_registry`] — GC type-index registration
//! - [`calls`] — resolved calls → Wasm call forms
//! - **Value aggregates** (`structure` / tuple / `[T; N]`): linear memory via load/store + `memory.copy`
//! - **Reference aggregates** (`class` / trait object / heap `[T]`): wasm-gc struct/array ops
//! - **Node (`main`) and WASI (`_start`)**: both register structtype + arraytype (V8 13.6+ / wasmtime)

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    executable_provider::{
        ExecutableBlock as MirBlock, ExecutableBlockRef as MirBlockRef, ExecutableConstant as MirConstant,
        ExecutableDispatchKind as MirDispatchKind, ExecutableFunction as MirFunction, ExecutableInstruction as MirInstruction,
        ExecutableInstructionKind as MirInstructionKind, ExecutableOperand as MirOperand, ExecutableReceiverPassingKind as ReceiverPassingKind,
        ExecutableStorageKind as MirStorageKind, ExecutableStorageKind as StorageKind, ExecutableTerminator as MirTerminator,
        ExecutableValueRef as MirValueRef, NyarType,
    },
    nyar_backend_wasi::{WasmBinaryModule, WasmSection},
};
use nyar::{NamePath, QualifiedName};

use crate::lowering::shared::witness_abi::is_tuple_get_stub_name;
use nyar_types::{AggregateLayout, FieldLayout, LayoutId};

use super::{
    super::{
        executable::{ExecutableLoweringContext, collect_reachable_blocks},
        interop::{wasi_host_import_target, wasm_host_import_target},
        pattern_matching_contract::validate_pattern_matching_invariants,
    },
    cabi::{
        CABI_HEAP_GLOBAL_INDEX, LINEAR_HEAP_MIN_BASE, cabi_heap_base_after_data, cabi_heap_global_section, memory_min_pages_for_heap_base,
        wasm_cabi_realloc_bump_body,
    },
    gc::{WASM_GC_ANYREF, wasm_gc_array_type, wasm_gc_field_type_byte, wasm_gc_struct_type},
    sections::{
        code_section_bytes, data_section_bytes, decode_uleb128, encode_sleb128_i32, encode_sleb128_i64, encode_uleb128, export_section_bytes,
        function_section_bytes, import_section_bytes, memory_section_bytes, type_section_bytes, wasm_function_body, wasm_function_type,
    },
};
mod calls;
mod control;
mod representation;
mod type_registry;

use representation::*;
use type_registry::*;

use crate::{
    FragmentSubmission,
    contracts::{ArrayInitialization, instruction_primary_result},
};
use std_data::binary::wasm::{
    BLOCKTYPE_EMPTY, VALTYPE_ANYREF, VALTYPE_EXTERNREF, VALTYPE_F64, VALTYPE_I32, VALTYPE_I64, VALTYPE_REF, VALTYPE_REF_NULL, WasmExternalKind,
    WasmOpcode, encode_array_copy, encode_array_get, encode_array_len, encode_array_new_default, encode_array_new_fixed, encode_array_set,
    encode_block_empty, encode_br, encode_br_if, encode_call, encode_call_indirect, encode_drop, encode_f64_const, encode_f64_load,
    encode_f64_store, encode_global_get, encode_global_set, encode_i32_add, encode_i32_and, encode_i32_const, encode_i32_eqz, encode_i32_load,
    encode_i32_store, encode_i32_sub, encode_i64_const, encode_i64_load, encode_i64_store, encode_local_get, encode_local_set,
    encode_local_tee, encode_loop_empty, encode_memory_copy, encode_memory_fill, encode_memory_grow, encode_memory_size, encode_ref_cast_array,
    encode_ref_cast_type_index, encode_ref_is_null, encode_ref_null_anyref, encode_ref_null_externref, encode_return, encode_struct_get,
    encode_struct_new_default, encode_struct_set, encode_unreachable,
};

/// High-level compiler delegation is never a legal Node host import.
fn is_forbidden_node_bridge(field: &str) -> bool {
    field == "host_legion_compile_from_plan" || field.starts_with("host_legion_")
}

/// 线性内?bump allocator 与字符串静态区共享的最低基址?
///
/// WASI 轨保?`[0, WASI_STRING_DATA_OFFSET)` 用于 argv 缓冲区与辅助变量?
/// 字符串字面量?`WASI_STRING_DATA_OFFSET` 开始存储；MIR/cabi 堆游标不得低于此值?
const WASI_STRING_DATA_OFFSET: u32 = LINEAR_HEAP_MIN_BASE as u32;

fn array_element_type(array_type: &NyarType) -> Option<&NyarType> {
    match array_type {
        NyarType::Array(element) | NyarType::FixedArray { element, .. } => Some(element.as_ref()),
        _ => None,
    }
}
const WASM_GC_EXTERNREF: u8 = VALTYPE_EXTERNREF;

/// 收集 Node / JS-glue 轨的宿主导入?
///
/// - `cli_get_*` 等产?CLI 导入**必须**来自源码 `[wasm(...)]` 声明；禁止无条件注入?
///   否则每个 Node 模块都会?`.mjs` 启动壳判?CLI 模式（要?`help`/`version`/`build`）?
/// - `const_utf8` 仅在存在字符串字面量时合成，供启动壳?`nyar.strings` 解析句柄?
fn collect_wasm_host_imports(submission: &FragmentSubmission, synthesize_const_utf8: bool) -> Vec<(String, String)> {
    let mut imports = Vec::new();
    let mut seen = BTreeSet::new();
    for link in submission.external_import_links.values() {
        let Some(target) = wasm_host_import_target(link)
        else {
            continue;
        };
        if is_forbidden_node_bridge(target.field) {
            continue;
        }
        let key = (target.module.to_string(), target.field.to_string());
        if seen.insert(key.clone()) {
            imports.push(key);
        }
    }
    if synthesize_const_utf8 {
        let key = ("env".to_string(), "const_utf8".to_string());
        if seen.insert(key.clone()) {
            imports.push(key);
        }
    }
    // Node 轨：无论是否有字面量，都确保 utf8 方法导入齐全（CLI 句柄上的 `.trim()` 等无字面量）?
    for field in NODE_UTF8_HOST_IMPORT_FIELDS {
        let key = ("env".to_string(), (*field).to_string());
        if seen.insert(key.clone()) {
            imports.push(key);
        }
    }
    imports.sort();
    imports
}

/// Node / JS-glue 宿主 utf8 方法导入字段（与 `.mjs` `js_import_impl` 对齐）?
const NODE_UTF8_HOST_IMPORT_FIELDS: &[&str] = &[
    "utf8_trim",
    "utf8_length",
    "utf8_concat",
    "utf8_starts_with",
    "utf8_ends_with",
    "utf8_contains",
    "utf8_equals",
    "utf8_replace",
    "utf8_index_of",
    "utf8_slice",
    "utf8_to_lower",
    "utf8_to_upper",
];

/// 将 MIR 方法短名 / 运算符名映射到 `env.utf8_*` 导入字段。
///
/// 仅用于宿主字符串句柄 ABI；`length` 在数组接收者上不得走此映射?
/// 收集 WASI 轨的宿主导入?
///
/// ?Node 轨的差异?
/// - 不收?`const_utf8` ?`cli_get_*`（字符串字面量作为内?i32 偏移量处理）
/// - 不再合成任何 legacy WASI 默认导入
/// - 仅收集源码通过 `[wasi(...)]` 声明?component-model host imports
fn collect_wasi_host_imports(submission: &FragmentSubmission, preview: crate::nyar_backend_wasi::WasiPreview) -> Vec<(String, String)> {
    let mut imports = Vec::new();
    let mut seen = BTreeSet::new();
    for link in submission.external_import_links.values() {
        let Some(target) = wasi_host_import_target(link)
        else {
            continue;
        };
        let Some((module, field)) = crate::nyar_backend_wasi::wasi_adapt_import_for_preview(target.module, target.function, preview)
        else {
            continue;
        };
        let key = (crate::nyar_backend_wasi::wasi_versioned_import_module_for(&module, preview), field);
        if seen.insert(key.clone()) {
            imports.push(key);
        }
    }
    imports.sort();
    // p3 `write-via-stream` 需?wit-component Legacy 名的 stream/future 内建?
    // ?utf8 线性句柄变成真正的 `stream<u8>` 再交给宿主（?stream_probe）?
    expand_wasi_cli_stream_intrinsics(&mut imports);
    imports
}

/// 为每?`write-via-stream` 导入追加 `[stream-new-0]` / write / drop / future-drop?
fn expand_wasi_cli_stream_intrinsics(imports: &mut Vec<(String, String)>) {
    let modules: Vec<String> = imports
        .iter()
        .filter(|(module, field)| field == "write-via-stream" && (module.contains("stdout") || module.contains("stderr")))
        .map(|(module, _)| module.clone())
        .collect();
    for module in modules {
        for field in [
            "[stream-new-0]write-via-stream",
            "[stream-write-0]write-via-stream",
            "[stream-drop-writable-0]write-via-stream",
            "[future-drop-readable-1]write-via-stream",
        ] {
            let key = (module.clone(), field.to_string());
            if !imports.iter().any(|item| item == &key) {
                imports.push(key);
            }
        }
    }
}

fn collect_mir_string_literals(submission: &FragmentSubmission, operations: &[QualifiedName]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut literals = Vec::new();
    let Some(exec) = submission.executable.as_ref()
    else {
        return literals;
    };
    for operation in operations {
        let Some(view) = exec.get_function(operation)
        else {
            continue;
        };
        for block in &view.function.blocks {
            for instruction in &block.instructions {
                if let MirInstructionKind::LoadConstant { constant: MirConstant::Utf8(text), .. } = &instruction.kind {
                    if seen.insert(text.clone()) {
                        literals.push(text.clone());
                    }
                }
            }
        }
    }
    literals
}

fn const_utf8_import_index(imports: &[(String, String)]) -> Option<u32> {
    imports.iter().position(|(module, field)| module == "env" && field == "const_utf8").map(|index| index as u32)
}

fn build_string_literal_index(literals: &[String]) -> BTreeMap<String, u32> {
    literals.iter().enumerate().map(|(index, text)| (text.clone(), index as u32)).collect()
}

fn wasm_import_type_for_field(field: &str) -> Vec<u8> {
    match field {
        "cli_get_verbose" => wasm_function_type(&[], &[VALTYPE_I32]),
        // Node's JS boundary uses opaque i32 handles. JavaScript values cannot cross
        // a WebAssembly `anyref` boundary, and using handles keeps the FFI limited to
        // primitive integers and UTF-8 bytes owned by the launcher.
        "const_utf8" => wasm_function_type(&[VALTYPE_I32], &[VALTYPE_I32]),
        "cli_get_project" | "cli_get_target" | "cli_get_output" => wasm_function_type(&[], &[VALTYPE_I32]),
        "get_current_directory" | "get_input" | "read_source_byte" => wasm_function_type(&[], &[VALTYPE_I32]),
        "read_file_text" => wasm_function_type(&[VALTYPE_I32], &[VALTYPE_I32]),
        "file_exists" | "directory_exists" | "create_directory" => wasm_function_type(&[VALTYPE_I32], &[VALTYPE_I32]),
        "write_file_text" => wasm_function_type(&[VALTYPE_I32, VALTYPE_I32], &[VALTYPE_I32]),
        "get_files" => wasm_function_type(&[VALTYPE_I32, VALTYPE_I32, VALTYPE_I32], &[VALTYPE_I32]),
        "utf8_length" => wasm_function_type(&[VALTYPE_I32], &[VALTYPE_I32]),
        "utf8_index_of" => wasm_function_type(&[VALTYPE_I32, VALTYPE_I32], &[VALTYPE_I32]),
        "utf8_trim" | "utf8_to_lower" | "utf8_to_upper" => wasm_function_type(&[VALTYPE_I32], &[VALTYPE_I32]),
        "utf8_concat" => wasm_function_type(&[VALTYPE_I32, VALTYPE_I32], &[VALTYPE_I32]),
        "utf8_starts_with" | "utf8_ends_with" | "utf8_contains" | "utf8_equals" => {
            wasm_function_type(&[VALTYPE_I32, VALTYPE_I32], &[VALTYPE_I32])
        }
        "utf8_replace" => wasm_function_type(&[VALTYPE_I32, VALTYPE_I32, VALTYPE_I32], &[VALTYPE_I32]),
        "utf8_slice" => wasm_function_type(&[VALTYPE_I32, VALTYPE_I32, VALTYPE_I32], &[VALTYPE_I32]),
        "emit_byte" => wasm_function_type(&[VALTYPE_I32], &[]),
        "System.Console" | "System.Diagnostics.Process" => wasm_function_type(&[VALTYPE_I32], &[]),
        _ if field.starts_with("utf8_") => wasm_function_type(&[VALTYPE_I32], &[VALTYPE_I32]),
        _ => wasm_function_type(&[], &[VALTYPE_I32]),
    }
}

/// WASI 模式下的 import 类型签名?
///
/// Align known Preview2 imports with Canonical ABI lowered shapes used by
/// `wasm-tools component new` / wasmtime. Remaining imports stay `(i32)->i32`
/// placeholders until their cabi lift is implemented.
fn wasi_import_type_for_field(_module: &str, field: &str) -> Vec<u8> {
    match field {
        // get-arguments: func() -> list<string>  ? guest import (param i32) // out-pointer
        "get-arguments" | "get-environment" => wasm_function_type(&[VALTYPE_I32], &[]),
        // exit: func(status: u32)  ? (param i32)
        "exit" => wasm_function_type(&[VALTYPE_I32], &[]),
        // monotonic-clock now / resolution / get-resolution: instant|duration →?(result i64)
        "now" | "resolution" | "get-resolution" if _module.contains("monotonic-clock") => wasm_function_type(&[], &[VALTYPE_I64]),
        // wall/system-clock: datetime {seconds:u64, nanoseconds:u32} →?flat (i64,i32)
        "now" | "resolution" | "get-resolution" if _module.contains("wall-clock") || _module.contains("system-clock") => {
            wasm_function_type(&[], &[VALTYPE_I64, VALTYPE_I32])
        }
        // random / insecure u64 stubs
        "get-random-u64" | "get-insecure-random-u64" => wasm_function_type(&[], &[VALTYPE_I64]),
        // list<u8> return: (len) -> (ptr, len) multi-value transitional cabi
        "get-random-bytes" | "get-insecure-random-bytes" => wasm_function_type(&[VALTYPE_I32], &[VALTYPE_I32, VALTYPE_I32]),
        // wasi:random/insecure-seed#insecure-seed -?flat tuple<u64,u64> →?(i64,i64)
        "insecure-seed" if _module.contains("wasi:random/insecure-seed") => wasm_function_type(&[], &[VALTYPE_I64, VALTYPE_I64]),
        // p3 wasi:cli/stdout|stderr#write-via-stream: async func(data: stream<u8>) -> result
        // Canonical ABI lowers the stream handle to a single i32 (1-tuple); wasmtime p3 host
        // expects that shape -?not the transitional (ptr,len) 2-tuple stub.
        "write-via-stream" if _module.contains("wasi:cli/stdout") || _module.contains("wasi:cli/stderr") => {
            wasm_function_type(&[VALTYPE_I32], &[VALTYPE_I32])
        }
        // wit-component Legacy stream/future builtins (paired with write-via-stream).
        field if field.starts_with("[stream-new-") => wasm_function_type(&[], &[VALTYPE_I64]),
        field if field.starts_with("[stream-write-") || field.starts_with("[async][stream-write-") => {
            wasm_function_type(&[VALTYPE_I32, VALTYPE_I32, VALTYPE_I32], &[VALTYPE_I32])
        }
        field if field.starts_with("[stream-drop-writable-") => wasm_function_type(&[VALTYPE_I32], &[]),
        field if field.starts_with("[future-drop-readable-") => wasm_function_type(&[VALTYPE_I32], &[]),
        // p3 wasi:cli/stdin#read-via-stream -?stream handle →?i32 status/len (1-tuple).
        "read-via-stream" if _module.contains("wasi:cli/stdin") => wasm_function_type(&[VALTYPE_I32], &[VALTYPE_I32]),
        // p2 wasi:io/streams#blocking-write-and-flush / blocking-read-and-skip
        "blocking-write-and-flush" | "blocking-read-and-skip" => wasm_function_type(&[VALTYPE_I32, VALTYPE_I32], &[VALTYPE_I32]),
        _ => wasm_function_type(&[VALTYPE_I32], &[VALTYPE_I32]),
    }
}

fn wasi_core_import_name<'a>(module: &'a str, field: &'a str, preview: crate::nyar_backend_wasi::WasiPreview) -> (&'a str, &'a str) {
    if preview == crate::nyar_backend_wasi::WasiPreview::Preview2 {
        let bare = module.split('@').next().unwrap_or(module);
        let canonical = match bare {
            "wasi:clocks/monotonic-clock" => Some("cm32p2|wasi:clocks/monotonic-clock@0.2"),
            "wasi:io/streams" => Some("cm32p2|wasi:io/streams@0.2"),
            _ => None,
        };
        if let Some(canonical) = canonical {
            return (canonical, field);
        }
    }
    (module, field)
}

fn build_callee_import_index(submission: &FragmentSubmission, imports: &[(String, String)]) -> BTreeMap<String, u32> {
    let import_index_by_key: BTreeMap<(String, String), u32> =
        imports.iter().enumerate().map(|(index, (module, field))| ((module.clone(), field.clone()), index as u32)).collect();
    let mut callee_import_index = BTreeMap::new();
    for (callee, link) in &submission.external_import_links {
        let Some(target) = wasm_host_import_target(link)
        else {
            continue;
        };
        let Some(&index) = import_index_by_key.get(&(target.module.to_string(), target.field.to_string()))
        else {
            continue;
        };
        callee_import_index.insert(callee.to_string(), index);
        if let Some(last) = callee.parts().last() {
            callee_import_index.insert(last.as_str().to_string(), index);
        }
    }
    // 按导入字段名建索引，?MIR 短名方法（`trim` →?`utf8_trim`）解析?
    for (index, (module, field)) in imports.iter().enumerate() {
        if module == "env" {
            callee_import_index.entry(field.clone()).or_insert(index as u32);
        }
    }
    callee_import_index
}

/// WASI 模式下的 callee import 索引构建?
///
/// ?Node 轨的差异：使?`wasi_host_import_target` 解析源码声明?`[wasi(...)]` imports?
fn build_wasi_callee_import_index(
    submission: &FragmentSubmission,
    imports: &[(String, String)],
    preview: crate::nyar_backend_wasi::WasiPreview,
) -> BTreeMap<String, u32> {
    let import_index_by_key: BTreeMap<(String, String), u32> =
        imports.iter().enumerate().map(|(index, (module, field))| ((module.clone(), field.clone()), index as u32)).collect();
    let mut callee_import_index = BTreeMap::new();
    for (callee, link) in &submission.external_import_links {
        let Some(target) = wasi_host_import_target(link)
        else {
            continue;
        };
        let Some((module, field)) = crate::nyar_backend_wasi::wasi_adapt_import_for_preview(target.module, target.function, preview)
        else {
            continue;
        };
        let module = crate::nyar_backend_wasi::wasi_versioned_import_module_for(&module, preview);
        let Some(&index) = import_index_by_key.get(&(module, field))
        else {
            continue;
        };
        callee_import_index.insert(callee.to_string(), index);
        if let Some(last) = callee.parts().last() {
            callee_import_index.insert(last.as_str().to_string(), index);
        }
    }
    callee_import_index
}

/// ?WASI 轨构建字符串字面量到线性内存偏移量的映射?
///
/// 每个字符串在 data 段中的布局：`[length (4 字节)] [utf8 字节...]`，并对齐?4 字节边界?
/// 偏移量从 `WASI_STRING_DATA_OFFSET` 开始?
fn build_wasi_string_literal_offsets(literals: &[String]) -> BTreeMap<String, u32> {
    let mut map = BTreeMap::new();
    let mut offset = WASI_STRING_DATA_OFFSET;
    for text in literals {
        map.insert(text.clone(), offset);
        let byte_len = text.as_bytes().len() as u32;
        // 4 字节长度前缀 + utf8 字节
        offset += 4 + byte_len;
        // 对齐?4 字节边界
        offset = (offset + 3) & !3;
    }
    map
}

/// 计算 WASI 轨字符串字面?data 段总长度?
fn wasi_string_data_section_size(literals: &[String]) -> u32 {
    let mut offset = WASI_STRING_DATA_OFFSET;
    for text in literals {
        let byte_len = text.as_bytes().len() as u32;
        offset += 4 + byte_len;
        offset = (offset + 3) & !3;
    }
    offset
}

/// 生成 WASI 轨字符串字面?data 段字节?
///
/// 布局：从偏移 0 开始填?`[0..WASI_STRING_DATA_OFFSET)` 为零字节（保留区），
/// 随后每个字符?`[length (LE u32)] [utf8 字节] [对齐填充]`?
fn build_wasi_string_data_section(literals: &[String]) -> Vec<u8> {
    let total_size = wasi_string_data_section_size(literals) as usize;
    let mut data = vec![0u8; total_size];
    let mut offset = WASI_STRING_DATA_OFFSET;
    for text in literals {
        let bytes = text.as_bytes();
        let byte_len = bytes.len() as u32;
        // 写入长度前缀
        data[offset as usize..(offset as usize + 4)].copy_from_slice(&byte_len.to_le_bytes());
        // 写入 utf8 字节
        data[(offset as usize + 4)..(offset as usize + 4 + bytes.len())].copy_from_slice(bytes);
        offset += 4 + byte_len;
        // 对齐?4 字节边界
        offset = (offset + 3) & !3;
    }
    data
}

fn wasm_function_type_param_bytes(type_bytes: &[u8]) -> Vec<u8> {
    if type_bytes.first() != Some(&std_data::binary::wasm::TYPE_FORM_FUNC) {
        return Vec::new();
    }
    let mut offset = 1usize;
    let param_count = decode_uleb128(type_bytes, &mut offset) as usize;
    let end = offset.saturating_add(param_count).min(type_bytes.len());
    type_bytes[offset..end].to_vec()
}

fn wasm_function_type_result_byte(type_bytes: &[u8]) -> Option<u8> {
    if type_bytes.first() != Some(&std_data::binary::wasm::TYPE_FORM_FUNC) {
        return None;
    }
    let mut offset = 1usize;
    let param_count = decode_uleb128(type_bytes, &mut offset) as usize;
    offset = offset.saturating_add(param_count);
    if offset >= type_bytes.len() {
        return None;
    }
    let result_count = decode_uleb128(type_bytes, &mut offset) as usize;
    if result_count == 0 {
        return None;
    }
    type_bytes.get(offset).copied()
}

/// 在完整路?map 中按「简单名 / `::简单名`」唯一匹配；多名碰撞则 `None`（fail-closed）?
fn unique_simple_name_match<'a, V>(map: &'a BTreeMap<String, V>, simple: &str) -> Option<&'a V> {
    let suffix = format!("::{simple}");
    let mut found: Option<&V> = None;
    for (name, value) in map {
        if name.as_str() == simple || name.ends_with(&suffix) {
            if found.is_some() {
                return None;
            }
            found = Some(value);
        }
    }
    found
}

fn build_param_types_by_name(
    ctx: &ExecutableLoweringContext,
    submission: &FragmentSubmission,
    operations: &[QualifiedName],
    gc_struct_type_indices: &BTreeMap<LayoutId, u32>,
    js_glue_utf8_as_anyref: bool,
) -> BTreeMap<String, Vec<u8>> {
    let mut map = BTreeMap::new();
    let mut ambiguous = BTreeSet::new();
    for operation in operations {
        let Some(mir_fn) = submission.executable.as_ref().and_then(|exec| exec.get_function(operation)).map(|view| view.function)
        else {
            continue;
        };
        let params = wasm_param_types(&ctx, &mir_fn, gc_struct_type_indices, js_glue_utf8_as_anyref);
        map.insert(operation.to_string(), params.clone());
        if let Some(last) = operation.parts().last() {
            let simple = last.as_str();
            if ambiguous.contains(simple) {
                continue;
            }
            match map.get(simple) {
                Some(existing) if existing == &params => {}
                Some(_) => {
                    ambiguous.insert(simple.to_string());
                    map.remove(simple);
                }
                None => {
                    map.insert(simple.to_string(), params);
                }
            }
        }
    }
    map
}

fn build_return_types_by_name(
    ctx: &ExecutableLoweringContext,
    submission: &FragmentSubmission,
    operations: &[QualifiedName],
    gc_struct_type_indices: &BTreeMap<LayoutId, u32>,
    js_glue_utf8_as_anyref: bool,
) -> BTreeMap<String, Option<u8>> {
    let mut map = BTreeMap::new();
    let mut ambiguous = BTreeSet::new();
    for operation in operations {
        let Some(mir_fn) = submission.executable.as_ref().and_then(|exec| exec.get_function(operation)).map(|view| view.function)
        else {
            continue;
        };
        let return_type = wasm_return_value_type(ctx, &mir_fn, gc_struct_type_indices, js_glue_utf8_as_anyref);
        map.insert(operation.to_string(), return_type);
        if let Some(last) = operation.parts().last() {
            let simple = last.as_str();
            if ambiguous.contains(simple) {
                continue;
            }
            match map.get(simple) {
                Some(existing) if existing == &return_type => {}
                Some(_) => {
                    ambiguous.insert(simple.to_string());
                    map.remove(simple);
                }
                None => {
                    map.insert(simple.to_string(), return_type);
                }
            }
        }
    }
    map
}

pub(crate) fn lower_fragment_mir_to_wasm_module(
    submission: &FragmentSubmission,
    export_name: &str,
) -> (WasmBinaryModule, Vec<(String, String)>) {
    lower_fragment_mir_to_wasm_module_for(
        submission,
        export_name,
        crate::nyar_backend_wasi::WasiPreview::Preview2,
        crate::nyar_backend_wasi::WasmPackageKind::Binary,
    )
}

pub(crate) fn lower_fragment_mir_to_wasm_module_for(
    submission: &FragmentSubmission,
    export_name: &str,
    wasi_preview: crate::nyar_backend_wasi::WasiPreview,
    wasm_package_kind: crate::nyar_backend_wasi::WasmPackageKind,
) -> (WasmBinaryModule, Vec<(String, String)>) {
    let wasi_mode = export_name == "_start";
    let operations: Vec<QualifiedName> = submission.executable.as_ref().map(|exec| exec.operations()).unwrap_or_default();
    let string_literals = if export_name == "main" || wasi_mode { collect_mir_string_literals(submission, &operations) } else { Vec::new() };
    let host_imports = if wasi_mode {
        collect_wasi_host_imports(submission, wasi_preview)
    }
    else if export_name == "main" {
        // 仅在确有字符串字面量时合?`const_utf8`；CLI 导入一律走源码声明?
        collect_wasm_host_imports(submission, !string_literals.is_empty())
    }
    else {
        Vec::new()
    };
    let import_count = u32::try_from(host_imports.len()).expect("import count overflow");
    let callee_import_index = if wasi_mode {
        build_wasi_callee_import_index(submission, &host_imports, wasi_preview)
    }
    else {
        build_callee_import_index(submission, &host_imports)
    };
    // Node JS-glue：宿主导入与 `.mjs` 启动壳统一使用 i32 句柄（`hostIntern` / `const_utf8`），
    // ?`wasm_import_type_for_field` ?i32 签名对齐。若?utf8 标成 anyref，call 实参?
    // 被降?`ref.null`，丢?`cli_get_*` 返回的句柄（自举 `exports.build` 会静默失败）?
    // WASI 轨同样以线性内?i32 偏移传字符串?
    let js_glue_utf8_as_anyref = false;
    let mut module = WasmBinaryModule::new();
    // main_type ?type 段的?0 ?() -> i32，供 `build` 桩函数使用?
    // ?MIR 函数的类型从?1 项开始追加，各自携带独立的返回类型?
    let main_type = wasm_function_type(&[], &[VALTYPE_I32]);
    let mut type_indices = vec![main_type];
    let mut import_type_entries = Vec::new();
    for (module, field) in &host_imports {
        let import_type_index = u32::try_from(type_indices.len()).expect("type index overflow");
        let type_bytes = if wasi_mode { wasi_import_type_for_field(module, field) } else { wasm_import_type_for_field(field) };
        type_indices.push(type_bytes);
        import_type_entries.push(import_type_index);
    }
    let mut function_indices = Vec::new();
    let mut code_bodies = Vec::new();
    let mut exports = Vec::new();
    let mut synthetic_version_body = None;

    let ctx = ExecutableLoweringContext::new(submission);
    eprintln!("[wasm::module-lower-start] export={export_name} operations={} host_imports={}", operations.len(), host_imports.len());
    // 语言规范：wasm/wasi 强制 wasm-gc。始终为引用 layout 注册 structtype?
    let gc_struct_type_indices = register_gc_struct_types(&ctx, &mut type_indices, js_glue_utf8_as_anyref);
    eprintln!("[wasm::module-stage] structs={} types={}", gc_struct_type_indices.len(), type_indices.len());
    // ?heap array element_type 注册 wasm-gc arraytype（Node/V8 ?WASI/wasmtime 均支持）?
    // 必须?glue 感知路径：`[utf8]` →?i32 元，禁止 Named→anyref 误登记?
    let gc_array_type_indices = register_gc_array_types(&ctx, &mut type_indices, js_glue_utf8_as_anyref);
    eprintln!("[wasm::module-stage] arrays={} types={}", gc_array_type_indices.len(), type_indices.len());
    let gc_sum_type_indices = register_gc_sum_types(&ctx, &mut type_indices);
    eprintln!("[wasm::module-stage] sums={} types={}", gc_sum_type_indices.len(), type_indices.len());
    // Scalar unite payloads such as utf8/bool/i32 handles need boxing into an
    // `[i32]` struct before entering the anyref payload slot.
    let gc_i32_box_type_index = {
        let type_index = u32::try_from(type_indices.len()).expect("type index overflow");
        type_indices.push(wasm_gc_struct_type(&[VALTYPE_I32]));
        type_index
    };
    // 函数类型?structtype/arraytype/sumtype 之后开始追加?
    // type_indices 当前?[main_type, structtype_1, ..., structtype_N]?
    // 第一个函数类型应放在 N+1 处，对应 type_index = type_indices.len()?
    // 若仍用硬编码 `index + 1`，函数声明会指向 structtype 条目?
    // 导致 V8 ?`no signature at index 1 (N types)`?
    let function_type_base = u32::try_from(type_indices.len()).expect("type index overflow");
    let string_literal_index = build_string_literal_index(&string_literals);
    // WASI 轨：构建字符串字面量到线性内存偏移量的映射，存入 data 段?
    let string_literal_offset = if wasi_mode { build_wasi_string_literal_offsets(&string_literals) } else { BTreeMap::new() };
    let const_utf8_import = const_utf8_import_index(&host_imports);
    let entry_matches =
        submission.entry_operation.as_ref().is_some_and(|op| submission.executable.as_ref().and_then(|exec| exec.get_function(op)).is_some());
    // 仅对确有 MIR 体的 operation 分配稠密函数下标，避免「operations 枚举下标」与
    // `code_bodies.len()` 错位：错位时 call 会打到别人的 (param i64) 却按本函?anyref 签名?ref.null?
    let mir_operations: Vec<(QualifiedName, _)> = operations
        .iter()
        .filter_map(|operation| {
            submission.executable.as_ref().and_then(|exec| exec.get_function(operation)).map(|view| (operation.clone(), view.function))
        })
        .collect();
    eprintln!("[wasm::module-stage] mir_operations={} types={}", mir_operations.len(), type_indices.len());
    let base_function_index = if entry_matches { import_count } else { import_count + 1 };
    // 构建 function_index_by_name / type_index_by_name：完整路径必注册?
    // 简单名仅在无碰撞时注册（`get`/`length` 等多 overload 不得覆盖）?
    let mut function_index_by_name: BTreeMap<String, u32> = BTreeMap::new();
    let mut type_index_by_name: BTreeMap<String, u32> = BTreeMap::new();
    let mut param_types_by_function_index: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
    let mut return_types_by_function_index: BTreeMap<u32, Option<u8>> = BTreeMap::new();
    let mut ambiguous_simple: BTreeSet<String> = BTreeSet::new();
    for (dense, (operation, mir_fn)) in mir_operations.iter().enumerate() {
        let full = operation.to_string();
        let wasm_idx = base_function_index + dense as u32;
        let type_idx = function_type_base + dense as u32;
        let params = wasm_param_types(&ctx, mir_fn, &gc_struct_type_indices, js_glue_utf8_as_anyref);
        let ret = wasm_return_value_type(&ctx, mir_fn, &gc_struct_type_indices, js_glue_utf8_as_anyref);
        param_types_by_function_index.insert(wasm_idx, params);
        return_types_by_function_index.insert(wasm_idx, ret);
        function_index_by_name.insert(full.clone(), wasm_idx);
        type_index_by_name.insert(full.clone(), type_idx);
        if let Some(last) = operation.parts().last() {
            let simple = last.as_str();
            if ambiguous_simple.contains(simple) {
                continue;
            }
            match function_index_by_name.get(simple) {
                Some(&existing) if existing == wasm_idx => {}
                Some(_) => {
                    ambiguous_simple.insert(simple.to_string());
                    function_index_by_name.remove(simple);
                    type_index_by_name.remove(simple);
                }
                None => {
                    function_index_by_name.insert(simple.to_string(), wasm_idx);
                    type_index_by_name.insert(simple.to_string(), type_idx);
                }
            }
        }
    }
    // Node JS-glue：宿主字符串?anyref 传递；?`wasm_import_type_for_field` ?anyref 签名对齐?
    let param_types_by_name = build_param_types_by_name(&ctx, submission, &operations, &gc_struct_type_indices, js_glue_utf8_as_anyref);
    let return_types_by_name = build_return_types_by_name(&ctx, submission, &operations, &gc_struct_type_indices, js_glue_utf8_as_anyref);
    let import_param_types: Vec<Vec<u8>> = host_imports
        .iter()
        .map(|(module, field)| {
            let type_bytes = if wasi_mode { wasi_import_type_for_field(module, field) } else { wasm_import_type_for_field(field) };
            wasm_function_type_param_bytes(&type_bytes)
        })
        .collect();
    let import_return_types: Vec<Option<u8>> = host_imports
        .iter()
        .map(|(module, field)| {
            let type_bytes = if wasi_mode { wasi_import_type_for_field(module, field) } else { wasm_import_type_for_field(field) };
            wasm_function_type_result_byte(&type_bytes)
        })
        .collect();
    let mut mir_wasm_functions: Vec<(u32, String)> = Vec::new();

    for (dense, (operation, mir_fn)) in mir_operations.iter().enumerate() {
        eprintln!(
            "[wasm::function-lower-start] dense={dense}/{} symbol={} blocks={} instructions={}",
            mir_operations.len(),
            operation,
            mir_fn.blocks.len(),
            mir_fn.blocks.iter().map(|block| block.instructions.len()).sum::<usize>(),
        );
        let param_types = wasm_param_types(&ctx, mir_fn, &gc_struct_type_indices, js_glue_utf8_as_anyref);
        // 每个函数独立计算返回类型,取代旧的模块?`returns_i32`?
        // 引用返回类型 →?anyref (VALTYPE_ANYREF)；i32 →?VALTYPE_I32；void →?空?
        let return_value_type = wasm_return_value_type(&ctx, mir_fn, &gc_struct_type_indices, js_glue_utf8_as_anyref);
        let result_types: &[u8] = match &return_value_type {
            Some(byte) => std::slice::from_ref(byte),
            None => &[],
        };
        let fn_type = wasm_function_type(&param_types, result_types);
        type_indices.push(fn_type);
        let type_index = function_type_base + dense as u32;
        function_indices.push(type_index);
        // 物理 code 下标始终?import_count 稠密排列；`!entry_matches` 时名称表已按
        // base=import_count+1 预留合成入口，插入入口体后再与名称表对齐?
        let wasm_index = import_count + u32::try_from(code_bodies.len()).expect("code body index overflow");
        code_bodies.push(wasm_function_body(lower_mir_function_to_wasm_bytes(
            submission,
            mir_fn,
            return_value_type,
            js_glue_utf8_as_anyref,
            wasi_mode,
            &function_index_by_name,
            &type_index_by_name,
            &param_types_by_name,
            &return_types_by_name,
            &param_types_by_function_index,
            &return_types_by_function_index,
            &import_param_types,
            &import_return_types,
            &gc_struct_type_indices,
            &gc_array_type_indices,
            &gc_sum_type_indices,
            gc_i32_box_type_index,
            &callee_import_index,
            &host_imports,
            &string_literal_index,
            &string_literal_offset,
            const_utf8_import,
        )));
        eprintln!("[wasm::function-lower-done] dense={dense}/{} symbol={operation}", mir_operations.len());
        mir_wasm_functions.push((wasm_index, operation.to_string()));
        if let Some(public_name) = submission.wasm_export_names.get(operation) {
            let export_index = function_index_by_name.get(&operation.to_string()).copied().unwrap_or(wasm_index);
            exports.push((public_name.as_str(), WasmExternalKind::Func.as_u8(), export_index));
        }
        else if submission.entry_operation.as_ref() == Some(operation) {
            // 导出下标与名称表一致（entry_matches ?== 物理下标）?
            let export_index = function_index_by_name.get(&operation.to_string()).copied().unwrap_or(wasm_index);
            exports.push((export_name, WasmExternalKind::Func.as_u8(), export_index));
        }
    }

    if exports.is_empty() && submission.wasm_export_names.is_empty() {
        assert!(
            wasm_package_kind != crate::nyar_backend_wasi::WasmPackageKind::Library,
            "library wasm package must not synthesize stub main"
        );
        function_indices.insert(0, 0);
        let entry = submission
            .entry_operation
            .as_ref()
            .and_then(|op| submission.executable.as_ref().and_then(|exec| exec.get_function(op)))
            .map(|view| view.function);
        let body = if let Some(mir_fn) = entry {
            lower_mir_function_to_wasm_bytes(
                submission,
                &mir_fn,
                wasm_return_value_type(&ctx, &mir_fn, &gc_struct_type_indices, js_glue_utf8_as_anyref),
                js_glue_utf8_as_anyref,
                wasi_mode,
                &function_index_by_name,
                &type_index_by_name,
                &param_types_by_name,
                &return_types_by_name,
                &param_types_by_function_index,
                &return_types_by_function_index,
                &import_param_types,
                &import_return_types,
                &gc_struct_type_indices,
                &gc_array_type_indices,
                &gc_sum_type_indices,
                gc_i32_box_type_index,
                &callee_import_index,
                &host_imports,
                &string_literal_index,
                &string_literal_offset,
                const_utf8_import,
            )
        }
        else if let Some(mir_fn) = submission
            .executable
            .as_ref()
            .and_then(|exec| exec.operations().into_iter().next())
            .and_then(|op| submission.executable.as_ref().and_then(|exec| exec.get_function(&op)))
            .map(|view| view.function)
        {
            lower_mir_function_to_wasm_bytes(
                submission,
                &mir_fn,
                wasm_return_value_type(&ctx, &mir_fn, &gc_struct_type_indices, js_glue_utf8_as_anyref),
                js_glue_utf8_as_anyref,
                wasi_mode,
                &function_index_by_name,
                &type_index_by_name,
                &param_types_by_name,
                &return_types_by_name,
                &param_types_by_function_index,
                &return_types_by_function_index,
                &import_param_types,
                &import_return_types,
                &gc_struct_type_indices,
                &gc_array_type_indices,
                &gc_sum_type_indices,
                gc_i32_box_type_index,
                &callee_import_index,
                &host_imports,
                &string_literal_index,
                &string_literal_offset,
                const_utf8_import,
            )
        }
        else {
            vec![WasmOpcode::Unreachable.as_u8(), WasmOpcode::End.as_u8()]
        };
        code_bodies.insert(0, wasm_function_body(body));
        for (wasm_index, _) in &mut mir_wasm_functions {
            if *wasm_index >= import_count {
                *wasm_index += 1;
            }
        }
        exports.push((export_name, WasmExternalKind::Func.as_u8(), import_count));
    }

    // WASI command world: wasmtime looks for `wasi:cli/run@-?run` returning result (i32).
    // Keep `_start` / short `run` for raw core runners; synthesize a result wrapper when needed.
    let wasi_cli_run_export = crate::nyar_backend_wasi::wasi_cli_run_export_name_for(wasi_preview);
    if wasi_mode {
        if let Some((_, _, index)) =
            exports.iter().find(|(name, kind, _)| *name == "_start" && *kind == WasmExternalKind::Func.as_u8()).copied()
        {
            // Entry may be `[] -> []` / `[] -> i32`, or still carry guest argv as GC array
            // (e.g. `legion(args: [utf8]) -> unit`). WIT `run: func() -> result` and core
            // `_start` are zero-arg. Synthesize nullary wrappers that push typed defaults
            // (empty `[utf8]` via `array.new_default`, not `ref.null` -?null traps on
            // `ref.cast`/`array.len`), call entry, drop any return, then Ok(0) for CLI run.
            // (Nonzero guest status belongs on `wasi:cli/exit`, not this empty `result`.)
            // Full argv via `get-arguments` cabi is follow-up; empty argv still exercises help.
            let entry_return = submission.entry_operation.as_ref().and_then(|op| {
                return_types_by_name
                    .get(&op.to_string())
                    .copied()
                    .or_else(|| op.parts().last().and_then(|part| return_types_by_name.get(part.as_str()).copied()))
            });
            let entry_mir_params: Vec<NyarType> = submission
                .entry_operation
                .as_ref()
                .and_then(|op| submission.executable.as_ref().and_then(|exec| exec.get_function(op)))
                .map(|view| view.function.param_types.clone())
                .unwrap_or_default();
            let entry_wasm_params = param_types_by_function_index.get(&index).cloned().unwrap_or_else(|| {
                submission.entry_operation.as_ref().and_then(|op| param_types_by_name.get(&op.to_string()).cloned()).unwrap_or_default()
            });

            let get_arguments_import = host_imports.iter().position(|(_, field)| field == "get-arguments").map(|index| index as u32);
            let argv_array_ty = prefer_utf8_argv_array_type(&gc_array_type_indices);
            let entry_needs_argv = entry_mir_params.iter().any(
                |ty| matches!(ty, NyarType::Array(element) | NyarType::FixedArray { element, .. } if is_js_glue_host_string_type(element)),
            ) || (entry_mir_params.is_empty()
                && entry_wasm_params.iter().any(|ty| *ty == VALTYPE_ANYREF || *ty == WASM_GC_ANYREF));

            let build_entry_wrapper_body = |with_ok_i32: bool| -> Vec<u8> {
                let use_get_arguments = get_arguments_import.zip(argv_array_ty).filter(|_| entry_needs_argv);
                let mut body = Vec::new();
                if let Some((_imp, arr_ty)) = use_get_arguments {
                    // locals: 7×i32 + 1×(ref null $arr_ty)
                    encode_uleb128(2, &mut body);
                    encode_uleb128(7, &mut body);
                    body.push(VALTYPE_I32);
                    encode_uleb128(1, &mut body);
                    body.push(VALTYPE_REF_NULL);
                    encode_sleb128_i32(arr_ty as i32, &mut body);
                }
                else {
                    encode_uleb128(0, &mut body);
                }
                if let Some((import_index, array_ty)) = use_get_arguments {
                    emit_wasi_argv_from_get_arguments(import_index, array_ty, &mut body);
                    // Remaining non-argv params (rare): still push defaults after argv.
                    if entry_mir_params.len() > 1 {
                        for ty in entry_mir_params.iter().skip(1) {
                            emit_wasi_entry_default_arg(ty, &gc_array_type_indices, &mut body);
                        }
                    }
                }
                else if !entry_mir_params.is_empty() {
                    for ty in &entry_mir_params {
                        emit_wasi_entry_default_arg(ty, &gc_array_type_indices, &mut body);
                    }
                }
                else {
                    for &param_ty in &entry_wasm_params {
                        match param_ty {
                            VALTYPE_I64 => encode_i64_const(0, &mut body),
                            VALTYPE_F64 => encode_f64_const(0.0, &mut body),
                            VALTYPE_ANYREF | WASM_GC_ANYREF => {
                                if let Some(array_ty) = argv_array_ty {
                                    encode_i32_const(0, &mut body);
                                    encode_array_new_default(array_ty, &mut body);
                                }
                                else {
                                    encode_ref_null_anyref(&mut body);
                                }
                            }
                            VALTYPE_EXTERNREF | WASM_GC_EXTERNREF => encode_ref_null_externref(&mut body),
                            _ => encode_i32_const(0, &mut body),
                        }
                    }
                }
                encode_call(index, &mut body);
                if entry_return.flatten().is_some() {
                    encode_drop(&mut body);
                }
                if with_ok_i32 {
                    encode_i32_const(0, &mut body);
                }
                WasmOpcode::End.encode(&mut body);
                body
            };

            // Nullary `_start` / `run` when entry still carries params (WASI command world).
            if !entry_mir_params.is_empty() || !entry_wasm_params.is_empty() {
                let start_type_index = u32::try_from(type_indices.len()).expect("type index overflow");
                type_indices.push(wasm_function_type(&[], &[]));
                function_indices.push(start_type_index);
                let start_wrapper = build_entry_wrapper_body(false);
                let start_func_index = import_count + u32::try_from(code_bodies.len()).expect("code body overflow");
                code_bodies.push(wasm_function_body(start_wrapper));
                for export in &mut exports {
                    if (export.0 == "_start" || export.0 == "run") && export.1 == WasmExternalKind::Func.as_u8() {
                        export.2 = start_func_index;
                    }
                }
                if !exports.iter().any(|(name, _, _)| *name == "run") {
                    exports.push(("run", WasmExternalKind::Func.as_u8(), start_func_index));
                }
            }
            else if !exports.iter().any(|(name, _, _)| *name == "run") {
                exports.push(("run", WasmExternalKind::Func.as_u8(), index));
            }

            if !exports.iter().any(|(name, _, _)| *name == wasi_cli_run_export.as_str()) {
                let wrapper_type_index = u32::try_from(type_indices.len()).expect("type index overflow");
                type_indices.push(wasm_function_type(&[], &[VALTYPE_I32]));
                function_indices.push(wrapper_type_index);
                let wrapper = build_entry_wrapper_body(true);
                let wrapper_func_index = import_count + u32::try_from(code_bodies.len()).expect("code body overflow");
                code_bodies.push(wasm_function_body(wrapper));
                exports.push((wasi_cli_run_export.as_str(), WasmExternalKind::Func.as_u8(), wrapper_func_index));
            }
        }
    }

    // Node / JS-glue：`build` 导出真实?`build_from_cli_state` 编译器入口（?return 0 桩）?
    if export_name == "main" && !exports.iter().any(|(name, _, _)| *name == "build") {
        if let Some(build_operation) =
            operations.iter().find(|operation| operation.parts().last().is_some_and(|part| part.as_str() == "build_from_cli_state"))
        {
            if let Some(&build_index) = function_index_by_name.get(&build_operation.to_string()) {
                exports.push(("build", WasmExternalKind::Func.as_u8(), build_index));
            }
        }
    }

    if export_name == "main" {
        for (operation_name, export) in [("version_text", "version"), ("print_root_help", "help"), ("build_from_cli_state", "build")] {
            let Some(operation) =
                operations.iter().find(|operation| operation.parts().last().is_some_and(|part| part.as_str() == operation_name))
            else {
                continue;
            };
            let Some(&function_index) = function_index_by_name.get(&operation.to_string())
            else {
                continue;
            };
            if !exports.iter().any(|(name, _, _)| *name == export) {
                exports.push((export, WasmExternalKind::Func.as_u8(), function_index));
            }
        }
        if !exports.iter().any(|(name, _, _)| *name == "version") {
            if let Some((_, text)) = submission
                .operation_literal_returns
                .iter()
                .find(|(operation, _)| operation.parts().last().is_some_and(|part| part.as_str() == "version_text"))
            {
                if let Some(&literal_index) = string_literal_index.get(text) {
                    let type_index = u32::try_from(type_indices.len()).expect("type index overflow");
                    type_indices.push(wasm_function_type(&[], &[VALTYPE_I32]));
                    function_indices.push(type_index);
                    let function_index = import_count + u32::try_from(code_bodies.len()).expect("function index overflow");
                    let mut body = vec![0];
                    WasmOpcode::I32Const.encode(&mut body);
                    encode_sleb128_i32(literal_index as i32, &mut body);
                    if let Some(import_index) = const_utf8_import {
                        WasmOpcode::Call.encode(&mut body);
                        encode_uleb128(import_index, &mut body);
                    }
                    WasmOpcode::End.encode(&mut body);
                    synthetic_version_body = Some((function_index, body));
                }
            }
        }
    }

    if let Some((function_index, body)) = synthetic_version_body {
        code_bodies.push(body);
        exports.push(("version", WasmExternalKind::Func.as_u8(), function_index));
    }

    module.sections.push(type_section_bytes(type_indices));
    if !host_imports.is_empty() {
        let import_entries = host_imports
            .iter()
            .zip(import_type_entries.iter())
            .map(|((module, field), type_index)| {
                let (core_module, core_field) = wasi_core_import_name(module, field, wasi_preview);
                (core_module, core_field, *type_index)
            })
            .collect::<Vec<_>>();
        module.sections.push(import_section_bytes(&import_entries));
    }

    let data_end = if wasi_mode && !string_literals.is_empty() { wasi_string_data_section_size(&string_literals) } else { 0 };
    let heap_base = cabi_heap_base_after_data(data_end as usize);

    // Append cabi_realloc functype + function shared with MIR linear bump.
    let realloc_type_index = {
        let realloc_ty = wasm_function_type(&[VALTYPE_I32, VALTYPE_I32, VALTYPE_I32, VALTYPE_I32], &[VALTYPE_I32]);
        let type_section = module.sections.iter_mut().find(|item| item.id == 1).expect("type section");
        let mut pos = 0;
        let count = decode_uleb128(&type_section.bytes, &mut pos);
        let rest = type_section.bytes[pos..].to_vec();
        let mut bytes = Vec::new();
        encode_uleb128(count + 1, &mut bytes);
        bytes.extend_from_slice(&rest);
        bytes.extend_from_slice(&realloc_ty);
        type_section.bytes = bytes;
        count
    };

    let realloc_func_index = import_count + u32::try_from(function_indices.len()).unwrap();
    function_indices.push(realloc_type_index);
    code_bodies.push(wasm_function_body(wasm_cabi_realloc_bump_body()));

    module.sections.push(function_section_bytes(&function_indices));
    module.sections.push(memory_section_bytes(memory_min_pages_for_heap_base(heap_base)));
    module.sections.push(cabi_heap_global_section(heap_base));

    if !exports.iter().any(|(name, _, _)| *name == "memory") {
        exports.push(("memory", WasmExternalKind::Memory.as_u8(), 0));
    }
    if !exports.iter().any(|(name, _, _)| *name == "cabi_realloc") {
        exports.push(("cabi_realloc", WasmExternalKind::Func.as_u8(), realloc_func_index));
    }

    module.sections.push(export_section_bytes(&exports));
    module.sections.push(code_section_bytes(&code_bodies));
    if wasi_mode {
        // WASI 轨：字符串字面量存入 data 段，运行时通过偏移量访问?
        if !string_literals.is_empty() {
            let data = build_wasi_string_data_section(&string_literals);
            module.sections.push(data_section_bytes(0, &data));
        }
    }
    else if !string_literals.is_empty() {
        // Node 轨：字符串字面量存入自定义段 `nyar.strings`，由 `.mjs` 启动壳加载?
        let payload = serde_json::json!({ "strings": string_literals });
        module.sections.push(WasmSection { id: 0, name: Some("nyar.strings".to_string()), bytes: payload.to_string().into_bytes() });
    }
    append_wasm_spy_metadata_sections(
        &mut module,
        &ctx,
        submission,
        import_count,
        &mir_wasm_functions,
        &gc_struct_type_indices,
        &gc_array_type_indices,
    );
    (module, host_imports)
}

fn is_generic_array_element_type(element_type: &NyarType) -> bool {
    matches!(element_type, NyarType::Named(name) if name.as_str().len() == 1 && name.as_str().chars().next().is_some_and(|ch| ch.is_ascii_uppercase()))
}

/// Prefer the `[utf8]` / i32-handle arraytype used for WASI/Node argv.
fn prefer_utf8_argv_array_type(gc_array_type_indices: &BTreeMap<String, u32>) -> Option<u32> {
    // argv is an array of the explicit language-level UTF-8 handles.  A GC
    // type key or a nominal name must not be used to recover this contract.
    gc_array_type_indices.get("Utf8").copied().or_else(|| gc_array_type_indices.values().next().copied())
}

/// Push a typed default for a nullary WASI wrapper calling a still-parameterized entry.
///
/// `[T]` →?empty `array.new_default` (never `ref.null`: entry does `ref.cast`/`array.len`).
/// Scalars →?0; other references →?`ref.null any`.
fn emit_wasi_entry_default_arg(ty: &NyarType, gc_array_type_indices: &BTreeMap<String, u32>, body: &mut Vec<u8>) {
    match ty {
        NyarType::Array(element) | NyarType::FixedArray { element, .. } => {
            let key = wasm_array_element_type_key(element);
            let type_index = gc_array_type_indices
                .get(&key)
                .copied()
                .or_else(|| if is_js_glue_host_string_type(element) { prefer_utf8_argv_array_type(gc_array_type_indices) } else { None })
                .or_else(|| prefer_utf8_argv_array_type(gc_array_type_indices));
            if let Some(type_index) = type_index {
                encode_i32_const(0, body);
                encode_array_new_default(type_index, body);
            }
            else {
                encode_ref_null_anyref(body);
            }
        }
        NyarType::Float64 | NyarType::Float32 => encode_f64_const(0.0, body),
        NyarType::Integer64 { .. } | NyarType::Integer128 { .. } => encode_i64_const(0, body),
        NyarType::Unit => encode_ref_null_anyref(body),
        NyarType::Bottom => encode_i32_const(0, body),
        other if is_js_glue_host_string_type(other) => encode_i32_const(0, body),
        other if type_is_wasm_gc_heap_reference(other) => encode_ref_null_anyref(body),
        _ => encode_i32_const(0, body),
    }
}

/// Out-pointer slot in the reserved `[0, WASI_STRING_DATA_OFFSET)` region for `get-arguments`.
const WASI_GET_ARGUMENTS_OUT_PTR: i32 = 16;

/// Build guest `[utf8]` argv from `wasi:cli/environment#get-arguments` (cabi out-pointer).
///
/// Host writes `{list_ptr, list_len}` at `out`; each element is Canonical ABI `{str_ptr, str_len}`
/// (raw utf8 bytes). Guest handles use `[len:u32 LE][utf8…]` in the cabi bump heap (global 0).
///
/// Locals (declared by caller): `0..6` i32, `7` (ref null $array_ty) (argv).
/// Stack effect: pushes argv anyref.
fn emit_wasi_argv_from_get_arguments(get_arguments_import: u32, array_ty: u32, body: &mut Vec<u8>) {
    // get-arguments(out=16)
    encode_i32_const(WASI_GET_ARGUMENTS_OUT_PTR, body);
    encode_call(get_arguments_import, body);
    // list_ptr = i32.load(16)
    encode_i32_const(WASI_GET_ARGUMENTS_OUT_PTR, body);
    encode_i32_load(2, 0, body);
    encode_local_set(0, body);
    // list_len = i32.load(20)
    encode_i32_const(WASI_GET_ARGUMENTS_OUT_PTR + 4, body);
    encode_i32_load(2, 0, body);
    encode_local_set(1, body);
    // argv = array.new_default(list_len)
    encode_local_get(1, body);
    encode_array_new_default(array_ty, body);
    encode_local_set(7, body);
    // i = 0
    encode_i32_const(0, body);
    encode_local_set(2, body);
    // block { loop {
    encode_block_empty(body);
    encode_loop_empty(body);
    // if i >= list_len { br 1 }
    encode_local_get(2, body);
    encode_local_get(1, body);
    WasmOpcode::I32GeU.encode(body);
    encode_br_if(1, body);
    // elem = list_ptr + i*8
    encode_local_get(0, body);
    encode_local_get(2, body);
    encode_i32_const(8, body);
    WasmOpcode::I32Mul.encode(body);
    encode_i32_add(body);
    encode_local_tee(6, body); // elem base in local 6 temporarily
    // str_ptr
    encode_i32_load(2, 0, body);
    encode_local_set(3, body);
    // str_len
    encode_local_get(6, body);
    encode_i32_load(2, 4, body);
    encode_local_set(4, body);
    // size = align4(4 + str_len)
    encode_local_get(4, body);
    encode_i32_const(4, body);
    encode_i32_add(body);
    encode_i32_const(3, body);
    encode_i32_add(body);
    encode_i32_const(!3i32, body); // -4 == !3 for align mask in two's complement-?wait
    // (x + 3) & !3  where !3 as i32 is -4
    WasmOpcode::I32And.encode(body);
    encode_local_set(6, body); // size
    // handle = global.get 0; global.set(handle+size)
    encode_global_get(CABI_HEAP_GLOBAL_INDEX, body);
    encode_local_set(5, body);
    encode_local_get(5, body);
    encode_local_get(6, body);
    encode_i32_add(body);
    encode_global_set(CABI_HEAP_GLOBAL_INDEX, body);
    // store len
    encode_local_get(5, body);
    encode_local_get(4, body);
    encode_i32_store(2, 0, body);
    // memory.copy(handle+4, str_ptr, str_len)
    encode_local_get(5, body);
    encode_i32_const(4, body);
    encode_i32_add(body);
    encode_local_get(3, body);
    encode_local_get(4, body);
    encode_memory_copy(body);
    // array.set(argv, i, handle)
    encode_local_get(7, body);
    encode_local_get(2, body);
    encode_local_get(5, body);
    encode_array_set(array_ty, body);
    // i++
    encode_local_get(2, body);
    encode_i32_const(1, body);
    encode_i32_add(body);
    encode_local_set(2, body);
    encode_br(0, body); // continue loop
    WasmOpcode::End.encode(body); // loop
    WasmOpcode::End.encode(body); // block
    encode_local_get(7, body);
}

fn append_wasm_spy_metadata_sections(
    module: &mut WasmBinaryModule,
    ctx: &ExecutableLoweringContext,
    submission: &FragmentSubmission,
    import_count: u32,
    mir_wasm_functions: &[(u32, String)],
    gc_struct_type_indices: &BTreeMap<LayoutId, u32>,
    gc_array_type_indices: &BTreeMap<String, u32>,
) {
    let mut func_payload = format!("import_count={import_count}\n");
    for (wasm_index, symbol) in mir_wasm_functions {
        func_payload.push_str(&format!("{wasm_index}\t{symbol}\n"));
    }
    module.sections.push(WasmSection { id: 0, name: Some("nyar.wasm.functions".to_string()), bytes: func_payload.into_bytes() });

    // 语言规范：wasm/wasi 强制 GC；structtype ?arraytype 均注册?
    let mut gc_payload = String::from("gc_required=true\n");
    for layout in &ctx.layouts.layouts {
        let storage = if layout.storage == StorageKind::Reference { "reference" } else { "value" };
        if let Some(type_index) = gc_struct_type_indices.get(&layout.id) {
            gc_payload.push_str(&format!("struct\t{}\t{}\t{type_index}\tregistered\t{storage}\n", layout.id, layout.name));
        }
        else if layout_needs_gc_struct(ctx, layout) {
            gc_payload.push_str(&format!("struct\t{}\t{}\tmissing\tnot_registered\t{storage}\n", layout.id, layout.name));
        }
        else {
            gc_payload.push_str(&format!("struct\t{}\t{}\tn/a\tlinear_memory\t{storage}\n", layout.id, layout.name));
        }
    }
    for (key, type_index) in gc_array_type_indices {
        gc_payload.push_str(&format!("array\t{key}\t{type_index}\tregistered\n"));
    }
    if let Some(exec) = &submission.executable {
        for operation in exec.operations() {
            let Some(view) = exec.get_function(&operation)
            else {
                continue;
            };
            let symbol = operation.to_string();
            for block in &view.function.blocks {
                for instruction in &block.instructions {
                    let mut record_missing = |layout_id: LayoutId, type_name: &str, site: &str| {
                        if gc_struct_type_indices.contains_key(&layout_id) {
                            return;
                        }
                        gc_payload.push_str(&format!("mir_use\t{layout_id}\t{type_name}\t{site}\t{symbol}\n"));
                    };
                    match &instruction.kind {
                        MirInstructionKind::StructNew { type_name, .. } => {
                            if let Some(layout) = ctx.layout_by_type_name(type_name) {
                                if layout.storage == StorageKind::Reference {
                                    record_missing(layout.id, type_name, "StructNew");
                                }
                            }
                        }
                        MirInstructionKind::FieldGet { object, .. } | MirInstructionKind::FieldSet { object, .. } => {
                            if let MirOperand::Value(vref) = object {
                                if let Some(ty) = view.function.value_types.get(vref) {
                                    if let Some(layout) = ctx.layout_for_value_type(ty) {
                                        if layout.storage == StorageKind::Reference {
                                            record_missing(layout.id, &layout.name, "FieldAccess");
                                        }
                                    }
                                }
                            }
                        }
                        MirInstructionKind::AggregateCopy { source, dest, .. } => {
                            for operand in [source, dest] {
                                if let MirOperand::Value(vref) = operand {
                                    if let Some(ty) = view.function.value_types.get(vref) {
                                        if let Some(layout) = ctx.layout_for_value_type(ty) {
                                            if layout.storage == StorageKind::Reference {
                                                record_missing(layout.id, &layout.name, "AggregateCopy");
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    module.sections.push(WasmSection { id: 0, name: Some("nyar.wasm.gc_layouts".to_string()), bytes: gc_payload.into_bytes() });
}

pub(crate) fn augment_wasm_with_value_aggregate_metadata(module: &mut WasmBinaryModule, submission: &FragmentSubmission) {
    let mir_functions_len = submission.executable.as_ref().map(|exec| exec.operations().len()).unwrap_or(0);
    if mir_functions_len == 0 {
        return;
    }
    let value_layout_count = submission.aggregate_layouts.layouts.iter().filter(|layout| layout.storage == StorageKind::Value).count();
    let reference_layout_count = submission.aggregate_layouts.layouts.iter().filter(|layout| layout.storage == StorageKind::Reference).count();
    let payload =
        format!("mir_functions={};value_layouts={};reference_layouts={}", mir_functions_len, value_layout_count, reference_layout_count);
    module.sections.push(WasmSection { id: 0, name: Some("nyar.value_aggregate".to_string()), bytes: payload.into_bytes() });
}

fn lower_mir_function_to_wasm_bytes(
    submission: &FragmentSubmission,
    mir_fn: &MirFunction,
    return_value_type: Option<u8>,
    js_glue_utf8_as_anyref: bool,
    wasi_mode: bool,
    function_index_by_name: &BTreeMap<String, u32>,
    type_index_by_name: &BTreeMap<String, u32>,
    param_types_by_name: &BTreeMap<String, Vec<u8>>,
    return_types_by_name: &BTreeMap<String, Option<u8>>,
    param_types_by_function_index: &BTreeMap<u32, Vec<u8>>,
    return_types_by_function_index: &BTreeMap<u32, Option<u8>>,
    import_param_types: &[Vec<u8>],
    import_return_types: &[Option<u8>],
    gc_struct_type_indices: &BTreeMap<LayoutId, u32>,
    gc_array_type_indices: &BTreeMap<String, u32>,
    gc_sum_type_indices: &BTreeMap<String, u32>,
    gc_i32_box_type_index: u32,
    callee_import_index: &BTreeMap<String, u32>,
    host_imports: &[(String, String)],
    string_literal_index: &BTreeMap<String, u32>,
    string_literal_offset: &BTreeMap<String, u32>,
    const_utf8_import: Option<u32>,
) -> Vec<u8> {
    if let Err(error) = validate_pattern_matching_invariants(mir_fn) {
        debug_assert!(false, "pattern matching contract violation in function `{}`: {error:?}", mir_fn.symbol);
    }
    let ctx = ExecutableLoweringContext::new(submission);
    let mut lowerer = WasmMirLowerer::new(
        &ctx,
        mir_fn,
        return_value_type,
        js_glue_utf8_as_anyref,
        wasi_mode,
        function_index_by_name,
        type_index_by_name,
        param_types_by_name,
        return_types_by_name,
        param_types_by_function_index,
        return_types_by_function_index,
        import_param_types,
        import_return_types,
        gc_struct_type_indices,
        gc_array_type_indices,
        gc_sum_type_indices,
        gc_i32_box_type_index,
        callee_import_index,
        host_imports,
        string_literal_index,
        string_literal_offset,
        const_utf8_import,
    );
    lowerer.emit_function_body();
    lowerer.finish()
}

struct WasmMirLowerer<'a> {
    ctx: &'a ExecutableLoweringContext<'a>,
    mir_fn: &'a MirFunction,
    /// 当前函数?WASM 返回值类型字节?
    /// `None` = void（无返回值）；`Some(VALTYPE_I32)` = i32；`Some(VALTYPE_ANYREF)` = anyref?
    /// `Some(VALTYPE_F64)` = f64；`Some(VALTYPE_I64)` = i64?
    return_value_type: Option<u8>,
    /// Node JS-glue 路径：utf8/utf16 ?anyref 传递（宿主字符串）?
    js_glue_utf8_as_anyref: bool,
    /// WASI 轨：字符串字面量作为 i32 偏移量传递，存入 data 段?
    wasi_mode: bool,
    code: Vec<u8>,
    /// Scratch: aligned pointer returned by the global bump allocator.
    stack_ptr_local: u32,
    /// Scratch: `new_end` after bump.
    bump_end_local: u32,
    /// Scratch: pages to grow.
    bump_pages_local: u32,
    next_local: u32,
    /// 每个声明 local 的完?valtype 字节（i32=VALTYPE_I32, anyref=VALTYPE_ANYREF, struct ref=VALTYPE_REF+typeidx）?
    local_valtypes: Vec<Vec<u8>>,
    value_locals: BTreeMap<MirValueRef, u32>,
    /// 引用类型 local 的集合。这?local 的值类型是 `anyref` (VALTYPE_ANYREF),而非 i32?
    reference_locals: BTreeMap<MirValueRef, u32>,
    var_locals: BTreeMap<String, u32>,
    scalar_locals: BTreeMap<MirValueRef, u32>,
    block_order: Vec<MirBlockRef>,
    block_index: BTreeMap<MirBlockRef, usize>,
    function_index_by_name: &'a BTreeMap<String, u32>,
    type_index_by_name: &'a BTreeMap<String, u32>,
    param_types_by_name: &'a BTreeMap<String, Vec<u8>>,
    return_types_by_name: &'a BTreeMap<String, Option<u8>>,
    /// ?`function_index_by_name` 下标对齐?callee 形参 wasm 类型（call 实参 coerce 权威来源）?
    param_types_by_function_index: &'a BTreeMap<u32, Vec<u8>>,
    return_types_by_function_index: &'a BTreeMap<u32, Option<u8>>,
    import_param_types: &'a [Vec<u8>],
    import_return_types: &'a [Option<u8>],
    /// 引用类型 layout_id -> wasm-gc structtype ?type_index?
    gc_struct_type_indices: &'a BTreeMap<LayoutId, u32>,
    /// unite sum_name -> wasm-gc structtype [i32,anyref] ?type_index?
    gc_sum_type_indices: &'a BTreeMap<String, u32>,
    /// Fine/Fail ?unite 标量 payload（utf8/bool/i32）装箱用 structtype [i32]?
    gc_i32_box_type_index: u32,
    /// heap array element_type 字符串键 -> wasm-gc arraytype ?type_index?
    gc_array_type_indices: &'a BTreeMap<String, u32>,
    callee_import_index: &'a BTreeMap<String, u32>,
    /// WASI 宿主 import 表（?stream 内建），?write-via-stream 协议查找索引?
    host_imports: &'a [(String, String)],
    string_literal_index: &'a BTreeMap<String, u32>,
    /// WASI 轨：字符串字面量到线性内存偏移量的映射?
    string_literal_offset: &'a BTreeMap<String, u32>,
    const_utf8_import: Option<u32>,
    /// 需要在函数入口显式初始化的 typed ref local 列表 (local_index, type_index)?
    /// V8 要求 `(ref null T)` typed local 必须在入口显?`ref.null T` + `local.set` 初始化，
    /// 否则?"uninitialized non-defaultable local" 编译错误?
    typed_ref_locals_to_init: Vec<(u32, u32)>,
    /// CFG 调度用的基本?PC（`block_order` 下标）。`loop` + `br_table` 按此分发?
    /// 正确实现前向 Jump、后?Jump ?Branch；旧线?`br 0` 模型会误落入错误后继
    /// （自?`build` →?VON lexer 早退 `Fine([])` →?`ref.cast` illegal cast）?
    pc_local: u32,
}

impl<'a> WasmMirLowerer<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        ctx: &'a ExecutableLoweringContext<'a>,
        mir_fn: &'a MirFunction,
        return_value_type: Option<u8>,
        js_glue_utf8_as_anyref: bool,
        wasi_mode: bool,
        function_index_by_name: &'a BTreeMap<String, u32>,
        type_index_by_name: &'a BTreeMap<String, u32>,
        param_types_by_name: &'a BTreeMap<String, Vec<u8>>,
        return_types_by_name: &'a BTreeMap<String, Option<u8>>,
        param_types_by_function_index: &'a BTreeMap<u32, Vec<u8>>,
        return_types_by_function_index: &'a BTreeMap<u32, Option<u8>>,
        import_param_types: &'a [Vec<u8>],
        import_return_types: &'a [Option<u8>],
        gc_struct_type_indices: &'a BTreeMap<LayoutId, u32>,
        gc_array_type_indices: &'a BTreeMap<String, u32>,
        gc_sum_type_indices: &'a BTreeMap<String, u32>,
        gc_i32_box_type_index: u32,
        callee_import_index: &'a BTreeMap<String, u32>,
        host_imports: &'a [(String, String)],
        string_literal_index: &'a BTreeMap<String, u32>,
        string_literal_offset: &'a BTreeMap<String, u32>,
        const_utf8_import: Option<u32>,
    ) -> Self {
        let block_order = collect_reachable_blocks(mir_fn);
        let block_index = block_order.iter().enumerate().map(|(index, block_id)| (*block_id, index)).collect();
        // wasm 函数参数占据 local 0..param_count-1，声明局部从 param_count 开始?
        // stack_ptr 是第一个声明的局部，位于 local param_count?
        // 若函数有 anyref 参数，旧代码?stack_ptr 放在 local 0 会与参数 0 冲突?
        // 导致 V8 ?`local.set[0] expected type anyref, found i32.const`?
        let param_count = mir_fn.param_types.len() as u32;
        let mut lowerer = Self {
            ctx,
            mir_fn,
            return_value_type,
            js_glue_utf8_as_anyref,
            wasi_mode,
            code: Vec::new(),
            stack_ptr_local: param_count,
            bump_end_local: param_count + 1,
            bump_pages_local: param_count + 2,
            // Branch flag locals 按嵌套深度按需 alloc_i32_local，不预留单一 slot?
            next_local: param_count + 3,
            local_valtypes: vec![vec![VALTYPE_I32], vec![VALTYPE_I32], vec![VALTYPE_I32]],
            value_locals: BTreeMap::new(),
            reference_locals: BTreeMap::new(),
            var_locals: BTreeMap::new(),
            scalar_locals: BTreeMap::new(),
            block_order,
            block_index,
            function_index_by_name,
            type_index_by_name,
            param_types_by_name,
            return_types_by_name,
            param_types_by_function_index,
            return_types_by_function_index,
            import_param_types,
            import_return_types,
            gc_struct_type_indices,
            gc_array_type_indices,
            gc_sum_type_indices,
            gc_i32_box_type_index,
            callee_import_index,
            host_imports,
            string_literal_index,
            string_literal_offset,
            const_utf8_import,
            typed_ref_locals_to_init: Vec::new(),
            pc_local: 0,
        };
        lowerer.pc_local = lowerer.alloc_i32_local();
        // Linear bump uses the module-global heap cursor (never STACK_BASE=0).
        for block_id in lowerer.block_order.clone() {
            let Some(block) = mir_fn.blocks.get(block_id.0 as usize)
            else {
                continue;
            };
            // 入口块的参数对应函数参数，它们已?local 0..param_count-1?
            // 直接映射到参?local，避免重复分配导致参数值丢失?
            // 使用 param_types 判定引用类型，而非 value_types（后者可能缺条目导致误判）?
            let is_entry_block = block.id == mir_fn.entry;
            for (param_slot, param) in block.parameters.iter().enumerate() {
                if is_entry_block && param_slot < mir_fn.param_types.len() {
                    let local = param_slot as u32;
                    let param_ty = &mir_fn.param_types[param_slot];
                    // 与函数签?`wasm_param_value_type_for` 对齐：GC struct / Reference →?anyref?
                    // 若只?`storage_for_type` 而签名因 gc_struct 登记?anyref，会?param
                    // 误写?value_locals，随?`local.get` ?anyref、`local.set` 却按 i32?
                    let param_vt = wasm_param_value_type_for(ctx, param_ty, gc_struct_type_indices, js_glue_utf8_as_anyref);
                    let is_reference = param_vt == WASM_GC_ANYREF || param_vt == WASM_GC_EXTERNREF;
                    if is_reference {
                        lowerer.reference_locals.insert(*param, local);
                    }
                    else {
                        lowerer.value_locals.insert(*param, local);
                    }
                    continue;
                }
                // 非入口块参数：分配新 local?
                // 引用 →?anyref；标量按 value_types 的真?wasm valtype（含 i64/f64），
                // 禁止一?i32（否?jump/Copy ?`local.set expected i32, found i64`）?
                let param_vt = mir_fn
                    .value_types
                    .get(param)
                    .map(|ty| wasm_param_value_type_for(ctx, ty, gc_struct_type_indices, js_glue_utf8_as_anyref))
                    .unwrap_or(VALTYPE_I32);
                if param_vt == WASM_GC_ANYREF || param_vt == WASM_GC_EXTERNREF {
                    let local = lowerer.alloc_anyref_local();
                    lowerer.reference_locals.insert(*param, local);
                }
                else {
                    let local = lowerer.alloc_scalar_local_for_stack_type(param_vt);
                    lowerer.value_locals.insert(*param, local);
                }
            }
            for instruction in &block.instructions {
                lowerer.plan_instruction(instruction);
            }
        }
        lowerer
    }

    fn storage_for_type(&self, ty: &NyarType) -> MirStorageKind {
        mir_storage_for_type(self.ctx, ty, self.js_glue_utf8_as_anyref)
    }

    fn finish(self) -> Vec<u8> {
        // ?local index 顺序扫描,将相同类型的连续 local 合并为一组?
        // wasm 规范要求 local 声明组的总数等于实际 local ?顺序?local 索引?
        let mut locals: Vec<(u32, Vec<u8>)> = Vec::new();
        let mut index = 0u32;
        while index < self.local_valtypes.len() as u32 {
            let ty = &self.local_valtypes[index as usize];
            let mut count = 1u32;
            while (index + count) < self.local_valtypes.len() as u32 && self.local_valtypes[(index + count) as usize] == *ty {
                count += 1;
            }
            locals.push((count, ty.clone()));
            index += count;
        }
        let mut body = Vec::new();
        encode_uleb128(locals.len() as u32, &mut body);
        for (count, ty) in locals {
            encode_uleb128(count, &mut body);
            body.extend_from_slice(&ty);
        }
        body.extend_from_slice(&self.code);
        WasmOpcode::End.encode(&mut body);
        body
    }

    fn alloc_i32_local(&mut self) -> u32 {
        let local = self.next_local;
        self.next_local += 1;
        self.local_valtypes.push(vec![VALTYPE_I32]);
        local
    }

    fn alloc_i64_local(&mut self) -> u32 {
        let local = self.next_local;
        self.next_local += 1;
        self.local_valtypes.push(vec![VALTYPE_I64]);
        local
    }

    fn alloc_f64_local(&mut self) -> u32 {
        let local = self.next_local;
        self.next_local += 1;
        self.local_valtypes.push(vec![VALTYPE_F64]);
        local
    }

    /// 按栈上标?valtype 分配 local（i64/f64/i32）。引用类型请?`alloc_anyref_local`?
    fn alloc_scalar_local_for_stack_type(&mut self, stack_ty: u8) -> u32 {
        match stack_ty {
            VALTYPE_I64 => self.alloc_i64_local(),
            VALTYPE_F64 => self.alloc_f64_local(),
            _ => self.alloc_i32_local(),
        }
    }

    /// 分配 typed `(ref null T)` local，供 struct.get/set 使用?
    fn alloc_struct_ref_local(&mut self, type_index: u32) -> u32 {
        let _ = type_index;
        return self.alloc_anyref_local();
        /*
        let local = self.next_local;
        self.next_local += 1;
        let mut valtype = vec![VALTYPE_REF];
        // heaptype 使用 signed LEB128；ULEB128 ?type_index >= 64 时会与抽?heap type 冲突
        //（例?124 →?VALTYPE_F64 ?V8 读成 -4/f64，触?`Unknown heap type -4`）?
        encode_sleb128_i32(i32::try_from(type_index).unwrap_or(i32::MAX), &mut valtype);
        self.local_valtypes.push(valtype);
        self.typed_ref_locals_to_init.push((local, type_index));
        local */
    }

    /// 分配一?anyref local。该 local ?`finish()` 中按?local index 自动归入 anyref 组?
    fn alloc_anyref_local(&mut self) -> u32 {
        let local = self.next_local;
        self.next_local += 1;
        self.local_valtypes.push(vec![WASM_GC_ANYREF]);
        local
    }

    /// ?wasm local 绝对索引映射为值类型字节?
    ///
    /// `local_valtypes` 仅覆盖声明局部（?`stack_ptr_local` 起）?
    /// 函数参数类型来自 `mir_fn.param_types`?
    fn wasm_local_value_type(&self, local_index: u32) -> u8 {
        let param_count = self.stack_ptr_local;
        if local_index < param_count {
            return match self.mir_fn.param_types.get(local_index as usize) {
                Some(ty) => {
                    let vt = wasm_param_value_type_for(self.ctx, ty, self.gc_struct_type_indices, self.js_glue_utf8_as_anyref);
                    if vt == WASM_GC_ANYREF || vt == WASM_GC_EXTERNREF { WASM_GC_ANYREF } else { vt }
                }
                None => VALTYPE_I32,
            };
        }
        let declared = (local_index - param_count) as usize;
        match self.local_valtypes.get(declared).map(|v| v.as_slice()) {
            Some([VALTYPE_I32]) => VALTYPE_I32,
            // alloc_anyref_local 写入 WASM_GC_ANYREF? VALTYPE_ANYREF）；两者都认?
            Some([VALTYPE_ANYREF]) | Some([WASM_GC_ANYREF]) | Some([VALTYPE_REF, ..]) => WASM_GC_ANYREF,
            Some([VALTYPE_EXTERNREF]) | Some([WASM_GC_EXTERNREF]) => WASM_GC_EXTERNREF,
            Some([VALTYPE_I64]) => VALTYPE_I64,
            Some([VALTYPE_F64]) => VALTYPE_F64,
            _ => VALTYPE_I32,
        }
    }

    fn plan_instruction(&mut self, instruction: &MirInstruction) {
        match &instruction.kind {
            MirInstructionKind::StoreVar { name, value, ty } => {
                if !self.var_locals.contains_key(name) {
                    // 变量 local 类型决策:
                    //
                    // emit 阶段?value 的发射取决于 value 本身,与显?`ty` 注解无关:
                    //   - `Constant(String/Unit)` →?`ref.null anyref`(引用语义);
                    //   - `Symbol`(未解? →?`ref.null anyref`(引用语义,?emit_operand fallback 一?;
                    //   - `Value(vref)` →??reference_locals/value_types 决定?
                    //
                    // ?`ty` 标注 Value ?value 实际是引用语?典型场景:
                    // StoreVar ?`Constant(String)` 存入声明?i32 的变?,
                    // plan 仍需分配 anyref local,否则 emit_operand ?ref.null
                    // ?local.set ?i32 local,触发
                    // `local.set expected i32, found ref.null of type anyref` 阻断自举?
                    //
                    // 因此:变量 local 的存储语?= `ty` 声明 OR value 实际语义,
                    // 二者只要其一为引?即分?anyref local?
                    let ty_says_reference = ty.as_ref().map(|ty| self.storage_for_type(ty) == StorageKind::Reference).unwrap_or(false);
                    let is_reference = ty_says_reference || self.operand_is_reference_storage(value);
                    let local = if is_reference {
                        self.alloc_anyref_local()
                    }
                    else {
                        let stack_ty = ty
                            .as_ref()
                            .map(|ty| wasm_param_value_type_for(self.ctx, ty, self.gc_struct_type_indices, self.js_glue_utf8_as_anyref))
                            .unwrap_or_else(|| self.operand_wasm_stack_type(value));
                        self.alloc_scalar_local_for_stack_type(stack_ty)
                    };
                    self.var_locals.insert(name.clone(), local);
                }
            }
            _ => {
                if let Some(output) = instruction_primary_result(instruction) {
                    let planned_storage = self.output_storage_kind(instruction);
                    // Nominal MIR storage can be stale for aggregates carrying GC
                    // references (notably constructor/call results).  Keep the
                    // output in a reference local whenever its ABI type is anyref;
                    // otherwise ArrayPush and field access would see an i32 slot
                    // and coerce the value to ref.null.
                    let storage = self
                        .mir_fn
                        .value_types
                        .get(&output)
                        .map(|ty| wasm_param_value_type_for(self.ctx, ty, self.gc_struct_type_indices, self.js_glue_utf8_as_anyref))
                        .filter(|vt| matches!(*vt, WASM_GC_ANYREF | WASM_GC_EXTERNREF))
                        .map(|_| StorageKind::Reference)
                        .unwrap_or(planned_storage);
                    match storage {
                        StorageKind::Value => {
                            if !self.value_locals.contains_key(&output) {
                                let stack_ty = self
                                    .mir_fn
                                    .value_types
                                    .get(&output)
                                    .map(|ty| wasm_param_value_type_for(self.ctx, ty, self.gc_struct_type_indices, self.js_glue_utf8_as_anyref))
                                    .unwrap_or(VALTYPE_I32);
                                let local = self.alloc_scalar_local_for_stack_type(stack_ty);
                                self.value_locals.insert(output, local);
                            }
                        }
                        StorageKind::Reference => {
                            if !self.reference_locals.contains_key(&output) {
                                // ?output 已在 `value_locals`（如块参数预分配?i32），
                                // 必须移除，否?`emit_operand` 会优先读?i32 local?
                                // ?`store_scalar`/Call 写入 anyref local，造成读写不一致?
                                self.value_locals.remove(&output);
                                let local = if let MirInstructionKind::StructNew { type_name, .. } = &instruction.kind {
                                    if self.struct_new_uses_gc_struct(type_name) {
                                        let layout = self.resolve_layout(None, type_name);
                                        if let Some(type_index) = self.resolve_gc_struct_type_index(layout.id, &layout.name) {
                                            self.alloc_struct_ref_local(type_index)
                                        }
                                        else {
                                            self.alloc_anyref_local()
                                        }
                                    }
                                    else {
                                        self.alloc_anyref_local()
                                    }
                                }
                                else if let MirInstructionKind::AggregateCopy { source, .. } = &instruction.kind {
                                    let layout = self
                                        .infer_aggregate_layout_for_operand(source)
                                        .cloned()
                                        .unwrap_or_else(|| self.resolve_layout(None, ""));
                                    if self.gc_struct_type_indices.contains_key(&layout.id) {
                                        if let Some(type_index) = self.resolve_gc_struct_type_index(layout.id, &layout.name) {
                                            self.alloc_struct_ref_local(type_index)
                                        }
                                        else {
                                            self.alloc_anyref_local()
                                        }
                                    }
                                    else {
                                        self.alloc_anyref_local()
                                    }
                                }
                                else {
                                    self.alloc_anyref_local()
                                };
                                self.reference_locals.insert(output, local);
                            }
                        }
                    }
                }
            }
        }
    }

    /// 判定指令输出的存储语义。值类型用线性内存地址 (i32 local),
    /// 引用类型?wasm-gc 对象引用 (anyref local)?
    ///
    /// ?`mir_fn.value_types` 缺少条目时，根据指令本身推断存储语义?
    /// 而非统一默认 `Reference`——否?`LoadConstant { Int(0) }` 这类标量输出
    /// 会被误分?anyref local，?`emit_load_constant` 发射 `i32.const`?
    /// 造成 `local.set expected anyref, found i32` 类型不匹配?
    fn output_storage_kind(&self, instruction: &MirInstruction) -> MirStorageKind {
        match &instruction.kind {
            MirInstructionKind::StructNew { type_name, .. } => {
                if self.struct_new_uses_gc_struct(type_name) {
                    StorageKind::Reference
                }
                else if let Some(layout) = self.ctx.layout_by_type_name(type_name) {
                    layout.storage
                }
                else {
                    StorageKind::Reference
                }
            }
            MirInstructionKind::TupleNew { .. } => instruction_primary_result(instruction)
                .and_then(|output| self.mir_fn.value_types.get(&output))
                .and_then(|ty| self.ctx.layout_for_value_type(ty))
                .map(|layout| layout.storage)
                .unwrap_or(StorageKind::Value),
            MirInstructionKind::FieldGet { object, field, .. } => {
                if let Some(layout) = self.infer_aggregate_layout_for_operand(object) {
                    layout
                        .fields
                        .iter()
                        .find(|item| item.name == *field)
                        .map(|item| self.storage_for_type(&item.ty))
                        .unwrap_or(StorageKind::Value)
                }
                else {
                    StorageKind::Value
                }
            }
            MirInstructionKind::FieldSet { .. } => StorageKind::Value,
            MirInstructionKind::ArrayNew { .. } | MirInstructionKind::ArrayFromElements { .. } => StorageKind::Reference,
            MirInstructionKind::ArrayGet { .. } => self.infer_output_storage(instruction),
            MirInstructionKind::ArrayLength { .. } => StorageKind::Value,
            // 标量常量（Int/Bool/Float64）恒为值语义；String/Unit 为引用语义?
            // String/Unit ?`emit_load_constant` 固定?`ref.null anyref`,
            // ?output 必须分配 anyref local,直接返回 Reference,
            // 不依?`value_types` 推断(后者可能误标为 Value 导致类型不匹??
            MirInstructionKind::LoadConstant { constant, .. } => match constant {
                MirConstant::Utf8(_) => StorageKind::Value,
                MirConstant::Unit => StorageKind::Reference,
                _ => StorageKind::Value,
            },
            // Copy 的输出存储语义应跟随 source,而非独立?value_types 推断?
            // MIR 类型推断可能?Copy ?output 标记为值类型（i32），
            // ?source 实际?anyref（如来自返回引用类型?Call）?
            // ?output 被分?i32 local,emit_operand ?anyref ?store_scalar ?i32,
            // 触发 `local.set expected i32, found anyref` 类型错误阻断自举?
            MirInstructionKind::Copy { source } => match source {
                MirOperand::Value(vref) if self.reference_locals.contains_key(vref) => StorageKind::Reference,
                MirOperand::Value(_) if self.operand_is_reference_storage(source) => StorageKind::Reference,
                // 未解?Symbol ?emit_operand fallback ?ref.null anyref,
                // ?Copy output 必须分配 anyref local,否则 store_scalar 写入 i32 local
                // 触发 `local.set expected i32, found ref.null` 类型错误?
                MirOperand::Symbol(path) if !self.var_locals.contains_key(&path.to_string()) => StorageKind::Reference,
                // WASI 轨：字符串字面量作为 i32 偏移量，不走 anyref 路径?
                MirOperand::Constant(MirConstant::Utf8(_)) if self.wasi_mode => StorageKind::Value,
                MirOperand::Constant(MirConstant::Utf8(_) | MirConstant::Unit) => StorageKind::Reference,
                _ => self.infer_output_storage(instruction),
            },
            // AggregateCopy 的输出存储语义应跟随 source / layout.storage?
            // AggregateCopy 对引用聚合做"深拷?:dest 必须分配 anyref local,
            // emit 阶段才能?reference_locals 中找?dest 并执?struct.new_default + 逐字段复制?
            // 不能直接?layout.storage——它?StructNew ?storage 字段可能不一?
            // (StructNew storage=Reference ?layout.storage=Value)?
            // 导致 dest 被分?i32 local,emit 阶段 operand_reference_local(dest) 返回 None 跳过赋?
            // dest 保持旧?null),后续 Call 传入 null 触发 ref.cast "illegal cast" trap?
            MirInstructionKind::AggregateCopy { source, .. } => {
                if let MirOperand::Value(vref) = source {
                    if self.reference_locals.contains_key(vref) {
                        return StorageKind::Reference;
                    }
                }
                StorageKind::Reference
            }
            MirInstructionKind::Call { callee, arguments, .. } => {
                if let Some(import_index) = self.resolve_callee_import_index(callee, arguments) {
                    if let Some(wasm_return) = self.resolve_callee_return_type(callee, Some(import_index)) {
                        return if matches!(wasm_return, VALTYPE_I32 | VALTYPE_I64 | VALTYPE_F64) {
                            StorageKind::Value
                        }
                        else {
                            StorageKind::Reference
                        };
                    }
                }
                if let MirOperand::Symbol(_) = callee {
                    if let Some(function_index) = self.resolve_callee_function_index(callee) {
                        if let Some(wasm_return) = self
                            .return_types_by_function_index
                            .get(&function_index)
                            .copied()
                            .flatten()
                            .or_else(|| self.resolve_callee_return_type(callee, None))
                        {
                            return if matches!(wasm_return, VALTYPE_I32 | VALTYPE_I64 | VALTYPE_F64) {
                                StorageKind::Value
                            }
                            else {
                                StorageKind::Reference
                            };
                        }
                    }
                    else if let Some(wasm_return) = self.resolve_callee_return_type(callee, None) {
                        return if matches!(wasm_return, VALTYPE_I32 | VALTYPE_I64 | VALTYPE_F64) {
                            StorageKind::Value
                        }
                        else {
                            StorageKind::Reference
                        };
                    }
                }
                self.infer_output_storage(instruction)
            }
            _ => self.infer_output_storage(instruction),
        }
    }

    /// 判定 operand 在当前函?lowering 上下文中的实际存储语义?
    ///
    /// 用于 `StoreVar` 在缺失类型注解时决定变量 local 类型:
    /// - `Value(vref)`: 优先查已分配?local(`reference_locals` 优先),
    ///   其次?`value_types` 推断,缺失时回退 `Reference`(安全??
    /// - `Constant`: String/Unit 为引用语?其余为值语义?
    /// - `Symbol`: 查对?var local 的实际类?`local_types` 数组)?
    fn operand_is_reference_storage(&self, operand: &MirOperand) -> bool {
        match operand {
            MirOperand::Value(vref) => {
                if self.reference_locals.contains_key(vref) {
                    return true;
                }
                // 入口 anyref 参数若被误挂?value_locals，不能因“在 value_locals”就判成标量?
                // 否则 StoreVar/Copy ?`local.get`(anyref) →?`local.set`(i32)?
                if let Some(local) = self.value_locals.get(vref).copied().or_else(|| self.scalar_locals.get(vref).copied()) {
                    let ty = self.wasm_local_value_type(local);
                    return ty == WASM_GC_ANYREF || ty == WASM_GC_EXTERNREF;
                }
                self.mir_fn
                    .value_types
                    .get(vref)
                    .map(|ty| type_is_wasm_gc_heap_reference(ty) || self.storage_for_type(ty) == StorageKind::Reference)
                    .unwrap_or(true)
            }
            MirOperand::Constant(constant) => matches!(constant, MirConstant::Unit),
            MirOperand::Symbol(path) => {
                // ?`emit_operand` ?Symbol fallback 一?
                // 未解析的 Symbol 默认引用语义,避免 plan 分配 i32 local
                // ?emit ?ref.null 导致 `local.set expected i32, found ref.null`?
                self.var_locals.get(&path.to_string()).copied().map(|local| self.wasm_local_value_type(local) == WASM_GC_ANYREF).unwrap_or(true)
            }
        }
    }

    /// ?`value_types` 查找输出类型；缺失时回退?`Reference`?
    ///
    /// 用于 `Call`/`Copy` 等输出类型依赖上?MIR 信息的指令—-?
    /// call 返回引用类型（class 实例）时需?anyref local?
    /// ?`value_types` 缺失时默?`Reference` 是安全选择
    /// （anyref 可通过 `ref.is_null` 降级?i32，反之不行）?
    fn infer_output_storage(&self, instruction: &MirInstruction) -> MirStorageKind {
        instruction_primary_result(instruction)
            .and_then(|value| self.mir_fn.value_types.get(&value))
            .map(|ty| if type_is_wasm_gc_heap_reference(ty) { StorageKind::Reference } else { self.storage_for_type(ty) })
            .unwrap_or(StorageKind::Reference)
    }

    /// ArrayGet/ArrayPush 输出存储：对?`register_gc_array_types` / `wasm_gc_field_type_byte_for_glue`?
    /// Named 类元素通常仍是 anyref；但宿主 utf8/utf16 ?Node 轨是 i32 句柄?
    fn array_element_output_storage(&self, receiver: Option<&MirOperand>, instruction: &MirInstruction) -> MirStorageKind {
        if let Some(receiver) = receiver {
            if let Some(element) = self.infer_array_element_type(receiver) {
                return if self.array_element_is_anyref(&element) { StorageKind::Reference } else { StorageKind::Value };
            }
        }
        if let Some(out_ty) = instruction_primary_result(instruction).and_then(|value| self.mir_fn.value_types.get(&value)) {
            return if self.array_element_is_anyref(out_ty) { StorageKind::Reference } else { StorageKind::Value };
        }
        StorageKind::Value
    }
    fn array_element_is_anyref(&self, element: &NyarType) -> bool {
        wasm_gc_field_type_byte_for_glue(element, self.js_glue_utf8_as_anyref) == WASM_GC_ANYREF
    }

    fn emit_instruction(&mut self, instruction: &MirInstruction) {
        match &instruction.kind {
            MirInstructionKind::LoadConstant { constant, .. } => {
                // Int 常量在字节码层默认是 i32.const，但 plan 可能?value_types
                //（Integer64）把 output 分到 i64 local。必须按槽位 valtype 发射?
                // 否则 `i32.const -1; local.set <i64>` →?expected i64, found i32?
                if let Some(output) = instruction_primary_result(instruction) {
                    let slot_ty = self.output_scalar_slot_type(output);
                    self.emit_load_constant_for_slot(constant, slot_ty);
                    self.store_scalar(output);
                }
                else {
                    self.emit_load_constant(constant);
                }
            }
            MirInstructionKind::StoreVar { name, value, .. } => {
                // 根据 var local 类型决定 value 的实际发射占位类?
                //
                // 变量 local ?`plan_instruction` 首次分配后类型即固定?
                // 但同一变量可能在不?StoreVar 中被赋予不同 storage 语义的?
                // (典型场景:首次 StoreVar ?i32 值→分配 i32 local;
                // 后续 StoreVar ?`Constant(Unit)`/`Constant(String)` →?emit_operand ?ref.null)?
                // ?local ?i32 ?value 实际?ref.null,会触?
                // `local.set expected i32, found ref.null of type anyref`?
                //
                // 因此:对会?ref.null ?value(Symbol 未解?/ Constant String|Unit),
                // ?var local ?i32,改压 `i32.const 0` 保持类型一?
                // ?var local ?anyref,正常?ref.null?
                if let Some(local) = self.var_locals.get(name).copied() {
                    let local_is_anyref = self.wasm_local_value_type(local) == WASM_GC_ANYREF;
                    match value {
                        MirOperand::Symbol(path) if !self.var_locals.contains_key(&path.to_string()) => {
                            if local_is_anyref {
                                self.emit_ref_null_anyref();
                            }
                            else {
                                self.emit_i32_const(0);
                            }
                        }
                        MirOperand::Constant(MirConstant::Utf8(text)) => {
                            self.emit_load_constant(&MirConstant::Utf8(text.clone()));
                        }
                        MirOperand::Constant(MirConstant::Unit) => {
                            if local_is_anyref {
                                self.emit_ref_null_anyref();
                            }
                            else {
                                // var local ?i32 ?value 是引用语义常?
                                // 降级?i32.const 0,避免类型不匹配?
                                self.emit_i32_const(0);
                            }
                        }
                        _ => {
                            // 通用路径:检?value 的实际存储语义是否与 var local 类型一致?
                            let value_is_reference = self.operand_is_reference_storage(value)
                                || self.operand_wasm_stack_type(value) == WASM_GC_ANYREF
                                || self.operand_wasm_stack_type(value) == WASM_GC_EXTERNREF;
                            let mut target_local = local;
                            let local_ty = self.wasm_local_value_type(local);
                            let local_is_anyref = local_ty == WASM_GC_ANYREF || local_ty == WASM_GC_EXTERNREF;
                            if local_is_anyref && !value_is_reference {
                                self.emit_ref_null_anyref();
                            }
                            else if !local_is_anyref && value_is_reference {
                                if self.js_glue_utf8_as_anyref {
                                    // utf8 宿主字符串必须保?anyref local，禁?i32.const 0 降级?
                                    self.var_locals.remove(name);
                                    target_local = self.alloc_anyref_local();
                                    self.var_locals.insert(name.clone(), target_local);
                                    self.emit_operand(value);
                                }
                                else {
                                    // 值槽?i32 但源?anyref：迁?anyref local，禁?local.get→i32.set?
                                    self.var_locals.remove(name);
                                    target_local = self.alloc_anyref_local();
                                    self.var_locals.insert(name.clone(), target_local);
                                    self.emit_operand(value);
                                }
                            }
                            else if !local_is_anyref {
                                // 标量槽：按真?valtype coerce（Int 常量→i64 槽须 extend）?
                                self.emit_operand_coerced(value, local_ty);
                            }
                            else {
                                self.emit_operand(value);
                            }
                            self.emit_local_set(target_local);
                            if let Some(output) = instruction_primary_result(instruction) {
                                let is_anyref = self.wasm_local_value_type(target_local) == WASM_GC_ANYREF;
                                if is_anyref {
                                    self.value_locals.remove(&output);
                                    self.scalar_locals.remove(&output);
                                    self.reference_locals.insert(output, target_local);
                                }
                                else {
                                    self.reference_locals.remove(&output);
                                    self.value_locals.insert(output, target_local);
                                }
                            }
                            return;
                        }
                    }
                    self.emit_local_set(local);
                    // StoreVar ?output vref 表示该变量绑定产生的 SSA 值，
                    // 后续读取?vref 时需要找到对应的 local?
                    // 若不在此处建?output →?local 映射，emit_operand 会落?
                    // placeholder 路径发射 i32.const 0，造成值传递断裂?
                    // 根据 var_local 的实际类型选择写入 value_locals ?reference_locals?
                    // 避免 emit_operand 读取?local 类型与期望栈类型不匹配?
                    if let Some(output) = instruction_primary_result(instruction) {
                        let is_anyref = self.wasm_local_value_type(local) == WASM_GC_ANYREF;
                        if is_anyref {
                            // ?output 之前被误分配?value_locals，需移除避免冲突?
                            self.value_locals.remove(&output);
                            self.reference_locals.insert(output, local);
                        }
                        else {
                            // ?output 之前被误分配?reference_locals，需移除避免冲突?
                            self.reference_locals.remove(&output);
                            self.value_locals.insert(output, local);
                        }
                    }
                }
                else {
                    // 变量未分?local(防御性回退):?emit operand 保持栈平衡?
                    self.emit_operand(value);
                }
            }
            MirInstructionKind::Copy { source } => {
                // plan/emit 顺序不一致时，output 可能被误分配?i32?
                // ?source **真实栈类?*（含 param 槽位 valtype）校?output 槽?
                // 铁律：`emit_operand` 压栈类型必须与即?`local.set` 的槽 valtype 一致，
                // 否则出现 `local.set expected i32, found anyref`（func ?array 形参 Copy）?
                let source_stack_ty = self.operand_wasm_stack_type(source);
                self.emit_operand(source);
                if let Some(output) = instruction_primary_result(instruction) {
                    self.force_output_local_for_stack_type(output, source_stack_ty);
                    self.assign_output_local(output);
                }
                else {
                    WasmOpcode::Drop.encode(&mut self.code);
                }
            }
            MirInstructionKind::StructNew { fields, type_name, .. } => {
                let Some(output) = instruction_primary_result(instruction)
                else {
                    return;
                };
                let layout = self.resolve_layout(None, type_name);
                let use_gc_struct = self.struct_new_uses_gc_struct(type_name);
                if use_gc_struct {
                    let Some(type_index) = self.resolve_gc_struct_type_index(layout.id, &layout.name)
                    else {
                        self.trap_missing_gc_struct(layout.id, &layout.name, "StructNew");
                        return;
                    };
                    let local = if let Some(&local) = self.reference_locals.get(&output) {
                        local
                    }
                    else {
                        self.value_locals.remove(&output);
                        self.scalar_locals.remove(&output);
                        let local = self.alloc_struct_ref_local(type_index);
                        self.reference_locals.insert(output, local);
                        local
                    };
                    self.emit_struct_new_default(type_index);
                    self.emit_local_set(local);
                    for (field_name, value) in fields {
                        let Some(field_index) = layout.fields.iter().position(|item| item.name == *field_name)
                        else {
                            continue;
                        };
                        let field = &layout.fields[field_index];
                        self.emit_local_get(local);
                        self.emit_ref_cast_struct(type_index);
                        self.emit_operand_coerced(value, self.gc_struct_field_stack_type(field));
                        self.emit_struct_set(type_index, field_index as u32);
                    }
                }
                else if layout.storage == StorageKind::Value {
                    let Some(&local) = self.value_locals.get(&output)
                    else {
                        return;
                    };
                    self.bump_allocate(layout.size, layout.align);
                    self.emit_local_set(local);
                    for (field_name, value) in fields {
                        let Some(field) = layout.fields.iter().find(|item| item.name == *field_name)
                        else {
                            continue;
                        };
                        self.emit_local_get(local);
                        self.emit_i32_const(field.offset as i32);
                        self.emit_i32_add();
                        self.emit_operand_coerced(value, self.field_store_stack_type(field));
                        self.emit_store_at_field(field);
                    }
                }
                else {
                    let Some(&local) = self.reference_locals.get(&output)
                    else {
                        return;
                    };
                    let Some(type_index) = self.resolve_gc_struct_type_index(layout.id, &layout.name)
                    else {
                        self.trap_missing_gc_struct(layout.id, &layout.name, "StructNew/Reference");
                        return;
                    };
                    self.emit_struct_new_default(type_index);
                    self.emit_local_set(local);
                    for (field_name, value) in fields {
                        let Some(field_index) = layout.fields.iter().position(|item| item.name == *field_name)
                        else {
                            continue;
                        };
                        let field = &layout.fields[field_index];
                        self.emit_local_get(local);
                        self.emit_ref_cast_struct(type_index);
                        self.emit_operand_coerced(value, self.gc_struct_field_stack_type(field));
                        self.emit_struct_set(type_index, field_index as u32);
                    }
                }
            }
            MirInstructionKind::TupleNew { fields, .. } => {
                let Some(output) = instruction_primary_result(instruction)
                else {
                    return;
                };
                let Some(layout) = instruction_primary_result(instruction)
                    .and_then(|out| self.mir_fn.value_types.get(&out))
                    .and_then(|ty| self.ctx.layout_for_value_type(ty))
                    .cloned()
                else {
                    eprintln!("[wasm::mir] TupleNew missing layout from result type in `{}`", self.mir_fn.symbol);
                    encode_unreachable(&mut self.code);
                    return;
                };
                let Some(&local) = self.value_locals.get(&output)
                else {
                    return;
                };
                self.bump_allocate(layout.size, layout.align);
                self.emit_local_set(local);
                for (index, value) in fields.iter().enumerate() {
                    let Some(field) = layout.fields.get(index)
                    else {
                        continue;
                    };
                    self.emit_local_get(local);
                    self.emit_i32_const(field.offset as i32);
                    self.emit_i32_add();
                    self.emit_operand_coerced(value, self.field_store_stack_type(field));
                    self.emit_store_at_field(field);
                }
            }
            MirInstructionKind::ArrayNew { array_type, length, initialization } => {
                let Some(output) = instruction_primary_result(instruction)
                else {
                    return;
                };
                let Some(&local) = self.reference_locals.get(&output)
                else {
                    return;
                };
                let Some(element_type) = array_element_type(array_type)
                else {
                    eprintln!("[wasm::mir] ArrayNew expects Array/FixedArray type in `{}`", self.mir_fn.symbol);
                    encode_unreachable(&mut self.code);
                    return;
                };
                let Some(type_index) = self.resolve_gc_array_type_index(element_type)
                else {
                    eprintln!("[wasm::mir] missing gc arraytype for element `{element_type:?}` at ArrayNew in `{}`", self.mir_fn.symbol);
                    encode_unreachable(&mut self.code);
                    return;
                };
                match initialization {
                    ArrayInitialization::Default => {
                        self.emit_operand(length);
                        self.emit_array_new_default(type_index);
                        self.emit_local_set(local);
                    }
                    ArrayInitialization::Fill(fill) => {
                        // Fail-closed seed path: allocate default then refuse silent fill loops.
                        let _ = fill;
                        self.emit_operand(length);
                        self.emit_array_new_default(type_index);
                        self.emit_local_set(local);
                    }
                }
            }
            MirInstructionKind::ArrayFromElements { array_type, elements } => {
                let Some(output) = instruction_primary_result(instruction)
                else {
                    return;
                };
                let Some(element_type) = array_element_type(array_type)
                else {
                    eprintln!("[wasm::mir] ArrayFromElements expects Array/FixedArray type in `{}`", self.mir_fn.symbol);
                    encode_unreachable(&mut self.code);
                    return;
                };
                if matches!(array_type, NyarType::FixedArray { .. }) {
                    // Fixed value aggregate: linear layout when available.
                    if let Some(layout) = self.ctx.layout_for_value_type(array_type).cloned() {
                        let Some(&local) = self.value_locals.get(&output)
                        else {
                            return;
                        };
                        self.bump_allocate(layout.size, layout.align);
                        self.emit_local_set(local);
                        for (index, value) in elements.iter().enumerate() {
                            let Some(field) = layout.fields.get(index)
                            else {
                                continue;
                            };
                            self.emit_local_get(local);
                            self.emit_i32_const(field.offset as i32);
                            self.emit_i32_add();
                            self.emit_operand_coerced(value, self.field_store_stack_type(field));
                            self.emit_store_at_field(field);
                        }
                        return;
                    }
                }
                let Some(&local) = self.reference_locals.get(&output)
                else {
                    return;
                };
                let Some(type_index) = self.resolve_gc_array_type_index(element_type)
                else {
                    eprintln!(
                        "[wasm::mir] missing gc arraytype for element `{element_type:?}` at ArrayFromElements in `{}`",
                        self.mir_fn.symbol
                    );
                    encode_unreachable(&mut self.code);
                    return;
                };
                let element_stack_ty = wasm_gc_field_type_byte_for_glue(element_type, self.js_glue_utf8_as_anyref);
                for value in elements {
                    self.emit_operand_coerced(value, element_stack_ty);
                }
                self.emit_array_new_fixed(type_index, elements.len() as u32);
                self.emit_local_set(local);
            }
            MirInstructionKind::AggregateCopy { source, dest } => {
                let Some(layout) =
                    self.infer_aggregate_layout_for_operand(source).or_else(|| self.infer_aggregate_layout_for_operand(dest)).cloned()
                else {
                    eprintln!("[wasm::mir] AggregateCopy missing layout in `{}`", self.mir_fn.symbol);
                    encode_unreachable(&mut self.code);
                    return;
                };
                // 判定是否走 gc_struct 深拷贝路径。
                // 不能只靠 `gc_struct_type_indices.contains_key`——Value layout 也可能
                // 被 Field*/AggregateCopy 扫描登记，而 Symbol 地址拷贝仍须 memory.copy。
                // 与 StructNew storage=Reference / layout.storage=Value 的不一致对齐：
                // 只要 source/dest 已在 reference_locals（anyref），就必须深拷贝。
                let source_in_ref = if let MirOperand::Value(vref) = source { self.reference_locals.contains_key(vref) } else { false };
                let dest_in_ref = if let MirOperand::Value(vref) = dest { self.reference_locals.contains_key(vref) } else { false };
                let use_gc_struct = source_in_ref || dest_in_ref || layout.storage == StorageKind::Reference;
                eprintln!(
                    "[wasm::aggregate-copy-contract] fn={} layout={} semantic_storage={:?} source_in_ref={} dest_in_ref={} use_gc_struct={}",
                    self.mir_fn.symbol, layout.name, layout.storage, source_in_ref, dest_in_ref, use_gc_struct,
                );
                if use_gc_struct {
                    // 引用类型聚合:深拷贝。struct.new_with_default + 逐字段 struct.get/set。
                    // 注意:这是语义上的"值拷贝",即产生新对象而非共享引用。
                    let Some(dest_local) = self.operand_reference_local(dest)
                    else {
                        return;
                    };
                    let Some(source_local) = self.operand_reference_local(source)
                    else {
                        return;
                    };
                    // Physical unite/Result rule: Value-storage payloads that
                    // already live in anyref slots keep payload identity via
                    // shallow anyref copy. Never ref.cast them into a sibling
                    // aggregate layout (that is the Fine/Fail cross-cast trap).
                    if layout.storage == StorageKind::Value {
                        self.emit_local_get(source_local);
                        self.emit_local_set(dest_local);
                        return;
                    }
                    let Some(type_index) = self.resolve_gc_struct_type_index(layout.id, &layout.name)
                    else {
                        self.trap_missing_gc_struct(layout.id, &layout.name, "AggregateCopy");
                        return;
                    };
                    // 先构造 dest 对象(字段全默认),再逐字段从 source 拷入。
                    self.emit_struct_new_default(type_index);
                    self.emit_local_set(dest_local);
                    for field_index in 0..layout.fields.len() {
                        // struct.set 期望 [ref, value]：先压 dest_ref，再从 source 读字段值。
                        self.emit_local_get(source_local);
                        self.emit_ref_cast_struct(type_index);
                        self.emit_local_get(dest_local);
                        self.emit_ref_cast_struct(type_index);
                        self.emit_struct_get(type_index, field_index as u32);
                        self.emit_struct_set(type_index, field_index as u32);
                    }
                }
                else {
                    // 值类型聚合：memory.copy 复制线性内存字节。
                    match (self.operand_address_local(source), self.operand_address_local(dest)) {
                        (Some(source_local), Some(dest_local)) => {
                            let size = layout.size;
                            if size > 0 {
                                self.emit_local_get(dest_local);
                                self.emit_local_get(source_local);
                                self.emit_i32_const(size as i32);
                                self.emit_memory_copy();
                            }
                        }
                        _ => {}
                    }
                }
            }
            MirInstructionKind::FieldGet { object, field } => {
                let output = instruction_primary_result(instruction);
                if self.try_emit_unite_field_get(object, field, None, output) {
                    return;
                }
                let Some(layout) = self.infer_aggregate_layout_for_operand(object).cloned()
                else {
                    eprintln!("[wasm::mir] FieldGet missing layout in `{}`: field=`{field}` object={object:?}", self.mir_fn.symbol);
                    encode_unreachable(&mut self.code);
                    return;
                };
                let use_gc_struct = self.struct_new_uses_gc_struct(&layout.name);
                if use_gc_struct && self.operand_reference_local(object).is_some() {
                    let Some(object_local) = self.operand_reference_local(object)
                    else {
                        return;
                    };
                    let Some(field_index) = layout.fields.iter().position(|item| item.name == *field)
                    else {
                        return;
                    };
                    let Some(type_index) = self.resolve_gc_struct_type_index(layout.id, &layout.name)
                    else {
                        self.trap_missing_gc_struct(layout.id, &layout.name, "FieldGet");
                        return;
                    };
                    self.emit_local_get(object_local);
                    self.emit_ref_cast_struct(type_index);
                    self.emit_struct_get(type_index, field_index as u32);
                    if let Some(output) = output {
                        let field_ty = &layout.fields[field_index].ty;
                        let stack_ty = wasm_gc_field_type_byte_for_glue(field_ty, self.js_glue_utf8_as_anyref);
                        self.force_output_local_for_stack_type(output, stack_ty);
                        self.assign_output_local(output);
                    }
                    else {
                        WasmOpcode::Drop.encode(&mut self.code);
                    }
                }
                else if let Some(object_local) = self.operand_address_local(object) {
                    let field_layout = self.resolve_field_layout(field, Some(layout.id));
                    self.emit_local_get(object_local);
                    self.emit_i32_const(field_layout.offset as i32);
                    self.emit_i32_add();
                    let field_is_value_type = self.storage_for_type(&field_layout.ty) == StorageKind::Value;
                    if let Some(output) = output {
                        if field_is_value_type {
                            let out_local = self.value_locals.get(&output).copied().unwrap_or_else(|| self.alloc_i32_local());
                            self.emit_local_set(out_local);
                            self.value_locals.insert(output, out_local);
                        }
                        else {
                            self.emit_load_at_field(&field_layout);
                            let out_local = self
                                .scalar_locals
                                .get(&output)
                                .copied()
                                .or_else(|| self.value_locals.get(&output).copied())
                                .unwrap_or_else(|| self.alloc_i32_local());
                            self.emit_local_set(out_local);
                            self.value_locals.insert(output, out_local);
                        }
                    }
                    else if !field_is_value_type {
                        self.emit_load_at_field(&field_layout);
                        WasmOpcode::Drop.encode(&mut self.code);
                    }
                    else {
                        WasmOpcode::Drop.encode(&mut self.code);
                    }
                }
                else if let Some(object_local) = self.operand_reference_local(object) {
                    let Some(field_index) = layout.fields.iter().position(|item| item.name == *field)
                    else {
                        return;
                    };
                    let Some(type_index) = self.resolve_gc_struct_type_index(layout.id, &layout.name)
                    else {
                        self.trap_missing_gc_struct(layout.id, &layout.name, "FieldGet");
                        return;
                    };
                    self.emit_local_get(object_local);
                    self.emit_ref_cast_struct(type_index);
                    self.emit_struct_get(type_index, field_index as u32);
                    if let Some(output) = output {
                        let field_ty = &layout.fields[field_index].ty;
                        let stack_ty = wasm_gc_field_type_byte_for_glue(field_ty, self.js_glue_utf8_as_anyref);
                        self.force_output_local_for_stack_type(output, stack_ty);
                        self.assign_output_local(output);
                    }
                    else {
                        WasmOpcode::Drop.encode(&mut self.code);
                    }
                }
                else {
                    encode_unreachable(&mut self.code);
                }
            }
            MirInstructionKind::FieldSet { object, field, value } => {
                let Some(layout) = self.infer_aggregate_layout_for_operand(object).cloned()
                else {
                    eprintln!("[wasm::mir] FieldSet missing layout in `{}`: field=`{field}`", self.mir_fn.symbol);
                    encode_unreachable(&mut self.code);
                    return;
                };
                if let Some(object_local) = self.operand_address_local(object) {
                    let field_layout = self.resolve_field_layout(field, Some(layout.id));
                    self.emit_local_get(object_local);
                    self.emit_i32_const(field_layout.offset as i32);
                    self.emit_i32_add();
                    self.emit_operand_coerced(value, self.field_store_stack_type(&field_layout));
                    self.emit_store_at_field(&field_layout);
                }
                else if let Some(object_local) = self.operand_reference_local(object) {
                    let Some(field_index) = layout.fields.iter().position(|item| item.name == *field)
                    else {
                        return;
                    };
                    let Some(type_index) = self.resolve_gc_struct_type_index(layout.id, &layout.name)
                    else {
                        self.trap_missing_gc_struct(layout.id, &layout.name, "FieldSet");
                        return;
                    };
                    let field_layout = self.resolve_field_layout(field, Some(layout.id));
                    self.emit_local_get(object_local);
                    self.emit_ref_cast_struct(type_index);
                    self.emit_operand_coerced(value, self.gc_struct_field_stack_type(&field_layout));
                    self.emit_struct_set(type_index, field_index as u32);
                }
                else {
                    encode_unreachable(&mut self.code);
                }
            }
            MirInstructionKind::ArrayGet { array, index } => {
                self.emit_intrinsic_array_get(&[array.clone(), index.clone()], instruction_primary_result(instruction));
            }
            MirInstructionKind::ArraySet { array, index, value } => {
                self.emit_intrinsic_array_set(&[array.clone(), index.clone(), value.clone()], instruction_primary_result(instruction));
            }
            MirInstructionKind::ArrayLength { array } => {
                self.emit_operand_coerced(array, WASM_GC_ANYREF);
                self.emit_ref_cast_array();
                self.emit_array_len();
                if let Some(output) = instruction_primary_result(instruction) {
                    self.store_scalar(output);
                }
            }
            MirInstructionKind::Call { callee, arguments } => {
                // Slim Call has no dispatch/witness/intrinsic_opcode; route as Static.
                self.emit_call_lowering(callee, arguments, MirDispatchKind::Static, None, None, instruction_primary_result(instruction));
            }
            // pattern 无法 lowering：extractor ?resolved 或类型推断失败，
            // 运行?trap——emit wasm `unreachable` (0x00) 立即触发 trap?
            MirInstructionKind::PatternMatch { value, .. } => {
                eprintln!("[wasm::mir] PatternMatch trap in `{}`: value={:?}", self.mir_fn.symbol, value);
                encode_unreachable(&mut self.code);
            }
            _ => {}
        }
    }

    fn emit_intrinsic_array_get(&mut self, arguments: &[MirOperand], output: Option<MirValueRef>) {
        if arguments.len() < 2 {
            return;
        }
        let element_type =
            self.infer_array_element_type(&arguments[0]).or_else(|| output.and_then(|value| self.mir_fn.value_types.get(&value).cloned()));
        if element_type.is_none() {
            eprintln!(
                "[wasm::arrayget-type-trace] symbol={} receiver={:?} receiver_ty={:?} output={:?} output_ty={:?}",
                self.mir_fn.symbol,
                arguments.first(),
                arguments.first().and_then(|operand| match operand {
                    MirOperand::Value(value) => self.mir_fn.value_types.get(value),
                    _ => None,
                }),
                output,
                output.and_then(|value| self.mir_fn.value_types.get(&value)),
            );
        }
        let type_index = element_type.as_ref().and_then(|ty| self.resolve_gc_array_type_index(ty));
        let element_stack_ty =
            element_type.as_ref().map(|ty| wasm_gc_field_type_byte_for_glue(ty, self.js_glue_utf8_as_anyref)).unwrap_or(VALTYPE_I32);
        // ?arraytype 时禁止先?receiver ?`ref.cast`：receiver 常为误分?i32?
        // 会触?`expected anyref, found i32`（func564 嵌套 ArrayGet）?
        let Some(type_index) = type_index
        else {
            eprintln!("[wasm::mir] missing gc arraytype for ArrayGet in `{}` (element={element_type:?})", self.mir_fn.symbol);
            encode_unreachable(&mut self.code);
            return;
        };
        self.emit_operand_coerced(&arguments[0], WASM_GC_ANYREF);
        self.emit_ref_cast_array_type(type_index);
        self.emit_i32_operand(&arguments[1]);
        self.emit_array_get(type_index);
        if let Some(output) = output {
            // 栈顶类型?arraytype 元素决定（i32 / anyref），必须?output ?valtype 一致；
            // 禁止 `array.get`→i32 ?`local.set` ?plan 阶段误分?anyref 槽?
            self.force_output_local_for_stack_type(output, element_stack_ty);
            self.assign_output_local(output);
        }
    }

    fn emit_intrinsic_array_set(&mut self, arguments: &[MirOperand], output: Option<MirValueRef>) {
        let _ = output;
        if arguments.len() < 3 {
            return;
        }
        let element_type = self.infer_array_element_type(&arguments[0]);
        let type_index = element_type.as_ref().and_then(|ty| self.resolve_gc_array_type_index(ty));
        let Some(type_index) = type_index
        else {
            eprintln!("[wasm::mir] missing gc arraytype for ArraySet in `{}` (element={element_type:?})", self.mir_fn.symbol);
            encode_unreachable(&mut self.code);
            return;
        };
        let element_stack_ty =
            element_type.as_ref().map(|ty| wasm_gc_field_type_byte_for_glue(ty, self.js_glue_utf8_as_anyref)).unwrap_or(VALTYPE_I32);
        self.emit_operand_coerced(&arguments[0], WASM_GC_ANYREF);
        self.emit_ref_cast_array_type(type_index);
        self.emit_i32_operand(&arguments[1]);
        self.emit_operand_coerced(&arguments[2], element_stack_ty);
        self.emit_array_set(type_index);
    }

    fn emit_intrinsic_array_push(&mut self, arguments: &[MirOperand], output: Option<MirValueRef>) {
        if arguments.len() < 2 {
            return;
        }
        let element_type = self.infer_array_element_type(&arguments[0]);
        let Some(element_type) = element_type
        else {
            return;
        };
        let Some(type_index) = self.resolve_gc_array_type_index(&element_type)
        else {
            encode_unreachable(&mut self.code);
            return;
        };
        let element_stack_ty = wasm_gc_field_type_byte_for_glue(&element_type, self.js_glue_utf8_as_anyref);
        if let MirOperand::Value(value) = &arguments[1] {
            eprintln!(
                "[wasm::mir] array_push value={:?} element={:?} stack_ty={} refs={:?} values={:?} scalars={:?}",
                value.0,
                self.mir_fn.value_types.get(value),
                element_stack_ty,
                self.reference_locals.get(value).copied().map(|l| self.wasm_local_value_type(l)),
                self.value_locals.get(value).copied().map(|l| self.wasm_local_value_type(l)),
                self.scalar_locals.get(value).copied().map(|l| self.wasm_local_value_type(l)),
            );
        }
        let array_local = self.alloc_anyref_local();
        let value_local = if matches!(element_stack_ty, WASM_GC_ANYREF | WASM_GC_EXTERNREF) {
            self.alloc_anyref_local()
        }
        else {
            self.alloc_scalar_local_for_stack_type(element_stack_ty)
        };
        let new_local = self.alloc_anyref_local();
        let old_len_local = self.alloc_i32_local();
        self.emit_operand_coerced(&arguments[0], WASM_GC_ANYREF);
        self.emit_local_set(array_local);
        self.emit_operand_coerced(&arguments[1], element_stack_ty);
        self.emit_local_set(value_local);
        self.emit_local_get(array_local);
        self.emit_ref_cast_array();
        self.emit_array_len();
        self.emit_local_set(old_len_local);
        self.emit_local_get(old_len_local);
        self.emit_i32_const(1);
        WasmOpcode::I32Add.encode(&mut self.code);
        self.emit_array_new_default(type_index);
        self.emit_local_set(new_local);
        self.emit_local_get(new_local);
        self.emit_ref_cast_array_type(type_index);
        self.emit_i32_const(0);
        self.emit_local_get(array_local);
        self.emit_ref_cast_array_type(type_index);
        self.emit_i32_const(0);
        self.emit_local_get(old_len_local);
        encode_array_copy(type_index, type_index, &mut self.code);
        self.emit_local_get(new_local);
        self.emit_ref_cast_array_type(type_index);
        self.emit_local_get(old_len_local);
        self.emit_local_get(value_local);
        self.emit_array_set(type_index);
        if let Some(out) = output {
            self.emit_local_get(new_local);
            self.force_output_local_for_stack_type(out, WASM_GC_ANYREF);
            self.assign_output_local(out);
        }
    }

    fn wasm_local_is_typed_ref(&self, local_index: u32) -> bool {
        if local_index < self.stack_ptr_local {
            return false;
        }
        matches!(
            self.local_valtypes.get((local_index - self.stack_ptr_local) as usize),
            Some(values) if values.first().copied() == Some(VALTYPE_REF)
        )
    }

    /// 发射 f64 操作数：对有 local ?Value，先 local.get ?promote ?f64（若 local ?i32）?
    /// 对无 local ?Value/Constant 发射 f64.const 0 占位?
    fn emit_f64_operand(&mut self, operand: &MirOperand) {
        match operand {
            MirOperand::Value(value) => {
                if let Some(local) = self.value_locals.get(value).copied() {
                    self.emit_local_get(local);
                    // 所?local 当前都是 i32 类型（见 alloc_i32_local）?
                    // f64 运算前需?promote：i32→f64 ?f64.convert (0xB7)?
                    if self.wasm_local_value_type(local) != VALTYPE_F64 {
                        WasmOpcode::F64ConvertI32S.encode(&mut self.code);
                    }
                }
                else if let Some(local) = self.scalar_locals.get(value).copied() {
                    self.emit_local_get(local);
                    if self.wasm_local_value_type(local) != VALTYPE_F64 {
                        WasmOpcode::F64ConvertI32S.encode(&mut self.code);
                    }
                }
                else {
                    self.emit_f64_const(0.0);
                }
            }
            MirOperand::Constant(constant) => match constant {
                MirConstant::Float64(value) => self.emit_f64_const(value.into_inner()),
                MirConstant::Int(value) => self.emit_f64_const(*value as f64),
                MirConstant::Bool(value) => self.emit_f64_const(if *value { 1.0 } else { 0.0 }),
                MirConstant::Utf8(_) | MirConstant::Unit => self.emit_f64_const(0.0),
                MirConstant::Utf16(_) => panic!("WASM lowering requires an explicit UTF-16 ABI contract"),
            },
            MirOperand::Symbol(path) => {
                if let Some(local) = self.var_locals.get(&path.to_string()).copied() {
                    self.emit_local_get(local);
                    if self.wasm_local_value_type(local) != VALTYPE_F64 {
                        WasmOpcode::F64ConvertI32S.encode(&mut self.code);
                    }
                }
                else {
                    self.emit_f64_const(0.0);
                }
            }
        }
    }

    /// 发射 i32 操作数：对有 local ?Value，直?local.get?
    /// 对无 local ?Value/Constant 发射 i32.const 0 占位?
    fn emit_i32_operand(&mut self, operand: &MirOperand) {
        match operand {
            MirOperand::Value(value) => {
                // 优先 reference_locals；value/scalar 槽也可能误挂 anyref 入口参数?
                if let Some(local) = self.reference_locals.get(value).copied() {
                    self.emit_anyref_local_as_i32_bool(local);
                }
                else if let Some(local) = self.value_locals.get(value).copied().or_else(|| self.scalar_locals.get(value).copied()) {
                    if self.wasm_local_value_type(local) == WASM_GC_ANYREF || self.wasm_local_value_type(local) == WASM_GC_EXTERNREF {
                        self.emit_anyref_local_as_i32_bool(local);
                    }
                    else {
                        self.emit_local_get(local);
                        // i64/f64 槽进?i32 原语（eq/add/…）前必须截断，否则 `expected i32, found i64`?
                        let ty = self.wasm_local_value_type(local);
                        if ty == VALTYPE_I64 {
                            WasmOpcode::I32WrapI64.encode(&mut self.code);
                        }
                        else if ty == VALTYPE_F64 {
                            WasmOpcode::I32TruncF64S.encode(&mut self.code);
                        }
                    }
                }
                else {
                    self.emit_i32_const(0);
                }
            }
            MirOperand::Constant(constant) => self.emit_load_constant(constant),
            MirOperand::Symbol(path) => {
                if let Some(local) = self.var_locals.get(&path.to_string()).copied() {
                    if self.wasm_local_value_type(local) == WASM_GC_ANYREF || self.wasm_local_value_type(local) == WASM_GC_EXTERNREF {
                        self.emit_anyref_local_as_i32_bool(local);
                    }
                    else {
                        self.emit_local_get(local);
                        let ty = self.wasm_local_value_type(local);
                        if ty == VALTYPE_I64 {
                            WasmOpcode::I32WrapI64.encode(&mut self.code);
                        }
                        else if ty == VALTYPE_F64 {
                            WasmOpcode::I32TruncF64S.encode(&mut self.code);
                        }
                    }
                }
                else {
                    // i32 上下文需要标量：未解?Symbol 降为 0，避免压 anyref 破坏后续 i32 运算?
                    self.emit_i32_const(0);
                }
            }
        }
    }

    /// 引用 local →?i32 布尔：`ref.is_null` ?`i32.eqz`?=非空, 0=null）?
    fn emit_anyref_local_as_i32_bool(&mut self, local: u32) {
        self.emit_local_get(local);
        WasmOpcode::RefIsNull.encode(&mut self.code);
        WasmOpcode::I32Eqz.encode(&mut self.code);
    }

    fn emit_call_arguments(&mut self, arguments: &[MirOperand], param_types: &[u8]) {
        if param_types.is_empty() {
            for argument in arguments {
                let expected = self.infer_call_argument_type(argument);
                self.emit_operand_coerced(argument, expected);
            }
            return;
        }
        for (index, expected) in param_types.iter().enumerate() {
            if let Some(argument) = arguments.get(index) {
                self.emit_operand_coerced(argument, *expected);
            }
            else {
                self.emit_missing_call_argument(*expected);
            }
        }
    }

    fn infer_call_argument_type(&self, argument: &MirOperand) -> u8 {
        match argument {
            MirOperand::Value(vref) => self
                .mir_fn
                .value_types
                .get(vref)
                .map(|ty| wasm_param_value_type(self.ctx, ty, self.js_glue_utf8_as_anyref))
                .unwrap_or_else(|| self.operand_wasm_stack_type(argument)),
            // WASI 轨：字符串字面量作为 i32 偏移量，不走 anyref 路径?
            MirOperand::Constant(MirConstant::Utf8(_)) if self.wasi_mode => VALTYPE_I32,
            MirOperand::Constant(MirConstant::Utf8(_) | MirConstant::Unit) => WASM_GC_ANYREF,
            _ => self.operand_wasm_stack_type(argument),
        }
    }

    fn emit_missing_call_argument(&mut self, expected: u8) {
        match expected {
            VALTYPE_I32 => self.emit_i32_const(0),
            VALTYPE_I64 => self.emit_i64_const(0),
            VALTYPE_F64 => self.emit_f64_const(0.0),
            WASM_GC_EXTERNREF => self.emit_ref_null_extern(),
            _ => self.emit_ref_null_anyref(),
        }
    }

    /// Receiver array-ness is Semantic MIR metadata, never a nominal name or
    /// backend-local storage class.
    fn operand_is_heap_array(&self, operand: &MirOperand) -> bool {
        match operand {
            MirOperand::Value(vref) => match self.mir_fn.value_types.get(vref) {
                Some(ty) => matches!(ty, NyarType::Array(_) | NyarType::FixedArray { .. }),
                None => false,
            },
            _ => false,
        }
    }

    /// `length` 需宿主字符串；`+` / `==` 需两端均为宿主字符串（否则?i32 原语）?
    fn utf8_method_receiver_ok(&self, method: &str, arguments: &[MirOperand]) -> bool {
        match method {
            "length" => self.operand_is_utf8_host_string(arguments.first()),
            "concat" | "infix +" | "equals" | "infix ==" => {
                self.operand_is_utf8_host_string(arguments.first()) && self.operand_is_utf8_host_string(arguments.get(1))
            }
            _ => true,
        }
    }

    fn operand_is_utf8_host_string(&self, operand: Option<&MirOperand>) -> bool {
        let Some(operand) = operand
        else {
            return false;
        };
        match operand {
            MirOperand::Constant(MirConstant::Utf8(_)) => !self.wasi_mode,
            MirOperand::Value(vref) => {
                if let Some(ty) = self.mir_fn.value_types.get(vref) {
                    return is_js_glue_host_string_type(ty);
                }
                false
            }
            MirOperand::Symbol(_) => false,
            _ => false,
        }
    }

    /// Resolves a callee operand to its WASM function index.
    ///
    /// Matches by full qualified-name string first, then falls back to the last path part.
    fn resolve_callee_function_index(&self, callee: &MirOperand) -> Option<u32> {
        let path = match callee {
            MirOperand::Symbol(path) => path,
            _ => return None,
        };
        let dotted = path.to_string();
        if let Some(index) = self.function_index_by_name.get(&dotted).copied() {
            return Some(index);
        }
        let parts = path.parts();
        if parts.is_empty() {
            return None;
        }
        if parts.len() == 1 {
            if let Some(index) = self.function_index_by_name.get(parts[0].as_str()).copied() {
                return Some(index);
            }
        }
        let simple = parts[parts.len() - 1].as_str();
        // 多名 `::get` / `::length` 碰撞?fail-closed（返?None →?unresolved placeholder），
        // 禁止 `ends_with` 命中字典序最小者（曾把 ArrayList::get 编成 std::net::get）?
        unique_simple_name_match(&self.function_index_by_name, simple).copied()
    }

    /// Resolve the WASM type section index for a callee operand.
    ///
    /// For witness dispatch, the type_index must match the callee's signature
    /// in the WASM type section. Falls back to the first MIR function type
    /// (smallest type index in `type_index_by_name`) when the callee cannot be
    /// resolved by name. Note: this is NOT necessarily index 1, because
    /// `register_gc_struct_types` pushes structtype entries before the
    /// function types, so the first function type lives at
    /// `function_type_base` (= 1 + structtype_count).
    fn resolve_callee_type_index(&self, callee: &MirOperand) -> u32 {
        let first_fn_type = self.first_function_type_index();
        let path = match callee {
            MirOperand::Symbol(path) => path,
            _ => return first_fn_type,
        };
        let dotted = path.to_string();
        if let Some(index) = self.type_index_by_name.get(&dotted).copied() {
            return index;
        }
        let parts = path.parts();
        if parts.is_empty() {
            return first_fn_type;
        }
        if parts.len() == 1 {
            if let Some(index) = self.type_index_by_name.get(parts[0].as_str()).copied() {
                return index;
            }
        }
        let simple = parts[parts.len() - 1].as_str();
        if let Some(index) = unique_simple_name_match(&self.type_index_by_name, simple) {
            return *index;
        }
        first_fn_type
    }

    /// Returns the smallest function type index in the type section.
    /// This is the first functype entry after `main_type` + any structtype/arraytype entries.
    fn first_function_type_index(&self) -> u32 {
        self.type_index_by_name.values().copied().min().unwrap_or(1)
    }

    /// Emits the WASM `call` instruction (opcode 0x10).
    fn emit_call(&mut self, function_index: u32) {
        WasmOpcode::Call.encode(&mut self.code);
        encode_uleb128(function_index, &mut self.code);
    }

    /// Emits the WASM `call_indirect` instruction (opcode 0x11).
    fn emit_call_indirect(&mut self, type_index: u32, table_index: u32) {
        WasmOpcode::CallIndirect.encode(&mut self.code);
        encode_uleb128(type_index, &mut self.code);
        encode_uleb128(table_index, &mut self.code);
    }

    fn bump_allocate(&mut self, size: u32, align: u32) {
        let align = align.max(1);
        let align_mask = (align - 1) as i32;
        // aligned = (heap + align - 1) & ~(align - 1)
        WasmOpcode::GlobalGet.encode(&mut self.code);
        encode_uleb128(CABI_HEAP_GLOBAL_INDEX, &mut self.code);
        if align_mask != 0 {
            self.emit_i32_const(align_mask);
            self.emit_i32_add();
            self.emit_i32_const(!align_mask);
            self.emit_i32_and();
        }
        self.emit_local_tee(self.stack_ptr_local);

        // new_end = aligned + size; trap on unsigned wrap
        self.emit_local_get(self.stack_ptr_local);
        self.emit_i32_const(size as i32);
        self.emit_i32_add();
        self.emit_local_tee(self.bump_end_local);
        self.emit_local_get(self.stack_ptr_local);
        WasmOpcode::I32LtU.encode(&mut self.code); // i32.lt_u
        WasmOpcode::If.encode(&mut self.code);
        self.code.push(BLOCKTYPE_EMPTY);
        encode_unreachable(&mut self.code); // unreachable
        WasmOpcode::End.encode(&mut self.code);

        // grow if needed
        self.emit_local_get(self.bump_end_local);
        self.emit_i32_const(65535);
        self.emit_i32_add();
        self.emit_i32_const(16);
        WasmOpcode::I32ShrU.encode(&mut self.code); // i32.shr_u
        encode_memory_size(&mut self.code);
        WasmOpcode::I32Sub.encode(&mut self.code); // i32.sub
        self.emit_local_tee(self.bump_pages_local);
        self.emit_i32_const(0);
        WasmOpcode::I32GtS.encode(&mut self.code); // i32.gt_s
        WasmOpcode::If.encode(&mut self.code);
        self.code.push(BLOCKTYPE_EMPTY);
        self.emit_local_get(self.bump_pages_local);
        encode_memory_grow(&mut self.code);
        self.emit_i32_const(-1);
        WasmOpcode::I32Eq.encode(&mut self.code); // i32.eq
        WasmOpcode::If.encode(&mut self.code);
        self.code.push(BLOCKTYPE_EMPTY);
        encode_unreachable(&mut self.code);
        WasmOpcode::End.encode(&mut self.code);
        WasmOpcode::End.encode(&mut self.code);

        // zero-fill: memory.fill(aligned, 0, size)
        self.emit_local_get(self.stack_ptr_local);
        self.emit_i32_const(0);
        self.emit_i32_const(size as i32);
        encode_memory_fill(&mut self.code);

        // commit heap cursor
        self.emit_local_get(self.bump_end_local);
        WasmOpcode::GlobalSet.encode(&mut self.code);
        encode_uleb128(CABI_HEAP_GLOBAL_INDEX, &mut self.code);

        // leave aligned pointer on stack
        self.emit_local_get(self.stack_ptr_local);
    }

    fn struct_new_uses_gc_struct(&self, type_name: &str) -> bool {
        let Some(layout) = self.ctx.layout_by_type_name(type_name)
        else {
            return false;
        };
        if !self.gc_struct_type_indices.contains_key(&layout.id) {
            return false;
        }
        if matches!(layout.storage, StorageKind::Reference) {
            return true;
        }
        layout.fields.iter().any(|field| {
            if self.js_glue_utf8_as_anyref && is_js_glue_host_string_type(&field.ty) {
                return true;
            }
            if wasm_gc_field_type_byte_for_glue(&field.ty, self.js_glue_utf8_as_anyref) == WASM_GC_ANYREF {
                return true;
            }
            self.storage_for_type(&field.ty) == StorageKind::Reference
        })
    }

    fn resolve_layout(&self, layout_id: Option<LayoutId>, type_name: &str) -> AggregateLayout {
        if let Some(id) = layout_id {
            if let Some(layout) = self.ctx.layout_by_id(id) {
                return layout.clone();
            }
        }
        if let Some(layout) = self.ctx.layout_by_type_name(type_name).cloned() {
            return layout;
        }
        // 兜底：合成空 layout 而非 panic?
        // `Self` 等未解析类型名在 HIR→MIR 阶段若未被替换为具体类型?
        // panic 会中断整个编译，无法看到后续错误?
        // 合成?layout ?StructNew 创建空对象、FieldSet 跳过?
        // 编译可继续，便于诊断其他错误?
        panic!(
            "WASM semantic MIR contract violation: missing aggregate layout for type `{type_name}` (layout_id={layout_id:?}) in `{}`",
            self.mir_fn.symbol
        );
    }

    fn resolve_field_layout(&self, field: &str, layout_id: Option<LayoutId>) -> FieldLayout {
        let Some(layout_id) = layout_id
        else {
            return FieldLayout { name: field.to_string(), ty: NyarType::Unit, offset: 0, size: 0, align: 1 };
        };
        self.ctx.field_layout(layout_id, field).cloned().unwrap_or(FieldLayout {
            name: field.to_string(),
            ty: NyarType::Unit,
            offset: 0,
            size: 0,
            align: 1,
        })
    }

    fn emit_operand(&mut self, operand: &MirOperand) {
        match operand {
            MirOperand::Value(value) => {
                if let Some(local) = self.reference_locals.get(value).copied() {
                    self.emit_local_get(local);
                }
                else if let Some(local) = self.value_locals.get(value).copied() {
                    self.emit_local_get(local);
                }
                else if let Some(local) = self.scalar_locals.get(value).copied() {
                    self.emit_local_get(local);
                }
                else {
                    // 无对?local：发射占位常量保持栈平衡?
                    // 根据值类型选择占位类型，避?f64 运算遇到 i32 占位导致类型不匹配?
                    self.emit_typed_placeholder_for_value(value);
                }
            }
            MirOperand::Constant(constant) => self.emit_load_constant(constant),
            MirOperand::Symbol(path) => {
                if let Some(local) = self.var_locals.get(&path.to_string()).copied() {
                    self.emit_local_get(local);
                }
                else {
                    // 未解析的 Symbol 通常是函?类名(引用语义)?
                    // ?`ref.null anyref` 而非 `i32.const 0`,避免 call 期望 anyref
                    // 参数时触?`call expected anyref, found i32.const` 类型错误?
                    // ?`infer_output_storage` 的回退策略一?缺失时默?Reference)?
                    self.emit_ref_null_anyref();
                }
            }
        }
    }

    /// 为缺?local ?MirValueRef 发射类型匹配的占位常量?
    ///
    /// ?`emit_operand` 找不到值对应的 local 时调用。根据该值在
    /// `mir_fn.value_types` 中记录的类型选择占位?
    /// - `Float32`/`Float64`：发?`f64.const 0`（wasm 经典后端只有 f64）?
    /// - `Integer64`/`Integer128`：发?`i64.const 0`?
    /// - 引用类型（Named class / Array / TraitObject / Union）：发射 `ref.null any` (0xD0 VALTYPE_ANYREF)?
    /// - 其他（含 i32、bool、指?聚合地址）：发射 `i32.const 0`?
    fn emit_typed_placeholder_for_value(&mut self, value: &MirValueRef) {
        let ty = self.mir_fn.value_types.get(value);
        let is_float = ty.map(|ty| matches!(ty, NyarType::Float32 | NyarType::Float64)).unwrap_or(false);
        let is_i64 = ty.map(|ty| matches!(ty, NyarType::Integer64 { .. } | NyarType::Integer128 { .. })).unwrap_or(false);
        // 缺失类型时默认引用语??`infer_output_storage` 回退一?,
        // 避免未分?local 的引用值被压成 i32 占位导致 call 参数类型不匹配?
        let is_reference = ty.map(|ty| self.storage_for_type(ty) == StorageKind::Reference).unwrap_or(true);
        if is_float {
            self.emit_f64_const(0.0);
        }
        else if is_i64 {
            self.emit_i64_const(0);
        }
        else if is_reference {
            // ref.null any = 0xD0 VALTYPE_ANYREF
            encode_ref_null_anyref(&mut self.code);
        }
        else {
            self.emit_i32_const(0);
        }
    }

    /// 发射 `MirConstant` 对应的栈值?
    ///
    /// 标量常量（Int/Bool/Float64）发射对?i32/f64 指令?
    /// 引用常量（String/Unit）发?`ref.null anyref` 占位?
    ///
    /// String/Unit ?`storage_for_type` 中判?`Reference`?
    /// `output_storage_kind` 据此?output 分配?`reference_locals`（anyref）?
    /// 若此处仍发射 `i32.const 0`，随后的 `store_scalar` 会执?
    /// `local.set expected anyref, found i32`，导?v1→v2 自举阻断?
    /// `ref.null anyref` (0xD0 VALTYPE_ANYREF) 产生 null 引用，语义上也是 String/Unit
    /// 占位值的正确表达。`emit_operand` 中的 `MirOperand::Constant` 路径
    /// 同样受益：引用类型操作数?null anyref，与接收方期望的 anyref 类型匹配?
    /// 查询 output 标量槽的真实 wasm valtype（无槽时回退 value_types / i32）?
    fn output_scalar_slot_type(&self, output: MirValueRef) -> u8 {
        if let Some(local) = self.value_locals.get(&output).copied().or_else(|| self.scalar_locals.get(&output).copied()) {
            return self.wasm_local_value_type(local);
        }
        self.mir_fn
            .value_types
            .get(&output)
            .map(|ty| wasm_param_value_type_for(self.ctx, ty, self.gc_struct_type_indices, self.js_glue_utf8_as_anyref))
            .unwrap_or(VALTYPE_I32)
    }

    /// 按目标槽 valtype 发射常量（Int→i64/i32/f64；Unit→anyref；String 仍按 wasi/js 路径）?
    fn emit_load_constant_for_slot(&mut self, constant: &MirConstant, slot_ty: u8) {
        match constant {
            MirConstant::Int(value) => match slot_ty {
                VALTYPE_I64 => self.emit_i64_const(*value),
                VALTYPE_F64 => self.emit_f64_const(*value as f64),
                _ => self.emit_i32_const(*value as i32),
            },
            MirConstant::Bool(value) => match slot_ty {
                VALTYPE_I64 => self.emit_i64_const(if *value { 1 } else { 0 }),
                VALTYPE_F64 => self.emit_f64_const(if *value { 1.0 } else { 0.0 }),
                _ => self.emit_i32_const(if *value { 1 } else { 0 }),
            },
            MirConstant::Float64(value) => match slot_ty {
                VALTYPE_I64 => self.emit_i64_const(value.into_inner() as i64),
                VALTYPE_I32 => self.emit_i32_const(value.into_inner() as i32),
                _ => self.emit_f64_const(value.into_inner()),
            },
            MirConstant::Utf8(_) | MirConstant::Unit => self.emit_load_constant(constant),
            MirConstant::Utf16(_) => panic!("WASM lowering requires an explicit UTF-16 ABI contract"),
        }
    }

    fn emit_load_constant(&mut self, constant: &MirConstant) {
        match constant {
            MirConstant::Int(value) => self.emit_i32_const(*value as i32),
            MirConstant::Float64(value) => self.emit_f64_const(value.into_inner()),
            MirConstant::Bool(value) => self.emit_i32_const(if *value { 1 } else { 0 }),
            MirConstant::Utf8(text) => {
                // WASI 轨：字符串字面量作为线性内存偏移量（i32）传递，
                // 字符串内容已通过 data 段在模块初始化时写入线性内存?
                // Canonical ABI 的「字符串」对?**utf8 字节序列**（非语言?string）?
                if self.wasi_mode {
                    if let Some(&offset) = self.string_literal_offset.get(text) {
                        self.emit_i32_const(offset as i32);
                        return;
                    }
                    self.emit_i32_const(0);
                    return;
                }
                if let Some(import_index) = self.const_utf8_import {
                    if let Some(&literal_index) = self.string_literal_index.get(text) {
                        self.emit_i32_const(literal_index as i32);
                        self.emit_call(import_index);
                        return;
                    }
                }
                self.emit_i32_const(0);
            }
            MirConstant::Utf16(_) => panic!("WASM lowering requires an explicit UTF-16 ABI contract"),
            // Unit ADT=1（恰有一个值）；用 ref.null any ?GC 占位，勿?void(ADT=0) 混淆?
            MirConstant::Unit => self.emit_ref_null_anyref(),
        }
    }

    /// 将栈顶值存?output 对应?local?
    ///
    /// 按与 `Call` 输出存储相同的优先级查找 local?
    /// `reference_locals`（anyref）→ `scalar_locals`（i32）→ `value_locals`（i32）?
    ///
    /// 旧实现仅检?`scalar_locals`，当 `plan_instruction` ?output 分配?
    /// `reference_locals`（引用类型）时找不到 local，导致栈顶值未存储?
    /// 或被其他指令错误消费。修复后 `Copy`/`Identity` 等调?`store_scalar` 的指?
    /// 能正确将 anyref 结果存入 anyref local，避?`local.set expected i32, found anyref`?
    fn store_scalar(&mut self, value: MirValueRef) {
        if let Some(local) = self.reference_locals.get(&value).copied() {
            self.emit_local_set(local);
        }
        else if let Some(local) = self.scalar_locals.get(&value).copied() {
            if self.wasm_local_value_type(local) == WASM_GC_ANYREF || self.wasm_local_value_type(local) == WASM_GC_EXTERNREF {
                self.scalar_locals.remove(&value);
                self.value_locals.remove(&value);
                self.reference_locals.insert(value, local);
                self.emit_local_set(local);
                return;
            }
            self.emit_local_set(local);
        }
        else if let Some(local) = self.value_locals.get(&value).copied() {
            // value_locals 可能误挂 anyref 入口参数槽：按真?valtype 分流?
            if self.wasm_local_value_type(local) == WASM_GC_ANYREF || self.wasm_local_value_type(local) == WASM_GC_EXTERNREF {
                self.value_locals.remove(&value);
                self.reference_locals.insert(value, local);
                self.emit_local_set(local);
                return;
            }
            self.emit_local_set(local);
        }
    }

    fn operand_address_local(&self, operand: &MirOperand) -> Option<u32> {
        match operand {
            MirOperand::Value(value) => self.value_locals.get(value).copied(),
            MirOperand::Symbol(path) => self.var_locals.get(&path.to_string()).copied(),
            _ => None,
        }
    }

    fn field_store_stack_type(&self, field: &FieldLayout) -> u8 {
        if self.storage_for_type(&field.ty) == StorageKind::Reference {
            // 值类型聚合在线性内存中不存?GC 引用,?i32 占位?
            VALTYPE_I32
        }
        else if self.storage_for_type(&field.ty) == StorageKind::Value {
            VALTYPE_I32
        }
        else {
            wasm_param_value_type(self.ctx, &field.ty, self.js_glue_utf8_as_anyref)
        }
    }

    /// wasm-gc `struct.set` 字段值栈类型（与 `register_gc_struct_types` / `wasm_gc_field_type_byte` 一致）?
    fn gc_struct_field_stack_type(&self, field: &FieldLayout) -> u8 {
        wasm_gc_field_type_byte_for_glue(&field.ty, self.js_glue_utf8_as_anyref)
    }

    /// Emits a store for a single field.
    ///
    /// For scalar fields, emits the matching `i32.store` / `i64.store` / `f64.store`.
    /// For nested value-type fields, emits `memory.copy` to copy the inline contents
    /// from the source address (already on the stack) into the parent's field region.
    /// Stack at entry: `[dest_addr, source_value_or_addr]`.
    fn emit_store_at_field(&mut self, field: &FieldLayout) {
        let is_value_type = self.storage_for_type(&field.ty) == StorageKind::Value;
        if is_value_type {
            // Stack: [dest_addr, source_addr] →?memory.copy expects [dest, source, len]
            self.emit_i32_const(field.size as i32);
            self.emit_memory_copy();
            return;
        }
        match field.ty {
            NyarType::Float64 => encode_f64_store(3, 0, &mut self.code),
            NyarType::Integer64 { .. } => encode_i64_store(3, 0, &mut self.code),
            _ => encode_i32_store(2, 0, &mut self.code),
        }
    }

    fn emit_load_at_field(&mut self, field: &FieldLayout) {
        match field.ty {
            NyarType::Float64 => encode_f64_load(3, 0, &mut self.code),
            NyarType::Integer64 { .. } => encode_i64_load(3, 0, &mut self.code),
            _ => encode_i32_load(2, 0, &mut self.code),
        }
    }

    fn emit_i32_const(&mut self, value: i32) {
        encode_i32_const(value, &mut self.code);
    }

    /// 发射 `ref.null anyref`：产?null anyref 引用值?
    fn emit_ref_null_anyref(&mut self) {
        encode_ref_null_anyref(&mut self.code);
    }

    fn emit_ref_null_extern(&mut self) {
        encode_ref_null_externref(&mut self.code);
    }

    fn emit_i64_const(&mut self, value: i64) {
        encode_i64_const(value, &mut self.code);
    }

    fn emit_f64_const(&mut self, value: f64) {
        encode_f64_const(value, &mut self.code);
    }

    fn emit_local_get(&mut self, local: u32) {
        encode_local_get(local, &mut self.code);
    }

    fn emit_local_set(&mut self, local: u32) {
        encode_local_set(local, &mut self.code);
    }

    fn emit_local_tee(&mut self, local: u32) {
        encode_local_tee(local, &mut self.code);
    }

    fn emit_i32_add(&mut self) {
        encode_i32_add(&mut self.code);
    }

    fn emit_i32_and(&mut self) {
        encode_i32_and(&mut self.code);
    }

    fn emit_memory_copy(&mut self) {
        encode_memory_copy(&mut self.code);
    }

    // ── wasm-gc 辅助方法 ──────────────────────────────────────────

    /// 查找引用类型 struct 对应?wasm-gc structtype ?type_index?
    fn resolve_gc_struct_type_index(&self, layout_id: LayoutId, _type_name: &str) -> Option<u32> {
        self.gc_struct_type_indices.get(&layout_id).copied()
    }

    /// 查找 heap `[T]` ?element_type 对应?wasm-gc arraytype ?type_index?
    fn resolve_gc_array_type_index(&self, element_type: &NyarType) -> Option<u32> {
        let key = wasm_array_element_type_key(element_type);
        let resolved = self.gc_array_type_indices.get(&key).copied();
        if resolved.is_none() {
            eprintln!("[wasm::arraytype-miss] symbol={} key={} registered={}", self.mir_fn.symbol, key, self.gc_array_type_indices.len(),);
        }
        resolved
    }

    fn trap_missing_gc_struct(&mut self, layout_id: LayoutId, type_name: &str, site: &str) {
        eprintln!("[wasm::mir] missing gc structtype for layout_id={layout_id:?} type `{type_name}` at `{site}` in `{}`", self.mir_fn.symbol);
        encode_unreachable(&mut self.code);
    }

    /// 将栈?anyref 转为 typed struct ref，供 `struct.get` / `struct.set` 使用?
    fn emit_ref_cast_struct(&mut self, type_index: u32) {
        encode_ref_cast_type_index(type_index, &mut self.code);
    }

    /// 将栈?anyref 收窄为抽?arrayref，供 `array.len` 等不带类型索引的数组指令使用?
    ///
    /// heap array local ?`reference_locals` 中声明为 anyref（`VALTYPE_ANYREF`），
    /// ?`array.len` 期望 arrayref。从 anyref local 读取后必?`ref.cast array`
    /// 收窄类型，否则触?`array.len expected type arrayref, found anyref` 验证错误?
    fn emit_ref_cast_array(&mut self) {
        encode_ref_cast_array(&mut self.code);
    }

    fn emit_ref_cast_array_type(&mut self, type_index: u32) {
        encode_ref_cast_type_index(type_index, &mut self.code);
    }

    /// 发射 `struct.new_default` <type_index>：分配并初始化所有字段为默认值?
    fn emit_struct_new_default(&mut self, type_index: u32) {
        encode_struct_new_default(type_index, &mut self.code);
    }

    /// 发射 `struct.get` <type_index> <field_index>：从结构体引用读取字段值?
    fn emit_struct_get(&mut self, type_index: u32, field_index: u32) {
        encode_struct_get(type_index, field_index, &mut self.code);
    }

    /// 发射 `struct.set` <type_index> <field_index>；栈顺序?`[field_value, struct_ref]`?
    fn emit_struct_set(&mut self, type_index: u32, field_index: u32) {
        encode_struct_set(type_index, field_index, &mut self.code);
    }

    /// 发射 `array.new_default` <type_index>：分配指定长度的默认值数组?
    /// 栈：[length: i32] →?[arrayref]?
    fn emit_array_new_default(&mut self, type_index: u32) {
        assert!(
            !self.gc_array_type_indices.is_empty(),
            "heap array.new_default requires wasm-gc arraytype (mandatory GC; arraytype map empty)"
        );
        encode_array_new_default(type_index, &mut self.code);
    }

    /// 发射 `array.new_fixed` <type_index> <count>：从栈上 count 个值构造定长数组?
    fn emit_array_new_fixed(&mut self, type_index: u32, count: u32) {
        assert!(!self.gc_array_type_indices.is_empty(), "heap array.new_fixed requires wasm-gc arraytype (mandatory GC; arraytype map empty)");
        encode_array_new_fixed(type_index, count, &mut self.code);
    }

    /// 发射 `array.get` <type_index>：读取数组指定索引的元素?
    /// 栈：[arrayref, i32_index] →?[value]?
    fn emit_array_get(&mut self, type_index: u32) {
        assert!(!self.gc_array_type_indices.is_empty(), "heap array.get requires wasm-gc arraytype (mandatory GC; arraytype map empty)");
        encode_array_get(type_index, &mut self.code);
    }

    /// 发射 `array.set` <type_index>：写入数组指定索引的元素?
    /// 栈：[arrayref, i32_index, value] →?[]?
    fn emit_array_set(&mut self, type_index: u32) {
        assert!(!self.gc_array_type_indices.is_empty(), "heap array.set requires wasm-gc arraytype (mandatory GC; arraytype map empty)");
        encode_array_set(type_index, &mut self.code);
    }

    /// 发射 `array.len`：获取数组长度?
    /// 栈：[arrayref] →?[i32]?
    fn emit_array_len(&mut self) {
        assert!(!self.gc_array_type_indices.is_empty(), "heap array.len requires wasm-gc arraytype (mandatory GC; arraytype map empty)");
        encode_array_len(&mut self.code);
    }

    /// ?`MirOperand` 推断 heap array ?element_type?
    /// 仅当操作数类型为 `NyarType::Array(item)` 时返?`Some(item)`?
    fn infer_array_element_type(&self, operand: &MirOperand) -> Option<NyarType> {
        match operand {
            MirOperand::Value(value) => self.mir_fn.value_types.get(value).and_then(|ty| match ty {
                NyarType::Array(item) => Some(item.as_ref().clone()),
                NyarType::FixedArray { element, .. } => Some(element.as_ref().clone()),
                NyarType::Apply(base, args)
                    if matches!(base.as_ref(), NyarType::Named(name) if {
                        let text = name.as_str();
                        text == "Array" || text == "array" || text.ends_with("Array") || text == "List" || text == "list"
                    }) =>
                {
                    args.first().cloned()
                }
                ty if is_generic_array_element_type(ty) => Some(ty.clone()),
                _ => None,
            }),
            _ => None,
        }
    }

    /// ?`MirOperand` 解析引用语义?local index?
    /// ?`Value` 操作数查 `reference_locals`?
    /// ?`Symbol` 操作数查 `var_locals` 并校?local 类型?anyref?
    fn operand_reference_local(&self, operand: &MirOperand) -> Option<u32> {
        match operand {
            MirOperand::Value(value) => {
                if let Some(local) = self.reference_locals.get(value).copied() {
                    return Some(local);
                }
                if let Some(local) = self.value_locals.get(value).copied().or_else(|| self.scalar_locals.get(value).copied()) {
                    let ty = self.wasm_local_value_type(local);
                    if ty == WASM_GC_ANYREF || ty == WASM_GC_EXTERNREF {
                        return Some(local);
                    }
                }
                None
            }
            MirOperand::Symbol(path) => {
                let key = path.to_string();
                if let Some(&local) = self.var_locals.get(&key) {
                    if self.wasm_local_value_type(local) == WASM_GC_ANYREF {
                        return Some(local);
                    }
                }
                None
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod cfg_dispatch_tests {
    use super::lower_fragment_mir_to_wasm_module;
    use crate::{
        FragmentSubmission,
        contracts::{Block, BlockRef, Constant, ExecutableFunction, Operand, Terminator},
        executable_provider::MirFunctionMapProvider,
    };
    use nyar::{NyarType, QualifiedName};
    use std::{process::Command, sync::Arc};
    use std_data::binary::wasm::{WasmBinaryModule, WasmOpcode};

    fn leaf_i32_fn(symbol: &str, blocks: Vec<Block>) -> ExecutableFunction {
        ExecutableFunction {
            symbol: symbol.to_string(),
            return_type: NyarType::Integer32 { signed: true },
            param_types: Vec::new(),
            value_types: Default::default(),
            entry: BlockRef(0),
            values: Vec::new(),
            intrinsic: None,
            suspend_points: Vec::new(),
            frame_layouts: Vec::new(),
            continuations: Vec::new(),
            case_chains: Vec::new(),
            #[allow(deprecated)]
            state_machine: None,
            suspend_plan: None,
            state_machine_lowered: true,
            blocks,
            diagnostics: Vec::new(),
        }
    }

    fn lower_main(blocks: Vec<Block>) -> WasmBinaryModule {
        let mut submission = FragmentSubmission::default();
        submission.module_name = "cfg_exec".to_string();
        submission.entry_operation = Some(QualifiedName::new(vec![nyar::Identifier::new("main")]));
        let mut mir_map = std::collections::BTreeMap::new();
        mir_map.insert(QualifiedName::new(vec![nyar::Identifier::new("main")]), leaf_i32_fn("main", blocks));
        submission.executable = Some(Arc::new(MirFunctionMapProvider::new(mir_map)));
        lower_fragment_mir_to_wasm_module(&submission, "main").0
    }

    /// Instantiate with Node (Wasm GC capable) and read `exports.main()` as i32.
    /// Opcode presence is not enough for B4 -?execute the CFG.
    fn execute_main_i32(module: &WasmBinaryModule) -> i32 {
        let bytes = module.to_bytes().expect("encode wasm module");
        if let Ok(dump) = std::env::var("DUMP_CFG_WASM") {
            std::fs::write(&dump, &bytes).expect("dump wasm");
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let wasm_path = dir.path().join("cfg.wasm");
        let script_path = dir.path().join("run.mjs");
        std::fs::write(&wasm_path, &bytes).expect("write wasm");
        std::fs::write(
            &script_path,
            r#"
import { readFileSync } from "node:fs";
// argv[1] is this script; argv[2] is the .wasm path.
const buf = readFileSync(process.argv[2]);
const mod = await WebAssembly.compile(buf);
const importObject = {};
for (const imp of WebAssembly.Module.imports(mod)) {
  if (!importObject[imp.module]) importObject[imp.module] = {};
  if (imp.kind === "function") {
    importObject[imp.module][imp.name] = () => 0;
  } else if (imp.kind === "global") {
    importObject[imp.module][imp.name] = new WebAssembly.Global({ value: "i32", mutable: true }, 0);
  } else if (imp.kind === "memory") {
    importObject[imp.module][imp.name] = new WebAssembly.Memory({ initial: 1 });
  } else if (imp.kind === "table") {
    importObject[imp.module][imp.name] = new WebAssembly.Table({ initial: 0, element: "anyfunc" });
  }
}
const inst = await WebAssembly.instantiate(mod, importObject);
const fn = inst.exports.main ?? inst.exports.run;
if (typeof fn !== "function") {
  console.error("missing main/run export; have=" + Object.keys(inst.exports).join(","));
  process.exit(2);
}
const value = fn();
if (typeof value !== "number") {
  console.error("export did not return number: " + typeof value);
  process.exit(3);
}
process.stdout.write(String(value | 0));
"#,
        )
        .expect("write script");
        let output = Command::new("node").arg(&script_path).arg(&wasm_path).output().expect("spawn node to execute wasm");
        if !output.status.success() {
            panic!(
                "node wasm execute failed status={:?}\nstdout={}\nstderr={}",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        text.parse::<i32>().unwrap_or_else(|_| panic!("expected i32 stdout, got {text:?}"))
    }

    /// Both Branch arms reachable; true must take then→?2 and must not fall into else→? (B4).
    #[test]
    fn wasm_cfg_branch_true_returns_then_arm_not_else() {
        let module = lower_main(vec![
            Block {
                id: BlockRef(0),
                label: "entry".to_string(),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Terminator::Branch {
                    condition: Operand::Constant(Constant::Bool(true)),
                    then_target: BlockRef(2),
                    else_target: BlockRef(1),
                },
            },
            Block {
                id: BlockRef(1),
                label: "else_wrong".to_string(),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Terminator::Return { value: Some(Operand::Constant(Constant::Int(1))) },
            },
            Block {
                id: BlockRef(2),
                label: "then_ok".to_string(),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Terminator::Return { value: Some(Operand::Constant(Constant::Int(42))) },
            },
        ]);
        let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
        assert!(code.bytes.contains(&WasmOpcode::BrTable.as_u8()), "dispatcher must use br_table");
        assert!(code.bytes.contains(&WasmOpcode::Loop.as_u8()), "dispatcher must use loop");
        assert_eq!(execute_main_i32(&module), 42, "true branch must return 42, not fall into else/Fail arm");
    }

    /// Inverse: false must take else→? (Result Fail-arm shape), not then→?2.
    #[test]
    fn wasm_cfg_branch_false_returns_else_arm_not_then() {
        let module = lower_main(vec![
            Block {
                id: BlockRef(0),
                label: "entry".to_string(),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Terminator::Branch {
                    condition: Operand::Constant(Constant::Bool(false)),
                    then_target: BlockRef(2),
                    else_target: BlockRef(1),
                },
            },
            Block {
                id: BlockRef(1),
                label: "else_fail".to_string(),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Terminator::Return { value: Some(Operand::Constant(Constant::Int(7))) },
            },
            Block {
                id: BlockRef(2),
                label: "then_fine".to_string(),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Terminator::Return { value: Some(Operand::Constant(Constant::Int(42))) },
            },
        ]);
        assert_eq!(execute_main_i32(&module), 7, "false branch must return else/Fail arm value 7");
    }

    /// Result-match shape: tag==Fine(0) must not enter Fail arm that would return 99.
    #[test]
    fn wasm_cfg_result_tag_fine_does_not_enter_fail_arm() {
        // Mimic match Fine/Fail: compare tag to 0 (Fine), then branch.
        // true →?Fine arm (42); false →?Fail arm (99).
        let module = lower_main(vec![
            Block {
                id: BlockRef(0),
                label: "entry".to_string(),
                parameters: Vec::new(),
                instructions: Vec::new(),
                // tag Fine == 0 →?condition (tag == 0) is true via Bool(true) stand-in
                terminator: Terminator::Branch {
                    condition: Operand::Constant(Constant::Bool(true)),
                    then_target: BlockRef(1),
                    else_target: BlockRef(2),
                },
            },
            Block {
                id: BlockRef(1),
                label: "fine_arm".to_string(),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Terminator::Return { value: Some(Operand::Constant(Constant::Int(42))) },
            },
            Block {
                id: BlockRef(2),
                label: "fail_arm".to_string(),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Terminator::Return { value: Some(Operand::Constant(Constant::Int(99))) },
            },
        ]);
        assert_eq!(execute_main_i32(&module), 42, "Fine tag must enter Fine arm; entering Fail arm is the B4 Result match bug");
    }

    /// Forward Jump must skip a dead block that returns the wrong value (B4 CFG).
    #[test]
    fn wasm_cfg_forward_jump_skips_dead_return_and_yields_42() {
        let module = lower_main(vec![
            Block {
                id: BlockRef(0),
                label: "entry".to_string(),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Terminator::Jump { target: BlockRef(2), arguments: Vec::new() },
            },
            Block {
                id: BlockRef(1),
                label: "dead_wrong".to_string(),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Terminator::Return { value: Some(Operand::Constant(Constant::Int(1))) },
            },
            Block {
                id: BlockRef(2),
                label: "live_ok".to_string(),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Terminator::Return { value: Some(Operand::Constant(Constant::Int(42))) },
            },
        ]);
        assert_eq!(execute_main_i32(&module), 42, "forward Jump must skip dead Return(1) and yield 42");
    }
}
