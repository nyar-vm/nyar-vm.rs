//! Host import / literal / type-map helpers for Wasm module build.
#![allow(deprecated)]

use std::collections::{BTreeMap, BTreeSet};

use super::*;

/// High-level compiler delegation is never a legal Node host import.
pub(super) fn is_forbidden_node_bridge(field: &str) -> bool {
    field == "host_legion_compile_from_plan" || field.starts_with("host_legion_")
}

/// 线性内?bump allocator 与字符串静态区共享的最低基址?
///
/// WASI 轨保?`[0, WASI_STRING_DATA_OFFSET)` 用于 argv 缓冲区与辅助变量?
/// 字符串字面量?`WASI_STRING_DATA_OFFSET` 开始存储；MIR/cabi 堆游标不得低于此值?
const WASI_STRING_DATA_OFFSET: u32 = LINEAR_HEAP_MIN_BASE as u32;

/// 收集 Node / JS-glue 轨的宿主导入?
///
/// - `cli_get_*` 等产?CLI 导入**必须**来自源码 `[wasm(...)]` 声明；禁止无条件注入?
///   否则每个 Node 模块都会?`.mjs` 启动壳判?CLI 模式（要?`help`/`version`/`build`）?
/// - `const_utf8` 仅在存在字符串字面量时合成，供启动壳?`nyar.strings` 解析句柄?
pub(super) fn collect_wasm_host_imports(submission: &FragmentSubmission, synthesize_const_utf8: bool) -> Vec<(String, String)> {
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
pub(super) fn collect_wasi_host_imports(submission: &FragmentSubmission, preview: crate::nyar_backend_wasi::WasiPreview) -> Vec<(String, String)> {
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
pub(super) fn expand_wasi_cli_stream_intrinsics(imports: &mut Vec<(String, String)>) {
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

pub(super) fn collect_mir_string_literals(submission: &FragmentSubmission, operations: &[QualifiedName]) -> Vec<String> {
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

pub(super) fn const_utf8_import_index(imports: &[(String, String)]) -> Option<u32> {
    imports.iter().position(|(module, field)| module == "env" && field == "const_utf8").map(|index| index as u32)
}

pub(super) fn build_string_literal_index(literals: &[String]) -> BTreeMap<String, u32> {
    literals.iter().enumerate().map(|(index, text)| (text.clone(), index as u32)).collect()
}

pub(super) fn wasm_import_type_for_field(field: &str) -> Vec<u8> {
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
        "print" | "print_line" | "error" | "error_line" => wasm_function_type(&[VALTYPE_I32], &[]),
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
pub(super) fn wasi_import_type_for_field(_module: &str, field: &str) -> Vec<u8> {
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

pub(super) fn wasi_core_import_name<'a>(module: &'a str, field: &'a str, preview: crate::nyar_backend_wasi::WasiPreview) -> (&'a str, &'a str) {
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

pub(super) fn build_callee_import_index(submission: &FragmentSubmission, imports: &[(String, String)]) -> BTreeMap<String, u32> {
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
pub(super) fn build_wasi_callee_import_index(
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
pub(super) fn build_wasi_string_literal_offsets(literals: &[String]) -> BTreeMap<String, u32> {
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
pub(super) fn wasi_string_data_section_size(literals: &[String]) -> u32 {
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
pub(super) fn build_wasi_string_data_section(literals: &[String]) -> Vec<u8> {
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

pub(super) fn wasm_function_type_param_bytes(type_bytes: &[u8]) -> Vec<u8> {
    if type_bytes.first() != Some(&std_data::binary::wasm::TYPE_FORM_FUNC) {
        return Vec::new();
    }
    let mut offset = 1usize;
    let param_count = decode_uleb128(type_bytes, &mut offset) as usize;
    let end = offset.saturating_add(param_count).min(type_bytes.len());
    type_bytes[offset..end].to_vec()
}

pub(super) fn wasm_function_type_result_byte(type_bytes: &[u8]) -> Option<u8> {
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

/// Short-name call target matching is forbidden (ADR 0008).
#[allow(dead_code)]
pub(super) fn unique_simple_name_match<'a, V>(_map: &'a BTreeMap<String, V>, _simple: &str) -> Option<&'a V> {
    None
}

pub(super) fn build_param_types_by_name(
    ctx: &ExecutableLoweringContext,
    submission: &FragmentSubmission,
    operations: &[QualifiedName],
    gc_struct_type_indices: &BTreeMap<LayoutId, u32>,
    js_glue_utf8_as_anyref: bool,
) -> BTreeMap<String, Vec<u8>> {
    let mut map = BTreeMap::new();
    for operation in operations {
        let Some(mir_fn) = submission.executable.as_ref().and_then(|exec| exec.get_function(operation)).map(|view| view.function)
        else {
            continue;
        };
        let params = wasm_param_types(&ctx, &mir_fn, gc_struct_type_indices, js_glue_utf8_as_anyref);
        // Exact qualified name only — no simple-name alias keys (ADR 0008).
        map.insert(operation.to_string(), params);
    }
    map
}

pub(super) fn build_return_types_by_name(
    ctx: &ExecutableLoweringContext,
    submission: &FragmentSubmission,
    operations: &[QualifiedName],
    gc_struct_type_indices: &BTreeMap<LayoutId, u32>,
    js_glue_utf8_as_anyref: bool,
) -> BTreeMap<String, Option<u8>> {
    let mut map = BTreeMap::new();
    for operation in operations {
        let Some(mir_fn) = submission.executable.as_ref().and_then(|exec| exec.get_function(operation)).map(|view| view.function)
        else {
            continue;
        };
        let return_type = wasm_return_value_type(ctx, &mir_fn, gc_struct_type_indices, js_glue_utf8_as_anyref);
        map.insert(operation.to_string(), return_type);
    }
    map
}
