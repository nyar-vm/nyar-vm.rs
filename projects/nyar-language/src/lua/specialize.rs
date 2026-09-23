//! PE / Futamura 1st projection for Lua: specialize interpret w.r.t. a concrete program.
//!
//! Residualizes the same match arms as [`super::interpret`] into a [`ResidualSink`],
//! rather than a hand-written divergent Lua compiler.

use std::collections::HashMap;

use std_data::text::lua::{LuaExpr, LuaLValue, LuaScript, LuaStmt};

use crate::pe::ResidualSink;

/// Specialize Lua `source` into `sink` (Futamura projection 1).
pub fn specialize_lua_into(source: &str, sink: &mut dyn ResidualSink) -> Result<(), String> {
    let script = LuaScript::parse(source).map_err(|error| format!("lua parse: {error}"))?;
    specialize_lua_script(&script, sink)
}

/// Specialize a parsed Lua script into `sink`.
pub fn specialize_lua_script(script: &LuaScript, sink: &mut dyn ResidualSink) -> Result<(), String> {
    let mut functions: HashMap<String, (Vec<String>, Vec<LuaStmt>)> = HashMap::new();
    let mut top_level = Vec::new();

    for stmt in &script.statements {
        if let LuaStmt::FunctionDef { name, params, body } = stmt {
            functions.insert(name.clone(), (params.clone(), body.clone()));
        }
        else {
            top_level.push(stmt.clone());
        }
    }

    for (name, (params, body)) in &functions {
        sink.begin_function(name);
        for param in params {
            sink.add_parameter(param);
        }
        residualize_block(body, sink, &functions, None)?;
        sink.push_nil();
        sink.ret();
        sink.end_function();
    }

    sink.begin_function("main");
    sink.push_nil();
    sink.store("__pe_last");
    residualize_block(&top_level, sink, &functions, None)?;
    sink.load("__pe_last");
    sink.ret();
    sink.end_function();
    Ok(())
}

fn residualize_block(
    statements: &[LuaStmt],
    sink: &mut dyn ResidualSink,
    functions: &HashMap<String, (Vec<String>, Vec<LuaStmt>)>,
    break_label: Option<&str>,
) -> Result<(), String> {
    for stmt in statements {
        residualize_stmt(stmt, sink, functions, break_label)?;
    }
    Ok(())
}

fn residualize_stmt(
    stmt: &LuaStmt,
    sink: &mut dyn ResidualSink,
    functions: &HashMap<String, (Vec<String>, Vec<LuaStmt>)>,
    break_label: Option<&str>,
) -> Result<(), String> {
    match stmt {
        LuaStmt::LocalDecl { names, values } => {
            for (index, name) in names.iter().enumerate() {
                if let Some(expr) = values.get(index) {
                    residualize_expr(expr, sink, functions)?;
                }
                else {
                    sink.push_nil();
                }
                sink.store(name);
            }
            Ok(())
        }
        LuaStmt::Assign { targets, values } => {
            let mut evaluated = Vec::with_capacity(values.len());
            for expr in values {
                residualize_expr(expr, sink, functions)?;
                // stash into temps so multi-assign left-to-right matches interpret
                let temp = format!("__pe_assign_{}", evaluated.len());
                sink.store(&temp);
                evaluated.push(temp);
            }
            for (index, target) in targets.iter().enumerate() {
                if let Some(temp) = evaluated.get(index) {
                    sink.load(temp);
                }
                else {
                    sink.push_nil();
                }
                residualize_assign(target, sink, functions)?;
            }
            Ok(())
        }
        LuaStmt::If { condition, then_block, else_block } => {
            let else_l = unique_label("else");
            let end_l = unique_label("endif");
            residualize_expr(condition, sink, functions)?;
            sink.jump_if_false(&else_l);
            residualize_block(then_block, sink, functions, break_label)?;
            sink.jump(&end_l);
            sink.label(&else_l);
            residualize_block(else_block, sink, functions, break_label)?;
            sink.label(&end_l);
            Ok(())
        }
        LuaStmt::While { condition, body } => {
            let head = unique_label("while_head");
            let end = unique_label("while_end");
            sink.label(&head);
            residualize_expr(condition, sink, functions)?;
            sink.jump_if_false(&end);
            residualize_block(body, sink, functions, Some(&end))?;
            sink.jump(&head);
            sink.label(&end);
            Ok(())
        }
        LuaStmt::Repeat { body, condition } => {
            let head = unique_label("repeat_head");
            let end = unique_label("repeat_end");
            sink.label(&head);
            residualize_block(body, sink, functions, Some(&end))?;
            residualize_expr(condition, sink, functions)?;
            sink.jump_if_false(&head);
            sink.label(&end);
            Ok(())
        }
        LuaStmt::ForNumeric { name, start, limit, step, body } => {
            let head = unique_label("for_head");
            let end = unique_label("for_end");
            let limit_slot = unique_label("for_limit");
            let step_slot = unique_label("for_step");
            residualize_expr(start, sink, functions)?;
            sink.store(name);
            residualize_expr(limit, sink, functions)?;
            sink.store(&limit_slot);
            if let Some(step_expr) = step {
                residualize_expr(step_expr, sink, functions)?;
            }
            else {
                sink.push_i64(1);
            }
            sink.store(&step_slot);
            sink.label(&head);
            // continue when (step>0 && i<=limit) || (step<0 && i>=limit)
            sink.load(&step_slot);
            sink.push_i64(0);
            sink.gt();
            let pos = unique_label("for_pos");
            let neg = unique_label("for_neg");
            let cont = unique_label("for_cont");
            sink.jump_if_true(&pos);
            sink.jump(&neg);
            sink.label(&pos);
            sink.load(name);
            sink.load(&limit_slot);
            sink.le();
            sink.jump_if_false(&end);
            sink.jump(&cont);
            sink.label(&neg);
            sink.load(name);
            sink.load(&limit_slot);
            sink.ge();
            sink.jump_if_false(&end);
            sink.label(&cont);
            residualize_block(body, sink, functions, Some(&end))?;
            sink.load(name);
            sink.load(&step_slot);
            sink.add();
            sink.store(name);
            sink.jump(&head);
            sink.label(&end);
            Ok(())
        }
        LuaStmt::Break => {
            let Some(label) = break_label
            else {
                return sink.unsupported("break outside loop");
            };
            sink.jump(label);
            Ok(())
        }
        LuaStmt::Return(value) => {
            if let Some(expr) = value {
                residualize_expr(expr, sink, functions)?;
            }
            else {
                sink.push_nil();
            }
            sink.store("__pe_last");
            sink.load("__pe_last");
            sink.ret();
            Ok(())
        }
        LuaStmt::ExprStmt(expr) => {
            residualize_expr(expr, sink, functions)?;
            // keep last expr result in `__pe_last` for main fallthrough (interpret returns it)
            sink.store("__pe_last");
            Ok(())
        }
        LuaStmt::FunctionDef { .. } => Ok(()), // already hoisted
        LuaStmt::Block(statements) => residualize_block(statements, sink, functions, break_label),
    }
}

fn residualize_assign(
    target: &LuaLValue,
    sink: &mut dyn ResidualSink,
    _functions: &HashMap<String, (Vec<String>, Vec<LuaStmt>)>,
) -> Result<(), String> {
    match target {
        LuaLValue::Name(name) => {
            sink.store(name);
            Ok(())
        }
        LuaLValue::Field { .. } | LuaLValue::Index { .. } => sink.unsupported("table assignment"),
    }
}

fn residualize_expr(
    expr: &LuaExpr,
    sink: &mut dyn ResidualSink,
    functions: &HashMap<String, (Vec<String>, Vec<LuaStmt>)>,
) -> Result<(), String> {
    match expr {
        LuaExpr::Number(value) => {
            if value.fract() == 0.0 && value.is_finite() {
                sink.push_i64(*value as i64);
            }
            else {
                return sink.unsupported("float literal");
            }
            Ok(())
        }
        LuaExpr::String(value) => {
            sink.push_string(value);
            Ok(())
        }
        LuaExpr::Bool(value) => {
            sink.push_bool(*value);
            Ok(())
        }
        LuaExpr::Nil => {
            sink.push_nil();
            Ok(())
        }
        LuaExpr::Ident(name) => {
            sink.load(name);
            Ok(())
        }
        LuaExpr::Table { .. } | LuaExpr::Index { .. } | LuaExpr::Field { .. } => sink.unsupported("tables"),
        LuaExpr::Binary { op, left, right } => match op.as_str() {
            "and" => {
                let false_l = unique_label("and_false");
                let end_l = unique_label("and_end");
                residualize_expr(left, sink, functions)?;
                sink.jump_if_false(&false_l);
                residualize_expr(right, sink, functions)?;
                sink.jump(&end_l);
                sink.label(&false_l);
                sink.push_bool(false);
                sink.label(&end_l);
                Ok(())
            }
            "or" => {
                let true_l = unique_label("or_true");
                let end_l = unique_label("or_end");
                residualize_expr(left, sink, functions)?;
                sink.jump_if_true(&true_l);
                residualize_expr(right, sink, functions)?;
                sink.jump(&end_l);
                sink.label(&true_l);
                sink.push_bool(true);
                sink.label(&end_l);
                Ok(())
            }
            _ => {
                residualize_expr(left, sink, functions)?;
                residualize_expr(right, sink, functions)?;
                match op.as_str() {
                    "+" => sink.add(),
                    "-" => sink.sub(),
                    "*" => sink.mul(),
                    "/" => sink.div(),
                    "%" => sink.rem(),
                    ".." => sink.concat(),
                    "==" => sink.eq(),
                    "~=" => sink.ne(),
                    "<" => sink.lt(),
                    "<=" => sink.le(),
                    ">" => sink.gt(),
                    ">=" => sink.ge(),
                    other => return sink.unsupported(&format!("binary op '{other}'")),
                }
                Ok(())
            }
        },
        LuaExpr::Unary { op, operand } => {
            residualize_expr(operand, sink, functions)?;
            match op.as_str() {
                "not" => {
                    sink.not();
                    Ok(())
                }
                "-" => {
                    // operand already on stack → 0 - operand
                    sink.store("__pe_neg");
                    sink.push_i64(0);
                    sink.load("__pe_neg");
                    sink.sub();
                    Ok(())
                }
                "#" => sink.unsupported("length operator"),
                other => sink.unsupported(&format!("unary op '{other}'")),
            }
        }
        LuaExpr::Call { name, args } => {
            for arg in args {
                residualize_expr(arg, sink, functions)?;
            }
            if name == "print" {
                sink.print(args.len());
                sink.store("__pe_last");
                sink.load("__pe_last");
                return Ok(());
            }
            if functions.contains_key(name) {
                sink.call(name, args.len());
                return Ok(());
            }
            sink.unsupported(&format!("call to '{name}'"))
        }
    }
}

fn unique_label(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("__{prefix}_{id}")
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
