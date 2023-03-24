//! End-to-end golden path: encode `.nyar` → load → execute.

use nyar_vm::{NyarVm, Value};
use nyar_format::{
    NyarConstant, NyarExport, NyarExportKind, NyarFunction, NyarHeadCode, NyarLayout, NyarModuleData, NYAR_VERSION,
    encode_module,
};

#[test]
fn golden_const_add_return_roundtrip() {
    let module = NyarModuleData {
        version: NYAR_VERSION,
        name: "golden".to_string(),
        constants: vec![NyarConstant::Integer32(0), NyarConstant::Integer32(1)],
        functions: vec![NyarFunction { name: "main".to_string(), arity: 0, local_count: 0, code_offset: 0, code_length: 12 }],
        imports: Vec::new(),
        exports: vec![NyarExport { kind: NyarExportKind::Function, symbol_name: "main".to_string(), function_index: 0 }],
        witness_entries: Vec::new(),
        code_bytes: vec![
            NyarHeadCode::Const as u8,
            0x00,
            0x00,
            0x00,
            0x00,
            NyarHeadCode::Const as u8,
            0x01,
            0x00,
            0x00,
            0x00,
            NyarHeadCode::I32Add as u8,
            NyarHeadCode::Return as u8,
        ],
        globals: Vec::new(),
        init_function_indices: Vec::new(),
        layouts: Vec::new(),
    };

    let bytes = encode_module(&module);
    let mut vm = NyarVm::new();
    let loaded = vm.load(&bytes).expect("load golden module");
    let result = vm.run(&loaded, "main", Vec::new()).expect("run main");
    assert_eq!(result, Value::I32(1));
}

#[test]
fn golden_object_new_field_get_set_roundtrip() {
    // ObjectNew(0); Const(0); FieldSet(0); FieldGet(0); Return
    // layout[0].field_count = 1；常量 0 = i32 42
    let mut code = Vec::new();
    code.push(NyarHeadCode::ObjectNew as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Const as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::FieldSet as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::FieldGet as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Return as u8);

    let module = NyarModuleData {
        version: NYAR_VERSION,
        name: "golden-layout".to_string(),
        constants: vec![NyarConstant::Integer32(42)],
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
        layouts: vec![NyarLayout { field_count: 1 }],
    };

    let bytes = encode_module(&module);
    let mut vm = NyarVm::new();
    let loaded = vm.load(&bytes).expect("load layout module");
    let result = vm.run(&loaded, "main", Vec::new()).expect("run main");
    assert_eq!(result, Value::I32(42));
}
