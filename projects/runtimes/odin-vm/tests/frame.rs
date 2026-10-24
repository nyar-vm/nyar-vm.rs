use nyar_jit::{build_baseline_deopt_map, materialize_interpreter_frames};
use odin_vm::{frame::Frame, *};

#[test]
fn from_deopt_restore_maps_absent_to_null() {
    let map = build_baseline_deopt_map(4, 2, &[7]);
    let entry = map.entry_at(7).expect("entry");
    let frames = materialize_interpreter_frames(entry, &[vec![]]).expect("materialize");
    let frame = Frame::from_deopt_restore(&frames[0], 0, None).expect("restore");
    assert_eq!(frame.function_index, 4);
    assert_eq!(frame.ip, 8);
    assert_eq!(frame.locals, vec![Value::Null, Value::Null]);
}

#[test]
fn from_deopt_restore_decodes_provided_i32() {
    let map = build_baseline_deopt_map(0, 1, &[0]);
    let entry = map.entry_at(0).expect("entry");
    let payload = odin_vm::deopt_value::encode_value_for_deopt(&Value::I32(42)).expect("encode");
    let frames = materialize_interpreter_frames(entry, &[vec![Some(payload)]]).expect("materialize");
    let frame = Frame::from_deopt_restore(&frames[0], 0, None).expect("restore");
    assert_eq!(frame.locals, vec![Value::I32(42)]);
}
