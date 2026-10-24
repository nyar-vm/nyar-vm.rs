use odin_bytecode::*;

#[test]
fn structural_opcodes_are_imm1() {
    assert_eq!(NyarHeadCode::ObjectNew.form(), NyarInstructionForm::Imm1);
    assert_eq!(NyarHeadCode::FieldGet.form(), NyarInstructionForm::Imm1);
    assert_eq!(NyarHeadCode::FieldSet.form(), NyarInstructionForm::Imm1);
    assert_eq!(NyarHeadCode::from_u8(0x50), Some(NyarHeadCode::ObjectNew));
    assert_eq!(NyarHeadCode::from_u8(0x51), Some(NyarHeadCode::FieldGet));
    assert_eq!(NyarHeadCode::from_u8(0x52), Some(NyarHeadCode::FieldSet));
}

#[test]
fn bytecode_format_version_tracks_nyar_version() {
    assert_eq!(BYTECODE_FORMAT_VERSION, NYAR_VERSION);
    // 未正式发布前格式基线统一为 v0；递增格式必须同时抬 `NYAR_VERSION`。
    assert_eq!(BYTECODE_FORMAT_VERSION, 0);
}

#[test]
fn obsolete_call_native_is_not_a_live_head_code() {
    assert_eq!(OBSOLETE_CALL_NATIVE, 0xD1);
    assert_eq!(NyarHeadCode::from_u8(OBSOLETE_CALL_NATIVE), None);
    assert_ne!(NyarHeadCode::CallImport as u8, OBSOLETE_CALL_NATIVE);
}

#[test]
fn emit_helpers_round_trip_with_decode_at() {
    let mut code = Vec::new();
    emit_plain(&mut code, NyarHeadCode::Return);
    emit_imm1(&mut code, NyarHeadCode::Constant, 7);
    emit_imm2(&mut code, NyarHeadCode::CallImport, 1, 2);
    let ret = decode_at(&code, 0);
    assert_eq!(ret.code, NyarHeadCode::Return);
    assert_eq!(ret.size, 1);
    let c = decode_at(&code, 1);
    assert_eq!(c.code, NyarHeadCode::Constant);
    assert_eq!(c.operand1, 7);
    assert_eq!(c.size, 5);
    let call = decode_at(&code, 6);
    assert_eq!(call.code, NyarHeadCode::CallImport);
    assert_eq!(call.operand1, 1);
    assert_eq!(call.operand2, 2);
    assert_eq!(call.size, 9);
}

#[test]
fn layouts_section_round_trips() {
    let data = NyarModuleData {
        version: NYAR_VERSION,
        name: "layout-test".into(),
        constants: Vec::new(),
        functions: Vec::new(),
        imports: Vec::new(),
        exports: Vec::new(),
        witness_entries: Vec::new(),
        code_bytes: Vec::new(),
        globals: Vec::new(),
        init_function_indices: Vec::new(),
        layouts: vec![NyarLayout { field_count: 2 }, NyarLayout { field_count: 0 }],
    };
    let bytes = encode_module(&data);
    let decoded = decode_module(&bytes).expect("decode");
    assert_eq!(decoded.layouts, data.layouts);
}
