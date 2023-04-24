use nyar_vm::{jit::JitError, NyarVm};

use nyar_bytecode::{NyarConstant, NyarExport, NyarExportKind, NyarFunction, NyarHeadCode, NyarModuleData, NYAR_VERSION, encode_module};

fn empty_module() -> nyar_vm::module::LoadedModule {
    let code = vec![NyarHeadCode::Return as u8];
    let data = NyarModuleData {
        version: NYAR_VERSION,
        name: "jit-hook".to_string(),
        constants: vec![NyarConstant::Integer32(0)],
        functions: vec![NyarFunction {
            name: "main".to_string(),
            arity: 0,
            local_count: 0,
            code_offset: 0,
            code_length: code.len() as i32,
        }],
        imports: Vec::new(),
        exports: vec![NyarExport { kind: NyarExportKind::Function, symbol_name: "main".to_string(), function_index: 0 }],
        witness_entries: Vec::new(),
        code_bytes: code,
        globals: Vec::new(),
        init_function_indices: Vec::new(),
        layouts: Vec::new(),
    };
    let bytes = encode_module(&data);
    NyarVm::new().load(&bytes).expect("load module")
}

#[test]
fn executor_default_jit_is_disabled() {
    let module = empty_module();
    let mut vm = NyarVm::new();
    assert!(!vm.jit_enabled());
    assert_eq!(vm.try_jit_compile(&module, 0), Err(JitError::Unsupported));
}

#[test]
fn stack_maps_available_without_jit_backend() {
    use nyar_bytecode::NyarHeadCode;

    let mut code = Vec::new();
    code.push(NyarHeadCode::ObjectNew as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Return as u8);
    let data = NyarModuleData {
        version: NYAR_VERSION,
        name: "stack-map".to_string(),
        constants: Vec::new(),
        functions: vec![NyarFunction {
            name: "main".to_string(),
            arity: 0,
            local_count: 2,
            code_offset: 0,
            code_length: code.len() as i32,
        }],
        imports: Vec::new(),
        exports: vec![NyarExport { kind: NyarExportKind::Function, symbol_name: "main".to_string(), function_index: 0 }],
        witness_entries: Vec::new(),
        code_bytes: code,
        globals: Vec::new(),
        init_function_indices: Vec::new(),
        layouts: vec![nyar_bytecode::NyarLayout { field_count: 0 }],
    };
    let module = NyarVm::new().load(&encode_module(&data)).expect("load");
    let maps = NyarVm::new().stack_maps_for(&module, 0).expect("stack maps");
    assert_eq!(maps.function_index, 0);
    assert!(!maps.entries.is_empty());
    assert_eq!(maps.entries[0].local_root_slots, vec![0, 1]);
    // ObjectNew 与 Return 均为 safepoint
    assert!(maps.entries.len() >= 2);
}

#[test]
fn stack_map_jit_backend_returns_artifact_maps() {
    use nyar_vm::jit::StackMapJit;

    let module = empty_module();
    let mut vm = NyarVm::new();
    vm.set_jit(Box::new(StackMapJit));
    assert!(vm.jit_enabled());
    let artifact = vm.try_jit_compile(&module, 0).expect("stack-map jit");
    assert_eq!(artifact.function_index, 0);
    assert!(!artifact.stack_maps.entries.is_empty());
    assert!(!artifact.deopt_map.entries.is_empty());
}

#[test]
fn baseline_scalar_jit_fast_path_adds_i32_args() {
    use nyar_vm::jit::BaselineScalarJit;
    use nyar_vm::Value;

    let mut code = Vec::new();
    code.push(NyarHeadCode::LoadArg as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::LoadArg as u8);
    code.extend_from_slice(&1i32.to_le_bytes());
    code.push(NyarHeadCode::I32Add as u8);
    code.push(NyarHeadCode::Return as u8);
    let data = NyarModuleData {
        version: NYAR_VERSION,
        name: "nj1-add".to_string(),
        constants: Vec::new(),
        functions: vec![NyarFunction {
            name: "add".to_string(),
            arity: 2,
            local_count: 2,
            code_offset: 0,
            code_length: code.len() as i32,
        }],
        imports: Vec::new(),
        exports: vec![NyarExport { kind: NyarExportKind::Function, symbol_name: "add".to_string(), function_index: 0 }],
        witness_entries: Vec::new(),
        code_bytes: code,
        globals: Vec::new(),
        init_function_indices: Vec::new(),
        layouts: Vec::new(),
    };
    let module = NyarVm::new().load(&encode_module(&data)).expect("load");
    let mut vm = NyarVm::new();
    vm.set_jit(Box::new(BaselineScalarJit));
    let artifact = vm.try_jit_compile(&module, 0).expect("compile");
    assert!(artifact.machine_code.is_some());
    let result = vm.run(&module, "add", vec![Value::I32(40), Value::I32(2)]).expect("run");
    assert_eq!(result, Value::I32(42));
}
