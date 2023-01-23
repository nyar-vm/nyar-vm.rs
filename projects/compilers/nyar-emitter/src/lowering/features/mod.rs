#[cfg(feature = "legacy-lanes-clr-jvm-native")]
pub(crate) use crate::lowering::backends::{clr_mir, jvm_mir};
#[cfg(any(feature = "legacy-lanes", feature = "legacy-lanes-clr-jvm-native"))]
pub(crate) use crate::lowering::backends::clr_types;

pub(crate) mod pattern_matching_contract;
pub(crate) mod physical_contract;
pub(crate) mod semantic_mir_contract;
pub(crate) mod singleton;
