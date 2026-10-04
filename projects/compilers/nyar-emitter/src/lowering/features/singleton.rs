//! Wasm singleton 元数据与访问器编码。

use crate::nyar_backend_wasi::WasmBinaryModule;
use nyar_types::{AggregateLayout, SingletonInstancePlan};

use crate::{FragmentSubmission, lowering::backends::wasm};








pub(crate) fn singleton_metadata_line(plan: &SingletonInstancePlan) -> String {
    let mode = if plan.is_lazy { "lazy" } else { "static" };
    let ctor = plan.constructor_symbol.as_deref().unwrap_or("-");
    let fin = plan.finalizer_symbol.as_deref().unwrap_or("-");
    let unload = if plan.supports_unload() { "unload" } else { "-" };
    format!("{}|{}|{}|{}|{}|{}|{}|{}", plan.namespace, plan.name, plan.instance_field, mode, plan.accessor_method(), ctor, fin, unload,)
}

fn nyar_singleton_export_name(plan: &SingletonInstancePlan) -> String {
    format!("{}__{}", plan.name, plan.accessor_method())
}

/// Builds the NyarVM export name for a lazy singleton's `unload` accessor.
pub(crate) fn nyar_singleton_unload_export_name(plan: &SingletonInstancePlan) -> String {
    format!("{}__{}", plan.name, nyar_types::SINGLETON_UNLOAD_ACCESSOR)
}

pub(crate) fn nyar_singleton_accessor_export_name(plan: &SingletonInstancePlan) -> String {
    nyar_singleton_export_name(plan)
}


fn singleton_layout<'a>(submission: &'a FragmentSubmission, plan: &SingletonInstancePlan) -> Option<&'a AggregateLayout> {
    submission.aggregate_layouts.layouts.iter().find(|layout| layout.name == plan.name && layout.namespace == plan.namespace)
}













/// Append legion singleton metadata sections for backends without dedicated singleton slots.
pub(crate) fn append_singleton_metadata_sections(module: &mut WasmBinaryModule, submission: &FragmentSubmission) {
    for plan in &submission.singleton_instances {
        let payload = singleton_metadata_line(plan);
        module.sections.insert(
            0,
            crate::nyar_backend_wasi::WasmSection { id: 0, name: Some(format!("legion.singleton.{}", plan.name)), bytes: payload.into_bytes() },
        );
    }
}

/// 为 WASM 模块注入 singleton INSTANCE 全局与 accessor 导出函数。
///
/// 对每个 singleton plan 生成：
/// - 一个可变 `i32` 全局，存储 INSTANCE 线性内存指针（与 heap global 共存；heap 占 index 0）。
/// - 一个导出函数 `{name}__{accessor}`：
///   - eager 模式：直接 `global.get` 返回全局。
///   - lazy 模式：判空后经 `cabi_realloc(0,0,align,size)` 真实 bump 分配，写回全局。
pub(crate) fn augment_wasm_with_singleton_accessors(module: &mut WasmBinaryModule, submission: &FragmentSubmission) {
    if submission.singleton_instances.is_empty() {
        return;
    }

    let realloc_index = ensure_wasm_cabi_realloc(module);

    let import_count = wasm::count_wasm_function_imports(module);
    let decl_count = wasm::count_wasm_function_decls(module);
    let first_new_function_index = import_count + decl_count;

    let accessor_type_index = wasm::count_wasm_types(module);
    let accessor_type = wasm::wasm_function_type(&[], &[0x7F]);
    wasm::append_wasm_types(module, &[accessor_type]);

    let singleton_count = submission.singleton_instances.len() as u32;
    let global_base = wasm::append_wasm_i32_globals(module, &vec![0; singleton_count as usize]);

    let type_indices: Vec<u32> = vec![accessor_type_index; singleton_count as usize];
    wasm::append_wasm_function_decls(module, &type_indices);

    let exports: Vec<(String, u8, u32)> = submission
        .singleton_instances
        .iter()
        .enumerate()
        .map(|(index, plan)| (nyar_singleton_export_name(plan), 0x00, first_new_function_index + index as u32))
        .collect();
    wasm::append_wasm_exports(module, &exports);

    let bodies: Vec<Vec<u8>> = submission
        .singleton_instances
        .iter()
        .enumerate()
        .map(|(index, plan)| {
            let layout = singleton_layout(submission, plan);
            let size = layout.map(|layout| layout.size.max(8)).unwrap_or(8);
            let align = layout.map(|layout| layout.align.max(1)).unwrap_or(8);
            wasm_singleton_accessor_body(global_base + index as u32, plan.is_lazy, realloc_index, size, align)
        })
        .collect();
    wasm::append_wasm_code_bodies(module, &bodies);
}

fn ensure_wasm_cabi_realloc(module: &mut WasmBinaryModule) -> u32 {
    if wasm::count_wasm_globals(module) == 0 {
        wasm::insert_wasm_section(module, wasm::cabi_heap_global_section(wasm::CABI_HEAP_DEFAULT_BASE));
    }
    if let Some(index) = find_wasm_export_func(module, "cabi_realloc") {
        return index;
    }
    let realloc_type = wasm::count_wasm_types(module);
    wasm::append_wasm_types(module, &[wasm::wasm_function_type(&[0x7F, 0x7F, 0x7F, 0x7F], &[0x7F])]);
    let func_index = wasm::count_wasm_function_imports(module) + wasm::count_wasm_function_decls(module);
    wasm::append_wasm_function_decls(module, &[realloc_type]);
    wasm::append_wasm_code_bodies(module, &[wasm::wasm_cabi_realloc_bump_body()]);
    wasm::append_wasm_exports(module, &[("cabi_realloc".to_string(), 0x00, func_index)]);
    func_index
}

fn find_wasm_export_func(module: &WasmBinaryModule, name: &str) -> Option<u32> {
    let section = module.sections.iter().find(|item| item.id == 7)?;
    let bytes = &section.bytes;
    let mut pos = 0;
    let count = wasm::decode_uleb128(bytes, &mut pos);
    for _ in 0..count {
        let name_len = wasm::decode_uleb128(bytes, &mut pos) as usize;
        let export_name = std::str::from_utf8(&bytes[pos..pos + name_len]).ok()?;
        pos += name_len;
        let kind = *bytes.get(pos)?;
        pos += 1;
        let index = wasm::decode_uleb128(bytes, &mut pos);
        if export_name == name && kind == 0x00 {
            return Some(index);
        }
    }
    None
}

/// 生成 WASM accessor 函数体字节。
///
/// eager：`global.get N; end`
/// lazy：判空后 `cabi_realloc(0,0,align,size)` 分配并写回。
fn wasm_singleton_accessor_body(global_index: u32, is_lazy: bool, realloc_index: u32, size: u32, align: u32) -> Vec<u8> {
    let mut body = vec![0x00];
    if is_lazy {
        body.push(0x23);
        wasm::encode_uleb128(global_index, &mut body);
        body.push(0x45);
        body.push(0x04);
        body.push(0x40);
        body.push(0x41);
        body.push(0x00);
        body.push(0x41);
        body.push(0x00);
        body.push(0x41);
        wasm::encode_sleb128_i32(align.max(1) as i32, &mut body);
        body.push(0x41);
        wasm::encode_sleb128_i32(size.max(8) as i32, &mut body);
        body.push(0x10);
        wasm::encode_uleb128(realloc_index, &mut body);
        body.push(0x24);
        wasm::encode_uleb128(global_index, &mut body);
        body.push(0x0B);
        body.push(0x23);
        wasm::encode_uleb128(global_index, &mut body);
    }
    else {
        body.push(0x23);
        wasm::encode_uleb128(global_index, &mut body);
    }
    body.push(0x0B);
    body
}












