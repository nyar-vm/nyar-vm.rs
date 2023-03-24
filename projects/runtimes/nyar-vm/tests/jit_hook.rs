use nyar_vm::{jit::JitError, NyarVm};

use nyar_format::{NyarConstant, NyarExport, NyarExportKind, NyarFunction, NyarHeadCode, NyarModuleData, NYAR_VERSION, encode_module};

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
