use nyar_gc::ObjectHeap;
use odin_bytecode::NyarImport;
use odin_vm::{
    host::{HostOp, ResolvedImport, execute_host_op, parse_host_op, resolve_import},
    value::Value,
};

#[test]
fn parses_all_seed_host_ops() {
    assert!(parse_host_op("alloc_record").is_none(), "struct ops must not revive string host bridge");
    assert_eq!(parse_host_op("print"), Some(HostOp::Print));
    assert_eq!(parse_host_op("i64_add"), Some(HostOp::I64Add));
    assert_eq!(parse_host_op("begin_phase"), Some(HostOp::BeginPhase));
    assert_eq!(parse_host_op("end_phase"), Some(HostOp::EndPhase));
    assert!(parse_host_op("not_a_real_host_op").is_none());
}

#[test]
fn begin_and_end_phase_update_heap_strategy() {
    let mut heap = ObjectHeap::new();
    execute_host_op(HostOp::BeginPhase, &[Value::String("request".into()), Value::I32(5)], &mut heap).expect("begin");
    assert_eq!(heap.strategy().phase_depth(), 1);
    assert_eq!(heap.policy().hints.phase.as_deref(), Some("request"));
    assert_eq!(heap.policy().hints.pause_budget_ms, Some(5));
    execute_host_op(HostOp::EndPhase, &[Value::String("request".into())], &mut heap).expect("end");
    assert_eq!(heap.strategy().phase_depth(), 0);
}

#[test]
fn resolves_non_host_as_external() {
    let import = NyarImport { kind: odin_bytecode::NyarImportKind::Function, module_name: "guest.mod".into(), symbol_name: "whatever".into() };
    assert_eq!(resolve_import(&import).unwrap(), ResolvedImport::External);
}
