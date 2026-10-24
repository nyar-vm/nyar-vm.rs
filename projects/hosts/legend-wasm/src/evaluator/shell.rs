//! Shared shell line helpers for Bash and PowerShell evaluators.

use crate::algebra::CoreEvaluator;

/// Strip surrounding single or double quotes.
pub fn strip_quotes(value: &str) -> String {
    let value = value.trim();
    if (value.starts_with('"') && value.ends_with('"')) || (value.starts_with('\'') && value.ends_with('\'')) {
        value[1..value.len().saturating_sub(1)].to_string()
    }
    else {
        value.to_string()
    }
}

/// Expand `$name` variables using the core evaluator environment.
pub fn expand_dollar_var(value: &str, core: &CoreEvaluator) -> String {
    let mut output = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '$' {
            let mut name = String::new();
            while let Some(&next) = chars.peek() {
                if next.is_ascii_alphanumeric() || next == '_' {
                    name.push(chars.next().unwrap());
                }
                else {
                    break;
                }
            }
            if !name.is_empty() {
                output.push_str(&core.var(&name).to_string_value());
            }
        }
        else {
            output.push(ch);
        }
    }
    output
}
