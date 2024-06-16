use nyar_emitter::nyar_backend_wasi::WasmPackageKind;
use nyar_language::{
    CompilerSourceGroup, ValkyrieCompiler,
    nyar::{CanonicalTarget, ClrSuspendStrategy},
    valkyrie::compile_pipeline::{CompilerBuildContext, compile_source_groups_to_artifacts},
    *,
};

#[test]
fn compile_evidence_is_stable_for_the_same_source_closure() {
    // 本夹具只验证证据哈希稳定性：使用无声明身份的内建 `bool`。数值原始类型名
    // （`i32` 等）必须由 `core/primitive` 的 `structure` 给出声明身份，不在该单组闭包内。
    let groups = [CompilerSourceGroup::single_source("app", "app", "[main] micro main() -> bool { return true }")];
    let context = CompilerBuildContext::new(
        "wasm32",
        CanonicalTarget::parse("node").expect("node"),
        ClrSuspendStrategy::default(),
        WasmPackageKind::Binary,
    );
    let dir = std::env::temp_dir().join("nyar_compile_evidence_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp output");
    let first =
        compile_source_groups_to_artifacts(&ValkyrieCompiler::default(), &groups, &context, &dir, "app", false, false).expect("first compile");
    let second =
        compile_source_groups_to_artifacts(&ValkyrieCompiler::default(), &groups, &context, &dir, "app", false, false).expect("second compile");
    assert_eq!(first.evidence, second.evidence);
    assert!(!first.evidence.semantic_closure_hash.is_empty());
    assert!(!first.evidence.canonical_surface_hash.is_empty());
    assert_eq!(first.evidence.semantic_closure_hash, semantic_closure_hash_for_groups(&groups));
    assert_eq!(first.evidence.host_provider_binding_hash, host_provider_binding_hash_for_bindings(&[]));
}
