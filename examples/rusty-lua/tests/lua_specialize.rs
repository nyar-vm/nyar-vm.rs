use nyar_analyzer::residual::ResidualSink;
use rusty_lua::*;

struct CountingSink {
    ops: usize,
}

impl ResidualSink for CountingSink {
    fn push_i64(&mut self, _: i64) {
        self.ops += 1;
    }
    fn push_bool(&mut self, _: bool) {
        self.ops += 1;
    }
    fn push_string(&mut self, _: &str) {
        self.ops += 1;
    }
    fn push_nil(&mut self) {
        self.ops += 1;
    }
    fn load(&mut self, _: &str) {
        self.ops += 1;
    }
    fn store(&mut self, _: &str) {
        self.ops += 1;
    }
    fn pop(&mut self) {
        self.ops += 1;
    }
    fn add(&mut self) {
        self.ops += 1;
    }
    fn sub(&mut self) {
        self.ops += 1;
    }
    fn mul(&mut self) {
        self.ops += 1;
    }
    fn div(&mut self) {
        self.ops += 1;
    }
    fn rem(&mut self) {
        self.ops += 1;
    }
    fn eq(&mut self) {
        self.ops += 1;
    }
    fn ne(&mut self) {
        self.ops += 1;
    }
    fn lt(&mut self) {
        self.ops += 1;
    }
    fn le(&mut self) {
        self.ops += 1;
    }
    fn gt(&mut self) {
        self.ops += 1;
    }
    fn ge(&mut self) {
        self.ops += 1;
    }
    fn concat(&mut self) {
        self.ops += 1;
    }
    fn not(&mut self) {
        self.ops += 1;
    }
    fn print(&mut self, _: usize) {
        self.ops += 1;
    }
    fn label(&mut self, _: &str) {
        self.ops += 1;
    }
    fn jump(&mut self, _: &str) {
        self.ops += 1;
    }
    fn jump_if_false(&mut self, _: &str) {
        self.ops += 1;
    }
    fn jump_if_true(&mut self, _: &str) {
        self.ops += 1;
    }
    fn begin_function(&mut self, _: &str) {
        self.ops += 1;
    }
    fn add_parameter(&mut self, _: &str) {}
    fn end_function(&mut self) {
        self.ops += 1;
    }
    fn call(&mut self, _: &str, _: usize) {
        self.ops += 1;
    }
    fn ret(&mut self) {
        self.ops += 1;
    }
}

#[test]
fn specializes_arithmetic_program() {
    let mut sink = CountingSink { ops: 0 };
    specialize_lua_into("local x = 1 + 2\nreturn x\n", &mut sink).expect("specialize");
    assert!(sink.ops > 5);
}
