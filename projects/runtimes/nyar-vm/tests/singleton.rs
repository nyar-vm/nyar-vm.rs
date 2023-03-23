//! Singleton global slot and accessor execution tests.
//!
//! 分配与字段访问走 `ObjectNew` / `FieldGet` / `FieldSet` + `layouts`，
//! 不再经字符串宿主 `alloc_record` / `record_*`。

use nyar_vm::{ModuleGlobals, NyarVm, Value};
use std_data::binary::nyar_ir::{
    NyarConstant, NyarExport, NyarExportKind, NyarFunction, NyarGlobal, NyarHeadCode, NyarLayout, NyarModuleData, NYAR_VERSION,
    encode_module,
};

fn emit_imm1(code: &mut Vec<u8>, opcode: NyarHeadCode, operand: i32) {
    code.push(opcode as u8);
    code.extend_from_slice(&operand.to_le_bytes());
}

#[test]
fn eager_singleton_init_and_accessor_roundtrip() {
    let mut code = Vec::new();
    // __init_singleton_Counter: ObjectNew(0); StoreGlobal(0); Return
    emit_imm1(&mut code, NyarHeadCode::ObjectNew, 0);
    emit_imm1(&mut code, NyarHeadCode::StoreGlobal, 0);
    code.push(NyarHeadCode::Return as u8);

    let accessor_offset = code.len() as i32;
    emit_imm1(&mut code, NyarHeadCode::LoadGlobal, 0);
    code.push(NyarHeadCode::Return as u8);

    let module = NyarModuleData {
        version: NYAR_VERSION,
        name: "singleton".to_string(),
        constants: Vec::new(),
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
        layouts: vec![NyarLayout { field_count: 0 }],
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
    // 惰性分配：两路径在 Return 处栈高度均为 1。
    //   LoadGlobal; Dup; JumpIfTrue -> hit; Pop; ObjectNew; Dup; StoreGlobal; Jump -> hit; hit: Return
    emit_imm1(&mut code, NyarHeadCode::LoadGlobal, 0);
    code.push(NyarHeadCode::Dup as u8);
    let jump_if_pc = code.len();
    code.push(NyarHeadCode::JumpIfTrue as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Pop as u8);
    emit_imm1(&mut code, NyarHeadCode::ObjectNew, 0);
    code.push(NyarHeadCode::Dup as u8);
    emit_imm1(&mut code, NyarHeadCode::StoreGlobal, 0);
    let jump_to_hit_pc = code.len();
    code.push(NyarHeadCode::Jump as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    let hit = code.len();
    code.push(NyarHeadCode::Return as u8);
    let to_hit_if = (hit as i32) - (jump_if_pc as i32);
    code[jump_if_pc + 1..jump_if_pc + 5].copy_from_slice(&to_hit_if.to_le_bytes());
    let to_hit_jump = (hit as i32) - (jump_to_hit_pc as i32);
    code[jump_to_hit_pc + 1..jump_to_hit_pc + 5].copy_from_slice(&to_hit_jump.to_le_bytes());

    let module = NyarModuleData {
        version: NYAR_VERSION,
        name: "lazy_singleton".to_string(),
        constants: Vec::new(),
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
        layouts: vec![NyarLayout { field_count: 0 }],
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

/// Singleton 字段读写：slot 0 = total；常量 0=42、1=1。
#[test]
fn singleton_field_read_write_roundtrip() {
    let mut code = Vec::new();

    let init_offset = code.len() as i32;
    emit_imm1(&mut code, NyarHeadCode::ObjectNew, 0);
    emit_imm1(&mut code, NyarHeadCode::StoreGlobal, 0);
    code.push(NyarHeadCode::Return as u8);
    let init_length = code.len() as i32 - init_offset;

    // set_total: LoadGlobal; Const(42); FieldSet(0); Pop; Return
    let set_total_offset = code.len() as i32;
    emit_imm1(&mut code, NyarHeadCode::LoadGlobal, 0);
    emit_imm1(&mut code, NyarHeadCode::Const, 0);
    emit_imm1(&mut code, NyarHeadCode::FieldSet, 0);
    code.push(NyarHeadCode::Pop as u8);
    code.push(NyarHeadCode::Return as u8);
    let set_total_length = code.len() as i32 - set_total_offset;

    // get_total: LoadGlobal; FieldGet(0); Return
    let get_total_offset = code.len() as i32;
    emit_imm1(&mut code, NyarHeadCode::LoadGlobal, 0);
    emit_imm1(&mut code, NyarHeadCode::FieldGet, 0);
    code.push(NyarHeadCode::Return as u8);
    let get_total_length = code.len() as i32 - get_total_offset;

    // increment_total: get → +1 → set
    let increment_offset = code.len() as i32;
    emit_imm1(&mut code, NyarHeadCode::LoadGlobal, 0);
    emit_imm1(&mut code, NyarHeadCode::FieldGet, 0);
    emit_imm1(&mut code, NyarHeadCode::Const, 1);
    code.push(NyarHeadCode::I32Add as u8);
    emit_imm1(&mut code, NyarHeadCode::StoreLocal, 0);
    emit_imm1(&mut code, NyarHeadCode::LoadGlobal, 0);
    emit_imm1(&mut code, NyarHeadCode::LoadLocal, 0);
    emit_imm1(&mut code, NyarHeadCode::FieldSet, 0);
    code.push(NyarHeadCode::Pop as u8);
    code.push(NyarHeadCode::Return as u8);
    let increment_length = code.len() as i32 - increment_offset;

    let module = NyarModuleData {
        version: NYAR_VERSION,
        name: "singleton_field_rw".to_string(),
        constants: vec![NyarConstant::Integer32(42), NyarConstant::Integer32(1)],
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
        layouts: vec![NyarLayout { field_count: 1 }],
    };

    let bytes = encode_module(&module);
    let mut vm = NyarVm::new();
    let loaded = vm.load(&bytes).expect("load module");
    let mut globals = ModuleGlobals::new(&loaded);

    vm.run_with_globals(&loaded, &mut globals, "set_total", Vec::new()).expect("run set_total");

    let total = vm.run_with_globals(&loaded, &mut globals, "get_total", Vec::new()).expect("run get_total");
    assert_eq!(total, Value::I32(42));

    vm.run_with_globals(&loaded, &mut globals, "increment_total", Vec::new()).expect("run increment_total");

    let total_after = vm.run_with_globals(&loaded, &mut globals, "get_total", Vec::new()).expect("run get_total again");
    assert_eq!(total_after, Value::I32(43));
}

/// 惰性 singleton：字段写入在多次调用间持久。
#[test]
fn lazy_singleton_field_write_persists_across_calls() {
    let mut code = Vec::new();

    // constants: 0 = i32 100
    let accessor_offset = code.len() as i32;
    // 惰性分配：两路径在 Return 处栈高度均为 1。
    //   LoadGlobal; Dup; JumpIfTrue -> hit; Pop; ObjectNew; Dup; StoreGlobal; Jump -> hit; hit: Return
    emit_imm1(&mut code, NyarHeadCode::LoadGlobal, 0);
    code.push(NyarHeadCode::Dup as u8);
    let jump_if_pc = code.len();
    code.push(NyarHeadCode::JumpIfTrue as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Pop as u8);
    emit_imm1(&mut code, NyarHeadCode::ObjectNew, 0);
    code.push(NyarHeadCode::Dup as u8);
    emit_imm1(&mut code, NyarHeadCode::StoreGlobal, 0);
    let jump_to_hit_pc = code.len();
    code.push(NyarHeadCode::Jump as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    let hit = code.len();
    code.push(NyarHeadCode::Return as u8);
    let to_hit_if = (hit as i32) - (jump_if_pc as i32);
    code[jump_if_pc + 1..jump_if_pc + 5].copy_from_slice(&to_hit_if.to_le_bytes());
    let to_hit_jump = (hit as i32) - (jump_to_hit_pc as i32);
    code[jump_to_hit_pc + 1..jump_to_hit_pc + 5].copy_from_slice(&to_hit_jump.to_le_bytes());
    let accessor_length = code.len() as i32 - accessor_offset;

    // set_field: Call(accessor); Const(100); FieldSet(0); Pop; Return
    let set_field_offset = code.len() as i32;
    emit_imm1(&mut code, NyarHeadCode::Call, 0);
    emit_imm1(&mut code, NyarHeadCode::Const, 0);
    emit_imm1(&mut code, NyarHeadCode::FieldSet, 0);
    code.push(NyarHeadCode::Pop as u8);
    code.push(NyarHeadCode::Return as u8);
    let set_field_length = code.len() as i32 - set_field_offset;

    // get_field: Call(accessor); FieldGet(0); Return
    let get_field_offset = code.len() as i32;
    emit_imm1(&mut code, NyarHeadCode::Call, 0);
    emit_imm1(&mut code, NyarHeadCode::FieldGet, 0);
    code.push(NyarHeadCode::Return as u8);
    let get_field_length = code.len() as i32 - get_field_offset;

    let module = NyarModuleData {
        version: NYAR_VERSION,
        name: "lazy_singleton_field_rw".to_string(),
        constants: vec![NyarConstant::Integer32(100)],
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
        layouts: vec![NyarLayout { field_count: 1 }],
    };

    let bytes = encode_module(&module);
    let mut vm = NyarVm::new();
    let loaded = vm.load(&bytes).expect("load module");
    let mut globals = ModuleGlobals::new(&loaded);

    vm.run_with_globals(&loaded, &mut globals, "set_field", Vec::new()).expect("run set_field");

    let first_read = vm.run_with_globals(&loaded, &mut globals, "get_field", Vec::new()).expect("run get_field");
    assert_eq!(first_read, Value::I32(100));

    let second_read = vm.run_with_globals(&loaded, &mut globals, "get_field", Vec::new()).expect("run get_field again");
    assert_eq!(second_read, Value::I32(100));

    let instance_a = vm.run_with_globals(&loaded, &mut globals, "Counter__get_instance", Vec::new()).expect("accessor call a");
    let instance_b = vm.run_with_globals(&loaded, &mut globals, "Counter__get_instance", Vec::new()).expect("accessor call b");
    assert_eq!(instance_a, instance_b);
    assert!(matches!(instance_a, Value::Object(_)));
}
