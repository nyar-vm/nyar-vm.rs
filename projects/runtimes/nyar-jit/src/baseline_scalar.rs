//! 有限标量模式匹配 → NJ1 机器码 blob（无原生可执行页）。

use nyar_bytecode::{NyarHeadCode, decode_at};

use crate::{
    JitCompileRequest, JitCompiledArtifact, JitCompiler, JitError, baseline_scalar_assumptions, build_baseline_deopt_map,
    build_conservative_stack_maps,
    machine_code::{
        I32Binop, I32Cmp, SelectArm, encode_ret_const_i32, encode_ret_i32_binop_imm_local,
        encode_ret_i32_binop_locals, encode_ret_i32_cmp_imm_local, encode_ret_i32_cmp_locals,
        encode_ret_i32_select_cmp_consts, encode_ret_i32_select_cmp_locals, encode_ret_i32_select_cmp_mixed,
        encode_ret_local, encode_ret_void,
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
        Ok(JitCompiledArtifact::with_machine_code(maps, deopt, machine_code)
            .with_assumptions(baseline_scalar_assumptions(request.module_version)))
    }
}

/// 尝试从外码切片匹配基线标量程序。
pub fn match_scalar_program(code: &[u8], constant_i32: &[Option<i32>]) -> Option<Vec<u8>> {
    if let Some(blob) = match_select_cmp_return(code, constant_i32) {
        return Some(blob);
    }
    if let Some(blob) = match_const_const_op_return(code, constant_i32) {
        return Some(blob);
    }
    if let Some(blob) = match_load_dup_op_return(code) {
        return Some(blob);
    }
    if let Some(blob) = match_const_load_op_return(code, constant_i32) {
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
    if let Some(blob) = match_pop_void_return(code) {
        return Some(blob);
    }
    if let Some(blob) = match_void_return(code) {
        return Some(blob);
    }
    None
}

/// 仅 `Return` → void 叶。
fn match_void_return(code: &[u8]) -> Option<Vec<u8>> {
    let ret = decode_at(code, 0);
    if ret.size == 0 || ret.code != NyarHeadCode::Return || ret.size as usize != code.len() {
        return None;
    }
    Some(encode_ret_void())
}

/// `Pop; Return` → void 叶（丢弃栈顶后无返回值）。
fn match_pop_void_return(code: &[u8]) -> Option<Vec<u8>> {
    let mut pc = 0usize;
    let pop = decode_at(code, pc);
    if pop.size == 0 || pop.code != NyarHeadCode::Pop {
        return None;
    }
    pc += pop.size as usize;
    let ret = decode_at(code, pc);
    if ret.size == 0 || ret.code != NyarHeadCode::Return || pc + ret.size as usize != code.len() {
        return None;
    }
    Some(encode_ret_void())
}

/// `Const; Const; Op; Return` → 编译期折叠为 `RetConstI32`。
fn match_const_const_op_return(code: &[u8], constant_i32: &[Option<i32>]) -> Option<Vec<u8>> {
    let mut pc = 0usize;
    let a_ins = decode_at(code, pc);
    if a_ins.size == 0 || a_ins.code != NyarHeadCode::Const || a_ins.operand1 < 0 {
        return None;
    }
    pc += a_ins.size as usize;
    let b_ins = decode_at(code, pc);
    if b_ins.size == 0 || b_ins.code != NyarHeadCode::Const || b_ins.operand1 < 0 {
        return None;
    }
    pc += b_ins.size as usize;
    let op = decode_at(code, pc);
    if op.size == 0 {
        return None;
    }
    pc += op.size as usize;
    let ret = decode_at(code, pc);
    if ret.size == 0 || pc + ret.size as usize != code.len() || ret.code != NyarHeadCode::Return {
        return None;
    }
    let lhs = constant_i32.get(a_ins.operand1 as usize).copied().flatten()?;
    let rhs = constant_i32.get(b_ins.operand1 as usize).copied().flatten()?;
    if let Some(binop) = head_to_binop(op.code) {
        let value = eval_i32_binop(binop, lhs, rhs);
        return Some(encode_ret_const_i32(value));
    }
    if let Some(cmp) = head_to_cmp(op.code) {
        return Some(encode_ret_const_i32(i32::from(cmp.eval(lhs, rhs))));
    }
    None
}

fn eval_i32_binop(binop: I32Binop, lhs: i32, rhs: i32) -> i32 {
    match binop {
        I32Binop::Add => lhs.wrapping_add(rhs),
        I32Binop::Sub => lhs.wrapping_sub(rhs),
        I32Binop::Mul => lhs.wrapping_mul(rhs),
        I32Binop::DivS => {
            if rhs == 0 {
                0
            } else {
                lhs.wrapping_div(rhs)
            }
        }
        I32Binop::RemS => {
            if rhs == 0 {
                0
            } else {
                lhs.wrapping_rem(rhs)
            }
        }
    }
}

/// `Load; Dup; Op; Return` → 同槽二元运算（如 `x*x`）。
fn match_load_dup_op_return(code: &[u8]) -> Option<Vec<u8>> {
    let mut pc = 0usize;
    let load = decode_at(code, pc);
    if load.size == 0 {
        return None;
    }
    let slot = load_slot(&load)?;
    pc += load.size as usize;
    let dup = decode_at(code, pc);
    if dup.size == 0 || dup.code != NyarHeadCode::Dup {
        return None;
    }
    pc += dup.size as usize;
    let op = decode_at(code, pc);
    if op.size == 0 {
        return None;
    }
    pc += op.size as usize;
    let ret = decode_at(code, pc);
    if ret.size == 0 || pc + ret.size as usize != code.len() || ret.code != NyarHeadCode::Return {
        return None;
    }
    if let Some(binop) = head_to_binop(op.code) {
        return Some(encode_ret_i32_binop_locals(binop, slot, slot));
    }
    if let Some(cmp) = head_to_cmp(op.code) {
        return Some(encode_ret_i32_cmp_locals(cmp, slot, slot));
    }
    None
}

/// `Const; Load; Op; Return` 或 `Load; Const; Op; Return`。
fn match_const_load_op_return(code: &[u8], constant_i32: &[Option<i32>]) -> Option<Vec<u8>> {
    let mut pc = 0usize;
    let first = decode_at(code, pc);
    if first.size == 0 {
        return None;
    }
    pc += first.size as usize;
    let second = decode_at(code, pc);
    if second.size == 0 {
        return None;
    }
    pc += second.size as usize;
    let op = decode_at(code, pc);
    if op.size == 0 {
        return None;
    }
    pc += op.size as usize;
    let ret = decode_at(code, pc);
    if ret.size == 0 || pc + ret.size as usize != code.len() || ret.code != NyarHeadCode::Return {
        return None;
    }

    let (imm, local, imm_on_left) = match (first.code, second.code) {
        (NyarHeadCode::Const, _) => {
            if first.operand1 < 0 {
                return None;
            }
            let imm = constant_i32.get(first.operand1 as usize).copied().flatten()?;
            let local = load_slot(&second)?;
            (imm, local, true)
        }
        (_, NyarHeadCode::Const) => {
            if second.operand1 < 0 {
                return None;
            }
            let imm = constant_i32.get(second.operand1 as usize).copied().flatten()?;
            let local = load_slot(&first)?;
            (imm, local, false)
        }
        _ => return None,
    };

    if let Some(binop) = head_to_binop(op.code) {
        return Some(encode_ret_i32_binop_imm_local(binop, imm, local, imm_on_left));
    }
    if let Some(cmp) = head_to_cmp(op.code) {
        return Some(encode_ret_i32_cmp_imm_local(cmp, imm, local, imm_on_left));
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

/// `Load a; Load b; Cmp; JumpIfFalse|JumpIfTrue else; (Load|Const) then; Return; (Load|Const) else; Return`
fn match_select_cmp_return(code: &[u8], constant_i32: &[Option<i32>]) -> Option<Vec<u8>> {
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
    if br.size == 0 || (br.code != NyarHeadCode::JumpIfFalse && br.code != NyarHeadCode::JumpIfTrue) {
        return None;
    }
    let invert = br.code == NyarHeadCode::JumpIfTrue;
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
    let a = load_slot(&a_ins)?;
    let b = load_slot(&b_ins)?;

    if let (Some(then_slot), Some(else_slot)) = (load_slot(&then_ins), load_slot(&else_ins)) {
        let (then_slot, else_slot) = if invert {
            (else_slot, then_slot)
        } else {
            (then_slot, else_slot)
        };
        return Some(encode_ret_i32_select_cmp_locals(cmp, a, b, then_slot, else_slot));
    }

    let then_imm = const_i32(&then_ins, constant_i32);
    let else_imm = const_i32(&else_ins, constant_i32);
    let then_slot = load_slot(&then_ins);
    let else_slot = load_slot(&else_ins);

    match (then_imm, else_imm, then_slot, else_slot) {
        (Some(then_imm), Some(else_imm), _, _) => {
            let (then_imm, else_imm) = if invert {
                (else_imm, then_imm)
            } else {
                (then_imm, else_imm)
            };
            Some(encode_ret_i32_select_cmp_consts(cmp, a, b, then_imm, else_imm))
        }
        (Some(then_imm), None, _, Some(else_slot)) => {
            let (then_arm, else_arm) = if invert {
                (SelectArm::Local(else_slot), SelectArm::Imm(then_imm))
            } else {
                (SelectArm::Imm(then_imm), SelectArm::Local(else_slot))
            };
            Some(encode_ret_i32_select_cmp_mixed(cmp, a, b, then_arm, else_arm))
        }
        (None, Some(else_imm), Some(then_slot), _) => {
            let (then_arm, else_arm) = if invert {
                (SelectArm::Imm(else_imm), SelectArm::Local(then_slot))
            } else {
                (SelectArm::Local(then_slot), SelectArm::Imm(else_imm))
            };
            Some(encode_ret_i32_select_cmp_mixed(cmp, a, b, then_arm, else_arm))
        }
        _ => None,
    }
}

fn const_i32(ins: &nyar_bytecode::NyarInstruction, constant_i32: &[Option<i32>]) -> Option<i32> {
    if ins.code != NyarHeadCode::Const || ins.operand1 < 0 {
        return None;
    }
    constant_i32.get(ins.operand1 as usize).copied().flatten()
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
    fn compiles_load_dup_binop() {
        let mut code = Vec::new();
        emit_load(&mut code, 0);
        code.push(NyarHeadCode::Dup as u8);
        code.push(NyarHeadCode::I32Mul as u8);
        code.push(NyarHeadCode::Return as u8);
        let mut jit = BaselineScalarJit;
        let artifact = jit.compile_function(&request(code)).expect("compile");
        let blob = artifact.machine_code.as_ref().expect("machine code");
        assert_eq!(
            decode_scalar_program(blob).unwrap(),
            ScalarProgram::RetI32BinopLocals {
                binop: I32Binop::Mul,
                a: 0,
                b: 0
            }
        );
    }

    #[test]
    fn folds_const_const_binop_to_ret_const() {
        let mut code = Vec::new();
        code.push(NyarHeadCode::Const as u8);
        code.extend_from_slice(&0i32.to_le_bytes());
        code.push(NyarHeadCode::Const as u8);
        code.extend_from_slice(&1i32.to_le_bytes());
        code.push(NyarHeadCode::I32Mul as u8);
        code.push(NyarHeadCode::Return as u8);
        let mut jit = BaselineScalarJit;
        let artifact = jit
            .compile_function(&request_with_constants(code, vec![Some(6), Some(7)]))
            .expect("compile");
        let blob = artifact.machine_code.as_ref().expect("machine code");
        assert_eq!(decode_scalar_program(blob).unwrap(), ScalarProgram::RetConstI32 { value: 42 });
    }

    #[test]
    fn compiles_const_local_binop_and_cmp() {
        let mut code = Vec::new();
        code.push(NyarHeadCode::Const as u8);
        code.extend_from_slice(&0i32.to_le_bytes());
        emit_load(&mut code, 0);
        code.push(NyarHeadCode::I32Add as u8);
        code.push(NyarHeadCode::Return as u8);
        let mut jit = BaselineScalarJit;
        let artifact = jit
            .compile_function(&request_with_constants(code, vec![Some(10)]))
            .expect("compile");
        let blob = artifact.machine_code.as_ref().expect("machine code");
        assert_eq!(
            decode_scalar_program(blob).unwrap(),
            ScalarProgram::RetI32BinopImmLocal {
                binop: I32Binop::Add,
                imm: 10,
                local: 0,
                imm_on_left: true
            }
        );

        let mut code = Vec::new();
        emit_load(&mut code, 1);
        code.push(NyarHeadCode::Const as u8);
        code.extend_from_slice(&0i32.to_le_bytes());
        code.push(NyarHeadCode::I32LtS as u8);
        code.push(NyarHeadCode::Return as u8);
        let artifact = jit
            .compile_function(&request_with_constants(code, vec![Some(5)]))
            .expect("compile");
        let blob = artifact.machine_code.as_ref().expect("machine code");
        assert_eq!(
            decode_scalar_program(blob).unwrap(),
            ScalarProgram::RetI32CmpImmLocal {
                cmp: I32Cmp::LtS,
                imm: 5,
                local: 1,
                imm_on_left: false
            }
        );
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
        code.push(NyarHeadCode::Const as u8);
        code.extend_from_slice(&0i32.to_le_bytes());
        code.push(NyarHeadCode::Return as u8);
        let else_pc = code.len();
        let rel = (else_pc as i32) - (br_pc as i32);
        code[offset_pos..offset_pos + 4].copy_from_slice(&rel.to_le_bytes());
        code.push(NyarHeadCode::Const as u8);
        code.extend_from_slice(&1i32.to_le_bytes());
        code.push(NyarHeadCode::Return as u8);

        let mut jit = BaselineScalarJit;
        let artifact = jit
            .compile_function(&request_with_constants(code, vec![Some(7), Some(9)]))
            .expect("compile");
        let blob = artifact.machine_code.as_ref().expect("machine code");
        assert_eq!(
            decode_scalar_program(blob).unwrap(),
            ScalarProgram::RetI32SelectCmpConsts {
                cmp: I32Cmp::Eq,
                a: 0,
                b: 1,
                then_imm: 7,
                else_imm: 9
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
        code.push(NyarHeadCode::Const as u8);
        code.extend_from_slice(&0i32.to_le_bytes());
        code.push(NyarHeadCode::Return as u8);
        let else_pc = code.len();
        let rel = (else_pc as i32) - (br_pc as i32);
        code[offset_pos..offset_pos + 4].copy_from_slice(&rel.to_le_bytes());
        emit_load(&mut code, 2);
        code.push(NyarHeadCode::Return as u8);

        let mut jit = BaselineScalarJit;
        let artifact = jit
            .compile_function(&request_with_constants(code, vec![Some(7)]))
            .expect("compile");
        let blob = artifact.machine_code.as_ref().expect("machine code");
        assert_eq!(
            decode_scalar_program(blob).unwrap(),
            ScalarProgram::RetI32SelectCmpMixed {
                cmp: I32Cmp::Eq,
                a: 0,
                b: 1,
                then_arm: SelectArm::Imm(7),
                else_arm: SelectArm::Local(2),
            }
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
    fn compiles_i32_neg_via_const0_sub() {
        // 发射器把 `-x` 降为 `Const(0); Load; I32Sub; Return`。
        let mut code = Vec::new();
        code.push(NyarHeadCode::Const as u8);
        code.extend_from_slice(&0i32.to_le_bytes());
        emit_load(&mut code, 0);
        code.push(NyarHeadCode::I32Sub as u8);
        code.push(NyarHeadCode::Return as u8);
        let mut jit = BaselineScalarJit;
        let artifact = jit
            .compile_function(&request_with_constants(code, vec![Some(0)]))
            .expect("compile");
        let blob = artifact.machine_code.as_ref().expect("machine code");
        assert_eq!(
            decode_scalar_program(blob).unwrap(),
            ScalarProgram::RetI32BinopImmLocal {
                binop: I32Binop::Sub,
                imm: 0,
                local: 0,
                imm_on_left: true,
            }
        );
    }
}
