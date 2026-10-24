use nyar_jit::{
    I32Binop, I32Cmp, SelectArm, encode_ret_const_i32, encode_ret_i32_binop_imm_local, encode_ret_i32_binop_locals,
    encode_ret_i32_cmp_imm_local, encode_ret_i32_cmp_locals, encode_ret_i32_select_cmp_consts, encode_ret_i32_select_cmp_locals,
    encode_ret_i32_select_cmp_mixed, encode_ret_local, encode_ret_void,
};
use odin_vm::{nj1_runtime::execute_nj1_blob, value::Value};

#[test]
fn executes_ret_local() {
    let blob = encode_ret_local(1);
    let locals = vec![Value::I32(1), Value::I32(99)];
    assert_eq!(execute_nj1_blob(&blob, &locals).unwrap(), Value::I32(99));
}

#[test]
fn executes_ret_i32_binops() {
    let locals = vec![Value::I32(40), Value::I32(2)];
    assert_eq!(execute_nj1_blob(&encode_ret_i32_binop_locals(I32Binop::Add, 0, 1), &locals).unwrap(), Value::I32(42));
    assert_eq!(execute_nj1_blob(&encode_ret_i32_binop_locals(I32Binop::DivS, 0, 1), &locals).unwrap(), Value::I32(20));
    assert_eq!(execute_nj1_blob(&encode_ret_i32_binop_locals(I32Binop::RemS, 0, 1), &locals).unwrap(), Value::I32(0));
    assert_eq!(execute_nj1_blob(&encode_ret_i32_binop_locals(I32Binop::DivS, 0, 1), &[Value::I32(7), Value::I32(0)]).unwrap(), Value::I32(0));
}

#[test]
fn executes_ret_const_i32() {
    assert_eq!(execute_nj1_blob(&encode_ret_const_i32(99), &[]).unwrap(), Value::I32(99));
}

#[test]
fn executes_ret_i32_imm_local() {
    let locals = vec![Value::I32(7)];
    assert_eq!(execute_nj1_blob(&encode_ret_i32_binop_imm_local(I32Binop::Add, 10, 0, true), &locals).unwrap(), Value::I32(17));
    assert_eq!(execute_nj1_blob(&encode_ret_i32_binop_imm_local(I32Binop::Sub, 3, 0, false), &locals).unwrap(), Value::I32(4));
    assert_eq!(execute_nj1_blob(&encode_ret_i32_cmp_imm_local(I32Cmp::LtS, 10, 0, false), &locals).unwrap(), Value::I32(1));
    assert_eq!(execute_nj1_blob(&encode_ret_i32_cmp_imm_local(I32Cmp::Eq, 7, 0, true), &locals).unwrap(), Value::I32(1));
}

#[test]
fn executes_ret_i32_select_cmp_consts() {
    let locals = vec![Value::I32(1), Value::I32(1)];
    assert_eq!(execute_nj1_blob(&encode_ret_i32_select_cmp_consts(I32Cmp::Eq, 0, 1, 7, 9), &locals).unwrap(), Value::I32(7));
    let locals = vec![Value::I32(1), Value::I32(0)];
    assert_eq!(execute_nj1_blob(&encode_ret_i32_select_cmp_consts(I32Cmp::Eq, 0, 1, 7, 9), &locals).unwrap(), Value::I32(9));
}

#[test]
fn executes_ret_i32_cmp_and_select() {
    let locals = vec![Value::I32(1), Value::I32(2), Value::I32(10), Value::I32(20)];
    assert_eq!(execute_nj1_blob(&encode_ret_i32_cmp_locals(I32Cmp::LtS, 0, 1), &locals).unwrap(), Value::I32(1));
    assert_eq!(execute_nj1_blob(&encode_ret_i32_cmp_locals(I32Cmp::Eq, 0, 1), &locals).unwrap(), Value::I32(0));
    assert_eq!(execute_nj1_blob(&encode_ret_i32_select_cmp_locals(I32Cmp::LtS, 0, 1, 2, 3), &locals).unwrap(), Value::I32(10));
    assert_eq!(execute_nj1_blob(&encode_ret_i32_select_cmp_locals(I32Cmp::GtS, 0, 1, 2, 3), &locals).unwrap(), Value::I32(20));
}

#[test]
fn executes_ret_i32_select_cmp_mixed_and_void() {
    let locals = vec![Value::I32(1), Value::I32(1), Value::I32(42)];
    assert_eq!(
        execute_nj1_blob(&encode_ret_i32_select_cmp_mixed(I32Cmp::Eq, 0, 1, SelectArm::Imm(7), SelectArm::Local(2)), &locals).unwrap(),
        Value::I32(7)
    );
    let locals = vec![Value::I32(1), Value::I32(0), Value::I32(42)];
    assert_eq!(
        execute_nj1_blob(&encode_ret_i32_select_cmp_mixed(I32Cmp::Eq, 0, 1, SelectArm::Imm(7), SelectArm::Local(2)), &locals).unwrap(),
        Value::I32(42)
    );
    assert_eq!(execute_nj1_blob(&encode_ret_void(), &[]).unwrap(), Value::Null);
}
