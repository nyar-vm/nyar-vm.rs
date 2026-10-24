use odin_vm::*;

#[test]
fn parse_args_json_array() {
    use odin_vm::value::Value;
    let args = json_bridge::parse_call_args_json("[1, 2, 3]").expect("parse");
    assert_eq!(args.len(), 3);
    assert!(matches!(args[0], Value::I32(1)));
}
