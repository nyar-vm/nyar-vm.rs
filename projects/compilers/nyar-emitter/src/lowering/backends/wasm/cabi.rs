//! Canonical ABI linear heap: shared bump cursor for `cabi_realloc` and MIR allocations.
use crate::nyar_backend_wasi::WasmSection;
use std_data::binary::wasm::{
    VALTYPE_I32, WasmOpcode, encode_i32_and, encode_i32_const, encode_if_empty, encode_local_get, encode_local_set, encode_local_tee,
    encode_memory_copy, encode_memory_fill, encode_memory_grow, encode_memory_size, encode_return, encode_unreachable,
};

use super::sections::{encode_uleb128, global_section_with_i32_inits};

pub(crate) const LINEAR_HEAP_MIN_BASE: i32 = 4096;
pub(crate) const CABI_HEAP_DEFAULT_BASE: i32 = LINEAR_HEAP_MIN_BASE;
pub(crate) const CABI_HEAP_GLOBAL_INDEX: u32 = 0;

pub(crate) fn align_up_u32(value: u32, align: u32) -> u32 {
    let align = align.max(1);
    let mask = align - 1;
    (value + mask) & !mask
}

pub(crate) fn cabi_heap_base_after_data(data_len: usize) -> i32 {
    let aligned = align_up_u32(u32::try_from(data_len).unwrap_or(u32::MAX), 8);
    i32::try_from(aligned.max(LINEAR_HEAP_MIN_BASE as u32)).unwrap_or(LINEAR_HEAP_MIN_BASE)
}

/// Minimum linear-memory pages so static data + cabi heap cursor fit at instantiate.
/// `heap_base` is the first free byte; active data occupies `[0, heap_base)`.
pub(crate) fn memory_min_pages_for_heap_base(heap_base: i32) -> u32 {
    let bytes = u64::from(heap_base.max(LINEAR_HEAP_MIN_BASE) as u32);
    let pages = bytes.div_ceil(65536).max(1);
    u32::try_from(pages).unwrap_or(u32::MAX)
}

pub(crate) fn cabi_heap_global_section(heap_base: i32) -> WasmSection {
    debug_assert!(heap_base >= LINEAR_HEAP_MIN_BASE, "heap base must be >= {LINEAR_HEAP_MIN_BASE}");
    global_section_with_i32_inits(&[heap_base.max(LINEAR_HEAP_MIN_BASE)])
}

pub(crate) fn wasm_cabi_realloc_bump_body() -> Vec<u8> {
    let mut body = Vec::new();
    encode_uleb128(1, &mut body);
    encode_uleb128(4, &mut body);
    body.push(VALTYPE_I32);

    encode_local_get(3, &mut body);
    WasmOpcode::I32Eqz.encode(&mut body);
    encode_if_empty(&mut body);
    encode_i32_const(0, &mut body);
    encode_return(&mut body);
    WasmOpcode::End.encode(&mut body);

    encode_local_get(0, &mut body);
    WasmOpcode::I32Eqz.encode(&mut body);
    WasmOpcode::I32Eqz.encode(&mut body);
    encode_local_get(3, &mut body);
    encode_local_get(1, &mut body);
    WasmOpcode::I32LeU.encode(&mut body);
    encode_i32_and(&mut body);
    encode_if_empty(&mut body);
    encode_local_get(0, &mut body);
    encode_return(&mut body);
    WasmOpcode::End.encode(&mut body);

    encode_local_get(2, &mut body);
    encode_i32_const(1, &mut body);
    WasmOpcode::I32LtU.encode(&mut body);
    encode_if_empty(&mut body);
    encode_i32_const(1, &mut body);
    encode_local_set(2, &mut body);
    WasmOpcode::End.encode(&mut body);

    WasmOpcode::GlobalGet.encode(&mut body);
    encode_uleb128(CABI_HEAP_GLOBAL_INDEX, &mut body);
    encode_local_get(2, &mut body);
    WasmOpcode::I32Add.encode(&mut body);
    encode_i32_const(1, &mut body);
    WasmOpcode::I32Sub.encode(&mut body);
    encode_local_get(2, &mut body);
    encode_i32_const(1, &mut body);
    WasmOpcode::I32Sub.encode(&mut body);
    encode_i32_const(-1, &mut body);
    WasmOpcode::I32Xor.encode(&mut body);
    encode_i32_and(&mut body);
    encode_local_set(4, &mut body);

    encode_local_get(4, &mut body);
    encode_local_get(3, &mut body);
    WasmOpcode::I32Add.encode(&mut body);
    encode_local_tee(5, &mut body);
    encode_local_get(4, &mut body);
    WasmOpcode::I32LtU.encode(&mut body);
    encode_if_empty(&mut body);
    encode_unreachable(&mut body);
    WasmOpcode::End.encode(&mut body);

    encode_local_get(5, &mut body);
    encode_i32_const(65535, &mut body);
    WasmOpcode::I32Add.encode(&mut body);
    encode_i32_const(16, &mut body);
    WasmOpcode::I32ShrU.encode(&mut body);
    encode_memory_size(&mut body);
    WasmOpcode::I32Sub.encode(&mut body);
    encode_local_tee(6, &mut body);
    encode_i32_const(0, &mut body);
    WasmOpcode::I32GtS.encode(&mut body);
    encode_if_empty(&mut body);
    encode_local_get(6, &mut body);
    encode_memory_grow(&mut body);
    encode_i32_const(-1, &mut body);
    WasmOpcode::I32Eq.encode(&mut body);
    encode_if_empty(&mut body);
    encode_unreachable(&mut body);
    WasmOpcode::End.encode(&mut body);
    WasmOpcode::End.encode(&mut body);

    encode_local_get(4, &mut body);
    encode_i32_const(0, &mut body);
    encode_local_get(3, &mut body);
    encode_memory_fill(&mut body);

    encode_local_get(0, &mut body);
    WasmOpcode::I32Eqz.encode(&mut body);
    WasmOpcode::I32Eqz.encode(&mut body);
    encode_local_get(1, &mut body);
    WasmOpcode::I32Eqz.encode(&mut body);
    WasmOpcode::I32Eqz.encode(&mut body);
    encode_i32_and(&mut body);
    encode_if_empty(&mut body);
    encode_local_get(1, &mut body);
    encode_local_get(3, &mut body);
    encode_local_get(1, &mut body);
    encode_local_get(3, &mut body);
    WasmOpcode::I32LtU.encode(&mut body);
    WasmOpcode::Select.encode(&mut body);
    encode_local_set(7, &mut body);
    encode_local_get(4, &mut body);
    encode_local_get(0, &mut body);
    encode_local_get(7, &mut body);
    encode_memory_copy(&mut body);
    WasmOpcode::End.encode(&mut body);

    encode_local_get(5, &mut body);
    WasmOpcode::GlobalSet.encode(&mut body);
    encode_uleb128(CABI_HEAP_GLOBAL_INDEX, &mut body);
    encode_local_get(4, &mut body);
    WasmOpcode::End.encode(&mut body);
    body
}

#[cfg(test)]
mod cabi_realloc_tests {
    use super::{
        CABI_HEAP_DEFAULT_BASE, LINEAR_HEAP_MIN_BASE, align_up_u32, cabi_heap_base_after_data, cabi_heap_global_section,
        memory_min_pages_for_heap_base, wasm_cabi_realloc_bump_body,
    };
    use std_data::binary::wasm::{WasmMiscOpcode, WasmOpcode};

    #[test]
    fn bump_body_is_not_null_stub() {
        let body = wasm_cabi_realloc_bump_body();
        assert!(body.contains(&WasmOpcode::GlobalGet.as_u8()), "global.get");
        assert!(body.windows(2).any(|w| w == [WasmOpcode::MemorySize.as_u8(), 0]), "memory.size");
        assert!(body.windows(2).any(|w| w == [WasmOpcode::PrefixMisc.as_u8(), WasmMiscOpcode::MemoryCopy.as_u8()]), "memory.copy");
    }

    #[test]
    fn heap_global_initializes_past_static_data() {
        assert_eq!(cabi_heap_base_after_data(0), LINEAR_HEAP_MIN_BASE);
        assert_eq!(cabi_heap_base_after_data(4097), 4104);
        assert_eq!(align_up_u32(5, 4), 8);
        let section = cabi_heap_global_section(CABI_HEAP_DEFAULT_BASE);
        assert_eq!(section.id, 6);
    }

    #[test]
    fn memory_pages_cover_heap_base_past_one_page() {
        assert_eq!(memory_min_pages_for_heap_base(LINEAR_HEAP_MIN_BASE), 1);
        assert_eq!(memory_min_pages_for_heap_base(65536), 1);
        assert_eq!(memory_min_pages_for_heap_base(65537), 2);
        assert_eq!(memory_min_pages_for_heap_base(113864), 2);
    }

    #[test]
    fn bump_body_checks_overflow_and_zero_fills() {
        let body = wasm_cabi_realloc_bump_body();
        assert!(body.windows(2).any(|w| w == [WasmOpcode::I32LtU.as_u8(), WasmOpcode::If.as_u8()]));
        assert!(body.windows(2).any(|w| w == [WasmOpcode::PrefixMisc.as_u8(), WasmMiscOpcode::MemoryFill.as_u8()]));
    }
}
