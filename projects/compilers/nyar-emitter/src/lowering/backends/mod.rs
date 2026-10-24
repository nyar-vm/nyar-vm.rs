pub(crate) use crate::lowering::{
    features::{pattern_matching_contract, singleton},
    sanitize_symbol,
    shared::{executable, interop},
};

#[cfg(feature = "ovm-lane")]
#[doc(hidden)]
pub mod ovm;
#[cfg(feature = "ovm-lane")]
#[path = "ovm/mir.rs"]
pub(crate) mod ovm_mir;

#[doc(hidden)]
pub mod wasm;
