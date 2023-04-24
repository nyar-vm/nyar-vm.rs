//! 有限标量模式匹配 → NJ1 机器码 blob（无原生可执行页）。

use nyar_bytecode::{NyarHeadCode, decode_at};

use crate::{
    JitCompileRequest, JitCompiledArtifact, JitCompiler, JitError, build_baseline_deopt_map, build_conservative_stack_maps,
    machine_code::{encode_ret_i32_add_locals, encode_ret_local},
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
        let machine_code = match_scalar_program(code).ok_or(JitError::Unsupported)?;
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
pub fn match_scalar_program(code: &[u8]) -> Option<Vec<u8>> {
    // LoadLocal a ; LoadLocal b ; I32Add ; Return
    if let Some(blob) = match_load_load_add_return(code) {
        return Some(blob);
    }
    // LoadLocal/LoadArg slot ; Return
    if let Some(blob) = match_load_return(code) {
        return Some(blob);
    }
    None
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

fn match_load_load_add_return(code: &[u8]) -> Option<Vec<u8>> {
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
    let add = decode_at(code, pc);
    if add.size == 0 {
        return None;
    }
    pc += add.size as usize;
    let ret = decode_at(code, pc);
    if ret.size == 0 || pc + ret.size as usize != code.len() {
        return None;
    }
    if add.code != NyarHeadCode::I32Add || ret.code != NyarHeadCode::Return {
        return None;
    }
    let a = match a_ins.code {
        NyarHeadCode::LoadLocal | NyarHeadCode::LoadArg if a_ins.operand1 >= 0 => a_ins.operand1 as u16,
        _ => return None,
    };
    let b = match b_ins.code {
        NyarHeadCode::LoadLocal | NyarHeadCode::LoadArg if b_ins.operand1 >= 0 => b_ins.operand1 as u16,
        _ => return None,
    };
    Some(encode_ret_i32_add_locals(a, b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine_code::{ScalarProgram, decode_scalar_program};
    use crate::request::JitFunctionSpec;

    fn request(code: Vec<u8>) -> JitCompileRequest {
        JitCompileRequest {
            module_version: 1,
            module_name: "t".into(),
            code_bytes: code.clone(),
            function_index: 0,
            function: JitFunctionSpec {
                code_offset: 0,
                code_length: code.len() as i32,
                local_count: 2,
                arity: 2,
                safepoint_indices: Vec::new(),
            },
        }
    }

    #[test]
    fn compiles_load_local_return() {
        // LoadLocal 1 (0x20 + i32 LE) ; Return (0x05)
        let mut code = vec![NyarHeadCode::LoadLocal as u8];
        code.extend_from_slice(&1i32.to_le_bytes());
        code.push(NyarHeadCode::Return as u8);
        let mut jit = BaselineScalarJit;
        let artifact = jit.compile_function(&request(code)).expect("compile");
        let blob = artifact.machine_code.as_ref().expect("machine code");
        assert_eq!(decode_scalar_program(blob).unwrap(), ScalarProgram::RetLocal { slot: 1 });
    }

    #[test]
    fn compiles_i32_add_locals() {
        let mut code = Vec::new();
        code.push(NyarHeadCode::LoadLocal as u8);
        code.extend_from_slice(&0i32.to_le_bytes());
        code.push(NyarHeadCode::LoadLocal as u8);
        code.extend_from_slice(&1i32.to_le_bytes());
        code.push(NyarHeadCode::I32Add as u8);
        code.push(NyarHeadCode::Return as u8);
        let mut jit = BaselineScalarJit;
        let artifact = jit.compile_function(&request(code)).expect("compile");
        let blob = artifact.machine_code.as_ref().expect("machine code");
        assert_eq!(decode_scalar_program(blob).unwrap(), ScalarProgram::RetI32AddLocals { a: 0, b: 1 });
    }

    #[test]
    fn rejects_unknown_shape() {
        let code = vec![NyarHeadCode::Nop as u8, NyarHeadCode::Return as u8];
        let mut jit = BaselineScalarJit;
        assert!(matches!(jit.compile_function(&request(code)), Err(JitError::Unsupported)));
    }
}
