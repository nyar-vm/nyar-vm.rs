use nyar_bytecode::{
    NyarConstant, NyarExport, NyarFunction, NyarGlobal, NyarImport, NyarLayout, NyarModuleData, NyarWitnessDispatchEntry, decode_module,
};

use crate::{
    error::NyarRuntimeError,
    executable::{ExecutableFunction, build_executable_table},
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
    /// Flat code section bytes（外码；诊断与 JIT 原始视图）。
    pub code_bytes: Vec<u8>,
    /// 按函数索引排列的预解码内码（加载期由外码生成）。
    pub executable: Vec<ExecutableFunction>,
    /// Module-level global slot metadata.
    pub globals: Vec<NyarGlobal>,
    /// Eager init function indices.
    pub init_function_indices: Vec<i32>,
    /// 稠密布局表（`ObjectNew` / 字段槽校验）。
    pub layouts: Vec<NyarLayout>,
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
        let executable = build_executable_table(&data.code_bytes, &data.functions)?;
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
            executable,
            globals: data.globals,
            init_function_indices: data.init_function_indices,
            layouts: data.layouts,
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
            .find(|export| export.kind == nyar_bytecode::NyarExportKind::Function && export.symbol_name == name)
            .map(|export| export.function_index as usize)
    }

    /// Resolves an exported global index by symbol name.
    pub fn export_global_index(&self, name: &str) -> Option<usize> {
        self.exports
            .iter()
            .find(|export| export.kind == nyar_bytecode::NyarExportKind::Global && export.symbol_name == name)
            .map(|export| export.function_index as usize)
    }
}
