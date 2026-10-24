//! `.legion` 模块编码：入口与各 section 序列化器。

use crate::{
    model::{
        NyarConstant, NyarExport, NyarFunction, NyarGlobal, NyarImport, NyarLayout, NyarModuleData, NyarSectionKind, NyarWitnessDispatchEntry,
    },
    opcode::{HEADER_SIZE, NYAR_MAGIC, SECTION_HEADER_SIZE},
};

/// Encode a minimal `.legion` module.
pub fn encode_module(data: &NyarModuleData) -> Vec<u8> {
    let mut sections: Vec<(NyarSectionKind, Vec<u8>)> = Vec::new();

    if !data.constants.is_empty() {
        sections.push((NyarSectionKind::Constants, encode_constants(&data.constants)));
    }
    if !data.functions.is_empty() {
        sections.push((NyarSectionKind::Functions, encode_functions(&data.functions)));
    }
    if !data.imports.is_empty() {
        sections.push((NyarSectionKind::Imports, encode_imports(&data.imports)));
    }
    if !data.exports.is_empty() {
        sections.push((NyarSectionKind::Exports, encode_exports(&data.exports)));
    }
    if !data.witness_entries.is_empty() {
        sections.push((NyarSectionKind::WitnessEntries, encode_witness_entries(&data.witness_entries)));
    }
    if !data.globals.is_empty() {
        sections.push((NyarSectionKind::Globals, encode_globals(&data.globals)));
    }
    if !data.init_function_indices.is_empty() {
        sections.push((NyarSectionKind::InitFunctions, encode_init_functions(&data.init_function_indices)));
    }
    if !data.layouts.is_empty() {
        sections.push((NyarSectionKind::Layouts, encode_layouts(&data.layouts)));
    }
    if !data.code_bytes.is_empty() {
        sections.push((NyarSectionKind::Code, data.code_bytes.clone()));
    }

    let name_bytes = data.name.as_bytes();
    let data_start = HEADER_SIZE + sections.len() * SECTION_HEADER_SIZE + 4 + name_bytes.len();
    let mut out = Vec::with_capacity(data_start + sections.iter().map(|(_, d)| d.len()).sum::<usize>());

    out.extend_from_slice(&NYAR_MAGIC.to_be_bytes());
    out.extend_from_slice(&data.version.to_le_bytes());
    out.extend_from_slice(&(sections.len() as i32).to_le_bytes());
    out.extend_from_slice(&(HEADER_SIZE as i32 + sections.len() as i32 * SECTION_HEADER_SIZE as i32).to_le_bytes());

    let mut offset = data_start as i32;
    for (kind, section_data) in &sections {
        out.push(*kind as u8);
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&(section_data.len() as i32).to_le_bytes());
        offset += section_data.len() as i32;
    }

    out.extend_from_slice(&(name_bytes.len() as i32).to_le_bytes());
    out.extend_from_slice(name_bytes);

    for (_, section_data) in sections {
        out.extend_from_slice(&section_data);
    }

    out
}

fn encode_constants(constants: &[NyarConstant]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(constants.len() as i32).to_le_bytes());
    for constant in constants {
        match constant {
            NyarConstant::Null => {
                out.push(0x00);
            }
            NyarConstant::Boolean(value) => {
                out.push(0x01);
                out.push(u8::from(*value));
            }
            NyarConstant::Integer32(value) => {
                out.push(0x11);
                out.extend_from_slice(&value.to_le_bytes());
            }
            NyarConstant::Float64(value) => {
                out.push(0x22);
                out.extend_from_slice(&value.to_le_bytes());
            }
            NyarConstant::String(value) => {
                out.push(0x05);
                let bytes = value.as_bytes();
                out.extend_from_slice(&(bytes.len() as i32).to_le_bytes());
                out.extend_from_slice(bytes);
            }
            NyarConstant::BigInt(value) => {
                out.push(0x06);
                out.extend_from_slice(&(value.len() as i32).to_le_bytes());
                out.extend_from_slice(value);
            }
        }
    }
    out
}

fn encode_functions(functions: &[NyarFunction]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(functions.len() as i32).to_le_bytes());
    for function in functions {
        write_string(&mut out, &function.name);
        out.extend_from_slice(&function.arity.to_le_bytes());
        out.extend_from_slice(&function.local_count.to_le_bytes());
        out.extend_from_slice(&function.code_offset.to_le_bytes());
        out.extend_from_slice(&function.code_length.to_le_bytes());
    }
    out
}

fn encode_imports(imports: &[NyarImport]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(imports.len() as i32).to_le_bytes());
    for import in imports {
        out.push(import.kind as u8);
        write_string(&mut out, &import.module_name);
        write_string(&mut out, &import.symbol_name);
    }
    out
}

fn encode_exports(exports: &[NyarExport]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(exports.len() as i32).to_le_bytes());
    for export in exports {
        out.push(export.kind as u8);
        write_string(&mut out, &export.symbol_name);
        out.extend_from_slice(&export.function_index.to_le_bytes());
    }
    out
}

fn encode_witness_entries(entries: &[NyarWitnessDispatchEntry]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(entries.len() as i32).to_le_bytes());
    for entry in entries {
        out.extend_from_slice(&entry.method_id.to_le_bytes());
        out.extend_from_slice(&entry.type_id.to_le_bytes());
        write_string(&mut out, &entry.method_name);
        out.extend_from_slice(&entry.function_index.to_le_bytes());
        out.extend_from_slice(&entry.interface_id.to_le_bytes());
        out.extend_from_slice(&entry.interface_method_index.to_le_bytes());
    }
    out
}

fn encode_globals(globals: &[NyarGlobal]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(globals.len() as i32).to_le_bytes());
    for global in globals {
        write_string(&mut out, &global.name);
        write_string(&mut out, &global.type_name);
    }
    out
}

fn encode_init_functions(indices: &[i32]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(indices.len() as i32).to_le_bytes());
    for index in indices {
        out.extend_from_slice(&index.to_le_bytes());
    }
    out
}

fn encode_layouts(layouts: &[NyarLayout]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(layouts.len() as i32).to_le_bytes());
    for layout in layouts {
        out.extend_from_slice(&layout.field_count.to_le_bytes());
    }
    out
}

fn write_string(out: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    out.extend_from_slice(&(bytes.len() as i32).to_le_bytes());
    out.extend_from_slice(bytes);
}
