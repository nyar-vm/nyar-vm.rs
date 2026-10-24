//! AWSL 模板表达式静态求值（SSG / SSR 子集）。

use serde_json::{Value, json};

use crate::awsl::ScriptBinding;

/// 求值上下文：合并 script 绑定默认值与请求/页面数据。
#[derive(Debug, Clone)]
pub struct ExprContext {
    root: Value,
    loop_stack: Vec<LoopFrame>,
}

#[derive(Debug, Clone)]
struct LoopFrame {
    item_var: String,
    index_var: String,
    item: Value,
    index: usize,
}

impl ExprContext {
    /// 从 script 绑定与外部 JSON 数据构建上下文（`data` 覆盖同名绑定）。
    pub fn from_bindings(bindings: &[ScriptBinding], data: &Value) -> Self {
        let mut root = json!({});
        if let Some(map) = root.as_object_mut() {
            for binding in bindings {
                if let Some(parsed) = parse_init_expr(&binding.init_expr) {
                    map.insert(binding.name.clone(), parsed);
                }
            }
            merge_value(map, data);
        }
        Self { root, loop_stack: Vec::new() }
    }

    /// 压入循环迭代作用域。
    pub fn push_loop(&mut self, item_var: &str, index_var: &str, item: Value, index: usize) {
        self.loop_stack.push(LoopFrame { item_var: item_var.to_string(), index_var: index_var.to_string(), item, index });
    }

    /// 弹出循环作用域。
    pub fn pop_loop(&mut self) {
        self.loop_stack.pop();
    }

    /// 求值表达式为 JSON 值。
    pub fn eval_value(&self, expr: &str) -> Value {
        let expr = strip_expr_braces(expr.trim());
        if expr.is_empty() {
            return Value::Null;
        }
        if let Some(path) = expr.strip_prefix("isActive(").and_then(|s| s.strip_suffix(')')) {
            let path = self.eval_string(path.trim());
            let current = self.lookup_var("__current_path").and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
            return Value::Bool(path == current);
        }
        self.eval_expression(expr)
    }

    /// 求值表达式为字符串。
    pub fn eval_string(&self, expr: &str) -> String {
        value_to_string(&self.eval_value(expr))
    }

    /// 求值表达式真假。
    pub fn eval_truthy(&self, expr: &str) -> bool {
        value_truthy(&self.eval_value(expr))
    }

    fn eval_expression(&self, expr: &str) -> Value {
        let expr = expr.trim();
        if let Some(path) = expr.strip_prefix("isActive(").and_then(|s| s.strip_suffix(')')) {
            let path = self.eval_string(path.trim());
            let current = self.lookup_var("__current_path").and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
            return Value::Bool(path == current);
        }
        if let Some(result) = self.parse_ternary(expr) {
            return result;
        }
        if let Some(result) = self.parse_string_concat(expr) {
            return result;
        }
        if (expr.starts_with('"') && expr.ends_with('"')) || (expr.starts_with('\'') && expr.ends_with('\'')) {
            return Value::String(unquote(expr));
        }
        if let Ok(n) = expr.parse::<f64>() {
            return Value::Number(serde_json::Number::from_f64(n).unwrap_or_else(|| 0.into()));
        }
        if expr == "true" {
            return Value::Bool(true);
        }
        if expr == "false" {
            return Value::Bool(false);
        }
        if expr == "null" || expr == "undefined" {
            return Value::Null;
        }
        if let Some(inner) = expr.strip_prefix('!').map(str::trim) {
            return Value::Bool(!value_truthy(&self.eval_expression(inner)));
        }
        if let Some((left, right)) = split_top_level_binary(expr, "==") {
            return Value::Bool(values_equal(&self.eval_expression(left), &self.eval_expression(right)));
        }
        if let Some((left, right)) = split_top_level_binary(expr, "!=") {
            return Value::Bool(!values_equal(&self.eval_expression(left), &self.eval_expression(right)));
        }
        if let Some(value) = self.lookup_var(expr) {
            return value;
        }
        if let Some((head, tail)) = expr.split_once('.') {
            if let Some(base) = self.lookup_var(head) {
                if let Some(v) = lookup_json_path(&base, tail) {
                    return v;
                }
            }
        }
        self.eval_binary_expression(expr).unwrap_or(Value::Null)
    }

    fn eval_binary_expression(&self, expr: &str) -> Option<Value> {
        let tokens = tokenize_expression(expr);
        if tokens.is_empty() {
            return None;
        }
        if tokens.len() == 1 {
            return Some(self.eval_expression(&tokens[0]));
        }
        self.eval_tokens_binary(&tokens)
    }

    fn eval_tokens_binary(&self, tokens: &[String]) -> Option<Value> {
        for precedence in (0..=8).rev() {
            if let Some(index) = find_operator_with_precedence(tokens, precedence) {
                let op = tokens[index].as_str();
                if op == "!" {
                    let operand = self.eval_tokens_binary(&tokens[index + 1..])?;
                    return Some(Value::Bool(!value_truthy(&operand)));
                }
                let left = self.eval_tokens_binary(&tokens[..index])?;
                let right = self.eval_tokens_binary(&tokens[index + 1..])?;
                return Some(apply_binary_op(&left, op, &right));
            }
        }
        None
    }

    fn parse_ternary(&self, expr: &str) -> Option<Value> {
        let q = find_top_level_char(expr, '?')?;
        let cond = expr[..q].trim();
        let rest = expr[q + 1..].trim();
        let colon = find_top_level_char(rest, ':')?;
        let when_true = rest[..colon].trim();
        let when_false = rest[colon + 1..].trim();
        if value_truthy(&self.eval_expression(cond)) { Some(self.eval_expression(when_true)) } else { Some(self.eval_expression(when_false)) }
    }

    fn parse_string_concat(&self, expr: &str) -> Option<Value> {
        if !expr.contains('+') {
            return None;
        }
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut in_string = false;
        let mut quote = '"';
        for ch in expr.chars() {
            if in_string {
                current.push(ch);
                if ch == quote {
                    in_string = false;
                }
                continue;
            }
            if ch == '"' || ch == '\'' {
                in_string = true;
                quote = ch;
                current.push(ch);
                continue;
            }
            if ch == '+' {
                if !current.trim().is_empty() {
                    parts.push(current.trim().to_string());
                }
                current.clear();
                continue;
            }
            current.push(ch);
        }
        if !current.trim().is_empty() {
            parts.push(current.trim().to_string());
        }
        if parts.len() < 2 {
            return None;
        }
        let mut out = String::new();
        for part in parts {
            out.push_str(&value_to_string(&self.eval_expression(&part)));
        }
        Some(Value::String(out))
    }

    fn lookup_var(&self, name: &str) -> Option<Value> {
        for frame in self.loop_stack.iter().rev() {
            if frame.item_var == name {
                return Some(frame.item.clone());
            }
            if frame.index_var == name {
                return Some(Value::Number(frame.index.into()));
            }
        }
        if let Some((head, tail)) = name.split_once('.') {
            let base = self.lookup_var(head)?;
            return lookup_json_path(&base, tail);
        }
        lookup_json_path(&self.root, name)
    }
}

fn merge_value(map: &mut serde_json::Map<String, Value>, overlay: &Value) {
    match overlay {
        Value::Object(overlay_map) => {
            for (key, value) in overlay_map {
                map.insert(key.clone(), value.clone());
            }
        }
        Value::Null => {}
        other => {
            map.insert("__data".into(), other.clone());
        }
    }
}

fn strip_expr_braces(expr: &str) -> &str {
    let trimmed = expr.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') { &trimmed[1..trimmed.len() - 1] } else { trimmed }
}

fn split_top_level_binary<'a>(expr: &'a str, op: &'a str) -> Option<(&'a str, &'a str)> {
    let mut in_string = false;
    let mut quote = '"';
    let mut depth_paren = 0;
    let oplen = op.len();
    let mut i = 0;
    while i + oplen <= expr.len() {
        let ch = expr[i..].chars().next()?;
        if in_string {
            if ch == quote {
                in_string = false;
            }
            i += ch.len_utf8();
            continue;
        }
        if ch == '"' || ch == '\'' {
            in_string = true;
            quote = ch;
            i += ch.len_utf8();
            continue;
        }
        match ch {
            '(' => depth_paren += 1,
            ')' => depth_paren -= 1,
            _ if depth_paren == 0 && expr[i..].starts_with(op) => {
                let left = expr[..i].trim();
                let right = expr[i + oplen..].trim();
                if !left.is_empty() && !right.is_empty() {
                    return Some((left, right));
                }
            }
            _ => {}
        }
        i += ch.len_utf8();
    }
    None
}

fn parse_init_expr(expr: &str) -> Option<Value> {
    let expr = expr.trim();
    if expr.is_empty() {
        return None;
    }
    if expr == "true" {
        return Some(Value::Bool(true));
    }
    if expr == "false" {
        return Some(Value::Bool(false));
    }
    if let Ok(n) = expr.parse::<i64>() {
        return Some(Value::Number(n.into()));
    }
    if (expr.starts_with('"') && expr.ends_with('"')) || (expr.starts_with('\'') && expr.ends_with('\'')) {
        return Some(Value::String(unquote(expr)));
    }
    if expr.starts_with('[') || expr.starts_with('{') {
        let normalized = normalize_awsl_json_literals(expr);
        if let Ok(value) = serde_json::from_str(&normalized) {
            return Some(value);
        }
    }
    Some(Value::String(expr.to_string()))
}

fn normalize_awsl_json_literals(expr: &str) -> String {
    let mut out = String::with_capacity(expr.len());
    let mut chars = expr.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '"' {
            if chars.peek() == Some(&'"') {
                chars.next();
                out.push('"');
                continue;
            }
            out.push(ch);
            continue;
        }
        out.push(ch);
    }
    out
}

fn apply_binary_op(left: &Value, op: &str, right: &Value) -> Value {
    match op {
        "||" => Value::Bool(value_truthy(left) || value_truthy(right)),
        "&&" => Value::Bool(value_truthy(left) && value_truthy(right)),
        "==" => Value::Bool(values_equal(left, right)),
        "!=" => Value::Bool(!values_equal(left, right)),
        "+" => {
            if let (Some(a), Some(b)) = (left.as_f64(), right.as_f64()) {
                Value::Number(serde_json::Number::from_f64(a + b).unwrap_or_else(|| 0.into()))
            }
            else {
                Value::String(format!("{}{}", value_to_string(left), value_to_string(right)))
            }
        }
        ">" | "<" | ">=" | "<=" => {
            if let (Some(a), Some(b)) = (left.as_f64(), right.as_f64()) {
                let result = match op {
                    ">" => a > b,
                    "<" => a < b,
                    ">=" => a >= b,
                    _ => a <= b,
                };
                Value::Bool(result)
            }
            else {
                let cmp = value_to_string(left).cmp(&value_to_string(right));
                let result = match op {
                    ">" => cmp == std::cmp::Ordering::Greater,
                    "<" => cmp == std::cmp::Ordering::Less,
                    ">=" => cmp != std::cmp::Ordering::Less,
                    _ => cmp != std::cmp::Ordering::Greater,
                };
                Value::Bool(result)
            }
        }
        _ => Value::Null,
    }
}

fn values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Null, Value::Null) => true,
        _ => value_to_string(left) == value_to_string(right),
    }
}

fn tokenize_expression(expr: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut quote = '"';
    let mut chars = expr.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_string {
            current.push(ch);
            if ch == quote {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                in_string = true;
                quote = ch;
                current.push(ch);
            }
            '(' | ')' => {
                flush_token(&mut current, &mut tokens);
                tokens.push(ch.to_string());
            }
            '?' | ':' if !current.contains('"') => {
                flush_token(&mut current, &mut tokens);
                tokens.push(ch.to_string());
            }
            '+' if !current.ends_with('+') => {
                flush_token(&mut current, &mut tokens);
                tokens.push("+".into());
            }
            '-' if !current.ends_with('-') => {
                flush_token(&mut current, &mut tokens);
                tokens.push("-".into());
            }
            '!' if chars.peek() == Some(&'=') => {
                flush_token(&mut current, &mut tokens);
                chars.next();
                tokens.push("!=".into());
            }
            '!' => {
                flush_token(&mut current, &mut tokens);
                tokens.push("!".into());
            }
            '=' if chars.peek() == Some(&'=') => {
                flush_token(&mut current, &mut tokens);
                chars.next();
                tokens.push("==".into());
            }
            '>' if chars.peek() == Some(&'=') => {
                flush_token(&mut current, &mut tokens);
                chars.next();
                tokens.push(">=".into());
            }
            '<' if chars.peek() == Some(&'=') => {
                flush_token(&mut current, &mut tokens);
                chars.next();
                tokens.push("<=".into());
            }
            '>' => {
                flush_token(&mut current, &mut tokens);
                tokens.push(">".into());
            }
            '<' => {
                flush_token(&mut current, &mut tokens);
                tokens.push("<".into());
            }
            '&' if chars.peek() == Some(&'&') => {
                flush_token(&mut current, &mut tokens);
                chars.next();
                tokens.push("&&".into());
            }
            '|' if chars.peek() == Some(&'|') => {
                flush_token(&mut current, &mut tokens);
                chars.next();
                tokens.push("||".into());
            }
            c if c.is_whitespace() => flush_token(&mut current, &mut tokens),
            _ => current.push(ch),
        }
    }
    flush_token(&mut current, &mut tokens);
    tokens
}

fn flush_token(current: &mut String, tokens: &mut Vec<String>) {
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        tokens.push(trimmed.to_string());
    }
    current.clear();
}

fn operator_precedence(op: &str) -> Option<u8> {
    match op {
        "||" => Some(1),
        "&&" => Some(2),
        "==" | "!=" => Some(3),
        ">" | "<" | ">=" | "<=" => Some(4),
        "+" | "-" => Some(5),
        "!" => Some(8),
        _ => None,
    }
}

fn find_operator_with_precedence(tokens: &[String], precedence: u8) -> Option<usize> {
    let mut depth = 0i32;
    let mut index = None;
    for (i, token) in tokens.iter().enumerate() {
        match token.as_str() {
            "(" => depth += 1,
            ")" => depth -= 1,
            _ if depth == 0 => {
                if operator_precedence(token) == Some(precedence) {
                    index = Some(i);
                }
            }
            _ => {}
        }
    }
    index
}

fn find_top_level_char(expr: &str, needle: char) -> Option<usize> {
    let mut in_string = false;
    let mut quote = '"';
    let mut depth_paren = 0;
    for (index, ch) in expr.char_indices() {
        if in_string {
            if ch == quote {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                in_string = true;
                quote = ch;
            }
            '(' => depth_paren += 1,
            ')' if depth_paren > 0 => depth_paren -= 1,
            c if c == needle && depth_paren == 0 => return Some(index),
            _ => {}
        }
    }
    None
}

fn lookup_json_path(value: &Value, path: &str) -> Option<Value> {
    let mut current = value.clone();
    for segment in path.split('.') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        current = match &current {
            Value::Object(map) => map.get(segment)?.clone(),
            _ => return None,
        };
    }
    Some(current)
}

fn value_truthy(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Number(n) => n.as_f64().is_some_and(|v| v != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn unquote(text: &str) -> String {
    if text.len() >= 2 { text[1..text.len() - 1].replace("\\\"", "\"").replace("\\'", "'") } else { text.to_string() }
}
#[cfg(test)]
#[path = "../../tests/path_wired/codegen_expr_eval.rs"]
mod codegen_expr_eval_tests;
