//! Tcl evaluator thin adapter — delegates to `nyar-language::tcl::interpret`.

use std::collections::HashMap;

use nyar_language::{TclValue, evaluate_tcl_source};

use crate::value::LegacyValue;

/// Evaluate a Tcl script via the language-package interpreter.
pub fn evaluate_tcl_script(source: &str, env: &mut HashMap<String, LegacyValue>) -> LegacyValue {
    let mut tcl_env: HashMap<String, TclValue> = env.iter().map(|(key, value)| (key.clone(), legacy_to_tcl(value))).collect();

    let result = match evaluate_tcl_source(source, &mut tcl_env) {
        Ok(value) => value,
        Err(_) => TclValue::Null,
    };

    env.clear();
    for (key, value) in tcl_env {
        env.insert(key, tcl_to_legacy(value));
    }

    tcl_to_legacy(result)
}

fn legacy_to_tcl(value: &LegacyValue) -> TclValue {
    match value {
        LegacyValue::Null => TclValue::Null,
        LegacyValue::Bool(value) => TclValue::Bool(*value),
        LegacyValue::Int(value) => TclValue::Int(*value),
        LegacyValue::Float(value) => TclValue::Float(*value),
        LegacyValue::String(value) => TclValue::String(value.clone()),
        other => TclValue::String(other.to_string_value()),
    }
}

fn tcl_to_legacy(value: TclValue) -> LegacyValue {
    match value {
        TclValue::Null => LegacyValue::Null,
        TclValue::Bool(value) => LegacyValue::Bool(value),
        TclValue::Int(value) => LegacyValue::Int(value),
        TclValue::Float(value) => LegacyValue::Float(value),
        TclValue::String(value) => LegacyValue::String(value),
    }
}
