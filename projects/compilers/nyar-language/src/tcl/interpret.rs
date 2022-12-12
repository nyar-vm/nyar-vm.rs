//! Tree-walking Tcl interpreter (legend demo subset).

use std::collections::HashMap;

use std_data::text::tcl::{TclCommand, TclError, TclScript, TclWord, split_tcl_list};

/// Runtime value for the Tcl tree interpreter.
#[derive(Debug, Clone, PartialEq)]
pub enum TclValue {
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

impl TclValue {
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
            Self::Int(value) => *value != 0,
            Self::Float(value) => *value != 0.0,
            Self::String(value) => !value.is_empty(),
            Self::Null => false,
        }
    }

    /// Display form used by `puts` / legend fixtures.
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
    /// Normal command result.
    Value(TclValue),
    /// `return` from a procedure or top-level script.
    Return(TclValue),
    /// `break` from a loop.
    Break,
    /// `continue` to the next loop iteration.
    Continue,
}

#[derive(Clone)]
struct TclProc {
    params: Vec<String>,
    body: String,
}

/// Evaluate Tcl source and return the last command result.
pub fn evaluate_tcl_source(source: &str, env: &mut HashMap<String, TclValue>) -> Result<TclValue, TclError> {
    let script = TclScript::parse(source)?;
    Ok(evaluate_tcl_script(&script, env))
}

/// Evaluate a parsed Tcl script.
pub fn evaluate_tcl_script(script: &TclScript, env: &mut HashMap<String, TclValue>) -> TclValue {
    let mut procs = HashMap::new();
    match eval_commands(&script.commands, env, &mut procs) {
        Flow::Return(value) | Flow::Value(value) => value,
        Flow::Break | Flow::Continue => TclValue::Null,
    }
}

fn eval_commands(commands: &[TclCommand], env: &mut HashMap<String, TclValue>, procs: &mut HashMap<String, TclProc>) -> Flow {
    let mut last = Flow::Value(TclValue::Null);
    for command in commands {
        last = eval_command(command, env, procs);
        if matches!(last, Flow::Return(_) | Flow::Break | Flow::Continue) {
            return last;
        }
    }
    last
}

fn eval_source(source: &str, env: &mut HashMap<String, TclValue>, procs: &mut HashMap<String, TclProc>) -> Flow {
    if source.trim().is_empty() {
        return Flow::Value(TclValue::Null);
    }
    match TclScript::parse(source) {
        Ok(script) => eval_commands(&script.commands, env, procs),
        Err(_) => Flow::Value(TclValue::Null),
    }
}

fn eval_command(command: &TclCommand, env: &mut HashMap<String, TclValue>, procs: &mut HashMap<String, TclProc>) -> Flow {
    match command {
        TclCommand::Set { name, value } => {
            let stored = parse_tcl_value(&expand_word(value, env, procs));
            env.insert(name.clone(), stored.clone());
            Flow::Value(stored)
        }
        TclCommand::Puts { words } => {
            let text = words.iter().map(|word| expand_word(word, env, procs)).collect::<Vec<_>>().join(" ");
            println!("{text}");
            Flow::Value(TclValue::String(text))
        }
        TclCommand::Expr { expression } => Flow::Value(eval_tcl_expr(&expand_word(expression, env, procs), env)),
        TclCommand::Incr { name, delta } => {
            let current = env.get(name).map(TclValue::to_i64).unwrap_or(0);
            let step = delta.as_ref().map(|value| expand_word(value, env, procs).parse::<i64>().unwrap_or(1)).unwrap_or(1);
            let next = TclValue::Int(current + step);
            env.insert(name.clone(), next.clone());
            Flow::Value(next)
        }
        TclCommand::If { test, body } => {
            if eval_condition(test, env, procs) {
                eval_source(body, env, procs)
            }
            else {
                Flow::Value(TclValue::Null)
            }
        }
        TclCommand::While { test, body } => {
            let mut last = Flow::Value(TclValue::Null);
            while eval_condition(test, env, procs) {
                last = eval_source(body, env, procs);
                match &last {
                    Flow::Return(_) => return last,
                    Flow::Break => return Flow::Value(TclValue::Null),
                    Flow::Continue => continue,
                    Flow::Value(_) => {}
                }
            }
            last
        }
        TclCommand::For { init, test, next, body } => {
            let flow = eval_source(init, env, procs);
            if matches!(flow, Flow::Return(_)) {
                return flow;
            }
            let mut last = Flow::Value(TclValue::Null);
            loop {
                if !eval_condition(test, env, procs) {
                    break;
                }
                last = eval_source(body, env, procs);
                match &last {
                    Flow::Return(_) => return last,
                    Flow::Break => return Flow::Value(TclValue::Null),
                    Flow::Continue => {}
                    Flow::Value(_) => {}
                }
                let step = eval_source(next, env, procs);
                if matches!(step, Flow::Return(_)) {
                    return step;
                }
            }
            last
        }
        TclCommand::Foreach { vars, list, body } => {
            let items = split_tcl_list(&expand_word(list, env, procs));
            let mut last = Flow::Value(TclValue::Null);
            let mut index = 0usize;
            while index < items.len() {
                for var in vars {
                    let value = items.get(index).cloned().unwrap_or_default();
                    env.insert(var.clone(), parse_tcl_value(&value));
                    index += 1;
                }
                last = eval_source(body, env, procs);
                match &last {
                    Flow::Return(_) => return last,
                    Flow::Break => return Flow::Value(TclValue::Null),
                    Flow::Continue => continue,
                    Flow::Value(_) => {}
                }
            }
            last
        }
        TclCommand::Proc { name, params, body } => {
            procs.insert(name.clone(), TclProc { params: params.clone(), body: body.clone() });
            Flow::Value(TclValue::Null)
        }
        TclCommand::Return { value } => {
            let result = value.as_ref().map(|word| parse_tcl_value(&expand_word(word, env, procs))).unwrap_or(TclValue::Null);
            Flow::Return(result)
        }
        TclCommand::List { words } => {
            let text = words.iter().map(|word| expand_word(word, env, procs)).collect::<Vec<_>>().join(" ");
            Flow::Value(TclValue::String(text))
        }
        TclCommand::Llength { list } => {
            let text = expand_word(list, env, procs);
            Flow::Value(TclValue::Int(split_tcl_list(&text).len() as i64))
        }
        TclCommand::Lindex { list, index } => {
            let text = expand_word(list, env, procs);
            let idx = expand_word(index, env, procs).parse::<i64>().unwrap_or(0);
            let items = split_tcl_list(&text);
            let item = if idx < 0 { String::new() } else { items.get(idx as usize).cloned().unwrap_or_default() };
            Flow::Value(parse_tcl_value(&item))
        }
        TclCommand::Call { name, args } => call_proc(name, args, env, procs),
    }
}

fn call_proc(name: &str, args: &[TclWord], env: &mut HashMap<String, TclValue>, procs: &mut HashMap<String, TclProc>) -> Flow {
    let Some(proc) = procs.get(name).cloned()
    else {
        return Flow::Value(TclValue::Null);
    };
    let evaluated: Vec<String> = args.iter().map(|arg| expand_word(arg, env, procs)).collect();
    let saved = env.clone();
    for (index, param) in proc.params.iter().enumerate() {
        let value = evaluated.get(index).cloned().unwrap_or_default();
        env.insert(param.clone(), parse_tcl_value(&value));
    }
    let result = eval_source(&proc.body, env, procs);
    *env = saved;
    match result {
        Flow::Return(value) | Flow::Value(value) => Flow::Value(value),
        Flow::Break | Flow::Continue => Flow::Value(TclValue::Null),
    }
}

fn expand_word(word: &TclWord, env: &mut HashMap<String, TclValue>, procs: &mut HashMap<String, TclProc>) -> String {
    if word.braced {
        return word.text.clone();
    }
    expand_substitutions(&word.text, env, procs)
}

/// Expand `$` / `${}` / `[...]` then evaluate a condition expression.
fn eval_condition(test: &str, env: &mut HashMap<String, TclValue>, procs: &mut HashMap<String, TclProc>) -> bool {
    let expanded = expand_substitutions(test, env, procs);
    eval_tcl_expr(&expanded, env).to_bool()
}

fn expand_substitutions(input: &str, env: &mut HashMap<String, TclValue>, procs: &mut HashMap<String, TclProc>) -> String {
    let mut output = String::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '$' => {
                if chars.peek() == Some(&'{') {
                    chars.next();
                    let mut name = String::new();
                    while let Some(next) = chars.next() {
                        if next == '}' {
                            break;
                        }
                        name.push(next);
                    }
                    output.push_str(&lookup(env, &name).to_string_value());
                }
                else {
                    let mut name = String::new();
                    while let Some(&next) = chars.peek() {
                        if next.is_ascii_alphanumeric() || next == '_' {
                            name.push(chars.next().unwrap());
                        }
                        else {
                            break;
                        }
                    }
                    if name.is_empty() {
                        output.push('$');
                    }
                    else {
                        output.push_str(&lookup(env, &name).to_string_value());
                    }
                }
            }
            '[' => {
                let mut depth = 1usize;
                let mut script = String::new();
                while let Some(next) = chars.next() {
                    if next == '[' {
                        depth += 1;
                        script.push(next);
                    }
                    else if next == ']' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        script.push(next);
                    }
                    else {
                        script.push(next);
                    }
                }
                let value = match eval_source(&script, env, procs) {
                    Flow::Return(inner) | Flow::Value(inner) => inner,
                    Flow::Break | Flow::Continue => TclValue::Null,
                };
                output.push_str(&value.to_string_value());
            }
            '\\' => {
                if let Some(escaped) = chars.next() {
                    output.push(escaped);
                }
            }
            other => output.push(other),
        }
    }
    output
}

fn lookup(env: &HashMap<String, TclValue>, name: &str) -> TclValue {
    env.get(name).cloned().unwrap_or(TclValue::Null)
}

fn parse_tcl_value(text: &str) -> TclValue {
    if let Ok(value) = text.parse::<i64>() {
        return TclValue::Int(value);
    }
    if let Ok(value) = text.parse::<f64>() {
        return TclValue::Float(value);
    }
    TclValue::String(text.to_string())
}

fn eval_tcl_expr(expression: &str, env: &HashMap<String, TclValue>) -> TclValue {
    let text = expand_expr_vars(expression.trim(), env);
    for op in [">=", "<=", "==", "!=", ">", "<"] {
        if let Some(index) = text.find(op) {
            let left_text = text[..index].trim().to_string();
            let right_text = text[index + op.len()..].trim().to_string();
            let left = parse_tcl_value(&left_text);
            let right = parse_tcl_value(&right_text);
            let ok = match op {
                ">=" => left.to_f64() >= right.to_f64(),
                "<=" => left.to_f64() <= right.to_f64(),
                "==" => eq(&left, &right),
                "!=" => !eq(&left, &right),
                ">" => left.to_f64() > right.to_f64(),
                "<" => left.to_f64() < right.to_f64(),
                _ => false,
            };
            return TclValue::Int(i64::from(ok));
        }
    }
    if let Some((left, right)) = split_binary(&text, '+') {
        return numeric_op(&parse_tcl_value(left), &parse_tcl_value(right), |a, b| a + b, |a, b| a + b);
    }
    if let Some((left, right)) = split_binary(&text, '-') {
        if !left.is_empty() {
            return numeric_op(&parse_tcl_value(left), &parse_tcl_value(right), |a, b| a - b, |a, b| a - b);
        }
    }
    if let Some((left, right)) = split_binary(&text, '*') {
        return numeric_op(&parse_tcl_value(left), &parse_tcl_value(right), |a, b| a * b, |a, b| a * b);
    }
    if let Some((left, right)) = split_binary(&text, '/') {
        let left_val = parse_tcl_value(left);
        let right_val = parse_tcl_value(right);
        if matches!(left_val, TclValue::Float(_)) || matches!(right_val, TclValue::Float(_)) {
            let divisor = right_val.to_f64();
            if divisor == 0.0 {
                return TclValue::Float(f64::NAN);
            }
            return TclValue::Float(left_val.to_f64() / divisor);
        }
        let divisor = right_val.to_i64();
        if divisor == 0 {
            return TclValue::Int(0);
        }
        return TclValue::Int(left_val.to_i64() / divisor);
    }
    parse_tcl_value(&text)
}

fn expand_expr_vars(input: &str, env: &HashMap<String, TclValue>) -> String {
    let mut output = String::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '$' {
            if chars.peek() == Some(&'{') {
                chars.next();
                let mut name = String::new();
                while let Some(next) = chars.next() {
                    if next == '}' {
                        break;
                    }
                    name.push(next);
                }
                output.push_str(&lookup(env, &name).to_string_value());
            }
            else {
                let mut name = String::new();
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_alphanumeric() || next == '_' {
                        name.push(chars.next().unwrap());
                    }
                    else {
                        break;
                    }
                }
                if name.is_empty() {
                    output.push('$');
                }
                else {
                    output.push_str(&lookup(env, &name).to_string_value());
                }
            }
        }
        else {
            output.push(ch);
        }
    }
    output
}

fn numeric_op(left: &TclValue, right: &TclValue, int_op: fn(i64, i64) -> i64, float_op: fn(f64, f64) -> f64) -> TclValue {
    if matches!(left, TclValue::Float(_)) || matches!(right, TclValue::Float(_)) {
        TclValue::Float(float_op(left.to_f64(), right.to_f64()))
    }
    else {
        TclValue::Int(int_op(left.to_i64(), right.to_i64()))
    }
}

fn eq(left: &TclValue, right: &TclValue) -> bool {
    match (left, right) {
        (TclValue::String(a), TclValue::String(b)) => a == b,
        _ => (left.to_f64() - right.to_f64()).abs() < f64::EPSILON,
    }
}

fn split_binary(text: &str, op: char) -> Option<(&str, &str)> {
    let index = text.rfind(op)?;
    let left = text[..index].trim();
    let right = text[index + op.len_utf8()..].trim();
    if right.is_empty() { None } else { Some((left, right)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_puts_set() {
        let mut env = HashMap::new();
        let result = evaluate_tcl_source("set x 1\nputs $x", &mut env).expect("eval");
        assert_eq!(result.to_string_value(), "1");
    }

    #[test]
    fn evaluates_proc_foreach() {
        let source = r#"
proc add {a b} {
  return [expr {$a + $b}]
}
set total 0
foreach x {10 20} {
  set total [add $total $x]
}
puts $total
"#;
        let mut env = HashMap::new();
        let result = evaluate_tcl_source(source, &mut env).expect("eval");
        assert_eq!(result.to_string_value(), "30");
    }

    #[test]
    fn evaluates_for_and_braced_subst() {
        let source = r#"
set name victory
proc greet {who} {
  return $who
}
set x 0
while {$x < 1} {
  incr x
}
if {$x == 1} {
  for {set i 0} {$i < 1} {incr i} {
    puts [greet ${name}]
  }
}
"#;
        let mut env = HashMap::new();
        let result = evaluate_tcl_source(source, &mut env).expect("eval");
        assert_eq!(result.to_string_value(), "victory");
    }
}
