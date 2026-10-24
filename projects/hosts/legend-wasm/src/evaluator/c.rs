//! C evaluator thin adapter — delegates to `nyar-language::c::interpret`.

use std::collections::HashMap;

use nyar_language::{CValue, evaluate_c_source};

use crate::value::LegacyValue;

/// Evaluate a C translation unit via the language-package interpreter.
pub fn evaluate_c_script(source: &str, env: &mut HashMap<String, LegacyValue>) -> LegacyValue {
    let mut c_env: HashMap<String, CValue> = env.iter().map(|(key, value)| (key.clone(), legacy_to_c(value))).collect();

    let result = match evaluate_c_source(source, &mut c_env) {
        Ok(value) => value,
        Err(_) => CValue::Null,
    };

    env.clear();
    for (key, value) in c_env {
        env.insert(key, c_to_legacy(value));
    }

    c_to_legacy(result)
}

fn legacy_to_c(value: &LegacyValue) -> CValue {
    match value {
        LegacyValue::Null => CValue::Null,
        LegacyValue::Bool(value) => CValue::Bool(*value),
        LegacyValue::Int(value) => CValue::Int(*value),
        LegacyValue::Float(value) => CValue::Float(*value),
        LegacyValue::String(value) => CValue::String(value.clone()),
        other => CValue::String(other.to_string_value()),
    }
}

fn c_to_legacy(value: CValue) -> LegacyValue {
    match value {
        CValue::Null => LegacyValue::Null,
        CValue::Bool(value) => LegacyValue::Bool(value),
        CValue::Int(value) => LegacyValue::Int(value),
        CValue::Float(value) => LegacyValue::Float(value),
        CValue::String(value) => LegacyValue::String(value),
    }
}
