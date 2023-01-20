//! Minimal JSON ↔ runtime `Value` bridge for CLI harnesses.

use miette::{IntoDiagnostic, Result};
use serde_json::Value as JsonValue;

use crate::{error::NyarRuntimeError, value::Value};

/// Parses a JSON value into a runtime `Value` (scalars only; arrays/objects map to strings for now).
pub fn value_from_json(json: &JsonValue) -> Result<Value, NyarRuntimeError> {
    Ok(match json {
        JsonValue::Null => Value::Null,
        JsonValue::Bool(value) => Value::Bool(*value),
        JsonValue::Number(number) => {
            if let Some(value) = number.as_i64() {
                if value >= i32::MIN as i64 && value <= i32::MAX as i64 {
                    Value::I32(value as i32)
                } else {
                    Value::I64(value)
                }
            } else if let Some(value) = number.as_f64() {
                Value::F64(value)
            } else {
                return Err(NyarRuntimeError::TypeMismatch { expected: "number", actual: number.to_string() });
            }
        }
        JsonValue::String(value) => Value::String(value.clone()),
        JsonValue::Array(_) | JsonValue::Object(_) => Value::String(json.to_string()),
    })
}

/// Serializes a runtime `Value` to JSON (scalars; heap values as diagnostic strings).
pub fn value_to_json(value: &Value) -> JsonValue {
    match value {
        Value::Null => JsonValue::Null,
        Value::Bool(value) => JsonValue::Bool(*value),
        Value::I32(value) => JsonValue::from(*value),
        Value::I64(value) => JsonValue::from(*value),
        Value::F32(value) => JsonValue::from(*value),
        Value::F64(value) => JsonValue::from(*value),
        Value::String(value) => JsonValue::String(value.clone()),
        Value::Object(id) => JsonValue::String(format!("object#{id}")),
        Value::Coroutine(id) => JsonValue::String(format!("coroutine#{id}")),
    }
}

/// Parses a JSON document containing a top-level array of call arguments.
pub fn parse_call_args_json(source: &str) -> Result<Vec<Value>> {
    let json: JsonValue = serde_json::from_str(source).into_diagnostic()?;
    let JsonValue::Array(items) = json else {
        return Err(miette::miette!("--args-json must be a JSON array"));
    };
    items.iter().map(value_from_json).collect::<Result<Vec<_>, _>>().map_err(Into::into)
}
