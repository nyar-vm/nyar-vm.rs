//! Wasm physical representation of NyarType / AggregateLayout.
//!
//! Owns unique storage and value-type rules for the Wasm GC backend:
//! Semantic MIR `StorageKind`, ABI param/return bytes, and GC field-type bytes.
//! This is Wasm **physical representation** only — Semantic MIR remains the
//! language type authority; backend only chooses Wasm encoding.
//!
//! Zero-logic extract from the former monolithic lowerer (B4 seam).

use super::{ExecutableLoweringContext, LayoutId, MirFunction, MirStorageKind, NyarType, StorageKind, WASM_GC_ANYREF};
use crate::lowering::backends::wasm::gc::wasm_gc_field_type_byte;
use nyar_types::AggregateLayout;
use std::collections::BTreeMap;
use std_data::binary::wasm::{VALTYPE_ANYREF, VALTYPE_F64, VALTYPE_I32, VALTYPE_I64};

pub(super) fn is_js_glue_host_string_type(ty: &NyarType) -> bool {
    // Only the explicit language encoding reaches the JS glue ABI. Nominal
    // names are not evidence of text semantics.
    matches!(ty, NyarType::Utf8)
}

pub(super) fn mir_storage_for_type(ctx: &ExecutableLoweringContext, ty: &NyarType, js_glue_utf8_as_anyref: bool) -> MirStorageKind {
    let _ = js_glue_utf8_as_anyref;
    if is_js_glue_host_string_type(ty) {
        return StorageKind::Value;
    }
    // Tagged unions are represented by GC structs on the WASM backend. Keep
    // their SSA values in anyref locals even when the nominal layout metadata
    // still reports a value-oriented payload representation; otherwise a
    // variant return can later be ref.cast from an i32 slot and trap with
    // `illegal cast`.
    if matches!(ty, NyarType::Union(_)) {
        return StorageKind::Reference;
    }
    ctx.storage_for_type(ty)
}

pub(super) fn type_uses_gc_struct_param(
    ctx: &ExecutableLoweringContext,
    ty: &NyarType,
    gc_struct_type_indices: &BTreeMap<LayoutId, u32>,
) -> bool {
    // 宿主 utf8/utf16 始终走 i32 句柄，即使同名 layout 被登记为 GC struct。
    if is_js_glue_host_string_type(ty) {
        return false;
    }
    let NyarType::Named(name) = ty
    else {
        return false;
    };
    let Some(layout) = ctx.layout_by_type_name(&name.to_string())
    else {
        return false;
    };
    if !gc_struct_type_indices.contains_key(&layout.id) {
        return false;
    }
    layout.storage == StorageKind::Reference || layout.fields.iter().any(|field| ctx.storage_for_type(&field.ty) == StorageKind::Reference)
}

pub(super) fn wasm_param_value_type_for(
    ctx: &ExecutableLoweringContext,
    ty: &NyarType,
    gc_struct_type_indices: &BTreeMap<LayoutId, u32>,
    js_glue_utf8_as_anyref: bool,
) -> u8 {
    // Node / JS-glue 宿主字符串必须是 i32 句柄，优先于「同名 GC struct layout」判定。
    // 否则 `utf8` 若被登记为 reference layout，call 实参会被 coerce 成 `ref.null`，
    // 丢掉 `cli_get_*` / `const_utf8` 返回的句柄。
    if is_js_glue_host_string_type(ty) {
        return VALTYPE_I32;
    }
    if type_uses_gc_struct_param(ctx, ty, gc_struct_type_indices) {
        return WASM_GC_ANYREF;
    }
    wasm_param_value_type(ctx, ty, js_glue_utf8_as_anyref)
}

pub(super) fn wasm_param_types(
    ctx: &ExecutableLoweringContext,
    mir_fn: &MirFunction,
    gc_struct_type_indices: &BTreeMap<LayoutId, u32>,
    js_glue_utf8_as_anyref: bool,
) -> Vec<u8> {
    mir_fn.param_types.iter().map(|ty| wasm_param_value_type_for(ctx, ty, gc_struct_type_indices, js_glue_utf8_as_anyref)).collect()
}

pub(super) fn wasm_gc_field_type_byte_for_glue(ty: &NyarType, js_glue_utf8_as_anyref: bool) -> u8 {
    // Node/WASI 宿主字符串默认是 i32 句柄（`const_utf8` / `utf8_*`）；
    // 仅当显式打开 js_glue_utf8_as_anyref 时才把 utf8/utf16 放进 anyref 字段/数组元。
    if is_js_glue_host_string_type(ty) {
        return if js_glue_utf8_as_anyref { VALTYPE_ANYREF } else { VALTYPE_I32 };
    }
    wasm_gc_field_type_byte(ty)
}

/// wasm/wasi 强制 GC：heap `[T]` / fixed array 在 layout 里常为 Value，
/// 但 lowering 必须用 anyref + `array.*`，不能分配 i32 槽后 `ref.cast`。
pub(super) fn type_is_wasm_gc_heap_reference(ty: &NyarType) -> bool {
    matches!(ty, NyarType::Array(_) | NyarType::FixedArray { .. })
}

/// Maps a MIR parameter type to its WASM value-type byte.
pub(super) fn wasm_param_value_type(ctx: &ExecutableLoweringContext, ty: &NyarType, js_glue_utf8_as_anyref: bool) -> u8 {
    let _ = js_glue_utf8_as_anyref;
    if matches!(ty, NyarType::Utf16) {
        panic!("WASM lowering requires an explicit UTF-16 ABI contract; it must not use the UTF-8/JS-string path")
    }
    if is_js_glue_host_string_type(ty) {
        return VALTYPE_I32;
    }
    if type_is_wasm_gc_heap_reference(ty) {
        return WASM_GC_ANYREF;
    }
    // Aggregates carrying references are represented by wasm-gc structs.
    // Keep their ABI as anyref even when stale nominal layout metadata says
    // Value; otherwise callers ref.cast an i32 return and trap.
    if matches!(ty, NyarType::Union(_)) {
        return WASM_GC_ANYREF;
    }
    if let NyarType::Named(name) = ty {
        if let Some(layout) = ctx.layout_by_type_name(&name.to_string()) {
            let has_ref_fields = layout.fields.iter().any(|field| ctx.storage_for_type(&field.ty) == StorageKind::Reference);
            if layout.storage == StorageKind::Reference || has_ref_fields {
                return WASM_GC_ANYREF;
            }
        }
    }
    // 引用类型参数用 anyref (VALTYPE_ANYREF) 传递;值类型参数仍用 i32 地址。
    if ctx.storage_for_type(ty) == StorageKind::Reference {
        return WASM_GC_ANYREF;
    }
    match ty {
        NyarType::Float64 | NyarType::Float32 => VALTYPE_F64,
        NyarType::Integer64 { .. } | NyarType::Integer128 { .. } => VALTYPE_I64,
        _ => VALTYPE_I32,
    }
}

/// 计算 MIR 函数返回值对应的 WASM 值类型字节。
pub(super) fn wasm_return_value_type(
    ctx: &ExecutableLoweringContext,
    mir_fn: &MirFunction,
    gc_struct_type_indices: &BTreeMap<LayoutId, u32>,
    js_glue_utf8_as_anyref: bool,
) -> Option<u8> {
    match &mir_fn.return_type {
        NyarType::Unit | NyarType::Bottom => None,
        other => Some(wasm_param_value_type_for(ctx, other, gc_struct_type_indices, js_glue_utf8_as_anyref)),
    }
}

pub(super) fn layout_needs_gc_struct(ctx: &ExecutableLoweringContext, layout: &AggregateLayout) -> bool {
    let has_ref_fields = layout.fields.iter().any(|field| ctx.storage_for_type(&field.ty) == StorageKind::Reference);
    layout.storage == StorageKind::Reference || has_ref_fields
}
