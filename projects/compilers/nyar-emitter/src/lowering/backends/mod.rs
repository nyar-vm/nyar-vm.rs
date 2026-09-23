pub(crate) use crate::lowering::{
    features::{pattern_matching_contract, singleton},
    sanitize_jvm_method_symbol, sanitize_operation_symbol, sanitize_symbol,
    shared::{executable, interop, intrinsic_opcode, nullable, suspend_sm, suspend_witness, witness_abi},
};

#[cfg(feature = "legacy-lanes")]
pub(crate) mod clr;
#[cfg(feature = "legacy-lanes")]
#[path = "clr/mir.rs"]
pub(crate) mod clr_mir;
#[cfg(feature = "legacy-lanes")]
#[path = "clr/nominal.rs"]
pub(crate) mod clr_nominal;
#[cfg(feature = "legacy-lanes")]
#[path = "clr/suspend.rs"]
pub(crate) mod clr_suspend;
#[cfg(feature = "legacy-lanes")]
#[path = "clr/types.rs"]
pub(crate) mod clr_types;
#[cfg(feature = "legacy-lanes")]
#[path = "clr/witness.rs"]
pub(crate) mod clr_witness;

#[cfg(feature = "legacy-lanes")]
pub(crate) mod jvm;
#[cfg(feature = "legacy-lanes")]
#[path = "jvm/mir.rs"]
pub(crate) mod jvm_mir;
#[cfg(feature = "legacy-lanes")]
#[path = "jvm/suspend.rs"]
pub(crate) mod jvm_suspend;
#[cfg(feature = "legacy-lanes")]
#[path = "jvm/witness.rs"]
pub(crate) mod jvm_witness;

#[cfg(feature = "legacy-lanes")]
pub(crate) mod native;
#[cfg(feature = "legacy-lanes")]
#[path = "native/witness.rs"]
pub(crate) mod witness;

#[cfg(feature = "legacy-lanes")]
pub(crate) mod nyar_vm;
#[cfg(feature = "legacy-lanes")]
#[path = "nyar_vm/mir.rs"]
pub(crate) mod nyar_vm_mir;

pub(crate) mod wasm;
