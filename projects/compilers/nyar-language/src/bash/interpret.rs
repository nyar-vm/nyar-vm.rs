//! Tree-walking Bash interpreter (legend demo subset).

use std::collections::HashMap;

use std_data::text::bash::{BashError, BashRedirect, BashScript, BashStmt};

/// Runtime value for the Bash tree interpreter.
#[derive(Debug, Clone, PartialEq)]
pub enum BashValue {
    /// Unit / uninitialized.
    Null,
    /// Boolean.
    Bool(bool),
    /// Signed integer.
    Int(i64),
    /// UTF-8 string.
    String(String),
}

impl BashValue {
    /// Coerce to `i64`.
    pub fn to_i64(&self) -> i64 {
        match self {
            Self::Int(value) => *value,
            Self::Bool(value) => i64::from(*value),
            Self::String(value) => value.parse().unwrap_or(0),
            Self::Null => 0,
        }
    }

    /// Truthiness (non-empty / non-zero).
    pub fn to_bool(&self) -> bool {
        match self {
            Self::Bool(value) => *value,
            Self::Int(value) => *value != 0,
            Self::String(value) => !value.is_empty(),
            Self::Null => false,
        }
    }

    /// Display form used by echo / printf / legend fixtures.
    pub fn to_string_value(&self) -> String {
        match self {
            Self::Null => String::new(),
            Self::Bool(value) => value.to_string(),
            Self::Int(value) => value.to_string(),
            Self::String(value) => value.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Flow {
    Value(BashValue),
    Return(BashValue),
    Break,
    Continue,
}

#[derive(Clone)]
struct BashFunction {
    body: Vec<BashStmt>,
}

/// Evaluate Bash source and return the script result (or last print payload).
pub fn evaluate_bash_source(source: &str, env: &mut HashMap<String, BashValue>) -> Result<BashValue, BashError> {
    let script = BashScript::parse(source)?;
    Ok(evaluate_bash_script(&script, env))
}

/// Evaluate a parsed Bash script.
pub fn evaluate_bash_script(script: &BashScript, env: &mut HashMap<String, BashValue>) -> BashValue {
    let mut locals = env.clone();
    if !locals.contains_key("?") {
        locals.insert("?".to_string(), BashValue::Int(0));
    }
    let mut functions: HashMap<String, BashFunction> = HashMap::new();
    let mut files: HashMap<String, String> = HashMap::new();
    let mut last_print = BashValue::Null;
    let mut last = Flow::Value(BashValue::Null);

    for stmt in &script.statements {
        last = eval_stmt(stmt, &mut locals, &mut functions, &mut files, &mut last_print);
        if matches!(last, Flow::Return(_)) {
            break;
        }
    }

    env.clear();
    env.extend(locals);

    let result = match last {
        Flow::Return(value) => value,
        Flow::Value(value) => value,
        Flow::Break | Flow::Continue => BashValue::Null,
    };

    match (&result, &last_print) {
        (BashValue::Int(0) | BashValue::Null, BashValue::String(_)) => last_print,
        (other, BashValue::String(_)) if other.to_i64() == 0 => last_print,
        (BashValue::Null, other) if !matches!(other, BashValue::Null) => other.clone(),
        _ => result,
    }
}

fn eval_stmt(
    stmt: &BashStmt,
    env: &mut HashMap<String, BashValue>,
    functions: &mut HashMap<String, BashFunction>,
    files: &mut HashMap<String, String>,
    last_print: &mut BashValue,
) -> Flow {
    match stmt {
        BashStmt::Assign { name, value } => {
            let stored = BashValue::String(expand_word(value, env));
            env.insert(name.clone(), stored.clone());
            set_status(env, 0);
            Flow::Value(stored)
        }
        BashStmt::Export { name, value } => {
            if let Some(value) = value.as_deref() {
                let stored = BashValue::String(expand_word(value, env));
                env.insert(name.clone(), stored.clone());
                set_status(env, 0);
                Flow::Value(stored)
            }
            else {
                set_status(env, 0);
                Flow::Value(env.get(name).cloned().unwrap_or(BashValue::Null))
            }
        }
        BashStmt::Command { words, redirects } => eval_command(words, redirects, env, functions, files, last_print),
        BashStmt::Pipeline { stages } => {
            let mut last = Flow::Value(BashValue::Null);
            for stage in stages {
                last = eval_stmt(stage, env, functions, files, last_print);
                if matches!(last, Flow::Return(_) | Flow::Break | Flow::Continue) {
                    break;
                }
            }
            last
        }
        BashStmt::AndOr { left, op, right } => {
            let left_val = eval_stmt(left, env, functions, files, last_print);
            let status = status_of(env);
            let run_right = if op == "&&" { status == 0 } else { status != 0 };
            if run_right { eval_stmt(right, env, functions, files, last_print) } else { left_val }
        }
        BashStmt::If { condition, then_body, else_body } => {
            eval_stmt(condition, env, functions, files, last_print);
            if status_of(env) == 0 {
                eval_block(then_body, env, functions, files, last_print)
            }
            else {
                eval_block(else_body, env, functions, files, last_print)
            }
        }
        BashStmt::While { condition, body } => {
            let mut last = Flow::Value(BashValue::Null);
            loop {
                eval_stmt(condition, env, functions, files, last_print);
                if status_of(env) != 0 {
                    break;
                }
                last = eval_block(body, env, functions, files, last_print);
                match &last {
                    Flow::Return(_) => return last,
                    Flow::Break => return Flow::Value(BashValue::Null),
                    Flow::Continue => continue,
                    Flow::Value(_) => {}
                }
            }
            last
        }
        BashStmt::For { var, items, body } => {
            let mut last = Flow::Value(BashValue::Null);
            for item in items {
                env.insert(var.clone(), BashValue::String(expand_word(item, env)));
                last = eval_block(body, env, functions, files, last_print);
                match &last {
                    Flow::Return(_) => return last,
                    Flow::Break => return Flow::Value(BashValue::Null),
                    Flow::Continue => continue,
                    Flow::Value(_) => {}
                }
            }
            last
        }
        BashStmt::FunctionDef { name, body } => {
            functions.insert(name.clone(), BashFunction { body: body.clone() });
            set_status(env, 0);
            Flow::Value(BashValue::Null)
        }
        BashStmt::Group(body) => eval_block(body, env, functions, files, last_print),
        BashStmt::Break => Flow::Break,
        BashStmt::Continue => Flow::Continue,
        BashStmt::Return(code) => {
            let code = code.unwrap_or(0);
            set_status(env, code);
            Flow::Return(BashValue::Int(code))
        }
    }
}

fn eval_block(
    statements: &[BashStmt],
    env: &mut HashMap<String, BashValue>,
    functions: &mut HashMap<String, BashFunction>,
    files: &mut HashMap<String, String>,
    last_print: &mut BashValue,
) -> Flow {
    let mut last = Flow::Value(BashValue::Null);
    for stmt in statements {
        last = eval_stmt(stmt, env, functions, files, last_print);
        if matches!(last, Flow::Return(_) | Flow::Break | Flow::Continue) {
            break;
        }
    }
    last
}

fn eval_command(
    words: &[String],
    redirects: &[BashRedirect],
    env: &mut HashMap<String, BashValue>,
    functions: &mut HashMap<String, BashFunction>,
    files: &mut HashMap<String, String>,
    last_print: &mut BashValue,
) -> Flow {
    if words.is_empty() {
        set_status(env, 0);
        return Flow::Value(BashValue::Null);
    }

    let expanded: Vec<String> = words.iter().map(|word| expand_word(word, env)).collect();
    let name = expanded[0].as_str();
    let args = &expanded[1..];

    if let Some(func) = functions.get(name).cloned() {
        for (index, arg) in args.iter().enumerate() {
            env.insert((index + 1).to_string(), BashValue::String(arg.clone()));
        }
        env.insert("#".to_string(), BashValue::Int(args.len() as i64));
        let result = eval_block(&func.body, env, functions, files, last_print);
        return match result {
            Flow::Return(value) => {
                set_status(env, value.to_i64());
                Flow::Value(value)
            }
            Flow::Value(other) => {
                set_status(env, 0);
                Flow::Value(other)
            }
            Flow::Break | Flow::Continue => {
                set_status(env, 0);
                Flow::Value(BashValue::Null)
            }
        };
    }

    match name {
        "echo" => {
            let text = join_echo_args(args);
            let rendered = apply_redirects(text, redirects, files, env);
            if rendered.print {
                println!("{}", rendered.text);
            }
            *last_print = BashValue::String(rendered.text.clone());
            set_status(env, 0);
            Flow::Value(BashValue::String(rendered.text))
        }
        "printf" => {
            let text = format_printf(args);
            let rendered = apply_redirects(text, redirects, files, env);
            if rendered.print {
                print!("{}", rendered.text);
            }
            let display = rendered.text.trim_end_matches('\n').to_string();
            *last_print = BashValue::String(display.clone());
            set_status(env, 0);
            Flow::Value(BashValue::String(display))
        }
        "cd" => {
            let path = args.first().cloned().unwrap_or_else(|| env.get("HOME").map(BashValue::to_string_value).unwrap_or_default());
            env.insert("PWD".to_string(), BashValue::String(path.clone()));
            set_status(env, 0);
            Flow::Value(BashValue::String(path))
        }
        "exit" => {
            let code = args.first().and_then(|value| value.parse::<i64>().ok()).unwrap_or(0);
            set_status(env, code);
            Flow::Return(BashValue::Int(code))
        }
        "true" => {
            set_status(env, 0);
            Flow::Value(BashValue::Bool(true))
        }
        "false" => {
            set_status(env, 1);
            Flow::Value(BashValue::Bool(false))
        }
        "test" | "[" => {
            let ok = eval_test(args, name == "[");
            set_status(env, if ok { 0 } else { 1 });
            Flow::Value(BashValue::Bool(ok))
        }
        _ => {
            set_status(env, 127);
            Flow::Value(BashValue::Null)
        }
    }
}

struct Redirected {
    text: String,
    print: bool,
}

fn apply_redirects(
    text: String,
    redirects: &[BashRedirect],
    files: &mut HashMap<String, String>,
    env: &HashMap<String, BashValue>,
) -> Redirected {
    let mut print = true;
    let mut out = text;
    for redirect in redirects {
        match redirect {
            BashRedirect::Write { path, append } => {
                let path = expand_word(path, env);
                print = false;
                if *append {
                    let existing = files.get(&path).cloned().unwrap_or_default();
                    files.insert(path, format!("{existing}{out}"));
                }
                else {
                    files.insert(path, out.clone());
                }
            }
            BashRedirect::Read { path } => {
                let path = expand_word(path, env);
                out = files.get(&path).cloned().unwrap_or_default();
            }
        }
    }
    Redirected { text: out, print }
}

fn join_echo_args(args: &[String]) -> String {
    args.iter().map(|arg| strip_quotes(arg)).collect::<Vec<_>>().join(" ")
}

fn format_printf(args: &[String]) -> String {
    let Some(fmt) = args.first()
    else {
        return String::new();
    };
    let fmt = strip_quotes(fmt);
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
            Some('d') | Some('i') | Some('s') => {
                let piece = args.get(arg_index).map(|value| strip_quotes(value)).unwrap_or_default();
                out.push_str(&piece);
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

fn eval_test(args: &[String], bracket: bool) -> bool {
    let args = if bracket {
        match args.last().map(String::as_str) {
            Some("]") => &args[..args.len().saturating_sub(1)],
            _ => args,
        }
    }
    else {
        args
    };

    if args.is_empty() {
        return false;
    }
    if args.len() == 1 {
        return !args[0].is_empty() && args[0] != "0";
    }
    if args.len() == 2 {
        return match args[0].as_str() {
            "-z" => args[1].is_empty(),
            "-n" => !args[1].is_empty(),
            _ => false,
        };
    }
    if args.len() >= 3 {
        let left = &args[0];
        let op = args[1].as_str();
        let right = &args[2];
        return match op {
            "=" | "==" => left == right,
            "!=" => left != right,
            "-eq" => parse_i64(left) == parse_i64(right),
            "-ne" => parse_i64(left) != parse_i64(right),
            "-lt" => parse_i64(left) < parse_i64(right),
            "-gt" => parse_i64(left) > parse_i64(right),
            "-le" => parse_i64(left) <= parse_i64(right),
            "-ge" => parse_i64(left) >= parse_i64(right),
            _ => false,
        };
    }
    false
}

fn expand_word(text: &str, env: &HashMap<String, BashValue>) -> String {
    let mut output = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '$' {
            output.push(ch);
            continue;
        }
        if chars.peek() == Some(&'?') {
            chars.next();
            output.push_str(&env.get("?").map(BashValue::to_string_value).unwrap_or_else(|| "0".to_string()));
            continue;
        }
        if chars.peek() == Some(&'#') {
            chars.next();
            output.push_str(&env.get("#").map(BashValue::to_string_value).unwrap_or_else(|| "0".to_string()));
            continue;
        }
        if chars.peek() == Some(&'{') {
            chars.next();
            let mut name = String::new();
            while let Some(&next) = chars.peek() {
                if next == '}' {
                    chars.next();
                    break;
                }
                name.push(chars.next().unwrap());
            }
            output.push_str(&env.get(&name).map(BashValue::to_string_value).unwrap_or_default());
            continue;
        }
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
            output.push_str(&env.get(&name).map(BashValue::to_string_value).unwrap_or_default());
        }
    }
    strip_quotes(&output)
}

fn strip_quotes(value: &str) -> String {
    let value = value.trim();
    if (value.starts_with('"') && value.ends_with('"')) || (value.starts_with('\'') && value.ends_with('\'')) {
        value[1..value.len().saturating_sub(1)].to_string()
    }
    else {
        value.to_string()
    }
}

fn parse_i64(text: &str) -> i64 {
    strip_quotes(text).parse().unwrap_or(0)
}

fn set_status(env: &mut HashMap<String, BashValue>, code: i64) {
    env.insert("?".to_string(), BashValue::Int(code));
}

fn status_of(env: &HashMap<String, BashValue>) -> i64 {
    env.get("?").map(BashValue::to_i64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_vars_if_else_echo() {
        let source = r#"
x=1
if [ $x -eq 1 ]; then
  echo ok
else
  echo no
fi
"#;
        let mut env = HashMap::new();
        let result = evaluate_bash_source(source, &mut env).expect("eval");
        assert_eq!(result.to_string_value(), "ok");
    }

    #[test]
    fn evaluates_loops_function_printf_and_exit() {
        let source = r#"
add() {
  printf "%s" "$1"
}
n=0
while [ $n -lt 1 ]; do
  n=1
done
for item in hi; do
  add $item
done
"#;
        let mut env = HashMap::new();
        let result = evaluate_bash_source(source, &mut env).expect("eval");
        assert_eq!(result.to_string_value(), "hi");
    }

    #[test]
    fn exit_stops_script_with_code() {
        let mut env = HashMap::new();
        let result = evaluate_bash_source("echo before\nexit 7\necho after", &mut env).expect("eval");
        assert_eq!(result.to_i64(), 7);
        assert_eq!(env.get("?").map(BashValue::to_i64), Some(7));
    }

    #[test]
    fn evaluates_elif_and_export() {
        let source = r#"
export x=0
if [ $x -eq 1 ]; then
  echo one
elif [ $x -eq 0 ]; then
  echo zero
else
  echo other
fi
"#;
        let mut env = HashMap::new();
        let result = evaluate_bash_source(source, &mut env).expect("eval");
        assert_eq!(result.to_string_value(), "zero");
        assert_eq!(env.get("x").map(BashValue::to_string_value).as_deref(), Some("0"));
    }
}
