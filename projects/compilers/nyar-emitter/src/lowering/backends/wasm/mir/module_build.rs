//! Assemble WasmBinaryModule from Semantic MIR fragments.
#![allow(deprecated)]

use std::collections::{BTreeMap, BTreeSet};

use super::*;

use super::module_imports::*;

pub(crate) fn lower_fragment_mir_to_wasm_module(
    submission: &FragmentSubmission,
    export_name: &str,
) -> (WasmBinaryModule, Vec<(String, String)>) {
    #[allow(deprecated)]
    {
        lower_fragment_mir_to_wasm_module_for(submission, export_name, crate::nyar_backend_wasi::WasiPreview::Preview2)
    }
}

/// See [`lower_fragment_mir_to_wasm_module`].
#[deprecated(note = "use Semantic MIR → prepare → WasmModuleModel → encode; not Wasm MIR")]
pub(crate) fn lower_fragment_mir_to_wasm_module_for(
    submission: &FragmentSubmission,
    export_name: &str,
    wasi_preview: crate::nyar_backend_wasi::WasiPreview,
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
        if submission.entry_operation.as_ref() == Some(operation) {
            // 导出下标与名称表一致（entry_matches ?== 物理下标）?
            let export_index = function_index_by_name.get(&operation.to_string()).copied().unwrap_or(wasm_index);
            exports.push((export_name, WasmExternalKind::Func.as_u8(), export_index));
        }
    }

    if exports.is_empty() {
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

pub(crate) fn is_generic_array_element_type(element_type: &NyarType) -> bool {
    matches!(element_type, NyarType::Named(name) if name.as_str().len() == 1 && name.as_str().chars().next().is_some_and(|ch| ch.is_ascii_uppercase()))
}

/// Prefer the `[utf8]` / i32-handle arraytype used for WASI/Node argv.
pub(crate) fn prefer_utf8_argv_array_type(gc_array_type_indices: &BTreeMap<String, u32>) -> Option<u32> {
    // Exact key only. Falling back to an arbitrary arraytype invents argv ABI (ADR 0008).
    gc_array_type_indices.get("Utf8").copied()
}

/// Push a typed default for a nullary WASI wrapper calling a still-parameterized entry.
///
/// `[T]` →?empty `array.new_default` (never `ref.null`: entry does `ref.cast`/`array.len`).
/// Scalars →?0; other references →?`ref.null any`.
pub(super) fn emit_wasi_entry_default_arg(ty: &NyarType, gc_array_type_indices: &BTreeMap<String, u32>, body: &mut Vec<u8>) {
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
pub(super) fn emit_wasi_argv_from_get_arguments(get_arguments_import: u32, array_ty: u32, body: &mut Vec<u8>) {
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

pub(super) fn append_wasm_spy_metadata_sections(
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
                        MirInstructionKind::StructNew { layout_id, type_name, storage, .. } if *storage == StorageKind::Reference => {
                            if let Some(id) = layout_id {
                                record_missing(*id, type_name, "StructNew");
                            }
                        }
                        MirInstructionKind::FieldGet { layout_id, storage, .. } | MirInstructionKind::FieldSet { layout_id, storage, .. }
                            if *storage == StorageKind::Reference =>
                        {
                            if let Some(id) = layout_id {
                                let type_name = ctx.layout_by_id(*id).map(|layout| layout.name.as_str()).unwrap_or("<unknown>");
                                record_missing(*id, type_name, "FieldAccess");
                            }
                        }
                        MirInstructionKind::AggregateCopy { layout_id, .. } => {
                            if let Some(layout) = ctx.layout_by_id(*layout_id) {
                                if layout.storage == StorageKind::Reference {
                                    record_missing(*layout_id, &layout.name, "AggregateCopy");
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
    let payload =
        format!("mir_functions={};value_layouts={};reference_layouts={}", mir_functions_len, value_layout_count, reference_layout_count);
    module.sections.push(WasmSection { id: 0, name: Some("nyar.value_aggregate".to_string()), bytes: payload.into_bytes() });
}

pub(super) fn lower_mir_function_to_wasm_bytes(
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
    #[allow(deprecated)]
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
