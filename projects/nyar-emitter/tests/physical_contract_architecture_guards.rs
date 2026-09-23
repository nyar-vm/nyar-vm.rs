//! Guards that keep physical planning separate from semantic reconstruction.

use std::{fs, path::PathBuf};

#[test]
fn physical_contract_is_free_of_legacy_semantic_fallbacks() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/lowering/features/physical_contract.rs");
    let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    for pattern in [
        ".symbol.contains(",
        ".symbol.starts_with(",
        ".symbol.ends_with(",
        "nyar_type_from_jvm_descriptor",
        "infer_descriptor",
        "operand_stack",
        "stack_shape",
        "java.lang.String",
        "System.String",
        "WASM_GC_ANYREF",
        "unwrap_or(NyarType::",
        "default_i32",
        "default_void",
        "default_object",
        "default_anyref",
        "\"von\"",
        "\"Option\"",
        "\"Result\"",
    ] {
        assert!(!source.contains(pattern), "physical contract must not contain legacy semantic fallback `{pattern}`");
    }
}
