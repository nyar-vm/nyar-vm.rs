//! Generate-time IR for Nyar bytecode emission.

use std_data::binary::nyar_ir::NyarHeadCode;

/// One constant-pool entry (unified index space).
#[derive(Debug, Clone, PartialEq)]
pub enum GenerateConstant {
    /// Null / nil.
    Null,
    /// Boolean.
    Bool(bool),
    /// Integer (encoded as `Integer32` in `.nyar`).
    Int64(i64),
    /// Float64.
    Float64(f64),
    /// UTF-8 string.
    String(String),
}

/// Constant pool for generate-time modules.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct GenerateConstantPool {
    /// Ordered entries; indices match Const operands.
    pub entries: Vec<GenerateConstant>,
}

impl GenerateConstantPool {
    /// Add null and return its pool index.
    pub fn add_null(&mut self) -> i32 {
        let index = self.entries.len() as i32;
        self.entries.push(GenerateConstant::Null);
        index
    }

    /// Add a bool and return its pool index.
    pub fn add_bool(&mut self, value: bool) -> i32 {
        let index = self.entries.len() as i32;
        self.entries.push(GenerateConstant::Bool(value));
        index
    }

    /// Add an `i64` constant.
    pub fn add_int64(&mut self, value: i64) -> i32 {
        let index = self.entries.len() as i32;
        self.entries.push(GenerateConstant::Int64(value));
        index
    }

    /// Add an `f64` constant.
    pub fn add_float64(&mut self, value: f64) -> i32 {
        let index = self.entries.len() as i32;
        self.entries.push(GenerateConstant::Float64(value));
        index
    }

    /// Add a string constant.
    pub fn add_string(&mut self, value: impl Into<String>) -> i32 {
        let index = self.entries.len() as i32;
        self.entries.push(GenerateConstant::Utf8(value.into()));
        index
    }

    /// Legacy accessor used by older stubs.
    pub fn int64s(&self) -> impl Iterator<Item = i64> + '_ {
        self.entries.iter().filter_map(|entry| match entry {
            GenerateConstant::Int64(value) => Some(*value),
            _ => None,
        })
    }

    /// Legacy accessor used by older stubs.
    pub fn strings(&self) -> impl Iterator<Item = &str> + '_ {
        self.entries.iter().filter_map(|entry| match entry {
            GenerateConstant::Utf8(value) => Some(value.as_str()),
            _ => None,
        })
    }
}

/// Import kind for generate-time modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerateImportKind {
    /// Function import.
    Function,
    /// Global import.
    Global,
    /// Module import.
    Module,
}

/// Export kind for generate-time modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerateExportKind {
    /// Function export.
    Function,
    /// Global export.
    Global,
}

/// Generate-time import entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerateModuleImport {
    /// Import kind.
    pub kind: GenerateImportKind,
    /// Module name.
    pub module_name: String,
    /// Symbol name.
    pub symbol_name: String,
}

/// Generate-time export entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerateModuleExport {
    /// Export kind.
    pub kind: GenerateExportKind,
    /// Exported name.
    pub name: String,
    /// Function index.
    pub function_index: i32,
}

/// Generate-time operand.
#[derive(Debug, Clone, PartialEq)]
pub enum GenerateOperand {
    /// 32-bit integer immediate.
    I32(i32),
    /// 64-bit integer immediate.
    I64(i64),
    /// 32-bit float immediate.
    F32(f32),
    /// 64-bit float immediate.
    F64(f64),
    /// String immediate.
    Str(String),
    /// Local slot.
    Local {
        /// Local index.
        index: i32,
    },
    /// Parameter slot.
    Param {
        /// Parameter index.
        index: i32,
    },
    /// Branch label (resolved to relative `I32` before encode).
    Label {
        /// Label name.
        name: String,
    },
    /// Function reference.
    FuncRef {
        /// Function name.
        name: String,
    },
    /// Constant pool reference.
    Const {
        /// Pool index.
        pool_index: i32,
    },
    /// Null placeholder.
    Null,
}

/// Generate-time instruction.
#[derive(Debug, Clone, PartialEq)]
pub struct GenerateInstruction {
    /// Opcode.
    pub opcode: NyarHeadCode,
    /// Operands.
    pub operands: Vec<GenerateOperand>,
}

impl GenerateInstruction {
    /// Create an instruction.
    pub fn new(opcode: NyarHeadCode, operands: Vec<GenerateOperand>) -> Self {
        Self { opcode, operands }
    }

    /// Encoded size in the `.nyar` code stream.
    pub fn encoded_size(&self) -> usize {
        self.opcode.code_size() as usize
    }
}

/// Generate-time function body.
#[derive(Debug, Clone, PartialEq)]
pub struct GenerateFunction {
    /// Function name.
    pub name: String,
    /// Parameter names.
    pub parameters: Vec<String>,
    /// Local variable names (excluding parameters).
    pub local_variables: Vec<String>,
    /// Instruction stream.
    pub instructions: Vec<GenerateInstruction>,
    /// Label name → instruction index.
    pub labels: std::collections::HashMap<String, usize>,
}

impl GenerateFunction {
    /// Create a function shell.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            parameters: Vec::new(),
            local_variables: Vec::new(),
            instructions: Vec::new(),
            labels: std::collections::HashMap::new(),
        }
    }

    /// Add a parameter.
    pub fn add_parameter(&mut self, name: impl Into<String>) {
        self.parameters.push(name.into());
    }

    /// Add a local variable.
    pub fn add_local_variable(&mut self, name: impl Into<String>) {
        self.local_variables.push(name.into());
    }

    /// Append an instruction.
    pub fn add_instruction(&mut self, instruction: GenerateInstruction) {
        self.instructions.push(instruction);
    }

    /// Total local slots (parameters + extras).
    pub fn slot_count(&self) -> i32 {
        (self.parameters.len() + self.local_variables.len()) as i32
    }

    /// Resolve label operands to relative `I32` jump offsets.
    pub fn resolve_labels(&mut self) {
        let mut byte_at = Vec::with_capacity(self.instructions.len() + 1);
        let mut pc = 0usize;
        for instruction in &self.instructions {
            byte_at.push(pc);
            pc += instruction.encoded_size();
        }
        byte_at.push(pc);

        let labels = self.labels.clone();
        for (index, instruction) in self.instructions.iter_mut().enumerate() {
            if !matches!(instruction.opcode, NyarHeadCode::Jump | NyarHeadCode::JumpIfTrue | NyarHeadCode::JumpIfFalse) {
                continue;
            }
            if let Some(GenerateOperand::Label { name }) = instruction.operands.first().cloned() {
                let Some(&target_index) = labels.get(&name)
                else {
                    continue;
                };
                let from = byte_at[index] as i32;
                let to = byte_at[target_index.min(byte_at.len() - 1)] as i32;
                instruction.operands = vec![GenerateOperand::I32(to.wrapping_sub(from))];
            }
        }
    }
}

/// Generate-time module.
#[derive(Debug, Clone, PartialEq)]
pub struct GenerateModule {
    /// Module name.
    pub name: String,
    /// Constant pool.
    pub constants: GenerateConstantPool,
    /// Functions.
    pub functions: Vec<GenerateFunction>,
    /// Imports.
    pub imports: Vec<GenerateModuleImport>,
    /// Exports.
    pub exports: Vec<GenerateModuleExport>,
}

impl GenerateModule {
    /// Create a module.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), constants: GenerateConstantPool::default(), functions: Vec::new(), imports: Vec::new(), exports: Vec::new() }
    }

    /// Append a function.
    pub fn add_function(&mut self, mut function: GenerateFunction) {
        function.resolve_labels();
        self.exports.push(GenerateModuleExport {
            kind: GenerateExportKind::Function,
            name: function.name.clone(),
            function_index: self.functions.len() as i32,
        });
        self.functions.push(function);
    }
}
