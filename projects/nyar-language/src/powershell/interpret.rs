//! Tree-walking PowerShell interpreter (legend demo subset).

use std::collections::HashMap;

use std_data::text::powershell::{PowerShellError, PowerShellScript, PsExpr, PsStmt};

/// Runtime value for the PowerShell tree interpreter.
#[derive(Debug, Clone, PartialEq)]
pub enum PowerShellValue {
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

impl PowerShellValue {
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

    /// Truthiness (non-zero / non-empty).
    pub fn to_bool(&self) -> bool {
        match self {
            Self::Bool(value) => *value,
            Self::Null => false,
            Self::Int(value) => *value != 0,
            Self::Float(value) => *value != 0.0,
            Self::String(value) => !value.is_empty(),
        }
    }

    /// Display form used by Write-Output / legend fixtures.
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

#[derive(Debug, Clone, PartialEq)]
enum Flow {
    Value(PowerShellValue),
    Return(PowerShellValue),
    Break,
    Continue,
}

#[derive(Clone)]
struct PsFunction {
    params: Vec<String>,
    body: Vec<PsStmt>,
}

/// Evaluate PowerShell source and return the script / last-print result.
pub fn evaluate_powershell_source(source: &str, env: &mut HashMap<String, PowerShellValue>) -> Result<PowerShellValue, PowerShellError> {
    let script = PowerShellScript::parse(source)?;
    Ok(evaluate_powershell_script(&script, env))
}

/// Evaluate a parsed PowerShell script.
pub fn evaluate_powershell_script(script: &PowerShellScript, env: &mut HashMap<String, PowerShellValue>) -> PowerShellValue {
    let mut locals = env.clone();
    let mut functions: HashMap<String, PsFunction> = HashMap::new();
    let mut last = Flow::Value(PowerShellValue::Null);
    let mut last_print = PowerShellValue::Null;

    for stmt in &script.statements {
        last = eval_stmt(stmt, &mut locals, &mut functions, &mut last_print);
        if let Flow::Return(value) = last {
            last = Flow::Value(value);
            break;
        }
    }

    env.clear();
    env.extend(locals);

    let last_value = match last {
        Flow::Value(value) | Flow::Return(value) => value,
        Flow::Break | Flow::Continue => PowerShellValue::Null,
    };

    match (&last_value, &last_print) {
        (_, PowerShellValue::String(_)) => last_print,
        (PowerShellValue::Null, other) if !matches!(other, PowerShellValue::Null) => other.clone(),
        _ => last_value,
    }
}

fn eval_stmt(
    stmt: &PsStmt,
    env: &mut HashMap<String, PowerShellValue>,
    functions: &mut HashMap<String, PsFunction>,
    last_print: &mut PowerShellValue,
) -> Flow {
    match stmt {
        PsStmt::Block(body) => eval_block(body, env, functions, last_print),
        PsStmt::Assign { name, value } => {
            let result = eval_expr(value, env, functions, last_print);
            env.insert(name.clone(), result.clone());
            Flow::Value(result)
        }
        PsStmt::If { condition, then_branch, else_branch } => {
            if eval_expr(condition, env, functions, last_print).to_bool() {
                eval_block(then_branch, env, functions, last_print)
            }
            else {
                eval_block(else_branch, env, functions, last_print)
            }
        }
        PsStmt::While { condition, body } => {
            let mut last = Flow::Value(PowerShellValue::Null);
            while eval_expr(condition, env, functions, last_print).to_bool() {
                last = eval_block(body, env, functions, last_print);
                match &last {
                    Flow::Return(_) | Flow::Break => break,
                    Flow::Continue => continue,
                    Flow::Value(_) => {}
                }
            }
            match last {
                Flow::Break => Flow::Value(PowerShellValue::Null),
                other => other,
            }
        }
        PsStmt::For { init, condition, step, body } => {
            if let Some(init_stmt) = init {
                let _ = eval_stmt(init_stmt, env, functions, last_print);
            }
            let mut last = Flow::Value(PowerShellValue::Null);
            loop {
                if let Some(cond) = condition
                    && !eval_expr(cond, env, functions, last_print).to_bool()
                {
                    break;
                }
                last = eval_block(body, env, functions, last_print);
                if matches!(last, Flow::Return(_) | Flow::Break) {
                    break;
                }
                if let Some(step_expr) = step {
                    let _ = eval_expr(step_expr, env, functions, last_print);
                }
            }
            match last {
                Flow::Break => Flow::Value(PowerShellValue::Null),
                other => other,
            }
        }
        PsStmt::Function { name, params, body } => {
            functions.insert(name.clone(), PsFunction { params: params.clone(), body: body.clone() });
            Flow::Value(PowerShellValue::Null)
        }
        PsStmt::Return(value) => {
            let result = value.as_ref().map(|expr| eval_expr(expr, env, functions, last_print)).unwrap_or(PowerShellValue::Null);
            Flow::Return(result)
        }
        PsStmt::Expr(expr) => Flow::Value(eval_expr(expr, env, functions, last_print)),
    }
}

fn eval_block(
    statements: &[PsStmt],
    env: &mut HashMap<String, PowerShellValue>,
    functions: &mut HashMap<String, PsFunction>,
    last_print: &mut PowerShellValue,
) -> Flow {
    let mut last = Flow::Value(PowerShellValue::Null);
    for stmt in statements {
        last = eval_stmt(stmt, env, functions, last_print);
        if matches!(last, Flow::Return(_) | Flow::Break | Flow::Continue) {
            break;
        }
    }
    last
}

fn eval_expr(
    expr: &PsExpr,
    env: &mut HashMap<String, PowerShellValue>,
    functions: &HashMap<String, PsFunction>,
    last_print: &mut PowerShellValue,
) -> PowerShellValue {
    match expr {
        PsExpr::Int(value) => PowerShellValue::Int(*value),
        PsExpr::Float(value) => PowerShellValue::Float(*value),
        PsExpr::String(value) => PowerShellValue::String(value.clone()),
        PsExpr::Bool(value) => PowerShellValue::Bool(*value),
        PsExpr::Null => PowerShellValue::Null,
        PsExpr::Var(name) | PsExpr::Ident(name) => env.get(name).cloned().unwrap_or(PowerShellValue::Null),
        PsExpr::Binary { op, left, right } => {
            if op == "=" {
                if let PsExpr::Var(name) = left.as_ref() {
                    let value = eval_expr(right, env, functions, last_print);
                    env.insert(name.clone(), value.clone());
                    return value;
                }
            }
            let left_val = eval_expr(left, env, functions, last_print);
            let right_val = eval_expr(right, env, functions, last_print);
            match op.as_str() {
                "+" => add(&left_val, &right_val),
                "-" => numeric_bin(&left_val, &right_val, |a, b| a - b),
                "*" => numeric_bin(&left_val, &right_val, |a, b| a * b),
                "/" => div(&left_val, &right_val),
                "%" => {
                    let divisor = right_val.to_i64();
                    if divisor == 0 { PowerShellValue::Int(0) } else { PowerShellValue::Int(left_val.to_i64() % divisor) }
                }
                "-eq" => PowerShellValue::Bool(eq(&left_val, &right_val)),
                "-ne" => PowerShellValue::Bool(!eq(&left_val, &right_val)),
                "-lt" => PowerShellValue::Bool(left_val.to_f64() < right_val.to_f64()),
                "-le" => PowerShellValue::Bool(left_val.to_f64() <= right_val.to_f64()),
                "-gt" => PowerShellValue::Bool(left_val.to_f64() > right_val.to_f64()),
                "-ge" => PowerShellValue::Bool(left_val.to_f64() >= right_val.to_f64()),
                "-and" => PowerShellValue::Bool(left_val.to_bool() && right_val.to_bool()),
                "-or" => PowerShellValue::Bool(left_val.to_bool() || right_val.to_bool()),
                "-xor" => PowerShellValue::Bool(left_val.to_bool() ^ right_val.to_bool()),
                "-like" => PowerShellValue::Bool(left_val.to_string_value().contains(&right_val.to_string_value())),
                "-notlike" => PowerShellValue::Bool(!left_val.to_string_value().contains(&right_val.to_string_value())),
                _ => PowerShellValue::Null,
            }
        }
        PsExpr::Unary { op, operand } => {
            let value = eval_expr(operand, env, functions, last_print);
            match op.as_str() {
                "-not" => PowerShellValue::Bool(!value.to_bool()),
                "-" => numeric_bin(&PowerShellValue::Int(0), &value, |a, b| a - b),
                _ => PowerShellValue::Null,
            }
        }
        PsExpr::Call { name, args } => {
            let evaluated: Vec<_> = args.iter().map(|arg| eval_expr(arg, env, functions, last_print)).collect();
            call_name(name, &evaluated, env, functions, last_print)
        }
        PsExpr::Pipeline { left, right } => {
            let piped = eval_expr(left, env, functions, last_print);
            match right.as_ref() {
                PsExpr::Call { name, args } => {
                    let mut evaluated = vec![piped];
                    evaluated.extend(args.iter().map(|arg| eval_expr(arg, env, functions, last_print)));
                    call_name(name, &evaluated, env, functions, last_print)
                }
                PsExpr::Ident(name) => call_name(name, &[piped], env, functions, last_print),
                other => eval_expr(other, env, functions, last_print),
            }
        }
    }
}

fn call_name(
    name: &str,
    args: &[PowerShellValue],
    env: &mut HashMap<String, PowerShellValue>,
    functions: &HashMap<String, PsFunction>,
    last_print: &mut PowerShellValue,
) -> PowerShellValue {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "write-output" | "write-host" | "echo" => {
            let text = args.iter().map(PowerShellValue::to_string_value).collect::<Vec<_>>().join(" ");
            println!("{text}");
            let value = PowerShellValue::String(text);
            *last_print = value.clone();
            return value;
        }
        "get-variable" => {
            let key = args.first().map(PowerShellValue::to_string_value).unwrap_or_default();
            return env.get(&key).cloned().unwrap_or(PowerShellValue::Null);
        }
        "set-variable" => {
            let key = args.first().map(PowerShellValue::to_string_value).unwrap_or_default();
            let value = args.get(1).cloned().unwrap_or(PowerShellValue::Null);
            env.insert(key, value.clone());
            return value;
        }
        _ => {}
    }

    if let Some(func) = functions.get(name).or_else(|| functions.get(&lower)).cloned() {
        let saved = std::mem::take(env);
        for (index, param) in func.params.iter().enumerate() {
            if let Some(arg) = args.get(index) {
                env.insert(param.clone(), arg.clone());
            }
        }
        let result = eval_block(&func.body, env, &mut functions.clone(), last_print);
        *env = saved;
        return match result {
            Flow::Return(value) | Flow::Value(value) => value,
            Flow::Break | Flow::Continue => PowerShellValue::Null,
        };
    }
    PowerShellValue::Null
}

fn is_integral(value: &PowerShellValue) -> bool {
    matches!(value, PowerShellValue::Int(_))
}

fn add(left: &PowerShellValue, right: &PowerShellValue) -> PowerShellValue {
    if let (PowerShellValue::String(left), PowerShellValue::String(right)) = (left, right) {
        return PowerShellValue::String(format!("{left}{right}"));
    }
    numeric_bin(left, right, |a, b| a + b)
}

fn div(left: &PowerShellValue, right: &PowerShellValue) -> PowerShellValue {
    let divisor = right.to_f64();
    if divisor == 0.0 {
        return PowerShellValue::Float(f64::NAN);
    }
    let result = left.to_f64() / divisor;
    if is_integral(left) && is_integral(right) { PowerShellValue::Int(result as i64) } else { PowerShellValue::Float(result) }
}

fn numeric_bin(left: &PowerShellValue, right: &PowerShellValue, op: impl Fn(f64, f64) -> f64) -> PowerShellValue {
    let result = op(left.to_f64(), right.to_f64());
    if is_integral(left) && is_integral(right) { PowerShellValue::Int(result as i64) } else { PowerShellValue::Float(result) }
}

fn eq(left: &PowerShellValue, right: &PowerShellValue) -> bool {
    match (left, right) {
        (PowerShellValue::Null, PowerShellValue::Null) => true,
        (PowerShellValue::Bool(left), PowerShellValue::Bool(right)) => left == right,
        (PowerShellValue::String(left), PowerShellValue::String(right)) => left == right,
        (PowerShellValue::Int(_) | PowerShellValue::Float(_), PowerShellValue::Int(_) | PowerShellValue::Float(_)) => {
            (left.to_f64() - right.to_f64()).abs() < f64::EPSILON
        }
        _ => false,
    }
}
