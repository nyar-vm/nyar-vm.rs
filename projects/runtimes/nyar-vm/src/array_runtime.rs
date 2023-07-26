//! 固定数组 `LayoutObject` 的运行时按下标读写。
//!
//! 由 `CallIntrinsic` 经稠密 intrinsic 下标调用；元素槽与 `FieldGet`/`FieldSet` 同为
//! `LayoutObject.slots[i]`，**禁止**字段名字符串寻址。

use nyar_gc::{ObjectHeap, ObjectId, ObjectPayload};

use crate::{error::NyarRuntimeError, value::Value};

/// 构造元素已填好的固定数组布局对象（供 harness / 单测注入）。
///
/// `layout_id` 仅作堆描述；`ArrayGet`/`ArraySet`/`ArrayLen` 只读 `slots`。
pub fn build_fixed_array(heap: &mut ObjectHeap, layout_id: u32, elements: &[Value]) -> Value {
    Value::Object(heap.alloc(ObjectPayload::LayoutObject {
        layout_id,
        slots: elements.to_vec(),
    }))
}

/// `ArrayLen`：布局对象槽位数。
pub fn array_len(heap: &ObjectHeap, array: &Value) -> Result<Value, NyarRuntimeError> {
    match array {
        Value::Null => Ok(Value::I32(0)),
        Value::Object(object_id) => Ok(Value::I32(layout_array_len(heap, *object_id)?)),
        other => Err(NyarRuntimeError::TypeMismatch { expected: "object", actual: other.type_name().to_string() }),
    }
}

/// `ArrayGet`：按下标读取元素槽。
pub fn array_get(heap: &ObjectHeap, array: &Value, index: &Value) -> Result<Value, NyarRuntimeError> {
    match array {
        Value::Null => Ok(Value::Null),
        Value::Object(object_id) => {
            let index = index_as_usize(index)?;
            read_layout_element(heap, *object_id, index)
        }
        other => Err(NyarRuntimeError::TypeMismatch { expected: "object", actual: other.type_name().to_string() }),
    }
}

/// `ArraySet`：按下标写入元素槽。
pub fn array_set(heap: &mut ObjectHeap, array: &Value, index: &Value, value: &Value) -> Result<Value, NyarRuntimeError> {
    match array {
        Value::Null => Ok(Value::Null),
        Value::Object(object_id) => {
            let index = index_as_usize(index)?;
            write_layout_element(heap, *object_id, index, value.clone())?;
            Ok(Value::Null)
        }
        other => Err(NyarRuntimeError::TypeMismatch { expected: "object", actual: other.type_name().to_string() }),
    }
}

/// `ArrayPush`：向可增长数组布局末尾追加元素；返回原数组引用。
pub fn array_push(heap: &mut ObjectHeap, array: &Value, value: &Value) -> Result<Value, NyarRuntimeError> {
    match array {
        Value::Null => Err(NyarRuntimeError::TypeMismatch {
            expected: "object",
            actual: "null".to_string(),
        }),
        Value::Object(object_id) => {
            heap.append_field(*object_id, value.clone()).map_err(|reason| match reason {
                "object not found" => NyarRuntimeError::ModuleLoad(format!("invalid object id {object_id}")),
                other => NyarRuntimeError::TypeMismatch {
                    expected: "layout array",
                    actual: other.to_string(),
                },
            })?;
            Ok(array.clone())
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

fn layout_array_len(heap: &ObjectHeap, object_id: ObjectId) -> Result<i32, NyarRuntimeError> {
    match heap.get(object_id) {
        Some(ObjectPayload::LayoutObject { slots, .. }) => Ok(slots.len() as i32),
        Some(_) => Err(NyarRuntimeError::TypeMismatch {
            expected: "layout array",
            actual: "non-layout object".to_string(),
        }),
        None => Err(NyarRuntimeError::ModuleLoad(format!("invalid object id {object_id}"))),
    }
}

fn read_layout_element(heap: &ObjectHeap, object_id: ObjectId, index: usize) -> Result<Value, NyarRuntimeError> {
    match heap.get(object_id) {
        Some(ObjectPayload::LayoutObject { slots, .. }) => {
            if index >= slots.len() {
                return Ok(Value::Null);
            }
            Ok(slots[index].clone())
        }
        Some(_) => Err(NyarRuntimeError::TypeMismatch {
            expected: "layout array",
            actual: "non-layout object".to_string(),
        }),
        None => Err(NyarRuntimeError::ModuleLoad(format!("invalid object id {object_id}"))),
    }
}

fn write_layout_element(heap: &mut ObjectHeap, object_id: ObjectId, index: usize, value: Value) -> Result<(), NyarRuntimeError> {
    // 与 FieldSet 同一写屏障入口，保证老→年轻记忆集不漏数组元素写。
    heap.set_field(object_id, index, value).map_err(|reason| match reason {
        "field slot out of range" => NyarRuntimeError::FieldSlotOutOfRange(index as i32),
        "object not found" => NyarRuntimeError::ModuleLoad(format!("invalid object id {object_id}")),
        other => NyarRuntimeError::TypeMismatch {
            expected: "layout array",
            actual: other.to_string(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn array_push_grows_layout_slots() {
        let mut heap = ObjectHeap::new();
        let array = build_fixed_array(&mut heap, 0, &[]);
        assert_eq!(array_len(&heap, &array).expect("len0"), Value::I32(0));
        array_push(&mut heap, &array, &Value::I64(2)).expect("push");
        array_push(&mut heap, &array, &Value::I64(7)).expect("push");
        assert_eq!(array_len(&heap, &array).expect("len2"), Value::I32(2));
        assert_eq!(array_get(&heap, &array, &Value::I32(0)).expect("get0"), Value::I64(2));
        assert_eq!(array_get(&heap, &array, &Value::I32(1)).expect("get1"), Value::I64(7));
    }

    #[test]
    fn fixed_array_len_get_set_use_layout_slots() {
        let mut heap = ObjectHeap::new();
        let array = build_fixed_array(&mut heap, 0, &[Value::I64(1), Value::I64(8), Value::I64(6)]);
        assert_eq!(array_len(&heap, &array).expect("len"), Value::I32(3));
        assert_eq!(array_get(&heap, &array, &Value::I32(0)).expect("get 0"), Value::I64(1));
        assert_eq!(array_get(&heap, &array, &Value::I32(1)).expect("get 1"), Value::I64(8));
        array_set(&mut heap, &array, &Value::I32(1), &Value::I64(99)).expect("set");
        assert_eq!(array_get(&heap, &array, &Value::I32(1)).expect("get after set"), Value::I64(99));
    }

    #[test]
    fn rejects_non_object_for_array_intrinsics() {
        let heap = ObjectHeap::new();
        let err = array_len(&heap, &Value::I32(1)).expect_err("non-object must fail");
        assert!(matches!(err, NyarRuntimeError::TypeMismatch { .. }));
    }

    #[test]
    fn array_set_records_old_to_young_barrier() {
        use nyar_gc::Generation;

        let mut heap = ObjectHeap::new();
        let array_id = heap.alloc_tenured(ObjectPayload::LayoutObject {
            layout_id: 0,
            slots: vec![Value::Null, Value::Null],
        });
        let young = heap.alloc(ObjectPayload::LayoutObject {
            layout_id: 1,
            slots: vec![],
        });
        let array = Value::Object(array_id);
        array_set(&mut heap, &array, &Value::I32(0), &Value::Object(young)).expect("set");
        assert!(!heap.barrier().remembered_set().is_empty());
        assert_eq!(heap.generation(array_id), Some(Generation::Tenured));
        assert_eq!(heap.generation(young), Some(Generation::Nursery));
    }
}
