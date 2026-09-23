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

/// Runtime stubs that backends explicitly inject. Call sites may only use these when the
/// callee path is a **bare** symbol exactly equal to the stub name (not `Foo.print`).
///
/// `tuple_get_0` 是 unite sum type payload 提取器：JVM 后端用 int 句柄表示
/// `VonParseResult`/`Option`/`Result` 等 sum type，无法用 `getfield` 访问
/// `payload` 字段，必须改走本 stub 取出堆上的 payload 对象引用。
pub(crate) const INJECTED_RUNTIME_STUBS: &[&str] = &[
    "panic",
    "unimplemented",
    "is_null",
    "unwrap_null",
    "print",
    "format",
    "von_parse_take_fail",
    "von_parse_take_fine",
    "tuple_get_0",
    "tuple_get_1",
];

pub(crate) fn is_injected_runtime_stub_symbol(path_parts: &[&str]) -> bool {
    match path_parts {
        [name] => INJECTED_RUNTIME_STUBS.contains(name) || is_tuple_get_stub_name(name),
        _ => false,
    }
}

/// MIR pattern extractors emit `tuple_get_N` for N-ary payloads; allow any index.
pub(crate) fn is_tuple_get_stub_name(name: &str) -> bool {
    name.strip_prefix("tuple_get_").is_some_and(|suffix| !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit()))
}
