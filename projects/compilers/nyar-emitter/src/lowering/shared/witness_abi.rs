//! Shared witness-slot ABI helpers: descriptors/signatures come from table + slot metadata,
//! not from bare Call method-name sniffing.

use nyar::{WitnessMethodSlotSubmission, WitnessSubmission};

use crate::{
    nyar_backend_clr::MsilType,
    nyar_backend_jvm::{JvmMethodDescriptor, JvmTypeDescriptor},
};

const JVM_OBJECT: &str = "java/lang/Object";

/// JVM descriptor for a witness impl method, matching what `jvm/witness` emits.
pub(crate) fn witness_slot_jvm_descriptor(table: &WitnessSubmission, method: &WitnessMethodSlotSubmission) -> JvmMethodDescriptor {
    let object = JvmTypeDescriptor::Object(JVM_OBJECT.to_string());
    if table.trait_name == "Future" && method.method_name == "poll" {
        return JvmMethodDescriptor::new(vec![object], JvmTypeDescriptor::Boolean);
    }
    if table.trait_name == "Iterator" && matches!(method.method_name.as_str(), "next" | "has_next" | "into_iterator") {
        let ret = if method.method_name == "has_next" { JvmTypeDescriptor::Boolean } else { object.clone() };
        return JvmMethodDescriptor::new(vec![object], ret);
    }
    // Default witness impl: (Object)Object — same as `lower_witness_impl_method` fallback.
    JvmMethodDescriptor::new(vec![object.clone()], object)
}

/// MSIL signature for a witness impl method, matching what `clr/witness` emits.
pub(crate) fn witness_slot_msil_signature(table: &WitnessSubmission, method: &WitnessMethodSlotSubmission) -> (MsilType, Vec<MsilType>) {
    if table.trait_name == "Future" && method.method_name == "poll" {
        return (MsilType::Bool, vec![MsilType::Object]);
    }
    if table.trait_name == "Iterator" && method.method_name == "next" {
        return (MsilType::Object, vec![MsilType::Object]);
    }
    if method.method_index == 0 && !table.result_literal.is_empty() {
        return (MsilType::String, vec![MsilType::Object]);
    }
    (MsilType::Object, vec![MsilType::Object])
}
