use odin_bytecode::{NyarFunction, NyarHeadCode, emit_imm1};
use odin_vm::executable::build_executable_function;

#[test]
fn resolves_jump_to_instruction_index() {
    let mut code = Vec::new();
    // 0: Jump -> Return (byte pc of Return)
    let jump_pc = 0usize;
    emit_imm1(&mut code, NyarHeadCode::Jump, 0);
    // unreachable Const
    emit_imm1(&mut code, NyarHeadCode::Constant, 0);
    let return_pc = code.len();
    code.push(NyarHeadCode::Return as u8);
    let offset = (return_pc as i32) - (jump_pc as i32);
    code[1..5].copy_from_slice(&offset.to_le_bytes());

    let function = NyarFunction { name: "main".into(), arity: 0, local_count: 0, code_offset: 0, code_length: code.len() as i32 };
    let exec = build_executable_function(&code, &function).expect("build");
    assert_eq!(exec.ops.len(), 3);
    assert_eq!(exec.ops[0].code, NyarHeadCode::Jump);
    assert_eq!(exec.ops[0].target, 2); // Return is instruction index 2
    assert_eq!(exec.ops[2].code, NyarHeadCode::Return);
    assert!(exec.safepoints.contains(&2));
}
