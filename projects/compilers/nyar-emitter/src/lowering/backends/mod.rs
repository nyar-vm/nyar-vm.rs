pub(crate) use crate::lowering::{
    features::{pattern_matching_contract, singleton},
    sanitize_jvm_method_symbol, sanitize_operation_symbol, sanitize_symbol,
    shared::{executable, interop, intrinsic_opcode, nullable, suspend_sm, suspend_witness, witness_abi},
};

#[cfg(any(feature = "nyar-vm-lane", feature = "legacy-lanes-clr-jvm-native"))]
#[path = "clr/types.rs"]
pub(crate) mod clr_types;
#[cfg(feature = "legacy-lanes-clr-jvm-native")]
pub(crate) mod clr;
#[cfg(feature = "legacy-lanes-clr-jvm-native")]
#[path = "clr/mir.rs"]
pub(crate) mod clr_mir;
#[cfg(feature = "legacy-lanes-clr-jvm-native")]
#[path = "clr/nominal.rs"]
pub(crate) mod clr_nominal;
#[cfg(feature = "legacy-lanes-clr-jvm-native")]
#[path = "clr/suspend.rs"]
pub(crate) mod clr_suspend;
#[cfg(feature = "legacy-lanes-clr-jvm-native")]
#[path = "clr/witness.rs"]
pub(crate) mod clr_witness;

#[cfg(feature = "legacy-lanes-clr-jvm-native")]
pub(crate) mod jvm;
#[cfg(feature = "legacy-lanes-clr-jvm-native")]
#[path = "jvm/mir.rs"]
pub(crate) mod jvm_mir;
#[cfg(feature = "legacy-lanes-clr-jvm-native")]
#[path = "jvm/suspend.rs"]
pub(crate) mod jvm_suspend;
#[cfg(feature = "legacy-lanes-clr-jvm-native")]
#[path = "jvm/witness.rs"]
pub(crate) mod jvm_witness;

#[cfg(feature = "legacy-lanes-clr-jvm-native")]
pub(crate) mod native;
#[cfg(feature = "legacy-lanes-clr-jvm-native")]
#[path = "native/witness.rs"]
pub(crate) mod witness;

#[cfg(feature = "nyar-vm-lane")]
pub(crate) mod nyar_vm;
#[cfg(feature = "nyar-vm-lane")]
#[path = "nyar_vm/mir.rs"]
pub(crate) mod nyar_vm_mir;

pub(crate) mod wasm;
