//! CLI 的 JSON 与运行时值边界；缺少导出类型和布局合同的结构值必须拒绝。

use miette::{IntoDiagnostic, Result};
use nyar_gc::ObjectHeap;
use serde_json::Value as JsonValue;

use crate::{error::NyarRuntimeError, value::Value};

/// 将 JSON 标量转换为运行时值，不分配堆对象。
pub fn value_from_json(json: &JsonValue) -> Result<Value, NyarRuntimeError> {
    materialize_value_from_json(json, None)
}

/// 转换 JSON 标量；提供堆也不能代替缺失的导出类型和布局合同。
pub fn materialize_value_from_json(json: &JsonValue, heap: Option<&mut ObjectHeap>) -> Result<Value, NyarRuntimeError> {
    Ok(match json {
        JsonValue::Null => Value::Null,
        JsonValue::Bool(value) => Value::Bool(*value),
        JsonValue::Number(number) => json_number_to_value(number)?,
        JsonValue::String(value) => Value::String(value.clone()),
        JsonValue::Array(_) => {
            if heap.is_none() {
                return Err(NyarRuntimeError::TypeMismatch {
                    expected: "heap-backed array contract",
                    actual: "JSON array without runtime heap".to_owned(),
                });
            }
            return Err(NyarRuntimeError::UnsupportedFeature("JSON array export type/layout contract"));
        }
        JsonValue::Object(_) => {
            return Err(NyarRuntimeError::TypeMismatch {
                expected: "declared object export contract",
                actual: "JSON object without layout contract".to_owned(),
            });
        }
    })
}

fn json_number_to_value(number: &serde_json::Number) -> Result<Value, NyarRuntimeError> {
    if let Some(value) = number.as_i64() {
        Ok(scalar_i64(value))
    }
    else if let Some(value) = number.as_f64() {
        Ok(Value::F64(value))
    }
    else {
        Err(NyarRuntimeError::TypeMismatch { expected: "number", actual: number.to_string() })
    }
}

fn scalar_i64(value: i64) -> Value {
    if value >= i32::MIN as i64 && value <= i32::MAX as i64 { Value::I32(value as i32) } else { Value::I64(value) }
}

/// 将运行时标量序列化为 JSON；结构值没有正式导出合同就直接失败。
pub fn value_to_json(value: &Value) -> Result<JsonValue, NyarRuntimeError> {
    value_to_json_with_heap(value, None)
}

/// 将运行时值序列化为 JSON；对象必须有明确的导出布局合同。
pub fn value_to_json_with_heap(value: &Value, heap: Option<&ObjectHeap>) -> Result<JsonValue, NyarRuntimeError> {
    match value {
        Value::Null => Ok(JsonValue::Null),
        Value::Bool(value) => Ok(JsonValue::Bool(*value)),
        Value::I32(value) => Ok(JsonValue::from(*value)),
        Value::I64(value) => Ok(JsonValue::from(*value)),
        Value::F32(value) => Ok(JsonValue::from(*value)),
        Value::F64(value) => Ok(JsonValue::from(*value)),
        Value::String(value) => Ok(JsonValue::String(value.clone())),
        Value::Object(id) => {
            if heap.is_none() {
                return Err(NyarRuntimeError::TypeMismatch {
                    expected: "declared object export contract",
                    actual: format!("object#{id} without heap"),
                });
            }
            Err(NyarRuntimeError::UnsupportedFeature("JSON object export type/layout contract"))
        }
        Value::Coroutine(id) => Err(NyarRuntimeError::TypeMismatch { expected: "JSON-exportable value", actual: format!("coroutine#{id}") }),
    }
}

/// 解析顶层调用参数列表；当前只接受标量参数。
pub fn parse_call_args_json(source: &str) -> Result<Vec<Value>> {
    let json: JsonValue = serde_json::from_str(source).into_diagnostic()?;
    let JsonValue::Array(items) = json
    else {
        return Err(miette::miette!("--args-json must be a JSON array"));
    };
    items.iter().map(value_from_json).collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// 在运行时堆边界解析调用参数；结构参数缺少导出合同则失败。
pub fn parse_call_args_json_with_heap(source: &str, heap: &mut ObjectHeap) -> Result<Vec<Value>> {
    let json: JsonValue = serde_json::from_str(source).into_diagnostic()?;
    let JsonValue::Array(items) = json
    else {
        return Err(miette::miette!("--args-json must be a JSON array"));
    };
    let mut args = Vec::with_capacity(items.len());
    for item in &items {
        args.push(materialize_value_from_json(item, Some(heap))?);
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nyar_gc::ObjectHeap;

    #[test]
    fn parse_args_json_scalars() {
        let args = parse_call_args_json("[1, 2, 3]").expect("parse");
        assert_eq!(args.len(), 3);
        assert!(matches!(args[0], Value::I32(1)));
    }

    #[test]
    fn structured_json_requires_export_contract() {
        let mut heap = ObjectHeap::new();
        assert!(materialize_value_from_json(&serde_json::json!([3, 3]), Some(&mut heap)).is_err());
        for layout_id in [0, 1, 42] {
            let id = heap.alloc(nyar_gc::ObjectPayload::LayoutObject { layout_id, slots: vec![Value::I32(3), Value::I32(3)] });
            assert!(value_to_json_with_heap(&Value::Object(id), Some(&heap)).is_err());
        }
    }

    #[test]
    fn json_object_requires_export_contract() {
        let result = value_from_json(&serde_json::json!({"value": 1}));
        assert!(matches!(result, Err(NyarRuntimeError::TypeMismatch { .. })));
    }

    #[test]
    fn json_array_requires_heap() {
        let result = value_from_json(&serde_json::json!([1, 2]));
        assert!(matches!(result, Err(NyarRuntimeError::TypeMismatch { .. })));
    }
}
