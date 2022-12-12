#![allow(deprecated)]

#[allow(deprecated)]
use super::*;

#[allow(deprecated)]
impl<'a> WasmMirLowerer<'a> {
    pub(crate) fn emit_intrinsic_opcode(&mut self, _opcode: IntrinsicOpcode, _arguments: &[MirOperand], _output: Option<MirValueRef>) {
        panic!("DELETED ADR 0010: intrinsic opcode authority removed");
    }

    pub(crate) fn emit_intrinsic_binary(&mut self, _op: IntrinsicBinaryOp, _arguments: &[MirOperand], _output: Option<MirValueRef>) {
        panic!("DELETED ADR 0010: intrinsic opcode authority removed");
    }
}
