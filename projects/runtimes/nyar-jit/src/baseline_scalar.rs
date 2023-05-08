//! 有限标量模式匹配 → NJ1 机器码 blob（无原生可执行页）。

use nyar_bytecode::{NyarHeadCode, decode_at};

use crate::{
    JitCompileRequest, JitCompiledArtifact, JitCompiler, JitError, build_baseline_deopt_map, build_conservative_stack_maps,
    machine_code::{
        I32Binop, I32Cmp, encode_ret_const_i32, encode_ret_i32_binop_locals, encode_ret_i32_cmp_locals,
        encode_ret_i32_select_cmp_locals, encode_ret_local,
    },
};

/// 识别极简外码模式并附带 NJ1 blob 的 JIT 后端。
#[derive(Debug, Default)]
pub struct BaselineScalarJit;

impl JitCompiler for BaselineScalarJit {
    fn enabled(&self) -> bool {
        true
    }

    fn compile_function(&mut self, request: &JitCompileRequest) -> Result<JitCompiledArtifact, JitError> {
        let code = request.function_code()?;
        let machine_code = match_scalar_program(code, &request.constant_i32).ok_or(JitError::Unsupported)?;
        let maps = build_conservative_stack_maps(
            request.function_index,
            request.function.local_count,
            &request.function.safepoint_indices,
        );
        let deopt = build_baseline_deopt_map(
            request.function_index,
            request.function.local_count,
            &request.function.safepoint_indices,
        );
        Ok(JitCompiledArtifact::with_machine_code(maps, deopt, machine_code))
    }
}

/// 尝试从外码切片匹配基线标量程序。
pub fn match_scalar_program(code: &[u8], constant_i32: &[Option<i32>]) -> Option<Vec<u8>> {
    if let Some(blob) = match_select_cmp_return(code) {
        return Some(blob);
    }
    if let Some(blob) = match_load_load_binop_return(code) {
        return Some(blob);
    }
    if let Some(blob) = match_const_return(code, constant_i32) {
        return Some(blob);
    }
    if let Some(blob) = match_load_return(code) {
        return Some(blob);
    }
    None
}

fn match_const_return(code: &[u8], constant_i32: &[Option<i32>]) -> Option<Vec<u8>> {
    let first = decode_at(code, 0);
    if first.size == 0 || first.code != NyarHeadCode::Const {
        return None;
    }
    let second = decode_at(code, first.size as usize);
    if second.size == 0 || second.code != NyarHeadCode::Return {
        return None;
    }
    if first.size as usize + second.size as usize != code.len() {
        return None;
    }
    if first.operand1 < 0 {
        return None;
    }
    let value = constant_i32.get(first.operand1 as usize).copied().flatten()?;
    Some(encode_ret_const_i32(value))
}

fn match_load_return(code: &[u8]) -> Option<Vec<u8>> {
    let first = decode_at(code, 0);
    if first.size == 0 {
        return None;
    }
    let second = decode_at(code, first.size as usize);
    if second.size == 0 {
        return None;
    }
    if first.size as usize + second.size as usize != code.len() {
        return None;
    }
    if second.code != NyarHeadCode::Return {
        return None;
    }
    let slot = match first.code {
        NyarHeadCode::LoadLocal | NyarHeadCode::LoadArg if first.operand1 >= 0 => first.operand1 as u16,
        _ => return None,
    };
    Some(encode_ret_local(slot))
}

fn match_load_load_binop_return(code: &[u8]) -> Option<Vec<u8>> {
    let mut pc = 0usize;
    let a_ins = decode_at(code, pc);
    if a_ins.size == 0 {
        return None;
    }
    pc += a_ins.size as usize;
    let b_ins = decode_at(code, pc);
    if b_ins.size == 0 {
        return None;
    }
    pc += b_ins.size as usize;
    let op = decode_at(code, pc);
    if op.size == 0 {
        return None;
    }
    pc += op.size as usize;
    let ret = decode_at(code, pc);
    if ret.size == 0 || pc + ret.size as usize != code.len() {
        return None;
    }
    if ret.code != NyarHeadCode::Return {
        return None;
    }
    let a = load_slot(&a_ins)?;
    let b = load_slot(&b_ins)?;
    if let Some(binop) = head_to_binop(op.code) {
        return Some(encode_ret_i32_binop_locals(binop, a, b));
    }
    if let Some(cmp) = head_to_cmp(op.code) {
        return Some(encode_ret_i32_cmp_locals(cmp, a, b));
    }
    None
}

/// `Load a; Load b; Cmp; JumpIfFalse else; Load then; Return; Load else; Return`
fn match_select_cmp_return(code: &[u8]) -> Option<Vec<u8>> {
    let mut pc = 0usize;
    let a_ins = decode_at(code, pc);
    if a_ins.size == 0 {
        return None;
    }
    pc += a_ins.size as usize;
    let b_ins = decode_at(code, pc);
    if b_ins.size == 0 {
        return None;
    }
    pc += b_ins.size as usize;
    let cmp_ins = decode_at(code, pc);
    if cmp_ins.size == 0 {
        return None;
    }
    let cmp = head_to_cmp(cmp_ins.code)?;
    pc += cmp_ins.size as usize;
    let br_pc = pc;
    let br = decode_at(code, pc);
    if br.size == 0 || br.code != NyarHeadCode::JumpIfFalse {
        return None;
    }
    pc += br.size as usize;
    let then_ins = decode_at(code, pc);
    if then_ins.size == 0 {
        return None;
    }
    pc += then_ins.size as usize;
    let then_ret = decode_at(code, pc);
    if then_ret.size == 0 || then_ret.code != NyarHeadCode::Return {
        return None;
    }
    pc += then_ret.size as usize;
    let else_pc = br_pc.wrapping_add(br.operand1 as usize);
    if else_pc != pc {
        return None;
    }
    let else_ins = decode_at(code, pc);
    if else_ins.size == 0 {
        return None;
    }
    pc += else_ins.size as usize;
    let else_ret = decode_at(code, pc);
    if else_ret.size == 0 || else_ret.code != NyarHeadCode::Return {
        return None;
    }
    pc += else_ret.size as usize;
    if pc != code.len() {
        return None;
    }
    Some(encode_ret_i32_select_cmp_locals(
        cmp,
        load_slot(&a_ins)?,
        load_slot(&b_ins)?,
        load_slot(&then_ins)?,
        load_slot(&else_ins)?,
    ))
}

fn load_slot(ins: &nyar_bytecode::NyarInstruction) -> Option<u16> {
    match ins.code {
        NyarHeadCode::LoadLocal | NyarHeadCode::LoadArg if ins.operand1 >= 0 => Some(ins.operand1 as u16),
        _ => None,
    }
}

fn head_to_binop(code: NyarHeadCode) -> Option<I32Binop> {
    match code {
        NyarHeadCode::I32Add => Some(I32Binop::Add),
        NyarHeadCode::I32Sub => Some(I32Binop::Sub),
        NyarHeadCode::I32Mul => Some(I32Binop::Mul),
        NyarHeadCode::I32DivS => Some(I32Binop::DivS),
        NyarHeadCode::I32RemS => Some(I32Binop::RemS),
        _ => None,
    }
}

fn head_to_cmp(code: NyarHeadCode) -> Option<I32Cmp> {
    match code {
        NyarHeadCode::I32Eq => Some(I32Cmp::Eq),
        NyarHeadCode::I32Ne => Some(I32Cmp::Ne),
        NyarHeadCode::I32LtS => Some(I32Cmp::LtS),
        NyarHeadCode::I32LeS => Some(I32Cmp::LeS),
        NyarHeadCode::I32GtS => Some(I32Cmp::GtS),
        NyarHeadCode::I32GeS => Some(I32Cmp::GeS),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine_code::{ScalarProgram, decode_scalar_program};
    use crate::request::JitFunctionSpec;

    fn request(code: Vec<u8>) -> JitCompileRequest {
        request_with_constants(code, Vec::new())
    }

    fn request_with_constants(code: Vec<u8>, constant_i32: Vec<Option<i32>>) -> JitCompileRequest {
        JitCompileRequest {
            module_version: 1,
            module_name: "t".into(),
            code_bytes: code.clone(),
            constant_i32,
            function_index: 0,
            function: JitFunctionSpec {
                code_offset: 0,
                code_length: code.len() as i32,
                local_count: 4,
                arity: 4,
                safepoint_indices: Vec::new(),
            },
        }
    }

    #[test]
    fn compiles_const_i32_return() {
        let mut code = Vec::new();
        code.push(NyarHeadCode::Const as u8);
        code.extend_from_slice(&0i32.to_le_bytes());
        code.push(NyarHeadCode::Return as u8);
        let mut jit = BaselineScalarJit;
        let artifact = jit
            .compile_function(&request_with_constants(code, vec![Some(123)]))
            .expect("compile");
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
            assert_eq!(
                decode_scalar_program(blob).unwrap(),
                ScalarProgram::RetI32BinopLocals { binop, a: 0, b: 1 }
            );
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
        assert_eq!(
            decode_scalar_program(blob).unwrap(),
            ScalarProgram::RetI32CmpLocals {
                cmp: I32Cmp::LtS,
                a: 0,
                b: 1
            }
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
            ScalarProgram::RetI32SelectCmpLocals {
                cmp: I32Cmp::Eq,
                a: 0,
                b: 1,
                then_slot: 2,
                else_slot: 3
            }
        );
    }
}
