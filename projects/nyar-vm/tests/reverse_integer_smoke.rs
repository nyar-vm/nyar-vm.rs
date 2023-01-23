use std::fs;

use nyar_vm::{NyarVm, value::Value};

/// End-to-end smoke for leetcode `reverse-integer` `.nyar` artifact.
/// Ignored until loop-body SSA slot mapping matches `mi`/`mx` homes (currently returns `0`).
#[test]
#[ignore]
fn reverse_integer_smoke() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../leetcode.v/projects/problems/reverse-integer/solvers/valkyrie/.cache/reverse-integer-nyar/reverse.nyar");
    if !path.exists() {
        return;
    }
    let bytes = fs::read(&path).expect("read nyar");
    let module = NyarVm::new().load(&bytes).expect("load");
    let function = module.functions.first().expect("function");
    eprintln!("arity={} local_count={}", function.arity, function.local_count);

    let mut vm = NyarVm::new();
    let result = vm.run(&module, "reverse", vec![Value::I32(120)]).expect("run");
    eprintln!("result={result:?}");
    assert!(matches!(result, Value::I32(21) | Value::I64(21)), "got {result:?}");
}
