use std_data::binary::nyar_ir::{
    NyarConstant, NyarExport, NyarFunction, NyarGlobal, NyarImport, NyarModuleData, NyarWitnessDispatchEntry, decode_module,
};

use crate::{
    error::NyarRuntimeError,
    host::{ResolvedImport, resolve_import},
    value::Value,
    verify::verify_module,
};

/// Persistent module global slots for singleton state across multiple runs.
#[derive(Debug, Clone)]
pub struct ModuleGlobals {
    slots: Vec<Value>,
    init_done: bool,
}

impl ModuleGlobals {
    /// Creates empty global slots sized for `module`.
    pub fn new(module: &LoadedModule) -> Self {
        Self { slots: vec![Value::Null; module.globals.len()], init_done: false }
    }

    pub(crate) fn slots_mut(&mut self) -> &mut [Value] {
        &mut self.slots
    }

    pub(crate) fn init_done(&self) -> bool {
        self.init_done
    }

    pub(crate) fn mark_init_done(&mut self) {
        self.init_done = true;
    }
}

/// Runtime view of a loaded `.nyar` module.
#[derive(Debug, Clone)]
pub struct LoadedModule {
    /// Module format version.
    pub version: u32,
    /// Module name.
    pub name: String,
    /// Constant pool.
    pub constants: Vec<NyarConstant>,
    /// Function table.
    pub functions: Vec<NyarFunction>,
    /// Import table（诊断 / 外部绑定；热路径用 [`Self::resolved_imports`]）。
    pub imports: Vec<NyarImport>,
    /// 加载期解析的导入槽（`CallImport` 只消费此表）。
    pub resolved_imports: Vec<ResolvedImport>,
    /// Export table.
    pub exports: Vec<NyarExport>,
    /// Witness dispatch table.
    pub witness_entries: Vec<NyarWitnessDispatchEntry>,
    /// Flat code section bytes.
    pub code_bytes: Vec<u8>,
    /// Module-level global slot metadata.
    pub globals: Vec<NyarGlobal>,
    /// Eager init function indices.
    pub init_function_indices: Vec<i32>,
}

impl LoadedModule {
    /// Loads a module from `.nyar` bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, NyarRuntimeError> {
        let data = decode_module(bytes).map_err(|error| NyarRuntimeError::ModuleLoad(error.to_string()))?;
        Self::from_data(data)
    }

    /// Wraps decoded module data after import / bytecode verify.
    pub fn from_data(data: NyarModuleData) -> Result<Self, NyarRuntimeError> {
        verify_module(&data)?;
        let resolved_imports = data.imports.iter().map(resolve_import).collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            version: data.version,
            name: data.name,
            constants: data.constants,
            functions: data.functions,
            imports: data.imports,
            resolved_imports,
            exports: data.exports,
            witness_entries: data.witness_entries,
            code_bytes: data.code_bytes,
            globals: data.globals,
            init_function_indices: data.init_function_indices,
        })
    }

    /// Returns a constant-pool entry by index.
    pub fn constant_at(&self, index: i32) -> Option<&NyarConstant> {
        if index < 0 {
            return None;
        }
        self.constants.get(index as usize)
    }

    /// Resolves an exported function index by symbol name.
    pub fn export_index(&self, name: &str) -> Option<usize> {
        self.exports
            .iter()
            .find(|export| export.kind == std_data::binary::nyar_ir::NyarExportKind::Function && export.symbol_name == name)
            .map(|export| export.function_index as usize)
    }

    /// Resolves an exported global index by symbol name.
    pub fn export_global_index(&self, name: &str) -> Option<usize> {
        self.exports
            .iter()
            .find(|export| export.kind == std_data::binary::nyar_ir::NyarExportKind::Global && export.symbol_name == name)
            .map(|export| export.function_index as usize)
    }
}
