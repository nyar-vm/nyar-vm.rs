//! Minimal stack compiler emitting [`GenerateModule`] IR.
//!
//! Used by bytecode-pipe **probes** (JS/Python stubs / `StackResidualSink`).
//! Host-script PE product path uses [`super::native_residual`] instead.

use std::collections::HashMap;

use std_data::binary::nyar_ir::NyarHeadCode;

use super::generate::{GenerateFunction, GenerateInstruction, GenerateModule, GenerateOperand};

/// Stack-style bytecode emitter for language compilers / PE residual sinks.
#[derive(Debug)]
pub struct StackCompiler {
    module: GenerateModule,
    current_function: Option<GenerateFunction>,
    locals: HashMap<String, i32>,
    next_local_index: i32,
}

impl StackCompiler {
    /// Create a compiler for the given module name.
    pub fn new(module_name: impl Into<String>) -> Self {
        Self { module: GenerateModule::new(module_name), current_function: None, locals: HashMap::new(), next_local_index: 0 }
    }

    /// Borrow the in-progress module.
    pub fn module(&self) -> &GenerateModule {
        &self.module
    }

    /// Consume the compiler and return the finished module.
    pub fn finish(mut self) -> GenerateModule {
        if self.current_function.is_some() {
            self.end_function();
        }
        self.module
    }

    /// Begin a new function.
    pub fn begin_function(&mut self, name: impl Into<String>) {
        self.current_function = Some(GenerateFunction::new(name));
        self.locals.clear();
        self.next_local_index = 0;
    }

    /// Register a parameter.
    pub fn add_parameter(&mut self, name: impl Into<String>) {
        let name = name.into();
        let function = self.current_function.as_mut().expect("begin_function first");
        function.add_parameter(name.clone());
        self.locals.insert(name, self.next_local_index);
        self.next_local_index += 1;
    }

    /// Ensure a named local slot exists; return its index.
    pub fn ensure_local(&mut self, name: impl Into<String>) -> i32 {
        let name = name.into();
        if let Some(&index) = self.locals.get(&name) {
            return index;
        }
        let index = self.next_local_index;
        self.next_local_index += 1;
        self.locals.insert(name.clone(), index);
        let function = self.current_function.as_mut().expect("begin_function first");
        // Parameters already occupy early indices; only extras go into local_variables.
        if index >= function.parameters.len() as i32 {
            function.add_local_variable(name);
        }
        index
    }

    /// End the current function and append it to the module.
    pub fn end_function(&mut self) {
        let function = self.current_function.take().expect("begin_function first");
        self.module.add_function(function);
    }

    /// Bind a label at the next instruction.
    pub fn emit_label(&mut self, name: impl Into<String>) {
        let function = self.current_function.as_mut().expect("begin_function first");
        let index = function.instructions.len();
        function.labels.insert(name.into(), index);
    }

    /// Emit an instruction with operands.
    pub fn emit(&mut self, opcode: NyarHeadCode, operands: Vec<GenerateOperand>) {
        let function = self.current_function.as_mut().expect("begin_function first");
        function.add_instruction(GenerateInstruction::new(opcode, operands));
    }

    /// Emit an instruction without operands.
    pub fn emit0(&mut self, opcode: NyarHeadCode) {
        self.emit(opcode, Vec::new());
    }

    /// Emit an `i64` constant and return its pool index.
    pub fn emit_const_i64(&mut self, value: i64) -> i32 {
        let index = self.module.constants.add_int64(value);
        self.emit(NyarHeadCode::Const, vec![GenerateOperand::Const { pool_index: index }]);
        index
    }

    /// Emit a bool constant.
    pub fn emit_const_bool(&mut self, value: bool) -> i32 {
        let index = self.module.constants.add_bool(value);
        self.emit(NyarHeadCode::Const, vec![GenerateOperand::Const { pool_index: index }]);
        index
    }

    /// Emit a null constant.
    pub fn emit_const_null(&mut self) -> i32 {
        let index = self.module.constants.add_null();
        self.emit(NyarHeadCode::Const, vec![GenerateOperand::Const { pool_index: index }]);
        index
    }

    /// Emit a string constant and return its pool index.
    pub fn emit_const_string(&mut self, value: impl Into<String>) -> i32 {
        let index = self.module.constants.add_string(value);
        self.emit(NyarHeadCode::Const, vec![GenerateOperand::Const { pool_index: index }]);
        index
    }

    /// Emit `LoadLocal`.
    pub fn emit_load_local(&mut self, name: &str) {
        let index = self.ensure_local(name);
        self.emit(NyarHeadCode::LoadLocal, vec![GenerateOperand::Local { index }]);
    }

    /// Emit `StoreLocal`.
    pub fn emit_store_local(&mut self, name: &str) {
        let index = self.ensure_local(name);
        self.emit(NyarHeadCode::StoreLocal, vec![GenerateOperand::Local { index }]);
    }

    /// Emit a jump to a label.
    pub fn emit_jump(&mut self, opcode: NyarHeadCode, label: &str) {
        self.emit(opcode, vec![GenerateOperand::Label { name: label.to_string() }]);
    }

    /// Emit `CallNative(name, argc)`.
    pub fn emit_call_native(&mut self, name: &str, argc: i32) {
        let index = self.module.constants.add_string(name);
        self.emit(NyarHeadCode::CallNative, vec![GenerateOperand::I32(index), GenerateOperand::I32(argc)]);
    }

    /// Emit `Call` by function name.
    pub fn emit_call(&mut self, name: &str) {
        self.emit(NyarHeadCode::Call, vec![GenerateOperand::FuncRef { name: name.to_string() }]);
    }

    /// Emit `return`.
    pub fn emit_return(&mut self) {
        self.emit0(NyarHeadCode::Return);
    }

    /// Emit a minimal `main` that returns an integer constant.
    pub fn emit_main_return_i64(&mut self, value: i64) {
        self.begin_function("main");
        self.emit_const_i64(value);
        self.emit_return();
        self.end_function();
    }
}
