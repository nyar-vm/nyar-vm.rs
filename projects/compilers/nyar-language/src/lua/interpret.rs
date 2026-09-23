//! Tree-walking Lua interpreter (legend mid-subset: control flow + tables).

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use std_data::text::lua::{LuaError, LuaExpr, LuaLValue, LuaScript, LuaStmt, LuaTableField};

/// Runtime table (array part + string-keyed map).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LuaTable {
    /// 1-based array part stored densely from index 1.
    pub array: Vec<LuaValue>,
    /// String-keyed map part.
    pub map: HashMap<String, LuaValue>,
}

impl LuaTable {
    fn length(&self) -> i64 {
        self.array.len() as i64
    }

    fn get(&self, key: &LuaValue) -> LuaValue {
        if let Some(index) = integer_key(key) {
            if index >= 1 {
                return self.array.get((index - 1) as usize).cloned().unwrap_or(LuaValue::Null);
            }
        }
        self.map.get(&key.to_string_value()).cloned().unwrap_or(LuaValue::Null)
    }

    fn set(&mut self, key: &LuaValue, value: LuaValue) {
        if let Some(index) = integer_key(key) {
            if index >= 1 {
                let idx = (index - 1) as usize;
                if matches!(value, LuaValue::Null) {
                    if idx < self.array.len() {
                        self.array[idx] = LuaValue::Null;
                        while self.array.last().map(LuaValue::is_null).unwrap_or(false) {
                            self.array.pop();
                        }
                    }
                    return;
                }
                if idx >= self.array.len() {
                    self.array.resize(idx + 1, LuaValue::Null);
                }
                self.array[idx] = value;
                return;
            }
        }
        let map_key = key.to_string_value();
        if matches!(value, LuaValue::Null) {
            self.map.remove(&map_key);
        }
        else {
            self.map.insert(map_key, value);
        }
    }
}

/// Runtime value for the Lua tree interpreter.
#[derive(Debug, Clone)]
pub enum LuaValue {
    /// Unit / nil.
    Null,
    /// Boolean.
    Bool(bool),
    /// Signed integer.
    Int(i64),
    /// Floating point.
    Float(f64),
    /// UTF-8 string.
    String(String),
    /// Table (shared by reference).
    Table(Rc<RefCell<LuaTable>>),
}

impl PartialEq for LuaValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Bool(left), Self::Bool(right)) => left == right,
            (Self::String(left), Self::String(right)) => left == right,
            (Self::Int(_) | Self::Float(_), Self::Int(_) | Self::Float(_)) => (self.to_f64() - other.to_f64()).abs() < f64::EPSILON,
            (Self::Table(left), Self::Table(right)) => Rc::ptr_eq(left, right) || *left.borrow() == *right.borrow(),
            _ => false,
        }
    }
}

impl LuaValue {
    fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Coerce to `i64`.
    pub fn to_i64(&self) -> i64 {
        match self {
            Self::Int(value) => *value,
            Self::Float(value) => *value as i64,
            Self::Bool(value) => i64::from(*value),
            Self::String(value) => value.parse().unwrap_or(0),
            Self::Null | Self::Table(_) => 0,
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
            Self::Null | Self::Table(_) => 0.0,
        }
    }

    /// Lua-like truthiness: only `nil` and `false` are false.
    pub fn to_bool(&self) -> bool {
        match self {
            Self::Bool(value) => *value,
            Self::Null => false,
            _ => true,
        }
    }

    /// Display form used by `print` / legend fixtures.
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
            Self::Table(table) => {
                let table = table.borrow();
                format!("table:#{}", table.length())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Flow {
    Value(LuaValue),
    Return(LuaValue),
    Break,
}

#[derive(Clone)]
struct LuaFunction {
    params: Vec<String>,
    body: Vec<LuaStmt>,
}

/// Evaluate Lua source and return the script result (or last expression).
pub fn evaluate_lua_source(source: &str, env: &mut HashMap<String, LuaValue>) -> Result<LuaValue, LuaError> {
    let script = LuaScript::parse(source)?;
    Ok(evaluate_lua_script(&script, env))
}

/// Evaluate a parsed Lua script.
pub fn evaluate_lua_script(script: &LuaScript, env: &mut HashMap<String, LuaValue>) -> LuaValue {
    let mut locals = env.clone();
    let mut functions: HashMap<String, LuaFunction> = HashMap::new();
    let mut last = Flow::Value(LuaValue::Null);

    for stmt in &script.statements {
        last = eval_stmt(stmt, &mut locals, &mut functions);
        if let Flow::Return(value) = last {
            last = Flow::Value(value);
            break;
        }
        if matches!(last, Flow::Break) {
            last = Flow::Value(LuaValue::Null);
            break;
        }
    }

    env.clear();
    env.extend(locals);

    match last {
        Flow::Value(value) | Flow::Return(value) => value,
        Flow::Break => LuaValue::Null,
    }
}

fn eval_stmt(stmt: &LuaStmt, env: &mut HashMap<String, LuaValue>, functions: &mut HashMap<String, LuaFunction>) -> Flow {
    match stmt {
        LuaStmt::LocalDecl { names, values } => {
            let evaluated: Vec<LuaValue> = values.iter().map(|expr| eval_expr(expr, env, functions)).collect();
            for (index, name) in names.iter().enumerate() {
                let value = evaluated.get(index).cloned().unwrap_or(LuaValue::Null);
                env.insert(name.clone(), value);
            }
            Flow::Value(LuaValue::Null)
        }
        LuaStmt::Assign { targets, values } => {
            let evaluated: Vec<LuaValue> = values.iter().map(|expr| eval_expr(expr, env, functions)).collect();
            let mut last = LuaValue::Null;
            for (index, target) in targets.iter().enumerate() {
                let value = evaluated.get(index).cloned().unwrap_or(LuaValue::Null);
                assign_lvalue(target, value.clone(), env, functions);
                last = value;
            }
            Flow::Value(last)
        }
        LuaStmt::If { condition, then_block, else_block } => {
            let cond = eval_expr(condition, env, functions);
            if cond.to_bool() { eval_block(then_block, env, functions) } else { eval_block(else_block, env, functions) }
        }
        LuaStmt::While { condition, body } => {
            let mut last = Flow::Value(LuaValue::Null);
            while eval_expr(condition, env, functions).to_bool() {
                last = eval_block(body, env, functions);
                match &last {
                    Flow::Return(_) | Flow::Break => break,
                    Flow::Value(_) => {}
                }
            }
            match last {
                Flow::Break => Flow::Value(LuaValue::Null),
                other => other,
            }
        }
        LuaStmt::Repeat { body, condition } => {
            let mut last = Flow::Value(LuaValue::Null);
            loop {
                last = eval_block(body, env, functions);
                match &last {
                    Flow::Return(_) | Flow::Break => break,
                    Flow::Value(_) => {}
                }
                if eval_expr(condition, env, functions).to_bool() {
                    break;
                }
            }
            match last {
                Flow::Break => Flow::Value(LuaValue::Null),
                other => other,
            }
        }
        LuaStmt::ForNumeric { name, start, limit, step, body } => {
            let mut current = eval_expr(start, env, functions).to_f64();
            let limit_val = eval_expr(limit, env, functions).to_f64();
            let step_val = step.as_ref().map(|expr| eval_expr(expr, env, functions).to_f64()).unwrap_or(1.0);
            if step_val == 0.0 {
                return Flow::Value(LuaValue::Null);
            }
            let mut last = Flow::Value(LuaValue::Null);
            loop {
                let continue_loop = if step_val > 0.0 { current <= limit_val } else { current >= limit_val };
                if !continue_loop {
                    break;
                }
                env.insert(name.clone(), number_value(current));
                last = eval_block(body, env, functions);
                match &last {
                    Flow::Return(_) | Flow::Break => break,
                    Flow::Value(_) => {}
                }
                current += step_val;
            }
            match last {
                Flow::Break => Flow::Value(LuaValue::Null),
                other => other,
            }
        }
        LuaStmt::Break => Flow::Break,
        LuaStmt::Return(value) => {
            let result = value.as_ref().map(|expr| eval_expr(expr, env, functions)).unwrap_or(LuaValue::Null);
            Flow::Return(result)
        }
        LuaStmt::ExprStmt(expr) => Flow::Value(eval_expr(expr, env, functions)),
        LuaStmt::FunctionDef { name, params, body } => {
            functions.insert(name.clone(), LuaFunction { params: params.clone(), body: body.clone() });
            Flow::Value(LuaValue::Null)
        }
        LuaStmt::Block(statements) => eval_block(statements, env, functions),
    }
}

fn eval_block(statements: &[LuaStmt], env: &mut HashMap<String, LuaValue>, functions: &mut HashMap<String, LuaFunction>) -> Flow {
    let mut last = Flow::Value(LuaValue::Null);
    for stmt in statements {
        last = eval_stmt(stmt, env, functions);
        if matches!(last, Flow::Return(_) | Flow::Break) {
            break;
        }
    }
    last
}

fn assign_lvalue(target: &LuaLValue, value: LuaValue, env: &mut HashMap<String, LuaValue>, functions: &HashMap<String, LuaFunction>) {
    match target {
        LuaLValue::Name(name) => {
            env.insert(name.clone(), value);
        }
        LuaLValue::Field { table, name } => {
            if let LuaValue::Table(tbl) = eval_expr(table, env, functions) {
                tbl.borrow_mut().map.insert(name.clone(), value);
            }
        }
        LuaLValue::Index { table, key } => {
            let table_val = eval_expr(table, env, functions);
            let key_val = eval_expr(key, env, functions);
            if let LuaValue::Table(tbl) = table_val {
                tbl.borrow_mut().set(&key_val, value);
            }
        }
    }
}

fn eval_expr(expr: &LuaExpr, env: &mut HashMap<String, LuaValue>, functions: &HashMap<String, LuaFunction>) -> LuaValue {
    match expr {
        LuaExpr::Number(value) => number_value(*value),
        LuaExpr::String(value) => LuaValue::String(value.clone()),
        LuaExpr::Bool(value) => LuaValue::Bool(*value),
        LuaExpr::Nil => LuaValue::Null,
        LuaExpr::Ident(name) => env.get(name).cloned().unwrap_or(LuaValue::Null),
        LuaExpr::Table { fields } => {
            let table = Rc::new(RefCell::new(LuaTable::default()));
            for field in fields {
                match field {
                    LuaTableField::Array(value) => {
                        let value = eval_expr(value, env, functions);
                        table.borrow_mut().array.push(value);
                    }
                    LuaTableField::Record { key, value } => {
                        let value = eval_expr(value, env, functions);
                        table.borrow_mut().map.insert(key.clone(), value);
                    }
                    LuaTableField::Indexed { key, value } => {
                        let key = eval_expr(key, env, functions);
                        let value = eval_expr(value, env, functions);
                        table.borrow_mut().set(&key, value);
                    }
                }
            }
            LuaValue::Table(table)
        }
        LuaExpr::Index { table, key } => {
            let table_val = eval_expr(table, env, functions);
            let key_val = eval_expr(key, env, functions);
            match table_val {
                LuaValue::Table(tbl) => tbl.borrow().get(&key_val),
                LuaValue::String(text) => {
                    let index = key_val.to_i64();
                    if index >= 1 {
                        text.chars().nth((index - 1) as usize).map(|ch| LuaValue::String(ch.to_string())).unwrap_or(LuaValue::Null)
                    }
                    else {
                        LuaValue::Null
                    }
                }
                _ => LuaValue::Null,
            }
        }
        LuaExpr::Field { table, name } => {
            let table_val = eval_expr(table, env, functions);
            match table_val {
                LuaValue::Table(tbl) => tbl.borrow().map.get(name).cloned().unwrap_or(LuaValue::Null),
                _ => LuaValue::Null,
            }
        }
        LuaExpr::Binary { op, left, right } => match op.as_str() {
            "and" => {
                let left_val = eval_expr(left, env, functions);
                if !left_val.to_bool() { left_val } else { eval_expr(right, env, functions) }
            }
            "or" => {
                let left_val = eval_expr(left, env, functions);
                if left_val.to_bool() { left_val } else { eval_expr(right, env, functions) }
            }
            _ => {
                let left_val = eval_expr(left, env, functions);
                let right_val = eval_expr(right, env, functions);
                match op.as_str() {
                    "+" => add(&left_val, &right_val),
                    "-" => numeric_bin(&left_val, &right_val, |a, b| a - b),
                    "*" => numeric_bin(&left_val, &right_val, |a, b| a * b),
                    "/" => div(&left_val, &right_val),
                    "%" => {
                        let divisor = right_val.to_i64();
                        if divisor == 0 { LuaValue::Int(0) } else { LuaValue::Int(left_val.to_i64() % divisor) }
                    }
                    ".." => LuaValue::String(format!("{}{}", left_val.to_string_value(), right_val.to_string_value())),
                    "==" => LuaValue::Bool(eq(&left_val, &right_val)),
                    "~=" => LuaValue::Bool(!eq(&left_val, &right_val)),
                    "<" => LuaValue::Bool(left_val.to_f64() < right_val.to_f64()),
                    "<=" => LuaValue::Bool(left_val.to_f64() <= right_val.to_f64()),
                    ">" => LuaValue::Bool(left_val.to_f64() > right_val.to_f64()),
                    ">=" => LuaValue::Bool(left_val.to_f64() >= right_val.to_f64()),
                    _ => LuaValue::Null,
                }
            }
        },
        LuaExpr::Unary { op, operand } => {
            let value = eval_expr(operand, env, functions);
            match op.as_str() {
                "not" => LuaValue::Bool(!value.to_bool()),
                "-" => numeric_bin(&LuaValue::Int(0), &value, |a, b| a - b),
                "#" => match value {
                    LuaValue::Table(table) => LuaValue::Int(table.borrow().length()),
                    LuaValue::String(text) => LuaValue::Int(text.chars().count() as i64),
                    other => LuaValue::Int(other.to_string_value().len() as i64),
                },
                _ => LuaValue::Null,
            }
        }
        LuaExpr::Call { name, args } => {
            let evaluated: Vec<LuaValue> = args.iter().map(|arg| eval_expr(arg, env, functions)).collect();
            if name == "print" {
                let text = evaluated.iter().map(LuaValue::to_string_value).collect::<Vec<_>>().join("\t");
                println!("{text}");
                return evaluated.last().cloned().unwrap_or(LuaValue::Null);
            }
            if name == "table_insert" {
                if let (Some(LuaValue::Table(table)), Some(value)) = (evaluated.first(), evaluated.get(1)) {
                    table.borrow_mut().array.push(value.clone());
                    return LuaValue::Null;
                }
                return LuaValue::Null;
            }
            if let Some(func) = functions.get(name).cloned() {
                // Snapshot outer bindings so nested functions can read enclosing locals
                // (demo closure-lite; not full upvalue semantics).
                let mut frame = env.clone();
                for (index, param) in func.params.iter().enumerate() {
                    frame.insert(param.clone(), evaluated.get(index).cloned().unwrap_or(LuaValue::Null));
                }
                let result = eval_block(&func.body, &mut frame, &mut functions.clone());
                return match result {
                    Flow::Return(value) | Flow::Value(value) => value,
                    Flow::Break => LuaValue::Null,
                };
            }
            LuaValue::Null
        }
    }
}

fn integer_key(key: &LuaValue) -> Option<i64> {
    match key {
        LuaValue::Int(value) if *value >= 1 => Some(*value),
        LuaValue::Float(value) if value.fract() == 0.0 && *value >= 1.0 && value.is_finite() => Some(*value as i64),
        _ => None,
    }
}

fn number_value(value: f64) -> LuaValue {
    if value.fract() == 0.0 && value.is_finite() { LuaValue::Int(value as i64) } else { LuaValue::Float(value) }
}

fn is_integral(value: &LuaValue) -> bool {
    matches!(value, LuaValue::Int(_))
}

fn add(left: &LuaValue, right: &LuaValue) -> LuaValue {
    if let (LuaValue::String(left), LuaValue::String(right)) = (left, right) {
        return LuaValue::String(format!("{left}{right}"));
    }
    numeric_bin(left, right, |a, b| a + b)
}

fn div(left: &LuaValue, right: &LuaValue) -> LuaValue {
    let divisor = right.to_f64();
    if divisor == 0.0 {
        return LuaValue::Float(f64::NAN);
    }
    let result = left.to_f64() / divisor;
    if is_integral(left) && is_integral(right) { LuaValue::Int(result as i64) } else { LuaValue::Float(result) }
}

fn numeric_bin(left: &LuaValue, right: &LuaValue, op: impl Fn(f64, f64) -> f64) -> LuaValue {
    let result = op(left.to_f64(), right.to_f64());
    if is_integral(left) && is_integral(right) { LuaValue::Int(result as i64) } else { LuaValue::Float(result) }
}

fn eq(left: &LuaValue, right: &LuaValue) -> bool {
    left == right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_loops_and_concat() {
        let mut env = HashMap::new();
        let source = r#"
local t = {10, 20, name = "lua"}
local sum = 0
for i = 1, #t do
  sum = sum + t[i]
end
local n = 0
repeat
  n = n + 1
until n >= 2
if sum == 30 and n == 2 then
  print(t.name .. ":" .. sum)
end
"#;
        let result = evaluate_lua_source(source, &mut env).expect("eval");
        assert_eq!(result.to_string_value(), "lua:30");
    }
}
