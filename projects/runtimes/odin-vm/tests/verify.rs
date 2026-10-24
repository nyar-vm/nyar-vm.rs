use odin_bytecode::{
    NYAR_VERSION, NyarConstant, NyarFunction, NyarHeadCode, NyarImportKind, NyarLayout, NyarModuleData, OBSOLETE_CALL_NATIVE, emit_imm1,
    emit_plain,
};
use odin_vm::{
    error::NyarRuntimeError,
    host::HOST_IMPORT_MODULE,
    verify::{MAX_FUNCTION_CODE_BYTES, MAX_OPERAND_STACK_HEIGHT, verify_module},
};

fn empty_module() -> NyarModuleData {
    NyarModuleData {
        version: NYAR_VERSION,
        name: "test".into(),
        constants: Vec::new(),
        functions: Vec::new(),
        imports: Vec::new(),
        exports: Vec::new(),
        witness_entries: Vec::new(),
        code_bytes: Vec::new(),
        globals: Vec::new(),
        init_function_indices: Vec::new(),
        layouts: Vec::new(),
    }
}

#[test]
fn accepts_empty_v2_module() {
    verify_module(&empty_module()).expect("empty v2 ok");
}

#[test]
fn rejects_invalid_export_contracts() {
    let mut module = empty_module();
    module.exports.push(odin_bytecode::NyarExport {
        kind: odin_bytecode::NyarExportKind::Global,
        symbol_name: "value".to_owned(),
        function_index: -1,
    });
    assert!(verify_module(&module).is_err());
    module.exports[0].function_index = 0;
    assert!(verify_module(&module).is_err());
    module.globals.push(odin_bytecode::NyarGlobal { name: "value".to_owned(), type_name: "test".to_owned() });
    verify_module(&module).expect("valid global export");
    module.exports.push(module.exports[0].clone());
    assert!(verify_module(&module).is_err());
    module.exports.pop();
    module.exports[0].symbol_name.clear();
    assert!(verify_module(&module).is_err());
}

#[test]
fn rejects_invalid_function_and_initializer_contracts() {
    let mut module = empty_module();
    module.functions.push(NyarFunction { name: "init".to_owned(), arity: -1, local_count: 0, code_offset: 0, code_length: 1 });
    module.code_bytes.push(NyarHeadCode::Return as u8);
    assert!(verify_module(&module).is_err());
    module.functions[0].arity = 0;
    module.functions[0].local_count = -1;
    assert!(verify_module(&module).is_err());
    module.functions[0].local_count = 0;
    module.init_function_indices.push(0);
    verify_module(&module).expect("zero-argument initializer");
    module.functions[0].arity = 1;
    assert!(verify_module(&module).is_err());
    module.functions[0].arity = 0;
    module.init_function_indices[0] = -1;
    assert!(verify_module(&module).is_err());
    module.init_function_indices[0] = 1;
    assert!(verify_module(&module).is_err());
}

#[test]
fn rejects_wrong_version() {
    let mut module = empty_module();
    module.version = 1;
    assert!(matches!(verify_module(&module), Err(NyarRuntimeError::ModuleLoad(_))));
}

#[test]
fn rejects_unknown_host_import() {
    let mut module = empty_module();
    module.imports.push(odin_bytecode::NyarImport {
        kind: NyarImportKind::Function,
        module_name: HOST_IMPORT_MODULE.into(),
        symbol_name: "not_a_real_host_op".into(),
    });
    let err = verify_module(&module).expect_err("unknown host");
    assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("unknown host import")));
}

#[test]
fn rejects_call_import_out_of_range() {
    let mut module = empty_module();
    let mut code = Vec::new();
    code.push(NyarHeadCode::CallImport as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.extend_from_slice(&0i32.to_le_bytes());
    module.code_bytes = code;
    assert!(matches!(verify_module(&module), Err(NyarRuntimeError::ImportIndexOutOfRange(0))));
}

#[test]
fn rejects_obsolete_call_native_byte() {
    let mut module = empty_module();
    module.code_bytes = vec![OBSOLETE_CALL_NATIVE, 0, 0, 0, 0, 0, 0, 0, 0];
    let err = verify_module(&module).expect_err("CallNative");
    assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("CallNative")));
}

#[test]
fn accepts_in_range_call_import() {
    let mut module = empty_module();
    module.imports.push(odin_bytecode::NyarImport {
        kind: NyarImportKind::Function,
        module_name: HOST_IMPORT_MODULE.into(),
        symbol_name: "print".into(),
    });
    let mut code = Vec::new();
    code.push(NyarHeadCode::CallImport as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.extend_from_slice(&1i32.to_le_bytes());
    // 无函数表覆盖时只做流式检查；补一个覆盖整段代码的函数以启用 CFG。
    module.functions.push(NyarFunction { name: "main".into(), arity: 0, local_count: 0, code_offset: 0, code_length: code.len() as i32 });
    // CallImport argc=1 需要栈上有 1 个值——构造 Const + CallImport 会更完整；此处仅保留索引检查路径：
    // 空栈 CallImport 应在函数校验中因 underflow 失败。
    module.code_bytes = code;
    let err = verify_module(&module).expect_err("underflow");
    assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("stack underflow")));
}

#[test]
fn accepts_const_call_import_return() {
    let mut module = empty_module();
    module.imports.push(odin_bytecode::NyarImport {
        kind: NyarImportKind::Function,
        module_name: HOST_IMPORT_MODULE.into(),
        symbol_name: "print".into(),
    });
    module.constants.push(NyarConstant::Integer32(1));
    let mut code = Vec::new();
    emit_imm1(&mut code, NyarHeadCode::Constant, 0);
    code.push(NyarHeadCode::CallImport as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.extend_from_slice(&1i32.to_le_bytes());
    emit_plain(&mut code, NyarHeadCode::Return);
    module.functions.push(NyarFunction { name: "main".into(), arity: 0, local_count: 0, code_offset: 0, code_length: code.len() as i32 });
    module.code_bytes = code;
    verify_module(&module).expect("balanced CallImport");
}

#[test]
fn rejects_object_new_out_of_range() {
    let mut module = empty_module();
    let mut code = Vec::new();
    code.push(NyarHeadCode::ObjectNew as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    module.code_bytes = code;
    assert!(matches!(verify_module(&module), Err(NyarRuntimeError::LayoutIndexOutOfRange(0))));
}

#[test]
fn accepts_object_new_with_layout() {
    let mut module = empty_module();
    module.layouts.push(NyarLayout { field_count: 2 });
    let mut code = Vec::new();
    code.push(NyarHeadCode::ObjectNew as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    module.code_bytes = code;
    // 无函数覆盖时流式检查通过即可。
    verify_module(&module).expect("ObjectNew ok");
}

#[test]
fn rejects_field_slot_without_fitting_layout() {
    let mut module = empty_module();
    module.layouts.push(NyarLayout { field_count: 1 });
    let mut code = Vec::new();
    code.push(NyarHeadCode::FieldGet as u8);
    code.extend_from_slice(&1i32.to_le_bytes());
    module.code_bytes = code;
    assert!(matches!(verify_module(&module), Err(NyarRuntimeError::FieldSlotOutOfRange(1))));
}

#[test]
fn rejects_jump_off_instruction_boundary() {
    let mut module = empty_module();
    let mut code = Vec::new();
    // Jump +1 落到 Imm1 立即数中间。
    emit_imm1(&mut code, NyarHeadCode::Jump, 1);
    emit_plain(&mut code, NyarHeadCode::Return);
    module.functions.push(NyarFunction { name: "main".into(), arity: 0, local_count: 0, code_offset: 0, code_length: code.len() as i32 });
    module.code_bytes = code;
    let err = verify_module(&module).expect_err("bad jump");
    assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("instruction boundary")));
}

#[test]
fn rejects_stack_height_mismatch_at_join() {
    // path_a 汇合高度 1，path_b 汇合高度 2。
    let mut module = empty_module();
    module.constants.push(NyarConstant::Integer32(0));
    let mut code = Vec::new();
    emit_imm1(&mut code, NyarHeadCode::Constant, 0); // condition
    let jif_at = code.len();
    emit_imm1(&mut code, NyarHeadCode::JumpIfFalse, 0); // -> path_b
    emit_imm1(&mut code, NyarHeadCode::Constant, 0);
    let jump_a_at = code.len();
    emit_imm1(&mut code, NyarHeadCode::Jump, 0);
    let path_b = code.len();
    emit_imm1(&mut code, NyarHeadCode::Constant, 0);
    emit_imm1(&mut code, NyarHeadCode::Constant, 0);
    let jump_b_at = code.len();
    emit_imm1(&mut code, NyarHeadCode::Jump, 0);
    let join = code.len();
    emit_plain(&mut code, NyarHeadCode::Return);

    let to_path_b = (path_b as i32) - (jif_at as i32);
    code[jif_at + 1..jif_at + 5].copy_from_slice(&to_path_b.to_le_bytes());
    let to_join_a = (join as i32) - (jump_a_at as i32);
    code[jump_a_at + 1..jump_a_at + 5].copy_from_slice(&to_join_a.to_le_bytes());
    let to_join_b = (join as i32) - (jump_b_at as i32);
    code[jump_b_at + 1..jump_b_at + 5].copy_from_slice(&to_join_b.to_le_bytes());

    module.functions.push(NyarFunction { name: "main".into(), arity: 0, local_count: 0, code_offset: 0, code_length: code.len() as i32 });
    module.code_bytes = code;
    let err = verify_module(&module).expect_err("height mismatch");
    assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("stack height mismatch")));
}

#[test]
fn rejects_i32_binop_on_object_ref() {
    let mut module = empty_module();
    module.constants.push(NyarConstant::Integer32(1));
    module.layouts.push(NyarLayout { field_count: 0 });
    let mut code = Vec::new();
    emit_imm1(&mut code, NyarHeadCode::Constant, 0);
    emit_imm1(&mut code, NyarHeadCode::ObjectNew, 0);
    emit_plain(&mut code, NyarHeadCode::I32Add);
    emit_plain(&mut code, NyarHeadCode::Return);
    module.functions.push(NyarFunction { name: "main".into(), arity: 0, local_count: 0, code_offset: 0, code_length: code.len() as i32 });
    module.code_bytes = code;
    let err = verify_module(&module).expect_err("type mismatch");
    assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("stack type mismatch")));
}

#[test]
fn rejects_operand_stack_deeper_than_limit() {
    let mut module = empty_module();
    module.constants.push(NyarConstant::Integer32(0));
    let mut code = Vec::new();
    // 超过 MAX_OPERAND_STACK_HEIGHT 次 Const 压栈。
    for _ in 0..=(MAX_OPERAND_STACK_HEIGHT as usize) {
        emit_imm1(&mut code, NyarHeadCode::Constant, 0);
    }
    emit_plain(&mut code, NyarHeadCode::Return);
    module.functions.push(NyarFunction { name: "main".into(), arity: 0, local_count: 0, code_offset: 0, code_length: code.len() as i32 });
    module.code_bytes = code;
    let err = verify_module(&module).expect_err("stack too deep");
    assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("exceeds limit")));
}

#[test]
fn accepts_balanced_branch_join() {
    let mut module = empty_module();
    module.constants.push(NyarConstant::Integer32(0));
    let mut code = Vec::new();
    emit_imm1(&mut code, NyarHeadCode::Constant, 0);
    let jif_at = code.len();
    emit_imm1(&mut code, NyarHeadCode::JumpIfFalse, 0);
    emit_imm1(&mut code, NyarHeadCode::Constant, 0);
    let jump_a_at = code.len();
    emit_imm1(&mut code, NyarHeadCode::Jump, 0);
    let path_b = code.len();
    emit_imm1(&mut code, NyarHeadCode::Constant, 0);
    let jump_b_at = code.len();
    emit_imm1(&mut code, NyarHeadCode::Jump, 0);
    let join = code.len();
    emit_plain(&mut code, NyarHeadCode::Return);

    let to_path_b = (path_b as i32) - (jif_at as i32);
    code[jif_at + 1..jif_at + 5].copy_from_slice(&to_path_b.to_le_bytes());
    let to_join_a = (join as i32) - (jump_a_at as i32);
    code[jump_a_at + 1..jump_a_at + 5].copy_from_slice(&to_join_a.to_le_bytes());
    let to_join_b = (join as i32) - (jump_b_at as i32);
    code[jump_b_at + 1..jump_b_at + 5].copy_from_slice(&to_join_b.to_le_bytes());

    module.functions.push(NyarFunction { name: "main".into(), arity: 0, local_count: 0, code_offset: 0, code_length: code.len() as i32 });
    module.code_bytes = code;
    verify_module(&module).expect("balanced join");
}

#[test]
fn accepts_store_local_i32_then_binop() {
    let mut module = empty_module();
    module.constants.push(NyarConstant::Integer32(1));
    module.constants.push(NyarConstant::Integer32(2));
    let mut code = Vec::new();
    emit_imm1(&mut code, NyarHeadCode::Constant, 0);
    emit_imm1(&mut code, NyarHeadCode::StoreLocal, 0);
    emit_imm1(&mut code, NyarHeadCode::Constant, 1);
    emit_imm1(&mut code, NyarHeadCode::StoreLocal, 1);
    emit_imm1(&mut code, NyarHeadCode::LoadLocal, 0);
    emit_imm1(&mut code, NyarHeadCode::LoadLocal, 1);
    emit_plain(&mut code, NyarHeadCode::I32Add);
    emit_plain(&mut code, NyarHeadCode::Return);
    module.functions.push(NyarFunction { name: "main".into(), arity: 0, local_count: 2, code_offset: 0, code_length: code.len() as i32 });
    module.code_bytes = code;
    verify_module(&module).expect("local i32 store/load");
}

#[test]
fn rejects_store_ref_then_i32_into_same_local() {
    let mut module = empty_module();
    module.constants.push(NyarConstant::Integer32(0));
    module.layouts.push(NyarLayout { field_count: 0 });
    let mut code = Vec::new();
    emit_imm1(&mut code, NyarHeadCode::ObjectNew, 0);
    emit_imm1(&mut code, NyarHeadCode::StoreLocal, 0);
    emit_imm1(&mut code, NyarHeadCode::Constant, 0);
    emit_imm1(&mut code, NyarHeadCode::StoreLocal, 0);
    emit_plain(&mut code, NyarHeadCode::Return);
    module.functions.push(NyarFunction { name: "main".into(), arity: 0, local_count: 1, code_offset: 0, code_length: code.len() as i32 });
    module.code_bytes = code;
    let err = verify_module(&module).expect_err("local conflict");
    assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("local type conflict")));
}

#[test]
fn rejects_function_code_over_size_budget() {
    let mut module = empty_module();
    let mut code = vec![NyarHeadCode::Nop as u8; MAX_FUNCTION_CODE_BYTES + 1];
    code.push(NyarHeadCode::Return as u8);
    module.functions.push(NyarFunction { name: "main".into(), arity: 0, local_count: 0, code_offset: 0, code_length: code.len() as i32 });
    module.code_bytes = code;
    let err = verify_module(&module).expect_err("size budget");
    assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("exceeds limit")));
}

#[test]
fn rejects_mixed_void_and_value_returns() {
    let mut module = empty_module();
    module.constants.push(NyarConstant::Integer32(1));
    let mut code = Vec::new();
    emit_imm1(&mut code, NyarHeadCode::Constant, 0);
    let jif_at = code.len();
    emit_imm1(&mut code, NyarHeadCode::JumpIfFalse, 0);
    emit_plain(&mut code, NyarHeadCode::Return); // void
    let value_ret = code.len();
    emit_imm1(&mut code, NyarHeadCode::Constant, 0);
    emit_plain(&mut code, NyarHeadCode::Return); // i32
    let to_value = (value_ret as i32) - (jif_at as i32);
    code[jif_at + 1..jif_at + 5].copy_from_slice(&to_value.to_le_bytes());
    module.functions.push(NyarFunction { name: "main".into(), arity: 0, local_count: 0, code_offset: 0, code_length: code.len() as i32 });
    module.code_bytes = code;
    let err = verify_module(&module).expect_err("return mismatch");
    assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("return type mismatch")));
}

#[test]
fn accepts_consistent_i32_returns() {
    let mut module = empty_module();
    module.constants.push(NyarConstant::Integer32(1));
    let mut code = Vec::new();
    emit_imm1(&mut code, NyarHeadCode::Constant, 0);
    let jif_at = code.len();
    emit_imm1(&mut code, NyarHeadCode::JumpIfFalse, 0);
    emit_imm1(&mut code, NyarHeadCode::Constant, 0);
    emit_plain(&mut code, NyarHeadCode::Return);
    let other = code.len();
    emit_imm1(&mut code, NyarHeadCode::Constant, 0);
    emit_plain(&mut code, NyarHeadCode::Return);
    let to_other = (other as i32) - (jif_at as i32);
    code[jif_at + 1..jif_at + 5].copy_from_slice(&to_other.to_le_bytes());
    module.functions.push(NyarFunction { name: "main".into(), arity: 0, local_count: 0, code_offset: 0, code_length: code.len() as i32 });
    module.code_bytes = code;
    verify_module(&module).expect("consistent i32 returns");
}

#[test]
fn rejects_load_arg_beyond_arity() {
    let mut module = empty_module();
    let mut code = Vec::new();
    // arity=1 却 LoadArg 1（仅 local_count 允许该槽）。
    emit_imm1(&mut code, NyarHeadCode::LoadArg, 1);
    emit_plain(&mut code, NyarHeadCode::Return);
    module.functions.push(NyarFunction { name: "main".into(), arity: 1, local_count: 2, code_offset: 0, code_length: code.len() as i32 });
    module.code_bytes = code;
    let err = verify_module(&module).expect_err("LoadArg arity");
    assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("out of arity")));
}

#[test]
fn rejects_load_ref_local_into_i32_binop() {
    let mut module = empty_module();
    module.layouts.push(NyarLayout { field_count: 0 });
    let mut code = Vec::new();
    emit_imm1(&mut code, NyarHeadCode::ObjectNew, 0);
    emit_imm1(&mut code, NyarHeadCode::StoreLocal, 0);
    emit_imm1(&mut code, NyarHeadCode::LoadLocal, 0);
    emit_imm1(&mut code, NyarHeadCode::LoadLocal, 0);
    emit_plain(&mut code, NyarHeadCode::I32Add);
    emit_plain(&mut code, NyarHeadCode::Return);
    module.functions.push(NyarFunction { name: "main".into(), arity: 0, local_count: 1, code_offset: 0, code_length: code.len() as i32 });
    module.code_bytes = code;
    let err = verify_module(&module).expect_err("ref as i32");
    assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("stack type mismatch")));
}

#[test]
fn rejects_jump_if_false_with_ref_condition() {
    let mut module = empty_module();
    module.layouts.push(NyarLayout { field_count: 0 });
    let mut code = Vec::new();
    emit_imm1(&mut code, NyarHeadCode::ObjectNew, 0);
    let br_pc = code.len();
    emit_imm1(&mut code, NyarHeadCode::JumpIfFalse, 0);
    emit_plain(&mut code, NyarHeadCode::Return);
    let else_pc = code.len();
    let rel = (else_pc as i32) - (br_pc as i32);
    code[br_pc + 1..br_pc + 5].copy_from_slice(&rel.to_le_bytes());
    emit_plain(&mut code, NyarHeadCode::Return);
    module.functions.push(NyarFunction { name: "main".into(), arity: 0, local_count: 0, code_offset: 0, code_length: code.len() as i32 });
    module.code_bytes = code;
    let err = verify_module(&module).expect_err("ref JumpIfFalse");
    assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("stack type mismatch")));
}

#[test]
fn accepts_i32_binop_discard_pop_void_return() {
    let mut module = empty_module();
    let mut code = Vec::new();
    emit_imm1(&mut code, NyarHeadCode::LoadLocal, 0);
    emit_imm1(&mut code, NyarHeadCode::LoadLocal, 1);
    emit_plain(&mut code, NyarHeadCode::I32Add);
    emit_plain(&mut code, NyarHeadCode::Pop);
    emit_plain(&mut code, NyarHeadCode::Return);
    module.functions.push(NyarFunction { name: "main".into(), arity: 0, local_count: 2, code_offset: 0, code_length: code.len() as i32 });
    module.code_bytes = code;
    verify_module(&module).expect("i32 Pop void");
}

#[test]
fn rejects_return_with_extra_stack_slots() {
    let mut module = empty_module();
    module.constants.push(NyarConstant::Integer32(1));
    module.constants.push(NyarConstant::Integer32(2));
    let mut code = Vec::new();
    emit_imm1(&mut code, NyarHeadCode::Constant, 0);
    emit_imm1(&mut code, NyarHeadCode::Constant, 1);
    emit_plain(&mut code, NyarHeadCode::Return);
    module.functions.push(NyarFunction { name: "main".into(), arity: 0, local_count: 0, code_offset: 0, code_length: code.len() as i32 });
    module.code_bytes = code;
    let err = verify_module(&module).expect_err("extra stack on return");
    assert!(matches!(
        err,
        NyarRuntimeError::ModuleLoad(message) if message.contains("leaves 2 values on stack")
    ));
}

#[test]
fn accepts_boolean_const_as_i32_jump_condition() {
    let mut module = empty_module();
    module.constants.push(NyarConstant::Boolean(true));
    let mut code = Vec::new();
    emit_imm1(&mut code, NyarHeadCode::Constant, 0);
    let br_pc = code.len();
    emit_imm1(&mut code, NyarHeadCode::JumpIfFalse, 0);
    emit_plain(&mut code, NyarHeadCode::Return);
    let else_pc = code.len();
    let rel = (else_pc as i32) - (br_pc as i32);
    code[br_pc + 1..br_pc + 5].copy_from_slice(&rel.to_le_bytes());
    emit_plain(&mut code, NyarHeadCode::Return);
    module.functions.push(NyarFunction { name: "main".into(), arity: 0, local_count: 0, code_offset: 0, code_length: code.len() as i32 });
    module.code_bytes = code;
    verify_module(&module).expect("boolean JumpIfFalse");
}
