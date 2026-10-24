//! Bash evaluator thin adapter - delegates to `nyar-language::bash::interpret`.

use std::collections::HashMap;

use nyar_language::{BashValue, evaluate_bash_source};

use crate::value::LegacyValue;

/// Evaluate a Bash script via the language-package interpreter.
pub fn evaluate_bash_script(source: &str, env: &mut HashMap<String, LegacyValue>) -> LegacyValue {
    let mut bash_env: HashMap<String, BashValue> = env.iter().map(|(key, value)| (key.clone(), legacy_to_bash(value))).collect();

    let result = match evaluate_bash_source(source, &mut bash_env) {
        Ok(value) => value,
        Err(_) => BashValue::Null,
    };

    env.clear();
    for (key, value) in bash_env {
        env.insert(key, bash_to_legacy(value));
    }

    bash_to_legacy(result)
}

fn legacy_to_bash(value: &LegacyValue) -> BashValue {
    match value {
        LegacyValue::Null => BashValue::Null,
        LegacyValue::Bool(value) => BashValue::Bool(*value),
        LegacyValue::Int(value) => BashValue::Int(*value),
        LegacyValue::Float(value) => BashValue::Int(*value as i64),
        LegacyValue::String(value) => BashValue::String(value.clone()),
        other => BashValue::String(other.to_string_value()),
    }
}

fn bash_to_legacy(value: BashValue) -> LegacyValue {
    match value {
        BashValue::Null => LegacyValue::Null,
        BashValue::Bool(value) => LegacyValue::Bool(value),
        BashValue::Int(value) => LegacyValue::Int(value),
        BashValue::String(value) => LegacyValue::String(value),
    }
}
