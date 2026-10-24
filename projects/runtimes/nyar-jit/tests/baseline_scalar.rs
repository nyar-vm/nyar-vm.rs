use nyar_jit::*;
use odin_bytecode::{NYAR_VERSION, NyarHeadCode};

fn request(code: Vec<u8>) -> JitCompileRequest {
    request_with_constants(code, Vec::new())
}

fn request_with_constants(code: Vec<u8>, constant_i32: Vec<Option<i32>>) -> JitCompileRequest {
    JitCompileRequest {
        module_version: NYAR_VERSION,
        module_name: "t".into(),
        code_bytes: code.clone(),
        constant_i32,
        function_index: 0,
        function: JitFunctionSpec { code_offset: 0, code_length: code.len() as i32, local_count: 4, arity: 4, safepoint_indices: Vec::new() },
    }
}

#[test]
fn compiles_const_i32_return() {
    let mut code = Vec::new();
    code.push(NyarHeadCode::Constant as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Return as u8);
    let mut jit = BaselineScalarJit;
    let artifact = jit.compile_function(&request_with_constants(code, vec![Some(123)])).expect("compile");
    let blob = artifact.machine_code.as_ref().expect("machine code");
    assert_eq!(decode_scalar_program(blob).unwrap(), ScalarProgram::RetConstI32 { value: 123 });
}

#[test]
fn compiles_i32_div_rem() {
    let mut jit = BaselineScalarJit;
    for (op, binop) in [(NyarHeadCode::I32DivS, I32Binop::DivS), (NyarHeadCode::I32RemS, I32Binop::RemS)] {
        let mut code = Vec::new();
        emit_load(&mut code, 0);
        emit_load(&mut code, 1);
        code.push(op as u8);
        code.push(NyarHeadCode::Return as u8);
        let artifact = jit.compile_function(&request(code)).expect("compile");
        let blob = artifact.machine_code.as_ref().expect("machine code");
        assert_eq!(decode_scalar_program(blob).unwrap(), ScalarProgram::RetI32BinopLocals { binop, a: 0, b: 1 });
    }
}

fn emit_load(code: &mut Vec<u8>, slot: i32) {
    code.push(NyarHeadCode::LoadLocal as u8);
    code.extend_from_slice(&slot.to_le_bytes());
}

#[test]
fn compiles_i32_cmp_return() {
    let mut code = Vec::new();
    emit_load(&mut code, 0);
    emit_load(&mut code, 1);
    code.push(NyarHeadCode::I32LtS as u8);
    code.push(NyarHeadCode::Return as u8);
    let mut jit = BaselineScalarJit;
    let artifact = jit.compile_function(&request(code)).expect("compile");
    let blob = artifact.machine_code.as_ref().expect("machine code");
    assert_eq!(decode_scalar_program(blob).unwrap(), ScalarProgram::RetI32CmpLocals { cmp: I32Cmp::LtS, a: 0, b: 1 });
}

#[test]
fn compiles_load_dup_binop() {
    let mut code = Vec::new();
    emit_load(&mut code, 0);
    code.push(NyarHeadCode::Dup as u8);
    code.push(NyarHeadCode::I32Mul as u8);
    code.push(NyarHeadCode::Return as u8);
    let mut jit = BaselineScalarJit;
    let artifact = jit.compile_function(&request(code)).expect("compile");
    let blob = artifact.machine_code.as_ref().expect("machine code");
    assert_eq!(decode_scalar_program(blob).unwrap(), ScalarProgram::RetI32BinopLocals { binop: I32Binop::Mul, a: 0, b: 0 });
}

#[test]
fn folds_const_const_binop_to_ret_const() {
    let mut code = Vec::new();
    code.push(NyarHeadCode::Constant as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Constant as u8);
    code.extend_from_slice(&1i32.to_le_bytes());
    code.push(NyarHeadCode::I32Mul as u8);
    code.push(NyarHeadCode::Return as u8);
    let mut jit = BaselineScalarJit;
    let artifact = jit.compile_function(&request_with_constants(code, vec![Some(6), Some(7)])).expect("compile");
    let blob = artifact.machine_code.as_ref().expect("machine code");
    assert_eq!(decode_scalar_program(blob).unwrap(), ScalarProgram::RetConstI32 { value: 42 });
}

#[test]
fn folds_const_const_binop_pop_to_ret_void() {
    let mut code = Vec::new();
    code.push(NyarHeadCode::Constant as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Constant as u8);
    code.extend_from_slice(&1i32.to_le_bytes());
    code.push(NyarHeadCode::I32Add as u8);
    code.push(NyarHeadCode::Pop as u8);
    code.push(NyarHeadCode::Return as u8);
    let mut jit = BaselineScalarJit;
    let artifact = jit.compile_function(&request_with_constants(code, vec![Some(6), Some(7)])).expect("compile");
    let blob = artifact.machine_code.as_ref().expect("machine code");
    assert_eq!(decode_scalar_program(blob).unwrap(), ScalarProgram::RetVoid);
}

#[test]
fn compiles_load_dup_op_pop_void() {
    let mut code = Vec::new();
    emit_load(&mut code, 0);
    code.push(NyarHeadCode::Dup as u8);
    code.push(NyarHeadCode::I32Mul as u8);
    code.push(NyarHeadCode::Pop as u8);
    code.push(NyarHeadCode::Return as u8);
    let mut jit = BaselineScalarJit;
    let artifact = jit.compile_function(&request(code)).expect("compile");
    let blob = artifact.machine_code.as_ref().expect("machine code");
    assert_eq!(decode_scalar_program(blob).unwrap(), ScalarProgram::RetVoid);
}

#[test]
fn compiles_const_local_binop_and_cmp() {
    let mut code = Vec::new();
    code.push(NyarHeadCode::Constant as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    emit_load(&mut code, 0);
    code.push(NyarHeadCode::I32Add as u8);
    code.push(NyarHeadCode::Return as u8);
    let mut jit = BaselineScalarJit;
    let artifact = jit.compile_function(&request_with_constants(code, vec![Some(10)])).expect("compile");
    let blob = artifact.machine_code.as_ref().expect("machine code");
    assert_eq!(
        decode_scalar_program(blob).unwrap(),
        ScalarProgram::RetI32BinopImmLocal { binop: I32Binop::Add, imm: 10, local: 0, imm_on_left: true }
    );

    let mut code = Vec::new();
    emit_load(&mut code, 1);
    code.push(NyarHeadCode::Constant as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::I32LtS as u8);
    code.push(NyarHeadCode::Return as u8);
    let artifact = jit.compile_function(&request_with_constants(code, vec![Some(5)])).expect("compile");
    let blob = artifact.machine_code.as_ref().expect("machine code");
    assert_eq!(
        decode_scalar_program(blob).unwrap(),
        ScalarProgram::RetI32CmpImmLocal { cmp: I32Cmp::LtS, imm: 5, local: 1, imm_on_left: false }
    );
}

#[test]
fn compiles_const_load_op_pop_void() {
    let mut code = Vec::new();
    code.push(NyarHeadCode::Constant as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    emit_load(&mut code, 0);
    code.push(NyarHeadCode::I32Add as u8);
    code.push(NyarHeadCode::Pop as u8);
    code.push(NyarHeadCode::Return as u8);
    let mut jit = BaselineScalarJit;
    let artifact = jit.compile_function(&request_with_constants(code, vec![Some(3)])).expect("compile");
    let blob = artifact.machine_code.as_ref().expect("machine code");
    assert_eq!(decode_scalar_program(blob).unwrap(), ScalarProgram::RetVoid);
}

#[test]
fn compiles_i32_select_cmp_consts() {
    let mut code = Vec::new();
    emit_load(&mut code, 0);
    emit_load(&mut code, 1);
    code.push(NyarHeadCode::I32Eq as u8);
    let br_pc = code.len();
    code.push(NyarHeadCode::JumpIfFalse as u8);
    let offset_pos = code.len();
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Constant as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Return as u8);
    let else_pc = code.len();
    let rel = (else_pc as i32) - (br_pc as i32);
    code[offset_pos..offset_pos + 4].copy_from_slice(&rel.to_le_bytes());
    code.push(NyarHeadCode::Constant as u8);
    code.extend_from_slice(&1i32.to_le_bytes());
    code.push(NyarHeadCode::Return as u8);

    let mut jit = BaselineScalarJit;
    let artifact = jit.compile_function(&request_with_constants(code, vec![Some(7), Some(9)])).expect("compile");
    let blob = artifact.machine_code.as_ref().expect("machine code");
    assert_eq!(
        decode_scalar_program(blob).unwrap(),
        ScalarProgram::RetI32SelectCmpConsts { cmp: I32Cmp::Eq, a: 0, b: 1, then_imm: 7, else_imm: 9 }
    );
}

#[test]
fn compiles_i32_select_cmp() {
    let mut code = Vec::new();
    emit_load(&mut code, 0);
    emit_load(&mut code, 1);
    code.push(NyarHeadCode::I32Eq as u8);
    let br_pc = code.len();
    code.push(NyarHeadCode::JumpIfFalse as u8);
    // placeholder; fill after then-branch size known
    let offset_pos = code.len();
    code.extend_from_slice(&0i32.to_le_bytes());
    emit_load(&mut code, 2);
    code.push(NyarHeadCode::Return as u8);
    let else_pc = code.len();
    let rel = (else_pc as i32) - (br_pc as i32);
    code[offset_pos..offset_pos + 4].copy_from_slice(&rel.to_le_bytes());
    emit_load(&mut code, 3);
    code.push(NyarHeadCode::Return as u8);

    let mut jit = BaselineScalarJit;
    let artifact = jit.compile_function(&request(code)).expect("compile");
    let blob = artifact.machine_code.as_ref().expect("machine code");
    assert_eq!(
        decode_scalar_program(blob).unwrap(),
        ScalarProgram::RetI32SelectCmpLocals { cmp: I32Cmp::Eq, a: 0, b: 1, then_slot: 2, else_slot: 3 }
    );
}

#[test]
fn compiles_i32_select_cmp_mixed() {
    let mut code = Vec::new();
    emit_load(&mut code, 0);
    emit_load(&mut code, 1);
    code.push(NyarHeadCode::I32Eq as u8);
    let br_pc = code.len();
    code.push(NyarHeadCode::JumpIfFalse as u8);
    let offset_pos = code.len();
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Constant as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Return as u8);
    let else_pc = code.len();
    let rel = (else_pc as i32) - (br_pc as i32);
    code[offset_pos..offset_pos + 4].copy_from_slice(&rel.to_le_bytes());
    emit_load(&mut code, 2);
    code.push(NyarHeadCode::Return as u8);

    let mut jit = BaselineScalarJit;
    let artifact = jit.compile_function(&request_with_constants(code, vec![Some(7)])).expect("compile");
    let blob = artifact.machine_code.as_ref().expect("machine code");
    assert_eq!(
        decode_scalar_program(blob).unwrap(),
        ScalarProgram::RetI32SelectCmpMixed { cmp: I32Cmp::Eq, a: 0, b: 1, then_arm: SelectArm::Imm(7), else_arm: SelectArm::Local(2) }
    );
}

#[test]
fn compiles_void_return() {
    let code = vec![NyarHeadCode::Return as u8];
    let mut jit = BaselineScalarJit;
    let artifact = jit.compile_function(&request(code)).expect("compile");
    let blob = artifact.machine_code.as_ref().expect("machine code");
    assert_eq!(decode_scalar_program(blob).unwrap(), ScalarProgram::RetVoid);
}

#[test]
fn compiles_pop_void_return() {
    let code = vec![NyarHeadCode::Pop as u8, NyarHeadCode::Return as u8];
    let mut jit = BaselineScalarJit;
    let artifact = jit.compile_function(&request(code)).expect("compile");
    let blob = artifact.machine_code.as_ref().expect("machine code");
    assert_eq!(decode_scalar_program(blob).unwrap(), ScalarProgram::RetVoid);
}

#[test]
fn compiles_load_pop_void_return() {
    let mut code = Vec::new();
    emit_load(&mut code, 0);
    code.push(NyarHeadCode::Pop as u8);
    code.push(NyarHeadCode::Return as u8);
    let mut jit = BaselineScalarJit;
    let artifact = jit.compile_function(&request(code)).expect("compile");
    let blob = artifact.machine_code.as_ref().expect("machine code");
    assert_eq!(decode_scalar_program(blob).unwrap(), ScalarProgram::RetVoid);
}

#[test]
fn compiles_binop_pop_void_return() {
    let mut code = Vec::new();
    emit_load(&mut code, 0);
    emit_load(&mut code, 1);
    code.push(NyarHeadCode::I32Add as u8);
    code.push(NyarHeadCode::Pop as u8);
    code.push(NyarHeadCode::Return as u8);
    let mut jit = BaselineScalarJit;
    let artifact = jit.compile_function(&request(code)).expect("compile");
    let blob = artifact.machine_code.as_ref().expect("machine code");
    assert_eq!(decode_scalar_program(blob).unwrap(), ScalarProgram::RetVoid);
}

#[test]
fn compiles_const_pop_void_return() {
    let mut code = Vec::new();
    code.push(NyarHeadCode::Constant as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Pop as u8);
    code.push(NyarHeadCode::Return as u8);
    let mut jit = BaselineScalarJit;
    let artifact = jit.compile_function(&request_with_constants(code, vec![Some(1)])).expect("compile");
    let blob = artifact.machine_code.as_ref().expect("machine code");
    assert_eq!(decode_scalar_program(blob).unwrap(), ScalarProgram::RetVoid);
}

#[test]
fn compiles_i32_neg_via_const0_sub() {
    // 发射器把 `-x` 降为 `Const(0); Load; I32Sub; Return`。
    let mut code = Vec::new();
    code.push(NyarHeadCode::Constant as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    emit_load(&mut code, 0);
    code.push(NyarHeadCode::I32Sub as u8);
    code.push(NyarHeadCode::Return as u8);
    let mut jit = BaselineScalarJit;
    let artifact = jit.compile_function(&request_with_constants(code, vec![Some(0)])).expect("compile");
    let blob = artifact.machine_code.as_ref().expect("machine code");
    assert_eq!(
        decode_scalar_program(blob).unwrap(),
        ScalarProgram::RetI32BinopImmLocal { binop: I32Binop::Sub, imm: 0, local: 0, imm_on_left: true }
    );
}
