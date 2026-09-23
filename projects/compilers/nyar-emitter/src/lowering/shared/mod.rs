pub(crate) mod executable;
pub(crate) mod interop;
pub(crate) mod intrinsic_opcode;
pub(crate) mod nullable;
pub(crate) mod suspend_sm;
pub(crate) mod suspend_witness;
pub(crate) mod witness_abi;

pub(crate) use witness_abi::{
    INJECTED_RUNTIME_STUBS, is_injected_runtime_stub_symbol, is_tuple_get_stub_name, witness_slot_jvm_descriptor, witness_slot_msil_signature,
};
