//! 固定数组记录布局（`__fixedarray` / `ArrayList._items`）的运行时读写下标。
//!
//! 由 `CallIntrinsic` 经稠密 intrinsic 下标调用；**禁止**按宿主名字符串分派。

use nyar_gc::{ObjectHeap, ObjectId, ObjectPayload};

use crate::{error::NyarRuntimeError, value::Value};

const RECORD_META_FIELDS: &[&str] = &["__type__", "_address", "_typing", "_capacity"];

/// 构造基数键（`"0"`、`"1"`…）的固定数组记录。
pub fn build_fixed_array_record(heap: &mut ObjectHeap, elements: &[Value]) -> Value {
    let mut fields = vec![("__type__".to_string(), Value::String("__fixedarray".to_string()))];
    for (index, value) in elements.iter().enumerate() {
        fields.push((index.to_string(), value.clone()));
    }
    fields.push(("_length".to_string(), Value::I32(elements.len() as i32)));
    Value::Object(heap.alloc(ObjectPayload::Record(fields)))
}

/// 构造 harness JSON 数字向量用的 `ArrayList` 形记录。
pub fn build_array_list_record(heap: &mut ObjectHeap, elements: &[Value]) -> Value {
    let items = build_fixed_array_record(heap, elements);
    let capacity = elements.len() as i32;
    let fields = vec![
        ("__type__".to_string(), Value::String("std.collection.ArrayList".to_string())),
        ("_items".to_string(), items),
        ("_capacity".to_string(), Value::I32(capacity)),
    ];
    Value::Object(heap.alloc(ObjectPayload::Record(fields)))
}

/// `ArrayLen`：固定数组记录长度；缺 `_length` 时按基数键推算。
pub fn array_len(heap: &ObjectHeap, array: &Value) -> Result<Value, NyarRuntimeError> {
    match array {
        Value::Null => Ok(Value::I32(0)),
        Value::Object(object_id) => Ok(Value::I32(fixed_array_len(heap, *object_id)?)),
        other => Err(NyarRuntimeError::TypeMismatch { expected: "object", actual: other.type_name().to_string() }),
    }
}

/// `ArrayGet`：按基数下标读取元素。
pub fn array_get(heap: &ObjectHeap, array: &Value, index: &Value) -> Result<Value, NyarRuntimeError> {
    match array {
        Value::Null => Ok(Value::Null),
        Value::Object(object_id) => {
            let index = index_as_usize(index)?;
            Ok(read_fixed_array_element(heap, *object_id, index))
        }
        other => Err(NyarRuntimeError::TypeMismatch { expected: "object", actual: other.type_name().to_string() }),
    }
}

/// `ArraySet`：按基数下标写入元素。
pub fn array_set(heap: &mut ObjectHeap, array: &Value, index: &Value, value: &Value) -> Result<Value, NyarRuntimeError> {
    match array {
        Value::Null => Ok(Value::Null),
        Value::Object(object_id) => {
            let index = index_as_usize(index)?;
            write_fixed_array_element(heap, *object_id, index, value.clone());
            Ok(Value::Null)
        }
        other => Err(NyarRuntimeError::TypeMismatch { expected: "object", actual: other.type_name().to_string() }),
    }
}

fn index_as_usize(value: &Value) -> Result<usize, NyarRuntimeError> {
    match value {
        Value::I32(value) if *value >= 0 => Ok(*value as usize),
        Value::I64(value) if *value >= 0 => Ok(*value as usize),
        other => Err(NyarRuntimeError::TypeMismatch { expected: "non-negative index", actual: other.type_name().to_string() }),
    }
}

fn fixed_array_len(heap: &ObjectHeap, object_id: ObjectId) -> Result<i32, NyarRuntimeError> {
    let payload = heap.get(object_id).ok_or_else(|| NyarRuntimeError::ModuleLoad(format!("invalid object id {object_id}")))?;
    let ObjectPayload::Record(fields) = payload
    else {
        return Err(NyarRuntimeError::TypeMismatch { expected: "record array", actual: "non-record object".to_string() });
    };
    if let Some(value) = fields.iter().find(|(key, _)| key == "_length").map(|(_, value)| value) {
        return Ok(match value {
            Value::I32(len) => *len,
            Value::I64(len) => *len as i32,
            _ => cardinal_field_count(fields),
        });
    }
    Ok(cardinal_field_count(fields))
}

fn cardinal_field_count(fields: &[(String, Value)]) -> i32 {
    let mut max_index: Option<usize> = None;
    for (key, _) in fields {
        if RECORD_META_FIELDS.contains(&key.as_str()) {
            continue;
        }
        if let Ok(index) = key.parse::<usize>() {
            max_index = Some(max_index.map_or(index, |current| current.max(index)));
        }
    }
    max_index.map(|index| (index + 1) as i32).unwrap_or(0)
}

fn read_fixed_array_element(heap: &ObjectHeap, object_id: ObjectId, index: usize) -> Value {
    match heap.get(object_id) {
        Some(ObjectPayload::Record(fields)) => field_value(fields, cardinal_field_key(index)).unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

fn write_fixed_array_element(heap: &mut ObjectHeap, object_id: ObjectId, index: usize, value: Value) {
    if let Some(ObjectPayload::Record(fields)) = heap.get_mut(object_id) {
        let key = cardinal_field_key(index);
        if let Some(entry) = fields.iter_mut().find(|(name, _)| name == &key) {
            entry.1 = value;
        }
        else {
            fields.push((key, value));
        }
        let len = cardinal_field_count(fields);
        if let Some(entry) = fields.iter_mut().find(|(name, _)| name == "_length") {
            entry.1 = Value::I32(len);
        }
    }
}

fn cardinal_field_key(index: usize) -> String {
    index.to_string()
}

fn field_value(fields: &[(String, Value)], key: String) -> Option<Value> {
    fields.iter().find(|(name, _)| name == &key).map(|(_, value)| value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_array_len_and_get_use_cardinal_fields() {
        let mut heap = ObjectHeap::new();
        let list = build_array_list_record(&mut heap, &[Value::I64(1), Value::I64(8), Value::I64(6)]);
        let list_id = match list {
            Value::Object(id) => id,
            _ => panic!("expected list object"),
        };
        let ObjectPayload::Record(list_fields) = heap.get(list_id).expect("payload")
        else {
            panic!("expected record");
        };
        let items = list_fields.iter().find(|(key, _)| key == "_items").map(|(_, value)| value.clone()).expect("_items");
        assert_eq!(array_len(&heap, &items).expect("len"), Value::I32(3));
        assert_eq!(array_get(&heap, &items, &Value::I32(0)).expect("get cardinal"), Value::I64(1));
        assert_eq!(array_get(&heap, &items, &Value::I32(1)).expect("get cardinal"), Value::I64(8));
    }
}
