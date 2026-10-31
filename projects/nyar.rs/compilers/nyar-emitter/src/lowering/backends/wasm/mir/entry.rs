//! Module-level entry functions.

use super::{
    super::{
        super::executable::ExecutableLoweringContext,
        cabi::{
            CABI_HEAP_GLOBAL_INDEX, cabi_heap_base_after_data, cabi_heap_global_section, memory_min_pages_for_heap_base,
            wasm_cabi_realloc_bump_body,
        },
        gc::{WASM_GC_ANYREF, wasm_gc_struct_type},
        sections::{
            code_section_bytes, data_section_bytes, decode_uleb128, elem_section_bytes, encode_sleb128_i32, encode_sleb128_i64, encode_uleb128,
            export_section_bytes, function_section_bytes, import_section_bytes, memory_section_bytes, table_section_bytes, type_section_bytes,
            wasm_function_body, wasm_function_type,
        },
    },
    MirInstructionKind, StorageKind, WASM_GC_EXTERNREF,
    imports::{
        build_callee_import_index, build_callee_wasm_operator_index, build_string_literal_index, build_wasi_callee_import_index,
        collect_mir_string_literals, collect_wasi_host_imports, collect_wasm_host_imports, const_utf8_import_index,
        host_import_signature_index, validate_string_literal_payloads, wasi_core_import_name, wasm_import_type_bytes_for_host,
    },
    library_glue,
    lowerer::lower_mir_function_to_wasm_bytes,
    representation::{
        is_js_glue_host_string_type, layout_needs_gc_struct, type_is_wasm_gc_heap_reference, wasm_param_types, wasm_return_value_type,
    },
    strings::{
        build_wasi_string_data_section, build_wasi_string_literal_offsets, wasi_string_data_section_size, wasm_function_type_param_bytes,
        wasm_function_type_result_byte,
    },
    type_registry::{register_gc_array_types, register_gc_struct_types, register_gc_sum_types, wasm_array_element_type_key},
};
use crate::{
    FragmentSubmission,
    backend_plan_views::NyarType,
    contracts::instruction_primary_result,
    nyar_backend_wasi::{WasiPreview, WasmBinaryModule, WasmPackageKind, WasmSection},
};
use acorn_wasm::{
    BLOCKTYPE_EMPTY, VALTYPE_ANYREF, VALTYPE_EXTERNREF, VALTYPE_F64, VALTYPE_I32, VALTYPE_I64, VALTYPE_REF_NULL, WasmExternalKind, WasmOpcode,
    encode_array_copy, encode_array_get, encode_array_len, encode_array_new_default, encode_array_new_fixed, encode_array_set,
    encode_block_empty, encode_br, encode_br_if, encode_call, encode_drop, encode_f64_const, encode_f64_load, encode_f64_store,
    encode_global_get, encode_global_set, encode_i32_add, encode_i32_and, encode_i32_const, encode_i32_load, encode_i32_store,
    encode_i64_const, encode_i64_load, encode_i64_store, encode_local_get, encode_local_set, encode_local_tee, encode_loop_empty,
    encode_memory_copy, encode_memory_fill, encode_memory_grow, encode_memory_size, encode_ref_cast_array, encode_ref_cast_type_index,
    encode_ref_null_anyref, encode_ref_null_externref, encode_struct_get, encode_struct_new_default, encode_struct_set, encode_unreachable,
};

use nyar_types::{AggregateLayout, FieldLayout, ItemInstanceId, LayoutId};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn build_param_types_by_instance(
    ctx: &ExecutableLoweringContext,
    submission: &FragmentSubmission,
    gc_struct_type_indices: &BTreeMap<LayoutId, u32>,
    js_glue_utf8_as_anyref: bool,
) -> BTreeMap<ItemInstanceId, Vec<u8>> {
    submission
        .backend_plan
        .instances()
        .into_iter()
        .map(|instance| {
            let view = submission.backend_plan.get_function(&instance).expect("Compiler 实例必须有函数体");
            (instance, wasm_param_types(ctx, &view.function, gc_struct_type_indices, js_glue_utf8_as_anyref))
        })
        .collect()
}

pub(super) fn build_return_types_by_instance(
    ctx: &ExecutableLoweringContext,
    submission: &FragmentSubmission,
    gc_struct_type_indices: &BTreeMap<LayoutId, u32>,
    js_glue_utf8_as_anyref: bool,
) -> BTreeMap<ItemInstanceId, Option<u8>> {
    submission
        .backend_plan
        .instances()
        .into_iter()
        .map(|instance| {
            let view = submission.backend_plan.get_function(&instance).expect("Compiler 实例必须有函数体");
            (instance, wasm_return_value_type(ctx, &view.function, gc_struct_type_indices, js_glue_utf8_as_anyref))
        })
        .collect()
}

pub(crate) fn lower_fragment_mir_to_wasm_module(
    submission: &FragmentSubmission,
    export_name: &str,
) -> crate::Result<(WasmBinaryModule, Vec<(String, String)>)> {
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
) -> crate::Result<(WasmBinaryModule, Vec<(String, String)>)> {
    // 存储类选择的唯一闸门：编码前拒绝无身份的名义类型残差。
    // 形态判断发生在 `storage_for_type` / `wasm_param_value_type` / GC 字段字节之前。
    crate::lowering::shared::executable::validate_storage_identities(submission).map_err(|error| {
        let owner = if error.function.is_empty() { error.site.clone() } else { format!("{} at {}", error.function, error.site) };
        format!("WASM pre-emission verifier: {owner} 携带无身份名义类型 `{}`；declaration / `TypeId` 缺失时不得按短名解析存储类", error.ty)
    })?;
    let wasi_mode = export_name == "_start";
    let operations = submission.backend_plan.instances();
    // 字符串字面量属于被发射模块本身，与导出名是 `main` 还是库导出无关；
    // 只有常量载体（`const_utf8` / WASI 偏移）在两种宿主形态下不同。
    let string_literals = collect_mir_string_literals(submission, &operations);
    let host_imports = if wasi_mode {
        collect_wasi_host_imports(submission, wasi_preview)
    }
    else if export_name == "main" {
        // 入口模块消费 plan 导入；确有字符串字面量时再合成 `const_utf8`。
        collect_wasm_host_imports(submission, !string_literals.is_empty())
    }
    else {
        // 库导出只补字符串字面量所需的宿主载体，保持既有导入表形状；
        // 文本编码身份不能因为导出名不是 `main` 就丢失。
        if string_literals.is_empty() { Vec::new() } else { vec![("env".to_string(), "const_utf8".to_string())] }
    };
    let import_count = u32::try_from(host_imports.len()).expect("import count overflow");
    let callee_import_index = if wasi_mode {
        build_wasi_callee_import_index(submission, &host_imports, wasi_preview)
    }
    else {
        build_callee_import_index(submission, &host_imports)
    };
    let callee_wasm_operator = build_callee_wasm_operator_index(submission);
    // Node JS-glue：宿主导入与 `.mjs` 启动壳统一使用 i32 句柄（`hostIntern` / `const_utf8`），
    // ?`wasm_import_type_for_field` ?i32 签名对齐。若?utf8 标成 anyref，call 实参?
    // 被降?`ref.null`，丢?`cli_get_*` 返回的句柄（自举 `exports.build` 会静默失败）?
    // WASI 轨同样以线性内?i32 偏移传字符串?
    let js_glue_utf8_as_anyref = false;
    let ctx = ExecutableLoweringContext::new(submission);
    let host_import_signatures = host_import_signature_index(submission);
    let mut module = WasmBinaryModule::new();
    // main_type ?type 段的?0 ?() -> i32，供 `build` 桩函数使用?
    // ?MIR 函数的类型从?1 项开始追加，各自携带独立的返回类型?
    let main_type = wasm_function_type(&[], &[VALTYPE_I32]);
    let mut type_indices = vec![main_type];
    let mut import_type_entries = Vec::new();
    for (module, field) in &host_imports {
        let import_type_index = u32::try_from(type_indices.len()).expect("type index overflow");
        let type_bytes =
            wasm_import_type_bytes_for_host(submission, &ctx, wasi_mode, module, field, js_glue_utf8_as_anyref, &host_import_signatures)?;
        type_indices.push(type_bytes);
        import_type_entries.push(import_type_index);
    }
    let mut function_indices = Vec::new();
    let mut code_bodies = Vec::new();
    let mut exports = Vec::new();
    eprintln!("[wasm::module-lower-start] export={export_name} operations={} host_imports={}", operations.len(), host_imports.len());
    // 语言规范：wasm/wasi 强制 wasm-gc。始终为引用 layout 注册 structtype?
    let gc_struct_type_indices = register_gc_struct_types(&ctx, &mut type_indices, js_glue_utf8_as_anyref);
    eprintln!("[wasm::module-stage] structs={} types={}", gc_struct_type_indices.len(), type_indices.len());
    // ?heap array element_type 注册 wasm-gc arraytype（Node/V8 ?WASI/wasmtime 均支持）?
    // 必须?glue 感知路径：`[utf8]` →?i32 元，禁止 Named→anyref 误登记?
    let gc_array_type_indices = register_gc_array_types(&ctx, &mut type_indices, js_glue_utf8_as_anyref);
    eprintln!("[wasm::module-stage] arrays={} types={}", gc_array_type_indices.len(), type_indices.len());
    // sum 的物理布局（tag + anyref payload）是后端私有决定，必须按 nominal identity 单独注册，
    // 让 `SumNew` 构造的对象与 `SumPayloadGet` / `SumVariantIs` 的 `ref.cast` 指向同一条目。
    let gc_sum_type_indices = register_gc_sum_types(&ctx, &mut type_indices);
    eprintln!("[wasm::module-stage] sums={} types={}", gc_sum_type_indices.len(), type_indices.len());
    // Scalar unite payloads such as utf8/bool/i32 handles need boxing into an
    // `[i32]` struct before entering the anyref payload slot.
    let gc_i32_box_type_index = {
        let type_index = u32::try_from(type_indices.len()).expect("type index overflow");
        type_indices.push(wasm_gc_struct_type(&[VALTYPE_I32]));
        type_index
    };
    let gc_i64_box_type_index = {
        let type_index = u32::try_from(type_indices.len()).expect("type index overflow");
        type_indices.push(wasm_gc_struct_type(&[VALTYPE_I64]));
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
    validate_string_literal_payloads(submission, &operations, &string_literal_index, &string_literal_offset, const_utf8_import, wasi_mode)?;
    // 仅对确有 MIR 体的 operation 分配稠密函数下标，避免「operations 枚举下标」与
    // `code_bodies.len()` 错位：错位时 call 会打到别人的 (param i64) 却按本函?anyref 签名?ref.null?
    let mir_operations: Vec<(nyar_types::ItemInstanceId, _)> = operations
        .iter()
        .filter_map(|operation| submission.backend_plan.get_function(operation).map(|view| (*operation, view.function)))
        .collect();
    eprintln!("[wasm::module-stage] mir_operations={} types={}", mir_operations.len(), type_indices.len());
    let base_function_index = import_count;
    let mut function_index_by_instance = BTreeMap::new();
    let mut type_index_by_instance = BTreeMap::new();
    let mut param_types_by_function_index: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
    let mut return_types_by_function_index: BTreeMap<u32, Option<u8>> = BTreeMap::new();
    for (dense, (instance, mir_fn)) in mir_operations.iter().enumerate() {
        let wasm_idx = base_function_index + dense as u32;
        let type_idx = function_type_base + dense as u32;
        let params = wasm_param_types(&ctx, mir_fn, &gc_struct_type_indices, js_glue_utf8_as_anyref);
        let ret = wasm_return_value_type(&ctx, mir_fn, &gc_struct_type_indices, js_glue_utf8_as_anyref);
        param_types_by_function_index.insert(wasm_idx, params);
        return_types_by_function_index.insert(wasm_idx, ret);
        function_index_by_instance.insert(*instance, wasm_idx);
        type_index_by_instance.insert(*instance, type_idx);
    }
    // 具名函数作值：预扫描全部可执行体，收集 `LoadFunctionRef` 引用的实例并按身份
    // 顺序登记 funcref table 槽位；只有被作值引用的函数才进 table。引用的实例没有
    // 函数体（导入或未生成）属于计划事实缺失，编码前结构化失败，禁止现场补造槽位。
    let mut function_value_instances: BTreeSet<ItemInstanceId> = BTreeSet::new();
    for (_, mir_fn) in mir_operations.iter() {
        for block in &mir_fn.blocks {
            for instruction in &block.instructions {
                if let MirInstructionKind::LoadFunctionRef { instance } = &instruction.kind {
                    function_value_instances.insert(*instance);
                }
            }
        }
    }
    let function_value_table_slots: BTreeMap<ItemInstanceId, u32> =
        function_value_instances.iter().copied().enumerate().map(|(slot, instance)| (instance, slot as u32)).collect();
    let function_value_table_functions: Vec<u32> = function_value_instances
        .iter()
        .map(|instance| {
            function_index_by_instance
                .get(instance)
                .copied()
                .ok_or_else(|| format!("WASM 函数值 `{instance:?}` 引用的实例没有可执行函数体"))
        })
        .collect::<crate::Result<Vec<_>>>()?;
    // 间接调用的 typeidx 只能反查模块已注册的 functype：按签名字节建表，
    // 禁止为间接调用新造类型条目或按参数个数猜签名。
    let mut type_index_by_signature: BTreeMap<(Vec<u8>, Option<u8>), u32> = BTreeMap::new();
    for (dense, (_, mir_fn)) in mir_operations.iter().enumerate() {
        let params = wasm_param_types(&ctx, mir_fn, &gc_struct_type_indices, js_glue_utf8_as_anyref);
        let ret = wasm_return_value_type(&ctx, mir_fn, &gc_struct_type_indices, js_glue_utf8_as_anyref);
        type_index_by_signature.entry((params, ret)).or_insert(function_type_base + dense as u32);
    }
    // Node JS-glue：宿主字符串?anyref 传递；?`wasm_import_type_for_field` ?anyref 签名对齐?
    let param_types_by_instance = build_param_types_by_instance(&ctx, submission, &gc_struct_type_indices, js_glue_utf8_as_anyref);
    let return_types_by_instance = build_return_types_by_instance(&ctx, submission, &gc_struct_type_indices, js_glue_utf8_as_anyref);
    let import_param_types: Vec<Vec<u8>> = host_imports
        .iter()
        .map(|(module, field)| -> crate::Result<Vec<u8>> {
            let type_bytes =
                wasm_import_type_bytes_for_host(submission, &ctx, wasi_mode, module, field, js_glue_utf8_as_anyref, &host_import_signatures)?;
            Ok(wasm_function_type_param_bytes(&type_bytes))
        })
        .collect::<crate::Result<Vec<_>>>()?;
    let import_return_types: Vec<Option<u8>> = host_imports
        .iter()
        .map(|(module, field)| -> crate::Result<Option<u8>> {
            let type_bytes =
                wasm_import_type_bytes_for_host(submission, &ctx, wasi_mode, module, field, js_glue_utf8_as_anyref, &host_import_signatures)?;
            Ok(wasm_function_type_result_byte(&type_bytes))
        })
        .collect::<crate::Result<Vec<_>>>()?;
    let mut mir_wasm_functions: Vec<(u32, String)> = Vec::new();

    for (dense, (instance, mir_fn)) in mir_operations.iter().enumerate() {
        let full = submission.backend_plan.abi_name_for_instance(*instance).ok_or_else(|| format!("函数实例缺少 ABI 标签"))?.to_string();
        eprintln!(
            "[wasm::function-lower-start] dense={dense}/{} symbol={} blocks={} instructions={}",
            mir_operations.len(),
            full,
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
        let wasm_index = import_count + u32::try_from(code_bodies.len()).expect("code body index overflow");
        code_bodies.push(wasm_function_body(lower_mir_function_to_wasm_bytes(
            submission,
            mir_fn,
            return_value_type,
            js_glue_utf8_as_anyref,
            wasi_mode,
            &function_index_by_instance,
            &type_index_by_instance,
            &param_types_by_instance,
            &return_types_by_instance,
            &param_types_by_function_index,
            &return_types_by_function_index,
            &import_param_types,
            &import_return_types,
            &gc_struct_type_indices,
            &gc_array_type_indices,
            &gc_sum_type_indices,
            gc_i32_box_type_index,
            gc_i64_box_type_index,
            &callee_import_index,
            &callee_wasm_operator,
            &host_imports,
            &string_literal_index,
            &string_literal_offset,
            const_utf8_import,
            &function_value_table_slots,
            &type_index_by_signature,
        )?));
        eprintln!("[wasm::function-lower-done] dense={dense}/{} symbol={full}", mir_operations.len());
        mir_wasm_functions.push((wasm_index, full.clone()));
        if let Some(public_name) = submission.backend_plan.wasm_export_names().get(instance) {
            exports.push((public_name.as_str(), WasmExternalKind::Func.as_u8(), wasm_index));
        }
        else if submission.backend_plan.entry_operation() == Some(*instance) {
            exports.push((export_name, WasmExternalKind::Func.as_u8(), wasm_index));
        }
    }

    if exports.is_empty() {
        return Err(format!("Wasm 缺少已绑定的入口或公开导出，禁止重发函数体或选择首函数"));
    }

    // WASI command world: wasmtime looks for `wasi:cli/run@-?run` returning result (i32).
    // Keep `_start` / short `run` for raw core runners; synthesize a result wrapper when needed.
    let wasi_cli_run_export = crate::nyar_backend_wasi::wasi_cli_run_export_name_for(wasi_preview);
    if wasi_mode {
        if let Some((_, _, index)) =
            exports.iter().find(|(name, kind, _)| *name == "_start" && *kind == WasmExternalKind::Func.as_u8()).copied()
        {
            // Entry may be `[] -> []` / `[] -> i32`, or still carry guest argv as GC array
            // (e.g. a guest entry taking `[utf8]` and returning unit). WIT `run: func() -> result` and core
            // `_start` are zero-arg. Synthesize nullary wrappers that push typed defaults
            // (empty `[utf8]` via `array.new_default`, not `ref.null` -?null traps on
            // `ref.cast`/`array.len`), call entry, drop any return, then Ok(0) for CLI run.
            // (Nonzero guest status belongs on `wasi:cli/exit`, not this empty `result`.)
            // Full argv via `get-arguments` cabi is follow-up; empty argv still exercises help.
            let entry_instance = submission.backend_plan.entry_operation().expect("WASI 入口必须保留 Compiler 实例身份");
            let entry_return = return_types_by_instance.get(&entry_instance).copied();
            let entry_mir_params: Vec<NyarType> = submission
                .backend_plan
                .entry_operation()
                .as_ref()
                .and_then(|instance| submission.backend_plan.get_function(instance))
                .map(|view| view.function.param_types.clone())
                .unwrap_or_default();
            let entry_wasm_params = param_types_by_function_index.get(&index).cloned().expect("WASI 入口缺少物理签名");

            let get_arguments_import = host_imports.iter().position(|(_, field)| field == "get-arguments").map(|index| index as u32);
            let argv_array_ty = gc_array_type_indices.get("Utf8").copied();
            let entry_needs_argv = entry_mir_params.iter().any(
                |ty| matches!(ty, NyarType::Array(element) | NyarType::FixedArray { element, .. } if is_js_glue_host_string_type(element.as_ref())),
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

    library_glue::append_library_mode_glue(wasm_package_kind, submission, &mut module)?;

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
    // funcref table：只为被作值引用的函数建表（id 4，必须排在 function(3) 与 memory(5) 之间）。
    if !function_value_table_functions.is_empty() {
        module.sections.push(table_section_bytes(u32::try_from(function_value_table_functions.len()).expect("funcref table 大小溢出")));
    }
    module.sections.push(memory_section_bytes(memory_min_pages_for_heap_base(heap_base)));
    module.sections.push(cabi_heap_global_section(heap_base));

    if !exports.iter().any(|(name, _, _)| *name == "memory") {
        exports.push(("memory", WasmExternalKind::Memory.as_u8(), 0));
    }
    if !exports.iter().any(|(name, _, _)| *name == "cabi_realloc") {
        exports.push(("cabi_realloc", WasmExternalKind::Func.as_u8(), realloc_func_index));
    }

    module.sections.push(export_section_bytes(&exports));
    // active elem 段：table 槽位 i 初始化为对应函数的下标（id 9，必须排在 export(7) 与 code(10) 之间）。
    if !function_value_table_functions.is_empty() {
        module.sections.push(elem_section_bytes(0, &function_value_table_functions));
    }
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
    Ok((module, host_imports))
}

pub(super) fn emit_wasi_entry_default_arg(ty: &NyarType, gc_array_type_indices: &BTreeMap<String, u32>, body: &mut Vec<u8>) {
    match ty {
        NyarType::Array(element) | NyarType::FixedArray { element, .. } => {
            let key = wasm_array_element_type_key(element);
            let type_index = gc_array_type_indices.get(&key).copied();
            if let Some(type_index) = type_index {
                encode_i32_const(0, body);
                encode_array_new_default(type_index, body);
            }
            else {
                panic!("WASI entry array parameter `{key}` 缺少已注册的 array type");
            }
        }
        NyarType::Float64 | NyarType::Float32 => encode_f64_const(0.0, body),
        NyarType::Integer64 { .. } | NyarType::Integer128 { .. } => encode_i64_const(0, body),
        NyarType::Unit => encode_ref_null_anyref(body),
        NyarType::Bottom => encode_i32_const(0, body),
        other if is_js_glue_host_string_type(other) => encode_i32_const(0, body),
        other if type_is_wasm_gc_heap_reference(other) => {
            panic!("WASI entry heap 参数 `{other:?}` 缺少 array type 合同")
        }
        _ => encode_i32_const(0, body),
    }
}

/// Out-pointer slot in the reserved `[0, WASI_STRING_DATA_OFFSET)` region for `get-arguments`.
pub(super) const WASI_GET_ARGUMENTS_OUT_PTR: i32 = 16;

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
    {
        let exec = &submission.backend_plan;
        for operation in exec.instances() {
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
                        MirInstructionKind::StructNew { nominal, .. } => {
                            if let Some(layout) = ctx.layout_by_nominal(*nominal) {
                                if layout.storage == StorageKind::Reference {
                                    record_missing(layout.id, &layout.name, "StructNew");
                                }
                            }
                        }
                        MirInstructionKind::FieldGet { field, .. } | MirInstructionKind::FieldSet { field, .. } => {
                            if let Some((layout, _, _)) = ctx.field_layout_by_id(*field) {
                                if layout.storage == StorageKind::Reference {
                                    record_missing(layout.id, &layout.name, "FieldAccess");
                                }
                            }
                        }
                        MirInstructionKind::AggregateCopy { layout_id, .. } => {
                            if let Some(layout) = ctx.layout_by_id(*layout_id) {
                                if layout.storage == StorageKind::Reference {
                                    record_missing(layout.id, &layout.name, "AggregateCopy");
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
    let mir_functions_len = submission.backend_plan.instances().len();
    if mir_functions_len == 0 {
        return;
    }
    let value_layout_count =
        submission.backend_plan.aggregate_layouts().layouts.iter().filter(|layout| layout.storage == StorageKind::Value).count();
    let reference_layout_count =
        submission.backend_plan.aggregate_layouts().layouts.iter().filter(|layout| layout.storage == StorageKind::Reference).count();
    let payload =
        format!("mir_functions={};value_layouts={};reference_layouts={}", mir_functions_len, value_layout_count, reference_layout_count);
    module.sections.push(WasmSection { id: 0, name: Some("nyar.value_aggregate".to_string()), bytes: payload.into_bytes() });
}
