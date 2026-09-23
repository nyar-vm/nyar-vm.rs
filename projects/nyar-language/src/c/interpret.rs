//! Tree-walking C interpreter (legend demo subset).

use std::collections::HashMap;

use std_data::text::c::{CError, CExpr, CFunction, CItem, CScript, CStmt, CVarDecl};

/// Runtime value for the C tree interpreter.
#[derive(Debug, Clone, PartialEq)]
pub enum CValue {
    /// Unit / uninitialized.
    Null,
    /// Boolean.
    Bool(bool),
    /// Signed integer.
    Int(i64),
    /// Floating point.
    Float(f64),
    /// UTF-8 string.
    String(String),
}

impl CValue {
    /// Coerce to `i64`.
    pub fn to_i64(&self) -> i64 {
        match self {
            Self::Int(value) => *value,
            Self::Float(value) => *value as i64,
            Self::Bool(value) => i64::from(*value),
            Self::String(value) => value.parse().unwrap_or(0),
            Self::Null => 0,
        }
    }

    /// Coerce to `f64`.
    pub fn to_f64(&self) -> f64 {
        match self {
            Self::Float(value) => *value,
            Self::Int(value) => *value as f64,
            Self::Bool(value) => {
                if *value {
                    1.0
                }
                else {
                    0.0
                }
            }
            Self::String(value) => value.parse().unwrap_or(0.0),
            Self::Null => 0.0,
        }
    }

    /// Truthiness (C-like non-zero).
    pub fn to_bool(&self) -> bool {
        match self {
            Self::Bool(value) => *value,
            Self::Int(value) => *value != 0,
            Self::Float(value) => *value != 0.0,
            Self::String(value) => !value.is_empty(),
            Self::Null => false,
        }
    }

    /// Display form used by printf / legend fixtures.
    pub fn to_string_value(&self) -> String {
        match self {
            Self::Null => String::new(),
            Self::Bool(value) => value.to_string(),
            Self::Int(value) => value.to_string(),
            Self::Float(value) => {
                if value.fract() == 0.0 && value.is_finite() {
                    format!("{:.0}", value)
                }
                else {
                    value.to_string()
                }
            }
            Self::String(value) => value.clone(),
        }
    }
}

/// Control-flow / evaluation result.
#[derive(Debug, Clone, PartialEq)]
enum Flow {
    Value(CValue),
    Return(CValue),
    Break,
    Continue,
}

/// Evaluate C source and return the `main` result (or last printf payload).
pub fn evaluate_c_source(source: &str, env: &mut HashMap<String, CValue>) -> Result<CValue, CError> {
    let script = CScript::parse(source)?;
    Ok(evaluate_c_script(&script, env))
}

/// Evaluate a parsed C script.
pub fn evaluate_c_script(script: &CScript, env: &mut HashMap<String, CValue>) -> CValue {
    let mut functions: HashMap<String, CFunction> = HashMap::new();
    let mut globals: HashMap<String, CValue> = env.clone();
    let mut last_print = CValue::Null;

    for item in &script.items {
        match item {
            CItem::Function(func) => {
                functions.insert(func.name.clone(), func.clone());
            }
            CItem::GlobalVar(decl) => {
                let value = decl.init.as_ref().map(|expr| eval_expr(expr, &mut globals, &functions, &mut last_print)).unwrap_or(CValue::Null);
                globals.insert(decl.name.clone(), value);
            }
        }
    }

    let result = if let Some(main) = functions.get("main").cloned() {
        match call_function(&main, &[], &mut globals, &functions, &mut last_print) {
            Flow::Return(value) | Flow::Value(value) => value,
            Flow::Break | Flow::Continue => CValue::Null,
        }
    }
    else {
        CValue::Null
    };

    env.clear();
    env.extend(globals);
    if !matches!(last_print, CValue::Null) && matches!(result, CValue::Int(0) | CValue::Null) {
        last_print
    }
    else if matches!(result, CValue::Null) {
        last_print
    }
    else {
        result
    }
}

fn call_function(
    func: &CFunction,
    args: &[CValue],
    globals: &mut HashMap<String, CValue>,
    functions: &HashMap<String, CFunction>,
    last_print: &mut CValue,
) -> Flow {
    let saved = globals.clone();
    for (index, param) in func.params.iter().enumerate() {
        let value = args.get(index).cloned().unwrap_or(CValue::Null);
        globals.insert(param.clone(), value);
    }
    let result = eval_block(&func.body, globals, functions, last_print);
    // Restore globals but keep mutations to existing global keys that were not params.
    let param_set: HashMap<&str, ()> = func.params.iter().map(|name| (name.as_str(), ())).collect();
    let mut restored = saved;
    for (key, value) in globals.iter() {
        if param_set.contains_key(key.as_str()) {
            continue;
        }
        restored.insert(key.clone(), value.clone());
    }
    *globals = restored;
    result
}

fn eval_block(
    statements: &[CStmt],
    env: &mut HashMap<String, CValue>,
    functions: &HashMap<String, CFunction>,
    last_print: &mut CValue,
) -> Flow {
    let mut last = Flow::Value(CValue::Null);
    for stmt in statements {
        last = eval_stmt(stmt, env, functions, last_print);
        match &last {
            Flow::Return(_) | Flow::Break | Flow::Continue => return last,
            Flow::Value(_) => {}
        }
    }
    last
}

fn eval_stmt(stmt: &CStmt, env: &mut HashMap<String, CValue>, functions: &HashMap<String, CFunction>, last_print: &mut CValue) -> Flow {
    match stmt {
        CStmt::Block(statements) => eval_block(statements, env, functions, last_print),
        CStmt::Decl(decl) => {
            bind_decl(decl, env, functions, last_print);
            Flow::Value(CValue::Null)
        }
        CStmt::Expr(expr) => Flow::Value(eval_expr(expr, env, functions, last_print)),
        CStmt::Return(value) => {
            let result = value.as_ref().map(|expr| eval_expr(expr, env, functions, last_print)).unwrap_or(CValue::Null);
            Flow::Return(result)
        }
        CStmt::If { condition, then_branch, else_branch } => {
            if eval_expr(condition, env, functions, last_print).to_bool() {
                eval_stmt(then_branch, env, functions, last_print)
            }
            else if let Some(else_branch) = else_branch {
                eval_stmt(else_branch, env, functions, last_print)
            }
            else {
                Flow::Value(CValue::Null)
            }
        }
        CStmt::While { condition, body } => {
            let mut last = Flow::Value(CValue::Null);
            while eval_expr(condition, env, functions, last_print).to_bool() {
                last = eval_stmt(body, env, functions, last_print);
                match &last {
                    Flow::Return(_) => return last,
                    Flow::Break => return Flow::Value(CValue::Null),
                    Flow::Continue => continue,
                    Flow::Value(_) => {}
                }
            }
            last
        }
        CStmt::For { init, condition, step, body } => {
            if let Some(init) = init {
                let flow = eval_stmt(init, env, functions, last_print);
                if matches!(flow, Flow::Return(_)) {
                    return flow;
                }
            }
            let mut last = Flow::Value(CValue::Null);
            loop {
                if let Some(condition) = condition {
                    if !eval_expr(condition, env, functions, last_print).to_bool() {
                        break;
                    }
                }
                last = eval_stmt(body, env, functions, last_print);
                match &last {
                    Flow::Return(_) => return last,
                    Flow::Break => return Flow::Value(CValue::Null),
                    Flow::Continue => {}
                    Flow::Value(_) => {}
                }
                if let Some(step) = step {
                    eval_expr(step, env, functions, last_print);
                }
            }
            last
        }
        CStmt::Break => Flow::Break,
        CStmt::Continue => Flow::Continue,
    }
}

fn bind_decl(decl: &CVarDecl, env: &mut HashMap<String, CValue>, functions: &HashMap<String, CFunction>, last_print: &mut CValue) {
    let value = decl.init.as_ref().map(|expr| eval_expr(expr, env, functions, last_print)).unwrap_or(CValue::Null);
    env.insert(decl.name.clone(), value);
}

fn eval_expr(expr: &CExpr, env: &mut HashMap<String, CValue>, functions: &HashMap<String, CFunction>, last_print: &mut CValue) -> CValue {
    match expr {
        CExpr::Int(value) => CValue::Int(*value),
        CExpr::Float(value) => CValue::Float(*value),
        CExpr::String(value) => CValue::String(value.clone()),
        CExpr::Char(value) => CValue::Int(*value),
        CExpr::Ident(name) => env.get(name).cloned().unwrap_or(CValue::Null),
        CExpr::Assign { name, value } => {
            let result = eval_expr(value, env, functions, last_print);
            env.insert(name.clone(), result.clone());
            result
        }
        CExpr::Unary { op, operand } => {
            let value = eval_expr(operand, env, functions, last_print);
            match op.as_str() {
                "!" => CValue::Int(i64::from(!value.to_bool())),
                "-" => {
                    if matches!(value, CValue::Float(_)) {
                        CValue::Float(-value.to_f64())
                    }
                    else {
                        CValue::Int(-value.to_i64())
                    }
                }
                _ => CValue::Null,
            }
        }
        CExpr::Binary { op, left, right } => {
            let left_val = eval_expr(left, env, functions, last_print);
            match op.as_str() {
                "&&" => {
                    if !left_val.to_bool() {
                        return CValue::Int(0);
                    }
                    let right_val = eval_expr(right, env, functions, last_print);
                    return CValue::Int(i64::from(right_val.to_bool()));
                }
                "||" => {
                    if left_val.to_bool() {
                        return CValue::Int(1);
                    }
                    let right_val = eval_expr(right, env, functions, last_print);
                    return CValue::Int(i64::from(right_val.to_bool()));
                }
                _ => {}
            }
            let right_val = eval_expr(right, env, functions, last_print);
            match op.as_str() {
                "+" => numeric_op(&left_val, &right_val, |a, b| a + b, |a, b| a + b),
                "-" => numeric_op(&left_val, &right_val, |a, b| a - b, |a, b| a - b),
                "*" => numeric_op(&left_val, &right_val, |a, b| a * b, |a, b| a * b),
                "/" => {
                    if matches!(left_val, CValue::Float(_)) || matches!(right_val, CValue::Float(_)) {
                        let divisor = right_val.to_f64();
                        if divisor == 0.0 { CValue::Float(f64::NAN) } else { CValue::Float(left_val.to_f64() / divisor) }
                    }
                    else {
                        let divisor = right_val.to_i64();
                        if divisor == 0 { CValue::Int(0) } else { CValue::Int(left_val.to_i64() / divisor) }
                    }
                }
                "%" => {
                    let divisor = right_val.to_i64();
                    if divisor == 0 { CValue::Int(0) } else { CValue::Int(left_val.to_i64() % divisor) }
                }
                "==" => CValue::Int(i64::from(eq(&left_val, &right_val))),
                "!=" => CValue::Int(i64::from(!eq(&left_val, &right_val))),
                "<" => CValue::Int(i64::from(left_val.to_f64() < right_val.to_f64())),
                "<=" => CValue::Int(i64::from(left_val.to_f64() <= right_val.to_f64())),
                ">" => CValue::Int(i64::from(left_val.to_f64() > right_val.to_f64())),
                ">=" => CValue::Int(i64::from(left_val.to_f64() >= right_val.to_f64())),
                _ => CValue::Null,
            }
        }
        CExpr::Call { name, args } => {
            let evaluated: Vec<CValue> = args.iter().map(|arg| eval_expr(arg, env, functions, last_print)).collect();
            if name == "printf" {
                let text = format_printf(&evaluated);
                print!("{text}");
                let rendered = text.trim_end_matches('\n').to_string();
                *last_print = CValue::String(rendered.clone());
                return CValue::Int(text.len() as i64);
            }
            if let Some(func) = functions.get(name).cloned() {
                return match call_function(&func, &evaluated, env, functions, last_print) {
                    Flow::Return(value) | Flow::Value(value) => value,
                    Flow::Break | Flow::Continue => CValue::Null,
                };
            }
            CValue::Null
        }
    }
}

fn numeric_op(left: &CValue, right: &CValue, int_op: fn(i64, i64) -> i64, float_op: fn(f64, f64) -> f64) -> CValue {
    if matches!(left, CValue::Float(_)) || matches!(right, CValue::Float(_)) {
        CValue::Float(float_op(left.to_f64(), right.to_f64()))
    }
    else {
        CValue::Int(int_op(left.to_i64(), right.to_i64()))
    }
}

fn eq(left: &CValue, right: &CValue) -> bool {
    match (left, right) {
        (CValue::String(a), CValue::String(b)) => a == b,
        _ => (left.to_f64() - right.to_f64()).abs() < f64::EPSILON,
    }
}

fn format_printf(args: &[CValue]) -> String {
    let Some(CValue::String(fmt)) = args.first()
    else {
        return args.iter().map(CValue::to_string_value).collect::<Vec<_>>().join(" ");
    };
    let mut out = String::new();
    let mut arg_index = 1usize;
    let mut chars = fmt.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some('d') | Some('i') => {
                out.push_str(&args.get(arg_index).cloned().unwrap_or(CValue::Null).to_i64().to_string());
                arg_index += 1;
            }
            Some('f') => {
                out.push_str(&args.get(arg_index).cloned().unwrap_or(CValue::Null).to_f64().to_string());
                arg_index += 1;
            }
            Some('s') => {
                out.push_str(&args.get(arg_index).cloned().unwrap_or(CValue::Null).to_string_value());
                arg_index += 1;
            }
            Some('c') => {
                let code = args.get(arg_index).cloned().unwrap_or(CValue::Null).to_i64();
                if let Some(ch) = char::from_u32(code as u32) {
                    out.push(ch);
                }
                arg_index += 1;
            }
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_main_printf() {
        let source = r#"
#include <stdio.h>
int main(void) {
    int x = 1 + 2;
    printf("%d\n", x);
    return 0;
}
"#;
        let mut env = HashMap::new();
        let result = evaluate_c_source(source, &mut env).expect("eval");
        assert_eq!(result.to_string_value(), "3");
    }
}
