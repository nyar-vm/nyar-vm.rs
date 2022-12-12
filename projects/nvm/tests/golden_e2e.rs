//! End-to-end golden path: encode `.nyar` → load → execute.

use nvm::{NyarVm, Value};
use std_data::binary::nyar_ir::{NyarConstant, NyarExport, NyarExportKind, NyarFunction, NyarHeadCode, NyarModuleData, encode_module};

#[test]
fn golden_const_add_return_roundtrip() {
    let module = NyarModuleData {
        version: 1,
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
    };

    let bytes = encode_module(&module);
    let mut vm = NyarVm::new();
    let loaded = vm.load(&bytes).expect("load golden module");
    let result = vm.run(&loaded, "main", Vec::new()).expect("run main");
    assert_eq!(result, Value::I32(1));
}
