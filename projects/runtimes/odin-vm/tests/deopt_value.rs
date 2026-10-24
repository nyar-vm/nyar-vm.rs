use odin_vm::*;

#[test]
fn roundtrips_scalars_and_string() {
    for value in
        [Value::Null, Value::Bool(true), Value::I32(-7), Value::I64(99), Value::F32(1.5), Value::F64(-2.25), Value::String("phase".into())]
    {
        let bytes = encode_value_for_deopt(&value).expect("encode");
        let decoded = decode_value_from_deopt(&bytes).expect("decode");
        assert_eq!(decoded, value);
    }
}

#[test]
fn rejects_bare_heap_refs_without_pin() {
    assert!(encode_value_for_deopt(&Value::Object(1)).is_err());
    assert!(encode_value_for_deopt(&Value::Coroutine(2)).is_err());
}

#[test]
fn roundtrips_object_via_root_handle() {
    let mut roots = HostRoots::new();
    let bytes = encode_value_for_deopt_pinned(&Value::Object(7), &mut roots).expect("encode");
    let decoded = decode_value_from_deopt_with_roots(&bytes, Some(&roots)).expect("decode");
    assert_eq!(decoded, Value::Object(7));
    assert!(decode_value_from_deopt(&bytes).is_err());
}
