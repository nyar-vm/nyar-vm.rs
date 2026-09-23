//! Singleton global slot and accessor execution tests.

use nvm::{ModuleGlobals, NyarVm, Value};
use std_data::binary::nyar_ir::{
    NyarConstant, NyarExport, NyarExportKind, NyarFunction, NyarGlobal, NyarHeadCode, NyarModuleData, encode_module,
};

fn emit_call_native(code: &mut Vec<u8>, name_index: i32, arg_count: i32) {
    code.push(NyarHeadCode::CallNative as u8);
    code.extend_from_slice(&name_index.to_le_bytes());
    code.extend_from_slice(&arg_count.to_le_bytes());
}

#[test]
fn eager_singleton_init_and_accessor_roundtrip() {
    let mut code = Vec::new();
    // __init_singleton_Counter
    code.push(NyarHeadCode::Const as u8);
    code.extend_from_slice(&0i32.to_le_bytes()); // "Counter"
    emit_call_native(&mut code, 1, 1); // "alloc_record"
    code.push(NyarHeadCode::StoreGlobal as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Return as u8);

    let accessor_offset = code.len() as i32;
    code.push(NyarHeadCode::LoadGlobal as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Return as u8);

    let module = NyarModuleData {
        version: 1,
        name: "singleton".to_string(),
        constants: vec![NyarConstant::String("Counter".to_string()), NyarConstant::String("alloc_record".to_string())],
        globals: vec![NyarGlobal { name: "Counter.INSTANCE".to_string(), type_name: "Counter".to_string() }],
        init_function_indices: vec![0],
        functions: vec![
            NyarFunction {
                name: "__init_singleton_Counter".to_string(),
                arity: 0,
                local_count: 0,
                code_offset: 0,
                code_length: accessor_offset,
            },
            NyarFunction {
                name: "Counter__instance".to_string(),
                arity: 0,
                local_count: 0,
                code_offset: accessor_offset,
                code_length: code.len() as i32 - accessor_offset,
            },
        ],
        imports: Vec::new(),
        exports: vec![
            NyarExport { kind: NyarExportKind::Global, symbol_name: "Counter.INSTANCE".to_string(), function_index: 0 },
            NyarExport { kind: NyarExportKind::Function, symbol_name: "Counter__instance".to_string(), function_index: 1 },
        ],
        witness_entries: Vec::new(),
        code_bytes: code,
    };

    let bytes = encode_module(&module);
    let mut vm = NyarVm::new();
    let loaded = vm.load(&bytes).expect("load module");
    let mut globals = ModuleGlobals::new(&loaded);
    let result = vm.run_with_globals(&loaded, &mut globals, "Counter__instance", Vec::new()).expect("run accessor");
    assert!(matches!(result, Value::Object(_)));
}

#[test]
fn lazy_singleton_accessor_allocates_once() {
    let mut code = Vec::new();
    let accessor_offset = 0i32;
    code.push(NyarHeadCode::LoadGlobal as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Dup as u8);
    code.push(NyarHeadCode::JumpIfTrue as u8);
    let jump_pos = code.len();
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Pop as u8);
    code.push(NyarHeadCode::Const as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    emit_call_native(&mut code, 1, 1);
    code.push(NyarHeadCode::StoreGlobal as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    let return_target = code.len();
    let offset = (return_target as i32) - (jump_pos as i32);
    code[jump_pos..jump_pos + 4].copy_from_slice(&offset.to_le_bytes());
    code.push(NyarHeadCode::LoadGlobal as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Return as u8);

    let module = NyarModuleData {
        version: 1,
        name: "lazy_singleton".to_string(),
        constants: vec![NyarConstant::String("Counter".to_string()), NyarConstant::String("alloc_record".to_string())],
        globals: vec![NyarGlobal { name: "Counter.INSTANCE".to_string(), type_name: "Counter".to_string() }],
        init_function_indices: Vec::new(),
        functions: vec![NyarFunction {
            name: "Counter__get_instance".to_string(),
            arity: 0,
            local_count: 0,
            code_offset: accessor_offset,
            code_length: code.len() as i32,
        }],
        imports: Vec::new(),
        exports: vec![NyarExport { kind: NyarExportKind::Function, symbol_name: "Counter__get_instance".to_string(), function_index: 0 }],
        witness_entries: Vec::new(),
        code_bytes: code,
    };

    let bytes = encode_module(&module);
    let mut vm = NyarVm::new();
    let loaded = vm.load(&bytes).expect("load module");
    let mut globals = ModuleGlobals::new(&loaded);
    let first = vm.run_with_globals(&loaded, &mut globals, "Counter__get_instance", Vec::new()).expect("first call");
    let second = vm.run_with_globals(&loaded, &mut globals, "Counter__get_instance", Vec::new()).expect("second call");
    assert_eq!(first, second);
    assert!(matches!(first, Value::Object(_)));
}

/// End-to-end test for singleton field read/write covering Task 5.2.
///
/// `NyarConstant` exposes `Integer32` (no `I64` variant) and the available
/// arithmetic opcode is `I32Add` (no `I64Add`/`LdcI64`), so integer field
/// values use `NyarConstant::Integer32` and assertions check `Value::I32`.
#[test]
fn singleton_field_read_write_roundtrip() {
    let mut code = Vec::new();

    // Function 0: __init_singleton_Counter
    // Const("Counter") -> CallNative(alloc_record, 1) -> StoreGlobal(0) -> Return
    let init_offset = code.len() as i32;
    code.push(NyarHeadCode::Const as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    emit_call_native(&mut code, 1, 1);
    code.push(NyarHeadCode::StoreGlobal as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Return as u8);
    let init_length = code.len() as i32 - init_offset;

    // Function 1: set_total
    // LoadGlobal(0) -> Const("total") -> Const(I32(42)) -> CallNative(record_set, 3) -> Pop -> Return
    let set_total_offset = code.len() as i32;
    code.push(NyarHeadCode::LoadGlobal as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Const as u8);
    code.extend_from_slice(&2i32.to_le_bytes());
    code.push(NyarHeadCode::Const as u8);
    code.extend_from_slice(&5i32.to_le_bytes());
    emit_call_native(&mut code, 3, 3);
    code.push(NyarHeadCode::Pop as u8);
    code.push(NyarHeadCode::Return as u8);
    let set_total_length = code.len() as i32 - set_total_offset;

    // Function 2: get_total
    // LoadGlobal(0) -> Const("total") -> CallNative(record_get, 2) -> Return
    let get_total_offset = code.len() as i32;
    code.push(NyarHeadCode::LoadGlobal as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Const as u8);
    code.extend_from_slice(&2i32.to_le_bytes());
    emit_call_native(&mut code, 4, 2);
    code.push(NyarHeadCode::Return as u8);
    let get_total_length = code.len() as i32 - get_total_offset;

    // Function 3: increment_total
    // Compute current+1, stash in local 0, then write back via record_set.
    // LoadGlobal(0) -> Const("total") -> CallNative(record_get, 2) -> Const(I32(1)) -> I32Add
    // -> StoreLocal(0) -> LoadGlobal(0) -> Const("total") -> LoadLocal(0) -> CallNative(record_set, 3) -> Pop -> Return
    let increment_offset = code.len() as i32;
    code.push(NyarHeadCode::LoadGlobal as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Const as u8);
    code.extend_from_slice(&2i32.to_le_bytes());
    emit_call_native(&mut code, 4, 2);
    code.push(NyarHeadCode::Const as u8);
    code.extend_from_slice(&6i32.to_le_bytes());
    code.push(NyarHeadCode::I32Add as u8);
    code.push(NyarHeadCode::StoreLocal as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::LoadGlobal as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Const as u8);
    code.extend_from_slice(&2i32.to_le_bytes());
    code.push(NyarHeadCode::LoadLocal as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    emit_call_native(&mut code, 3, 3);
    code.push(NyarHeadCode::Pop as u8);
    code.push(NyarHeadCode::Return as u8);
    let increment_length = code.len() as i32 - increment_offset;

    let module = NyarModuleData {
        version: 1,
        name: "singleton_field_rw".to_string(),
        constants: vec![
            NyarConstant::String("Counter".to_string()),
            NyarConstant::String("alloc_record".to_string()),
            NyarConstant::String("total".to_string()),
            NyarConstant::String("record_set".to_string()),
            NyarConstant::String("record_get".to_string()),
            NyarConstant::Integer32(42),
            NyarConstant::Integer32(1),
        ],
        globals: vec![NyarGlobal { name: "Counter.INSTANCE".to_string(), type_name: "Counter".to_string() }],
        init_function_indices: vec![0],
        functions: vec![
            NyarFunction {
                name: "__init_singleton_Counter".to_string(),
                arity: 0,
                local_count: 0,
                code_offset: init_offset,
                code_length: init_length,
            },
            NyarFunction {
                name: "set_total".to_string(),
                arity: 0,
                local_count: 0,
                code_offset: set_total_offset,
                code_length: set_total_length,
            },
            NyarFunction {
                name: "get_total".to_string(),
                arity: 0,
                local_count: 0,
                code_offset: get_total_offset,
                code_length: get_total_length,
            },
            NyarFunction {
                name: "increment_total".to_string(),
                arity: 0,
                local_count: 1,
                code_offset: increment_offset,
                code_length: increment_length,
            },
        ],
        imports: Vec::new(),
        exports: vec![
            NyarExport { kind: NyarExportKind::Function, symbol_name: "set_total".to_string(), function_index: 1 },
            NyarExport { kind: NyarExportKind::Function, symbol_name: "get_total".to_string(), function_index: 2 },
            NyarExport { kind: NyarExportKind::Function, symbol_name: "increment_total".to_string(), function_index: 3 },
        ],
        witness_entries: Vec::new(),
        code_bytes: code,
    };

    let bytes = encode_module(&module);
    let mut vm = NyarVm::new();
    let loaded = vm.load(&bytes).expect("load module");
    let mut globals = ModuleGlobals::new(&loaded);

    // First call triggers the eager init (allocates Counter singleton into global 0).
    vm.run_with_globals(&loaded, &mut globals, "set_total", Vec::new()).expect("run set_total");

    let total = vm.run_with_globals(&loaded, &mut globals, "get_total", Vec::new()).expect("run get_total");
    assert_eq!(total, Value::I32(42));

    vm.run_with_globals(&loaded, &mut globals, "increment_total", Vec::new()).expect("run increment_total");

    let total_after = vm.run_with_globals(&loaded, &mut globals, "get_total", Vec::new()).expect("run get_total again");
    assert_eq!(total_after, Value::I32(43));
}

/// End-to-end test for lazy singleton field writes persisting across calls covering Task 5.3.
#[test]
fn lazy_singleton_field_write_persists_across_calls() {
    let mut code = Vec::new();

    // Function 0: accessor (lazy null-check pattern, mirrors lazy_singleton_accessor_allocates_once).
    let accessor_offset = code.len() as i32;
    code.push(NyarHeadCode::LoadGlobal as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Dup as u8);
    code.push(NyarHeadCode::JumpIfTrue as u8);
    let jump_pos = code.len();
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Pop as u8);
    code.push(NyarHeadCode::Const as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    emit_call_native(&mut code, 1, 1);
    code.push(NyarHeadCode::StoreGlobal as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    let return_target = code.len();
    let offset = (return_target as i32) - (jump_pos as i32);
    code[jump_pos..jump_pos + 4].copy_from_slice(&offset.to_le_bytes());
    code.push(NyarHeadCode::LoadGlobal as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Return as u8);
    let accessor_length = code.len() as i32 - accessor_offset;

    // Function 1: set_field
    // Call(accessor) -> Const("value") -> Const(I32(100)) -> CallNative(record_set, 3) -> Pop -> Return
    let set_field_offset = code.len() as i32;
    code.push(NyarHeadCode::Call as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Const as u8);
    code.extend_from_slice(&2i32.to_le_bytes());
    code.push(NyarHeadCode::Const as u8);
    code.extend_from_slice(&5i32.to_le_bytes());
    emit_call_native(&mut code, 3, 3);
    code.push(NyarHeadCode::Pop as u8);
    code.push(NyarHeadCode::Return as u8);
    let set_field_length = code.len() as i32 - set_field_offset;

    // Function 2: get_field
    // Call(accessor) -> Const("value") -> CallNative(record_get, 2) -> Return
    let get_field_offset = code.len() as i32;
    code.push(NyarHeadCode::Call as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Const as u8);
    code.extend_from_slice(&2i32.to_le_bytes());
    emit_call_native(&mut code, 4, 2);
    code.push(NyarHeadCode::Return as u8);
    let get_field_length = code.len() as i32 - get_field_offset;

    let module = NyarModuleData {
        version: 1,
        name: "lazy_singleton_field_rw".to_string(),
        constants: vec![
            NyarConstant::String("Counter".to_string()),
            NyarConstant::String("alloc_record".to_string()),
            NyarConstant::String("value".to_string()),
            NyarConstant::String("record_set".to_string()),
            NyarConstant::String("record_get".to_string()),
            NyarConstant::Integer32(100),
        ],
        globals: vec![NyarGlobal { name: "Counter.INSTANCE".to_string(), type_name: "Counter".to_string() }],
        init_function_indices: Vec::new(),
        functions: vec![
            NyarFunction {
                name: "Counter__get_instance".to_string(),
                arity: 0,
                local_count: 0,
                code_offset: accessor_offset,
                code_length: accessor_length,
            },
            NyarFunction {
                name: "set_field".to_string(),
                arity: 0,
                local_count: 0,
                code_offset: set_field_offset,
                code_length: set_field_length,
            },
            NyarFunction {
                name: "get_field".to_string(),
                arity: 0,
                local_count: 0,
                code_offset: get_field_offset,
                code_length: get_field_length,
            },
        ],
        imports: Vec::new(),
        exports: vec![
            NyarExport { kind: NyarExportKind::Function, symbol_name: "Counter__get_instance".to_string(), function_index: 0 },
            NyarExport { kind: NyarExportKind::Function, symbol_name: "set_field".to_string(), function_index: 1 },
            NyarExport { kind: NyarExportKind::Function, symbol_name: "get_field".to_string(), function_index: 2 },
        ],
        witness_entries: Vec::new(),
        code_bytes: code,
    };

    let bytes = encode_module(&module);
    let mut vm = NyarVm::new();
    let loaded = vm.load(&bytes).expect("load module");
    let mut globals = ModuleGlobals::new(&loaded);

    // set_field triggers lazy allocation via the accessor, then writes value=100.
    vm.run_with_globals(&loaded, &mut globals, "set_field", Vec::new()).expect("run set_field");

    let first_read = vm.run_with_globals(&loaded, &mut globals, "get_field", Vec::new()).expect("run get_field");
    assert_eq!(first_read, Value::I32(100));

    let second_read = vm.run_with_globals(&loaded, &mut globals, "get_field", Vec::new()).expect("run get_field again");
    assert_eq!(second_read, Value::I32(100));

    // Accessor must allocate only once: the singleton object id stays stable across calls.
    let instance_a = vm.run_with_globals(&loaded, &mut globals, "Counter__get_instance", Vec::new()).expect("accessor call a");
    let instance_b = vm.run_with_globals(&loaded, &mut globals, "Counter__get_instance", Vec::new()).expect("accessor call b");
    assert_eq!(instance_a, instance_b);
    assert!(matches!(instance_a, Value::Object(_)));
}
