//! Call emission that consumes only resolved call contracts (ADR 0008).

#![allow(deprecated)]
use super::super::*;

#[allow(deprecated)]
impl<'a> WasmMirLowerer<'a> {
    /// Missing call arguments are forbidden — arity must match the contract.
    pub(crate) fn emit_missing_call_argument_fail_closed(&mut self, expected: u8) {
        panic!(
            "WASM emit fail-closed: missing call argument (expected valtype {expected:#x}) in `{}`; refuse invent 0/ref.null (ADR 0008)",
            self.mir_fn.symbol
        );
    }
}
