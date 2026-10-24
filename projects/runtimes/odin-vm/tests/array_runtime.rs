use nyar_gc::{ObjectHeap, ObjectPayload, Value};
use odin_vm::{
    array_runtime::{array_get, array_len, array_push, array_set, build_fixed_array},
    error::NyarRuntimeError,
};

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
    let array_id = heap.alloc_tenured(ObjectPayload::LayoutObject { layout_id: 0, slots: vec![Value::Null, Value::Null] });
    let young = heap.alloc(ObjectPayload::LayoutObject { layout_id: 1, slots: vec![] });
    let array = Value::Object(array_id);
    array_set(&mut heap, &array, &Value::I32(0), &Value::Object(young)).expect("set");
    assert!(!heap.barrier().remembered_set().is_empty());
    assert_eq!(heap.generation(array_id), Some(Generation::Tenured));
    assert_eq!(heap.generation(young), Some(Generation::Nursery));
}
