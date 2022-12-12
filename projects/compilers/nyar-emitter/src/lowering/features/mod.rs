#[cfg(feature = "legacy-lanes")]
pub(crate) use crate::lowering::backends::{clr_mir, clr_types, jvm_mir};

pub(crate) mod pattern_matching_contract;
pub(crate) mod physical_contract;
pub(crate) mod semantic_mir_contract;
pub(crate) mod singleton;
