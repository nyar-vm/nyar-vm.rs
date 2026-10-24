//! PowerShell evaluator thin adapter — delegates to `nyar-language::powershell::interpret`.

use std::collections::HashMap;

use nyar_language::{PowerShellValue, evaluate_powershell_source};

use crate::value::LegacyValue;

/// Evaluate a PowerShell script via the language-package interpreter.
pub fn evaluate_powershell_script(source: &str, env: &mut HashMap<String, LegacyValue>) -> LegacyValue {
    let mut ps_env: HashMap<String, PowerShellValue> = env.iter().map(|(key, value)| (key.clone(), legacy_to_ps(value))).collect();

    let result = match evaluate_powershell_source(source, &mut ps_env) {
        Ok(value) => value,
        Err(_) => PowerShellValue::Null,
    };

    env.clear();
    for (key, value) in ps_env {
        env.insert(key, ps_to_legacy(value));
    }

    ps_to_legacy(result)
}

fn legacy_to_ps(value: &LegacyValue) -> PowerShellValue {
    match value {
        LegacyValue::Null => PowerShellValue::Null,
        LegacyValue::Bool(value) => PowerShellValue::Bool(*value),
        LegacyValue::Int(value) => PowerShellValue::Int(*value),
        LegacyValue::Float(value) => PowerShellValue::Float(*value),
        LegacyValue::String(value) => PowerShellValue::String(value.clone()),
        other => PowerShellValue::String(other.to_string_value()),
    }
}

fn ps_to_legacy(value: PowerShellValue) -> LegacyValue {
    match value {
        PowerShellValue::Null => LegacyValue::Null,
        PowerShellValue::Bool(value) => LegacyValue::Bool(value),
        PowerShellValue::Int(value) => LegacyValue::Int(value),
        PowerShellValue::Float(value) => LegacyValue::Float(value),
        PowerShellValue::String(value) => LegacyValue::String(value),
    }
}
