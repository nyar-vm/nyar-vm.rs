//! `.legion` 模块解码：入口与各 section 解析器。

use crate::{
    errors::NyarDecodeError,
    model::{
        NyarConstant, NyarExport, NyarExportKind, NyarFunction, NyarGlobal, NyarImport, NyarImportKind, NyarLayout, NyarModuleData,
        NyarWitnessDispatchEntry,
    },
    opcode::{HEADER_SIZE, NYAR_MAGIC, SECTION_HEADER_SIZE},
};

/// Decode a `.legion` module from bytes.
pub fn decode_module(data: &[u8]) -> Result<NyarModuleData, NyarDecodeError> {
    if data.len() < HEADER_SIZE {
        return Err(NyarDecodeError::TooShort);
    }

    let magic = u32::from_be_bytes(data[0..4].try_into().expect("header"));
    if magic != NYAR_MAGIC {
        return Err(NyarDecodeError::InvalidMagic(magic));
    }

    let version = u32::from_le_bytes(data[4..8].try_into().expect("version"));
    let section_count = i32::from_le_bytes(data[8..12].try_into().expect("sections")) as usize;
    let name_offset = i32::from_le_bytes(data[12..16].try_into().expect("name")) as usize;

    let headers_start = HEADER_SIZE;
    let headers_end = headers_start + section_count * SECTION_HEADER_SIZE;
    if data.len() < headers_end {
        return Err(NyarDecodeError::TooShort);
    }

    let mut sections = Vec::with_capacity(section_count);
    for i in 0..section_count {
        let base = headers_start + i * SECTION_HEADER_SIZE;
        sections.push(NyarSectionHeader {
            kind: data[base],
            offset: i32::from_le_bytes(data[base + 1..base + 5].try_into().expect("offset")),
            size: i32::from_le_bytes(data[base + 5..base + 9].try_into().expect("size")),
        });
    }

    let name = read_length_prefixed_string(data, name_offset)?;

    let mut module = NyarModuleData {
        version,
        name,
        constants: Vec::new(),
        functions: Vec::new(),
        imports: Vec::new(),
        exports: Vec::new(),
        witness_entries: Vec::new(),
        code_bytes: Vec::new(),
        globals: Vec::new(),
        init_function_indices: Vec::new(),
        layouts: Vec::new(),
    };

    for section in sections {
        let offset = section.offset as usize;
        let size = section.size as usize;
        if offset + size > data.len() {
            return Err(NyarDecodeError::TooShort);
        }

        let slice = &data[offset..offset + size];
        match section.kind {
            0x01 => decode_constants(slice, &mut module.constants)?,
            0x02 => decode_functions(slice, &mut module.functions)?,
            0x03 => module.code_bytes = slice.to_vec(),
            0x04 => decode_imports(slice, &mut module.imports)?,
            0x05 => decode_exports(slice, &mut module.exports)?,
            0x06 => decode_witness_entries(slice, &mut module.witness_entries)?,
            0x07 => decode_globals(slice, &mut module.globals)?,
            0x08 => decode_init_functions(slice, &mut module.init_function_indices)?,
            0x09 => decode_layouts(slice, &mut module.layouts)?,
            kind => return Err(NyarDecodeError::InvalidSectionKind(kind)),
        }
    }

    Ok(module)
}

#[derive(Debug, Clone, Copy)]
struct NyarSectionHeader {
    kind: u8,
    offset: i32,
    size: i32,
}

fn read_length_prefixed_string(data: &[u8], offset: usize) -> Result<String, NyarDecodeError> {
    if offset + 4 > data.len() {
        return Err(NyarDecodeError::TooShort);
    }
    let len = i32::from_le_bytes(data[offset..offset + 4].try_into().expect("len")) as usize;
    let start = offset + 4;
    let end = start + len;
    if end > data.len() {
        return Err(NyarDecodeError::TooShort);
    }
    String::from_utf8(data[start..end].to_vec()).map_err(|_| NyarDecodeError::InvalidUtf8)
}

fn decode_constants(slice: &[u8], out: &mut Vec<NyarConstant>) -> Result<(), NyarDecodeError> {
    if slice.len() < 4 {
        return Err(NyarDecodeError::TooShort);
    }
    let count = i32::from_le_bytes(slice[0..4].try_into().expect("count")) as usize;
    let mut offset = 4;
    for _ in 0..count {
        if offset >= slice.len() {
            return Err(NyarDecodeError::TooShort);
        }
        let kind = slice[offset];
        offset += 1;
        let (constant, consumed) = decode_constant(kind, &slice[offset..])?;
        offset += consumed;
        out.push(constant);
    }
    Ok(())
}

fn decode_constant(kind: u8, slice: &[u8]) -> Result<(NyarConstant, usize), NyarDecodeError> {
    match kind {
        0x00 => Ok((NyarConstant::Null, 0)),
        0x01 => {
            if slice.is_empty() {
                return Err(NyarDecodeError::TooShort);
            }
            Ok((NyarConstant::Boolean(slice[0] != 0), 1))
        }
        0x11 => {
            if slice.len() < 4 {
                return Err(NyarDecodeError::TooShort);
            }
            Ok((NyarConstant::Integer32(i32::from_le_bytes(slice[0..4].try_into().expect("i32"))), 4))
        }
        0x22 => {
            if slice.len() < 8 {
                return Err(NyarDecodeError::TooShort);
            }
            Ok((NyarConstant::Float64(f64::from_le_bytes(slice[0..8].try_into().expect("f64"))), 8))
        }
        0x05 => {
            if slice.len() < 4 {
                return Err(NyarDecodeError::TooShort);
            }
            let len = i32::from_le_bytes(slice[0..4].try_into().expect("len")) as usize;
            if slice.len() < 4 + len {
                return Err(NyarDecodeError::TooShort);
            }
            let value = String::from_utf8(slice[4..4 + len].to_vec()).map_err(|_| NyarDecodeError::InvalidUtf8)?;
            Ok((NyarConstant::String(value), 4 + len))
        }
        0x06 => {
            if slice.len() < 4 {
                return Err(NyarDecodeError::TooShort);
            }
            let len = i32::from_le_bytes(slice[0..4].try_into().expect("len")) as usize;
            if slice.len() < 4 + len {
                return Err(NyarDecodeError::TooShort);
            }
            Ok((NyarConstant::BigInt(slice[4..4 + len].to_vec()), 4 + len))
        }
        kind => Err(NyarDecodeError::InvalidConstantKind(kind)),
    }
}

fn decode_functions(slice: &[u8], out: &mut Vec<NyarFunction>) -> Result<(), NyarDecodeError> {
    if slice.len() < 4 {
        return Err(NyarDecodeError::TooShort);
    }
    let count = i32::from_le_bytes(slice[0..4].try_into().expect("count")) as usize;
    let mut offset = 4;
    for _ in 0..count {
        let (name, consumed) = read_string_at(slice, offset)?;
        offset += consumed;
        if offset + 16 > slice.len() {
            return Err(NyarDecodeError::TooShort);
        }
        out.push(NyarFunction {
            name,
            arity: i32::from_le_bytes(slice[offset..offset + 4].try_into().expect("arity")),
            local_count: i32::from_le_bytes(slice[offset + 4..offset + 8].try_into().expect("locals")),
            code_offset: i32::from_le_bytes(slice[offset + 8..offset + 12].try_into().expect("off")),
            code_length: i32::from_le_bytes(slice[offset + 12..offset + 16].try_into().expect("len")),
        });
        offset += 16;
    }
    Ok(())
}

fn decode_imports(slice: &[u8], out: &mut Vec<NyarImport>) -> Result<(), NyarDecodeError> {
    if slice.len() < 4 {
        return Err(NyarDecodeError::TooShort);
    }
    let count = i32::from_le_bytes(slice[0..4].try_into().expect("count")) as usize;
    let mut offset = 4;
    for _ in 0..count {
        if offset >= slice.len() {
            return Err(NyarDecodeError::TooShort);
        }
        let kind = match slice[offset] {
            0 => NyarImportKind::Function,
            kind => return Err(NyarDecodeError::InvalidImportKind(kind)),
        };
        offset += 1;
        let (module_name, consumed) = read_string_at(slice, offset)?;
        offset += consumed;
        let (symbol_name, consumed) = read_string_at(slice, offset)?;
        offset += consumed;
        out.push(NyarImport { kind, module_name, symbol_name });
    }
    Ok(())
}

fn decode_exports(slice: &[u8], out: &mut Vec<NyarExport>) -> Result<(), NyarDecodeError> {
    if slice.len() < 4 {
        return Err(NyarDecodeError::TooShort);
    }
    let count = i32::from_le_bytes(slice[0..4].try_into().expect("count")) as usize;
    let mut offset = 4;
    for _ in 0..count {
        if offset >= slice.len() {
            return Err(NyarDecodeError::TooShort);
        }
        let kind = match slice[offset] {
            0 => NyarExportKind::Function,
            1 => NyarExportKind::Global,
            kind => return Err(NyarDecodeError::InvalidExportKind(kind)),
        };
        offset += 1;
        let (symbol_name, consumed) = read_string_at(slice, offset)?;
        offset += consumed;
        if offset + 4 > slice.len() {
            return Err(NyarDecodeError::TooShort);
        }
        out.push(NyarExport { kind, symbol_name, function_index: i32::from_le_bytes(slice[offset..offset + 4].try_into().expect("idx")) });
        offset += 4;
    }
    Ok(())
}

fn decode_globals(slice: &[u8], out: &mut Vec<NyarGlobal>) -> Result<(), NyarDecodeError> {
    if slice.len() < 4 {
        return Err(NyarDecodeError::TooShort);
    }
    let count = i32::from_le_bytes(slice[0..4].try_into().expect("count")) as usize;
    let mut offset = 4;
    for _ in 0..count {
        let (name, consumed) = read_string_at(slice, offset)?;
        offset += consumed;
        let (type_name, consumed) = read_string_at(slice, offset)?;
        offset += consumed;
        out.push(NyarGlobal { name, type_name });
    }
    Ok(())
}

fn decode_init_functions(slice: &[u8], out: &mut Vec<i32>) -> Result<(), NyarDecodeError> {
    if slice.len() < 4 {
        return Err(NyarDecodeError::TooShort);
    }
    let count = i32::from_le_bytes(slice[0..4].try_into().expect("count")) as usize;
    let mut offset = 4;
    for _ in 0..count {
        if offset + 4 > slice.len() {
            return Err(NyarDecodeError::TooShort);
        }
        out.push(i32::from_le_bytes(slice[offset..offset + 4].try_into().expect("idx")));
        offset += 4;
    }
    Ok(())
}

fn decode_layouts(slice: &[u8], out: &mut Vec<NyarLayout>) -> Result<(), NyarDecodeError> {
    if slice.len() < 4 {
        return Err(NyarDecodeError::TooShort);
    }
    let count = i32::from_le_bytes(slice[0..4].try_into().expect("count")) as usize;
    let mut offset = 4;
    for _ in 0..count {
        if offset + 4 > slice.len() {
            return Err(NyarDecodeError::TooShort);
        }
        let field_count = i32::from_le_bytes(slice[offset..offset + 4].try_into().expect("field_count"));
        if field_count < 0 {
            return Err(NyarDecodeError::TooShort);
        }
        out.push(NyarLayout { field_count });
        offset += 4;
    }
    Ok(())
}

fn decode_witness_entries(slice: &[u8], out: &mut Vec<NyarWitnessDispatchEntry>) -> Result<(), NyarDecodeError> {
    if slice.len() < 4 {
        return Err(NyarDecodeError::TooShort);
    }
    let count = i32::from_le_bytes(slice[0..4].try_into().expect("count")) as usize;
    let mut offset = 4;
    for _ in 0..count {
        if offset + 12 > slice.len() {
            return Err(NyarDecodeError::TooShort);
        }
        let method_id = i32::from_le_bytes(slice[offset..offset + 4].try_into().expect("mid"));
        let type_id = i32::from_le_bytes(slice[offset + 4..offset + 8].try_into().expect("tid"));
        offset += 8;
        let (method_name, consumed) = read_string_at(slice, offset)?;
        offset += consumed;
        if offset + 12 > slice.len() {
            return Err(NyarDecodeError::TooShort);
        }
        out.push(NyarWitnessDispatchEntry {
            method_id,
            type_id,
            method_name,
            function_index: i32::from_le_bytes(slice[offset..offset + 4].try_into().expect("fi")),
            interface_id: i32::from_le_bytes(slice[offset + 4..offset + 8].try_into().expect("ii")),
            interface_method_index: i32::from_le_bytes(slice[offset + 8..offset + 12].try_into().expect("imi")),
        });
        offset += 12;
    }
    Ok(())
}

fn read_string_at(slice: &[u8], offset: usize) -> Result<(String, usize), NyarDecodeError> {
    if offset + 4 > slice.len() {
        return Err(NyarDecodeError::TooShort);
    }
    let len = i32::from_le_bytes(slice[offset..offset + 4].try_into().expect("len")) as usize;
    let start = offset + 4;
    let end = start + len;
    if end > slice.len() {
        return Err(NyarDecodeError::TooShort);
    }
    let value = String::from_utf8(slice[start..end].to_vec()).map_err(|_| NyarDecodeError::InvalidUtf8)?;
    Ok((value, 4 + len))
}
