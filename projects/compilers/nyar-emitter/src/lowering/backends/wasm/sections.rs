//! Wasm binary section codecs and module mutation helpers.
use crate::nyar_backend_wasi::{WasmBinaryModule, WasmSection};
use std_data::binary::wasm::{
    FIELD_MUTABLE, LIMITS_HAS_MAX, LIMITS_MIN_ONLY, TYPE_FORM_FUNC, VALTYPE_FUNCREF, VALTYPE_I32, WasmExternalKind, WasmOpcode,
    encode_functype_raw, encode_i32_const, encode_uleb128 as std_encode_uleb128, write_sleb128_i32,
};

pub(crate) fn wasm_function_type(params: &[u8], results: &[u8]) -> Vec<u8> {
    encode_functype_raw(params, results)
}

pub(crate) fn type_section_bytes(types: Vec<Vec<u8>>) -> WasmSection {
    let mut bytes = Vec::new();
    encode_uleb128(u32::try_from(types.len()).unwrap(), &mut bytes);
    for ty in types {
        bytes.extend_from_slice(&ty);
    }
    WasmSection { id: 1, name: None, bytes }
}

pub(crate) fn import_section_bytes(imports: &[(&str, &str, u32)]) -> WasmSection {
    let mut bytes = Vec::new();
    encode_uleb128(u32::try_from(imports.len()).unwrap(), &mut bytes);
    for (module, field, type_index) in imports {
        encode_name(module, &mut bytes);
        encode_name(field, &mut bytes);
        bytes.push(WasmExternalKind::Func.as_u8());
        encode_uleb128(*type_index, &mut bytes);
    }
    WasmSection { id: 2, name: None, bytes }
}

pub(crate) fn function_section_bytes(type_indices: &[u32]) -> WasmSection {
    let mut bytes = Vec::new();
    encode_uleb128(u32::try_from(type_indices.len()).unwrap(), &mut bytes);
    for type_index in type_indices {
        encode_uleb128(*type_index, &mut bytes);
    }
    WasmSection { id: 3, name: None, bytes }
}

pub(crate) fn memory_section_bytes(min_pages: u32) -> WasmSection {
    let mut bytes = Vec::new();
    encode_uleb128(1, &mut bytes);
    bytes.push(LIMITS_MIN_ONLY);
    encode_uleb128(min_pages, &mut bytes);
    WasmSection { id: 5, name: None, bytes }
}

pub(crate) fn export_section_bytes(exports: &[(&str, u8, u32)]) -> WasmSection {
    let mut bytes = Vec::new();
    encode_uleb128(u32::try_from(exports.len()).unwrap(), &mut bytes);
    for (name, kind, index) in exports {
        encode_name(name, &mut bytes);
        bytes.push(*kind);
        encode_uleb128(*index, &mut bytes);
    }
    WasmSection { id: 7, name: None, bytes }
}

pub(crate) fn code_section_bytes(bodies: &[Vec<u8>]) -> WasmSection {
    let mut bytes = Vec::new();
    encode_uleb128(u32::try_from(bodies.len()).unwrap(), &mut bytes);
    for body in bodies {
        encode_uleb128(u32::try_from(body.len()).unwrap(), &mut bytes);
        bytes.extend_from_slice(body);
    }
    WasmSection { id: 10, name: None, bytes }
}

pub(crate) fn data_section_bytes(offset: u32, data: &[u8]) -> WasmSection {
    let mut bytes = Vec::new();
    encode_uleb128(1, &mut bytes);
    bytes.push(0);
    encode_i32_const(i32::try_from(offset).unwrap(), &mut bytes);
    WasmOpcode::End.encode(&mut bytes);
    encode_uleb128(u32::try_from(data.len()).unwrap(), &mut bytes);
    bytes.extend_from_slice(data);
    WasmSection { id: 11, name: None, bytes }
}

pub(crate) fn wasm_function_body(instructions: Vec<u8>) -> Vec<u8> {
    instructions
}

pub(crate) fn encode_name(value: &str, bytes: &mut Vec<u8>) {
    encode_uleb128(u32::try_from(value.len()).unwrap(), bytes);
    bytes.extend_from_slice(value.as_bytes());
}

pub(crate) fn encode_uleb128(value: u32, bytes: &mut Vec<u8>) {
    std_encode_uleb128(value, bytes);
}

pub(crate) fn encode_sleb128_i32(value: i32, bytes: &mut Vec<u8>) {
    write_sleb128_i32(value, bytes);
}

/// 将 `i64` 编码为 `SLEB128` 字节序列并追加到 `bytes`。
pub(crate) fn encode_sleb128_i64(value: i64, bytes: &mut Vec<u8>) {
    std_data::binary::wasm::encode_sleb128_i64(value, bytes);
}

/// 从字节切片解码 `ULEB128` 值，推进读取位置。
pub(crate) fn decode_uleb128(bytes: &[u8], pos: &mut usize) -> u32 {
    let mut result = 0u32;
    let mut shift = 0u32;
    loop {
        let byte = bytes[*pos];
        *pos += 1;
        result |= ((byte & 0x7F) as u32) << shift;
        if byte & 0x80 == 0 {
            return result;
        }
        shift += 7;
    }
}

/// 统计 `WASM` 模块中类型段（`id=1`）已有的类型数量。
pub(crate) fn count_wasm_types(module: &WasmBinaryModule) -> u32 {
    let Some(section) = module.sections.iter().find(|item| item.id == 1)
    else {
        return 0;
    };
    let mut pos = 0;
    decode_uleb128(&section.bytes, &mut pos)
}

/// 统计 `WASM` 模块中导入段（`id=2`）里的函数导入数量。
pub(crate) fn count_wasm_function_imports(module: &WasmBinaryModule) -> u32 {
    let Some(section) = module.sections.iter().find(|item| item.id == 2)
    else {
        return 0;
    };
    let bytes = &section.bytes;
    let mut pos = 0;
    let count = decode_uleb128(bytes, &mut pos);
    let mut func_count = 0;
    for _ in 0..count {
        let mod_len = decode_uleb128(bytes, &mut pos) as usize;
        pos += mod_len;
        let field_len = decode_uleb128(bytes, &mut pos) as usize;
        pos += field_len;
        let kind = bytes[pos];
        pos += 1;
        match kind {
            0 => {
                decode_uleb128(bytes, &mut pos);
                func_count += 1;
            }
            1 => {
                pos += 1;
                let flags = bytes[pos];
                pos += 1;
                decode_uleb128(bytes, &mut pos);
                if flags & LIMITS_HAS_MAX != 0 {
                    decode_uleb128(bytes, &mut pos);
                }
            }
            2 => {
                let flags = bytes[pos];
                pos += 1;
                decode_uleb128(bytes, &mut pos);
                if flags & LIMITS_HAS_MAX != 0 {
                    decode_uleb128(bytes, &mut pos);
                }
            }
            3 => {
                pos += 1;
                pos += 1;
            }
            _ => {}
        }
    }
    func_count
}

/// 统计 `WASM` 模块中函数段（`id=3`）声明的函数数量。
pub(crate) fn count_wasm_function_decls(module: &WasmBinaryModule) -> u32 {
    let Some(section) = module.sections.iter().find(|item| item.id == 3)
    else {
        return 0;
    };
    let mut pos = 0;
    decode_uleb128(&section.bytes, &mut pos)
}

/// 向类型段追加新的函数类型条目。
pub(crate) fn append_wasm_types(module: &mut WasmBinaryModule, new_types: &[Vec<u8>]) {
    match module.sections.iter_mut().find(|item| item.id == 1) {
        Some(section) => {
            let mut pos = 0;
            let count = decode_uleb128(&section.bytes, &mut pos);
            let mut bytes = Vec::new();
            encode_uleb128(count + new_types.len() as u32, &mut bytes);
            bytes.extend_from_slice(&section.bytes[pos..]);
            for ty in new_types {
                bytes.extend_from_slice(ty);
            }
            section.bytes = bytes;
        }
        None => {
            let mut bytes = Vec::new();
            encode_uleb128(new_types.len() as u32, &mut bytes);
            for ty in new_types {
                bytes.extend_from_slice(ty);
            }
            insert_wasm_section(module, WasmSection { id: 1, name: None, bytes });
        }
    }
}

/// 向函数段追加函数声明（类型索引）。
pub(crate) fn append_wasm_function_decls(module: &mut WasmBinaryModule, type_indices: &[u32]) {
    match module.sections.iter_mut().find(|item| item.id == 3) {
        Some(section) => {
            let mut pos = 0;
            let count = decode_uleb128(&section.bytes, &mut pos);
            let mut bytes = Vec::new();
            encode_uleb128(count + type_indices.len() as u32, &mut bytes);
            bytes.extend_from_slice(&section.bytes[pos..]);
            for ti in type_indices {
                encode_uleb128(*ti, &mut bytes);
            }
            section.bytes = bytes;
        }
        None => {
            let mut bytes = Vec::new();
            encode_uleb128(type_indices.len() as u32, &mut bytes);
            for ti in type_indices {
                encode_uleb128(*ti, &mut bytes);
            }
            insert_wasm_section(module, WasmSection { id: 3, name: None, bytes });
        }
    }
}

/// 向导出段追加新的导出条目。
pub(crate) fn append_wasm_exports(module: &mut WasmBinaryModule, new_exports: &[(String, u8, u32)]) {
    match module.sections.iter_mut().find(|item| item.id == 7) {
        Some(section) => {
            let mut pos = 0;
            let count = decode_uleb128(&section.bytes, &mut pos);
            let mut bytes = Vec::new();
            encode_uleb128(count + new_exports.len() as u32, &mut bytes);
            bytes.extend_from_slice(&section.bytes[pos..]);
            for (name, kind, index) in new_exports {
                encode_name(name, &mut bytes);
                bytes.push(*kind);
                encode_uleb128(*index, &mut bytes);
            }
            section.bytes = bytes;
        }
        None => {
            let mut bytes = Vec::new();
            encode_uleb128(new_exports.len() as u32, &mut bytes);
            for (name, kind, index) in new_exports {
                encode_name(name, &mut bytes);
                bytes.push(*kind);
                encode_uleb128(*index, &mut bytes);
            }
            insert_wasm_section(module, WasmSection { id: 7, name: None, bytes });
        }
    }
}

/// 向代码段追加函数体。
pub(crate) fn append_wasm_code_bodies(module: &mut WasmBinaryModule, bodies: &[Vec<u8>]) {
    match module.sections.iter_mut().find(|item| item.id == 10) {
        Some(section) => {
            let mut pos = 0;
            let count = decode_uleb128(&section.bytes, &mut pos);
            let mut bytes = Vec::new();
            encode_uleb128(count + bodies.len() as u32, &mut bytes);
            bytes.extend_from_slice(&section.bytes[pos..]);
            for body in bodies {
                encode_uleb128(body.len() as u32, &mut bytes);
                bytes.extend_from_slice(body);
            }
            section.bytes = bytes;
        }
        None => {
            let mut bytes = Vec::new();
            encode_uleb128(bodies.len() as u32, &mut bytes);
            for body in bodies {
                encode_uleb128(body.len() as u32, &mut bytes);
                bytes.extend_from_slice(body);
            }
            insert_wasm_section(module, WasmSection { id: 10, name: None, bytes });
        }
    }
}

/// 按 `id` 顺序将段插入到 `WASM` 模块的正确位置。
pub(crate) fn insert_wasm_section(module: &mut WasmBinaryModule, section: WasmSection) {
    let insert_pos = module.sections.iter().position(|item| item.id > section.id).unwrap_or(module.sections.len());
    module.sections.insert(insert_pos, section);
}

/// 构建包含 `count` 个可变 `i32` 全局（初始值为 `0`）的全局段。
pub(crate) fn global_section_bytes(count: u32) -> WasmSection {
    global_section_with_i32_inits(&vec![0; count as usize])
}

/// 构建可变 `i32` 全局段，使用给定初始值。
pub(crate) fn global_section_with_i32_inits(inits: &[i32]) -> WasmSection {
    let mut bytes = Vec::new();
    encode_uleb128(u32::try_from(inits.len()).unwrap(), &mut bytes);
    for &init in inits {
        bytes.push(VALTYPE_I32);
        bytes.push(FIELD_MUTABLE);
        encode_i32_const(init, &mut bytes);
        WasmOpcode::End.encode(&mut bytes);
    }
    WasmSection { id: 6, name: None, bytes }
}

/// 统计全局段中已有全局数量。
pub(crate) fn count_wasm_globals(module: &WasmBinaryModule) -> u32 {
    let Some(section) = module.sections.iter().find(|item| item.id == 6)
    else {
        return 0;
    };
    let mut pos = 0;
    decode_uleb128(&section.bytes, &mut pos)
}

/// 追加可变 `i32` 全局到已有全局段（不存在则创建），返回追加前的全局基数。
pub(crate) fn append_wasm_i32_globals(module: &mut WasmBinaryModule, inits: &[i32]) -> u32 {
    if inits.is_empty() {
        return count_wasm_globals(module);
    }
    let base = count_wasm_globals(module);
    if let Some(section) = module.sections.iter_mut().find(|item| item.id == 6) {
        let mut pos = 0;
        let old_count = decode_uleb128(&section.bytes, &mut pos);
        let rest = section.bytes[pos..].to_vec();
        let mut bytes = Vec::new();
        encode_uleb128(old_count + u32::try_from(inits.len()).unwrap(), &mut bytes);
        bytes.extend_from_slice(&rest);
        for &init in inits {
            bytes.push(VALTYPE_I32);
            bytes.push(FIELD_MUTABLE);
            encode_i32_const(init, &mut bytes);
            WasmOpcode::End.encode(&mut bytes);
        }
        section.bytes = bytes;
    }
    else {
        insert_wasm_section(module, global_section_with_i32_inits(inits));
    }
    base
}

/// WASM-GC 引用类型在栈/局部/全局中的值类型字节。
///
/// - `0x6F` = `externref`（host 传入的对象引用）
/// - `0x6E` = `anyref`（GC 提案统一引用类型）
///
/// 本后端统一使用 `anyref` (0x6E) 作为引用类型的局部/全局类型，因为

pub(crate) fn table_section_bytes(min_size: u32) -> WasmSection {
    let mut bytes = Vec::new();
    encode_uleb128(1, &mut bytes);
    bytes.push(VALTYPE_FUNCREF);
    bytes.push(LIMITS_MIN_ONLY);
    encode_uleb128(min_size, &mut bytes);
    WasmSection { id: 4, name: None, bytes }
}

pub(crate) fn elem_section_bytes(table_index: u32, function_indices: &[u32]) -> WasmSection {
    let mut bytes = Vec::new();
    encode_uleb128(1, &mut bytes);
    encode_uleb128(table_index, &mut bytes);
    encode_i32_const(0, &mut bytes);
    WasmOpcode::End.encode(&mut bytes);
    encode_uleb128(u32::try_from(function_indices.len()).unwrap(), &mut bytes);
    for index in function_indices {
        encode_uleb128(*index, &mut bytes);
    }
    WasmSection { id: 9, name: None, bytes }
}

#[allow(dead_code)]
pub(crate) const _WASM_TYPE_FORM_FUNC: u8 = TYPE_FORM_FUNC;
