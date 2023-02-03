use std::fs;

use nyar_vm::{NyarVm, value::Value};

#[test]
fn reverse_integer_smoke() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../leetcode.v/projects/problems/reverse-integer/solvers/valkyrie/.cache/reverse-integer-nyar/reverse.nyar");
    if !path.exists() {
        return;
    }
    let bytes = fs::read(&path).expect("read nyar");
    let module = NyarVm::new().load(&bytes).expect("load");
    let mut vm = NyarVm::new();
    let result = vm.run(&module, "reverse", vec![Value::I32(120)]).expect("run");
    assert!(matches!(result, Value::I32(21) | Value::I64(21)), "got {result:?}");
}
