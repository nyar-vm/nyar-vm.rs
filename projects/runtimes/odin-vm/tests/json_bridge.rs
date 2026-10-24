use nyar_gc::{ObjectHeap, Value};
use odin_vm::{
    error::NyarRuntimeError,
    json_bridge::{materialize_value_from_json, parse_call_args_json, value_from_json, value_to_json_with_heap},
};

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
