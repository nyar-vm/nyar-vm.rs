//! Operand stack-width checks against MIR `value_types` (ADR 0008).
//!
//! Physical locals may validate an already-planned Representation; they must
//! not invent language types when MIR metadata is missing.

#![allow(deprecated)]
use super::super::*;

#[allow(deprecated)]
impl<'a> WasmMirLowerer<'a> {
    /// Require MIR `value_types` for an SSA value that needs a storage class.
    pub(crate) fn require_value_type(&self, value: MirValueRef) -> &NyarType {
        self.mir_fn.value_types.get(&value).unwrap_or_else(|| {
            panic!(
                "WASM emit fail-closed: missing value_types for %{} in `{}` (ADR 0008)",
                value.0, self.mir_fn.symbol
            )
        })
    }

    /// Storage for instruction output: MIR type required, no Reference default.
    pub(crate) fn output_storage_from_contract(&self, instruction: &MirInstruction) -> MirStorageKind {
        else {
            return StorageKind::Value;
        };
        let ty = self.require_value_type(value);
        if type_is_wasm_gc_heap_reference(ty) {
            StorageKind::Reference
        }
        else {
            self.storage_for_type(ty)
        }
    }
}
