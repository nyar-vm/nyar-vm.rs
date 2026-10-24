//! **Probe-only** residual sink → [`StackCompiler`] / `.nyar` bytecode.
//!
//! This is **not** the host-script PE product path. Product specialization is
//! [`super::native_residual`] (`NativeResidualSink` → native / x64 / PE).
//! Kept for JS/Python bytecode-pipe probes and temporary stack→`.nyar` experiments.

use nyar_language::ResidualSink;
use std_data::binary::nyar_ir::NyarHeadCode;

use super::{GenerateModule, StackCompiler};

/// Probe [`ResidualSink`] backed by [`StackCompiler`] (`.nyar` / nyar-vm lane).
///
/// Do **not** use this as the Lua / host-script PE destination — use
/// [`super::NativeResidualSink`] instead.
pub struct StackResidualSink {
    compiler: StackCompiler,
}

impl StackResidualSink {
    /// Create a probe sink for `module_name`.
    pub fn new(module_name: impl Into<String>) -> Self {
        Self { compiler: StackCompiler::new(module_name) }
    }

    /// Finish the probe module.
    pub fn finish(self) -> GenerateModule {
        self.compiler.finish()
    }
}

impl ResidualSink for StackResidualSink {
    fn push_i64(&mut self, value: i64) {
        self.compiler.emit_const_i64(value);
    }

    fn push_bool(&mut self, value: bool) {
        self.compiler.emit_const_bool(value);
    }

    fn push_string(&mut self, value: &str) {
        self.compiler.emit_const_string(value);
    }

    fn push_nil(&mut self) {
        self.compiler.emit_const_null();
    }

    fn load(&mut self, name: &str) {
        self.compiler.emit_load_local(name);
    }

    fn store(&mut self, name: &str) {
        self.compiler.emit_store_local(name);
    }

    fn pop(&mut self) {
        self.compiler.emit0(NyarHeadCode::Pop);
    }

    fn add(&mut self) {
        self.compiler.emit0(NyarHeadCode::I32Add);
    }

    fn sub(&mut self) {
        self.compiler.emit0(NyarHeadCode::I32Sub);
    }

    fn mul(&mut self) {
        self.compiler.emit0(NyarHeadCode::I32Mul);
    }

    fn div(&mut self) {
        self.compiler.emit0(NyarHeadCode::I32DivS);
    }

    fn rem(&mut self) {
        self.compiler.emit0(NyarHeadCode::I32RemS);
    }

    fn eq(&mut self) {
        self.compiler.emit0(NyarHeadCode::I32Eq);
    }

    fn ne(&mut self) {
        self.compiler.emit0(NyarHeadCode::I32Ne);
    }

    fn lt(&mut self) {
        self.compiler.emit0(NyarHeadCode::I32LtS);
    }

    fn le(&mut self) {
        self.compiler.emit0(NyarHeadCode::I32LeS);
    }

    fn gt(&mut self) {
        self.compiler.emit0(NyarHeadCode::I32GtS);
    }

    fn ge(&mut self) {
        self.compiler.emit0(NyarHeadCode::I32GeS);
    }

    fn concat(&mut self) {
        self.compiler.emit_call_native("string_concat", 2);
    }

    fn not(&mut self) {
        let false_l = unique_label("not_false");
        let end_l = unique_label("not_end");
        self.compiler.emit_jump(NyarHeadCode::JumpIfFalse, &false_l);
        self.compiler.emit_const_bool(false);
        self.compiler.emit_jump(NyarHeadCode::Jump, &end_l);
        self.compiler.emit_label(&false_l);
        self.compiler.emit_const_bool(true);
        self.compiler.emit_label(&end_l);
    }

    fn print(&mut self, argc: usize) {
        self.compiler.emit_call_native("print", argc as i32);
    }

    fn label(&mut self, name: &str) {
        self.compiler.emit_label(name);
    }

    fn jump(&mut self, label: &str) {
        self.compiler.emit_jump(NyarHeadCode::Jump, label);
    }

    fn jump_if_false(&mut self, label: &str) {
        self.compiler.emit_jump(NyarHeadCode::JumpIfFalse, label);
    }

    fn jump_if_true(&mut self, label: &str) {
        self.compiler.emit_jump(NyarHeadCode::JumpIfTrue, label);
    }

    fn begin_function(&mut self, name: &str) {
        self.compiler.begin_function(name);
    }

    fn add_parameter(&mut self, name: &str) {
        self.compiler.add_parameter(name);
    }

    fn end_function(&mut self) {
        self.compiler.end_function();
    }

    fn call(&mut self, name: &str, argc: usize) {
        let _ = argc;
        self.compiler.emit_call(name);
    }

    fn ret(&mut self) {
        self.compiler.emit_return();
    }
}

fn unique_label(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("__sink_{prefix}_{id}")
}
