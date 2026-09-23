//! Mandatory wasm-gc type builders for Valkyrie reference semantics on wasm/wasi.
use nyar::NyarType;
use std_data::binary::wasm::{
    VALTYPE_ANYREF, VALTYPE_F64, VALTYPE_I32, VALTYPE_I64, WasmValueType, encode_arraytype_raw, encode_structtype_raw,
};

/// Abbreviated nyref valtype byte (semantic alias into std-data).
pub(crate) const WASM_GC_ANYREF: u8 = VALTYPE_ANYREF;

/// WASM-GC structtype 类型段条目。
pub(crate) fn wasm_gc_struct_type(field_types: &[u8]) -> Vec<u8> {
    encode_structtype_raw(field_types)
}

/// WASM-GC rraytype 类型段条目（GC MVP）。
pub(crate) fn wasm_gc_array_type(element_type: u8) -> Vec<u8> {
    encode_arraytype_raw(element_type)
}

/// 将 NyarType 映射到 wasm-gc 字段值类型字节。
pub(crate) fn wasm_gc_field_type_byte(ty: &NyarType) -> u8 {
    match ty {
        NyarType::Float64 | NyarType::Float32 => VALTYPE_F64,
        NyarType::Integer64 { .. } | NyarType::Integer128 { .. } => VALTYPE_I64,
        NyarType::Named(_) | NyarType::Array(_) | NyarType::TraitObject(_) | NyarType::Union(_) => VALTYPE_ANYREF,
        _ => VALTYPE_I32,
    }
}

/// 将 NyarType 映射为语义值类型。
pub(crate) fn wasm_gc_field_value_type(ty: &NyarType) -> WasmValueType {
    WasmValueType::from_single_byte(wasm_gc_field_type_byte(ty)).unwrap_or(WasmValueType::I32)
}
