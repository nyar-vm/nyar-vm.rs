//! Lua evaluator thin adapter — delegates to `nyar-language::lua::interpret`.

use std::collections::HashMap;

use nyar_language::{LuaValue, evaluate_lua_source};

use crate::value::LegacyValue;

/// Evaluate a Lua script via the language-package interpreter.
pub fn evaluate_lua_script(source: &str, env: &mut HashMap<String, LegacyValue>) -> LegacyValue {
    let mut lua_env: HashMap<String, LuaValue> = env.iter().map(|(key, value)| (key.clone(), legacy_to_lua(value))).collect();

    let result = match evaluate_lua_source(source, &mut lua_env) {
        Ok(value) => value,
        Err(_) => LuaValue::Null,
    };

    env.clear();
    for (key, value) in lua_env {
        env.insert(key, lua_to_legacy(value));
    }

    lua_to_legacy(result)
}

fn legacy_to_lua(value: &LegacyValue) -> LuaValue {
    match value {
        LegacyValue::Null => LuaValue::Null,
        LegacyValue::Bool(value) => LuaValue::Bool(*value),
        LegacyValue::Int(value) => LuaValue::Int(*value),
        LegacyValue::Float(value) => LuaValue::Float(*value),
        LegacyValue::String(value) => LuaValue::String(value.clone()),
        other => LuaValue::String(other.to_string_value()),
    }
}

fn lua_to_legacy(value: LuaValue) -> LegacyValue {
    match value {
        LuaValue::Null => LegacyValue::Null,
        LuaValue::Bool(value) => LegacyValue::Bool(value),
        LuaValue::Int(value) => LegacyValue::Int(value),
        LuaValue::Float(value) => LegacyValue::Float(value),
        LuaValue::String(value) => LegacyValue::String(value),
        LuaValue::Table(_) => LegacyValue::String(value.to_string_value()),
    }
}
