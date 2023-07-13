//! JSON ↔ runtime `Value` bridge for CLI / leetcode harnesses.

use miette::{IntoDiagnostic, Result};
use nyar_gc::{ObjectHeap, ObjectPayload};
use serde_json::Value as JsonValue;

use crate::{array_runtime::build_fixed_array, error::NyarRuntimeError, value::{ObjectId, Value}};

const JSON_FIXED_ARRAY_LAYOUT: u32 = 0;
const JSON_ARRAY_LIST_LAYOUT: u32 = 1;

/// Parses a JSON value into a runtime `Value` without heap allocation (scalars only).
pub fn value_from_json(json: &JsonValue) -> Result<Value, NyarRuntimeError> {
    materialize_value_from_json(json, None)
}

/// Parses a JSON value into a runtime `Value`, allocating `ArrayList` / fixed-array shells on `heap`.
pub fn materialize_value_from_json(json: &JsonValue, heap: Option<&mut ObjectHeap>) -> Result<Value, NyarRuntimeError> {
    Ok(match json {
        JsonValue::Null => Value::Null,
        JsonValue::Bool(value) => Value::Bool(*value),
        JsonValue::Number(number) => json_number_to_value(number)?,
        JsonValue::String(value) => Value::String(value.clone()),
        JsonValue::Array(items) => {
            let Some(heap) = heap else {
                return Err(NyarRuntimeError::TypeMismatch {
                    expected: "heap-backed array contract",
                    actual: "JSON array without runtime heap".to_owned(),
                });
            };
            build_array_list_from_json_array(heap, items)?
        }
        JsonValue::Object(_) => return Err(NyarRuntimeError::TypeMismatch {
            expected: "declared object export contract",
            actual: "JSON object without layout contract".to_owned(),
        }),
    })
}

fn json_number_to_value(number: &serde_json::Number) -> Result<Value, NyarRuntimeError> {
    if let Some(value) = number.as_i64() {
        Ok(scalar_i64(value))
    } else if let Some(value) = number.as_f64() {
        Ok(Value::F64(value))
    } else {
        Err(NyarRuntimeError::TypeMismatch { expected: "number", actual: number.to_string() })
    }
}

fn scalar_i64(value: i64) -> Value {
    if value >= i32::MIN as i64 && value <= i32::MAX as i64 {
        Value::I32(value as i32)
    } else {
        Value::I64(value)
    }
}

/// `ArrayList { _items, _capacity }` 与 std 降低后的两字段布局对齐。
fn build_array_list_from_json_array(heap: &mut ObjectHeap, items: &[JsonValue]) -> Result<Value, NyarRuntimeError> {
    let elements = items
        .iter()
        .map(|item| materialize_value_from_json(item, Some(heap)))
        .collect::<Result<Vec<_>, _>>()?;
    let items_array = build_fixed_array(heap, JSON_FIXED_ARRAY_LAYOUT, &elements);
    let capacity = Value::I32(items.len() as i32);
    Ok(Value::Object(heap.alloc(ObjectPayload::LayoutObject {
        layout_id: JSON_ARRAY_LIST_LAYOUT,
        slots: vec![items_array, capacity],
    })))
}

/// 将标量或已声明的数组布局序列化为 JSON；无法证明布局时直接失败。
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
            let Some(heap) = heap else {
                return Err(NyarRuntimeError::TypeMismatch {
                    expected: "declared object export contract",
                    actual: format!("object#{id} without heap"),
                });
            };
            layout_object_to_json_array(heap, *id)
        }
        Value::Coroutine(id) => Err(NyarRuntimeError::TypeMismatch {
            expected: "JSON-exportable value",
            actual: format!("coroutine#{id}"),
        }),
    }
}

fn layout_object_to_json_array(heap: &ObjectHeap, object_id: ObjectId) -> Result<JsonValue, NyarRuntimeError> {
    let Some(ObjectPayload::LayoutObject { layout_id, slots }) = heap.get(object_id) else {
        return Err(NyarRuntimeError::TypeMismatch {
            expected: "declared layout object",
            actual: format!("unknown object#{object_id}"),
        });
    };
    match *layout_id {
        JSON_FIXED_ARRAY_LAYOUT => slots.iter().map(|slot| value_to_json_with_heap(slot, Some(heap))).collect(),
        JSON_ARRAY_LIST_LAYOUT => {
            let Some(Value::Object(items_id)) = slots.first() else {
                return Err(NyarRuntimeError::TypeMismatch {
                    expected: "declared ArrayList layout",
                    actual: "ArrayList without items field".to_owned(),
                });
            };
            layout_object_to_json_array(heap, *items_id)
        }
        _ => Err(NyarRuntimeError::TypeMismatch {
            expected: "declared JSON export layout",
            actual: format!("layout id {layout_id}"),
        }),
    }
}

/// Parses a JSON document containing a top-level array of call arguments (scalars only).
pub fn parse_call_args_json(source: &str) -> Result<Vec<Value>> {
    let json: JsonValue = serde_json::from_str(source).into_diagnostic()?;
    let JsonValue::Array(items) = json else {
        return Err(miette::miette!("--args-json must be a JSON array"));
    };
    items.iter().map(value_from_json).collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Parses call arguments and materializes JSON arrays into heap `ArrayList` shells.
pub fn parse_call_args_json_with_heap(source: &str, heap: &mut ObjectHeap) -> Result<Vec<Value>> {
    let json: JsonValue = serde_json::from_str(source).into_diagnostic()?;
    let JsonValue::Array(items) = json else {
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
    fn materialize_json_array_as_array_list() {
        let mut heap = ObjectHeap::new();
        let value = materialize_value_from_json(&JsonValue::Array(vec![JsonValue::from(3), JsonValue::from(3)]), Some(&mut heap))
            .expect("materialize");
        let json = value_to_json_with_heap(&value, Some(&heap)).expect("array export");
        assert_eq!(json, JsonValue::Array(vec![JsonValue::from(3), JsonValue::from(3)]));
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
