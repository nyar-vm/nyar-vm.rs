//! Convert [`GenerateModule`] into `.nyar` bytes.

use miette::Result;
use std_data::binary::nyar_ir::{
    NyarConstant, NyarExport, NyarExportKind, NyarFunction, NyarImport, NyarImportKind, NyarModuleData, encode_module,
};

use super::generate::{GenerateConstant, GenerateExportKind, GenerateImportKind, GenerateModule, GenerateOperand};

/// Nyar bytecode compiler for generate-time modules.
#[derive(Debug, Default, Clone, Copy)]
pub struct NyarBytecodeCompiler;

impl NyarBytecodeCompiler {
    /// Compile a generate-time module into `.nyar` bytes.
    pub fn compile(&self, module: &GenerateModule) -> Result<Vec<u8>> {
        Ok(encode_module(&self.to_module_data(module)?))
    }

    /// Convert a generate-time module into [`NyarModuleData`].
    pub fn to_module_data(&self, module: &GenerateModule) -> Result<NyarModuleData> {
        let mut code_bytes = Vec::new();
        let mut functions = Vec::new();

        for function in &module.functions {
            let code_offset = code_bytes.len() as i32;
            for instruction in &function.instructions {
                code_bytes.push(instruction.opcode as u8);
                for operand in &instruction.operands {
                    emit_operand(&mut code_bytes, operand, module)?;
                }
            }
            functions.push(NyarFunction {
                name: function.name.clone(),
                arity: function.parameters.len() as i32,
                local_count: function.slot_count(),
                code_offset,
                code_length: (code_bytes.len() as i32) - code_offset,
            });
        }

        Ok(NyarModuleData {
            version: 1,
            name: module.name.clone(),
            constants: convert_constants(&module.constants),
            functions,
            imports: convert_imports(&module.imports),
            exports: convert_exports(&module.exports),
            witness_entries: Vec::new(),
            code_bytes,
            globals: Vec::new(),
            init_function_indices: Vec::new(),
        })
    }
}

fn emit_operand(writer: &mut Vec<u8>, operand: &GenerateOperand, module: &GenerateModule) -> Result<()> {
    match operand {
        GenerateOperand::I32(value) => writer.extend_from_slice(&value.to_le_bytes()),
        GenerateOperand::I64(value) => writer.extend_from_slice(&value.to_le_bytes()),
        GenerateOperand::F32(value) => writer.extend_from_slice(&value.to_le_bytes()),
        GenerateOperand::F64(value) => writer.extend_from_slice(&value.to_le_bytes()),
        GenerateOperand::Str(value) => emit_string(writer, value),
        GenerateOperand::Local { index } | GenerateOperand::Param { index } | GenerateOperand::Const { pool_index: index } => {
            writer.extend_from_slice(&index.to_le_bytes())
        }
        GenerateOperand::Label { name } => {
            // Should have been resolved; encode 0 as a soft fallback.
            let _ = name;
            writer.extend_from_slice(&0i32.to_le_bytes());
        }
        GenerateOperand::FuncRef { name } => {
            let index = find_function_index(module, name);
            writer.extend_from_slice(&index.to_le_bytes());
        }
        GenerateOperand::Null => writer.extend_from_slice(&0i32.to_le_bytes()),
    }
    Ok(())
}

fn emit_string(writer: &mut Vec<u8>, value: &str) {
    writer.extend_from_slice(&(value.len() as i32).to_le_bytes());
    writer.extend_from_slice(value.as_bytes());
}

fn find_function_index(module: &GenerateModule, name: &str) -> i32 {
    module.functions.iter().position(|function| function.name == name).unwrap_or(0) as i32
}

fn convert_constants(pool: &super::generate::GenerateConstantPool) -> Vec<NyarConstant> {
    pool.entries
        .iter()
        .map(|entry| match entry {
            GenerateConstant::Null => NyarConstant::Null,
            GenerateConstant::Bool(value) => NyarConstant::Boolean(*value),
            GenerateConstant::Int64(value) => NyarConstant::Integer32(*value as i32),
            GenerateConstant::Float64(value) => NyarConstant::Float64(*value),
            GenerateConstant::Utf8(value) => NyarConstant::String(value.clone()),
        })
        .collect()
}

fn convert_imports(imports: &[super::generate::GenerateModuleImport]) -> Vec<NyarImport> {
    imports
        .iter()
        .map(|import| {
            let kind = match import.kind {
                GenerateImportKind::Function | GenerateImportKind::Global | GenerateImportKind::Module => NyarImportKind::Function,
            };
            NyarImport { kind, module_name: import.module_name.clone(), symbol_name: import.symbol_name.clone() }
        })
        .collect()
}

fn convert_exports(exports: &[super::generate::GenerateModuleExport]) -> Vec<NyarExport> {
    exports
        .iter()
        .map(|export| {
            let kind = match export.kind {
                GenerateExportKind::Function | GenerateExportKind::Global => NyarExportKind::Function,
            };
            NyarExport { kind, symbol_name: export.name.clone(), function_index: export.function_index }
        })
        .collect()
}
