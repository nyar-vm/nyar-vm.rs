use odin_bytecode::{NYAR_VERSION, NyarConstant, NyarExport, NyarExportKind, NyarFunction, NyarHeadCode, NyarModuleData, encode_module};
use odin_vm::{
    executor::{Executor, select_local_roots},
    jit::StackMapEntry,
    module::LoadedModule,
    *,
};

#[test]
fn runs_const_add_return_bytecode() {
    let code = vec![
        NyarHeadCode::Constant as u8,
        0,
        0,
        0,
        0,
        NyarHeadCode::Constant as u8,
        1,
        0,
        0,
        0,
        NyarHeadCode::I32Add as u8,
        NyarHeadCode::Return as u8,
    ];

    let data = NyarModuleData {
        version: NYAR_VERSION,
        name: "test".to_string(),
        constants: vec![NyarConstant::Integer32(0), NyarConstant::Integer32(1)],
        functions: vec![NyarFunction { name: "main".to_string(), arity: 0, local_count: 0, code_offset: 0, code_length: code.len() as i32 }],
        imports: Vec::new(),
        exports: vec![NyarExport { kind: NyarExportKind::Function, symbol_name: "main".to_string(), function_index: 0 }],
        witness_entries: Vec::new(),
        code_bytes: code,
        globals: Vec::new(),
        init_function_indices: Vec::new(),
        layouts: Vec::new(),
    };

    let bytes = encode_module(&data);
    let module = LoadedModule::from_bytes(&bytes).expect("load module");
    let mut executor = Executor::new();
    let result = executor.run(&module, 0, Vec::new()).expect("execute");
    assert_eq!(result, Value::I32(1));
    assert!(executor.run(&module, 0, vec![Value::I32(2)]).is_err());
    let mut argument_module = module.clone();
    argument_module.functions[0].arity = 1;
    assert!(executor.run(&argument_module, 0, Vec::new()).is_err());
    let mut vm = odin_vm::OdinVm::new();
    assert!(vm.run(&module, "main", vec![Value::I32(2)]).is_err());
    assert!(vm.run(&argument_module, "main", Vec::new()).is_err());
}

#[test]
fn select_local_roots_keeps_only_mapped_slots() {
    let locals = vec![Value::I32(1), Value::I32(2), Value::I32(3)];
    let entry = StackMapEntry { instruction_index: 0, local_root_slots: vec![0, 2], operand_root_depth: None };
    let selected = select_local_roots(&locals, &entry);
    assert_eq!(selected, vec![Value::I32(1), Value::I32(3)]);
}

#[test]
fn concurrent_collect_exposes_root_handshake_on_executor() {
    let mut executor = Executor::new();
    *executor.heap_mut().policy_mut() = GcPolicy::concurrent_mark_reserved();
    let live = executor.heap_mut().alloc(ObjectPayload::LayoutObject { layout_id: 0, slots: vec![] });
    let stack = [Value::Object(live)];
    let _ = executor.collect_for_policy(GcRoots { stack: &stack, frame_locals: &[], globals: &[], frame_coroutines: &[] });
    let hs = executor.last_root_handshake();
    assert_eq!(hs.stack_slots, 1);
    assert!(hs.gray_after_roots >= 1);
    let _ = executor.last_trace_poll();
}
