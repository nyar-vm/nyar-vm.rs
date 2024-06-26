use nyar_emitter::nyar_backend_wasi::WasmPackageKind;
use nyar_language::{
    CompilerSourceGroup, CompilerSourceInput, GeneratedSourceGroup, ValkyrieCompiler,
    nyar::{CanonicalTarget, ClrSuspendStrategy},
    valkyrie::compile_pipeline::{CompilerBuildContext, compile_source_groups_to_artifacts},
    *,
};

fn evidence_context() -> CompilerBuildContext {
    CompilerBuildContext::new("wasm32", CanonicalTarget::parse("node").expect("node"), ClrSuspendStrategy::default(), WasmPackageKind::Binary)
}

fn temp_output(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp output");
    dir
}

#[test]
fn compile_evidence_is_stable_for_the_same_source_closure() {
    // 本夹具只验证证据哈希稳定性：使用无声明身份的内建 `bool`。数值原始类型名
    // （`i32` 等）必须由 `core/primitive` 的 `structure` 给出声明身份，不在该单组闭包内。
    let groups = [CompilerSourceGroup::single_source("app", "app", "[main] micro main() -> bool { return true }").as_entry()];
    let context = evidence_context();
    let dir = temp_output("nyar_compile_evidence_test");
    let input = CompilerSourceInput::application(groups.to_vec());
    let first =
        compile_source_groups_to_artifacts(&ValkyrieCompiler::default(), &input, &context, &dir, "app", false, false).expect("first compile");
    let second =
        compile_source_groups_to_artifacts(&ValkyrieCompiler::default(), &input, &context, &dir, "app", false, false).expect("second compile");
    assert_eq!(first.evidence, second.evidence);
    assert!(!first.evidence.semantic_closure_hash.is_empty());
    assert!(!first.evidence.canonical_surface_hash.is_empty());
    assert_eq!(first.evidence.semantic_closure_hash, semantic_closure_hash_for_groups(&groups, "wasm32"));
    assert_eq!(first.evidence.host_provider_binding_hash, host_provider_binding_hash_for_bindings(&[]));
    assert!(first.evidence.generated_source_hash.is_empty(), "无生成输入时不得伪造生成来源摘要");
}

#[test]
fn generated_source_input_is_composed_and_bound_into_evidence() {
    let groups = vec![CompilerSourceGroup::single_source("app", "app", "[main] micro main() -> bool { return true }").as_entry()];
    let generated = vec![
        GeneratedSourceGroup::new(
            "asgard.awsl.wasm",
            CompilerSourceGroup::single_source("asgard.generated", "asgard.generated", "[export] micro hydrate() -> bool { return false }")
                .as_entry()
                .with_direct_dependencies(vec!["app".into()]),
        )
        .expect("生成组必须由 producer 与显式入口建立"),
    ];
    let input = CompilerSourceInput::with_generated(groups, generated).expect("生成组追加在应用组之后");

    let composed = input.compose().expect("合成唯一源码闭包");
    assert_eq!(composed.len(), 2);
    assert!(composed.last().is_some_and(|group| group.dependency_key == "asgard.generated"));

    let dir = temp_output("nyar_generated_source_evidence_test");
    let report = compile_source_groups_to_artifacts(&ValkyrieCompiler::default(), &input, &evidence_context(), &dir, "app", false, false)
        .expect("应用入口与生成导出共同编译");
    assert_eq!(report.evidence.generated_source_hash, generated_source_hash(&input.generated));
    assert!(!report.evidence.generated_source_hash.is_empty(), "生成源码必须进入编译证据");
    assert_eq!(report.evidence.semantic_closure_hash, semantic_closure_hash_for_groups(&composed, "wasm32"));
}

#[test]
fn generated_source_group_must_be_an_entry_group() {
    let group = CompilerSourceGroup::single_source("asgard.generated", "asgard.generated", "[export] micro hydrate() -> bool { return false }");
    let error = GeneratedSourceGroup::new("asgard.awsl.wasm", group).expect_err("生成组不能依赖顺序推断入口");
    assert!(error.to_string().contains("必须显式标记入口"), "{error}");
}

#[test]
fn generated_source_group_must_depend_on_already_provided_groups() {
    let groups = vec![CompilerSourceGroup::single_source("app", "app", "[main] micro main() -> bool { return true }").as_entry()];
    let generated = GeneratedSourceGroup::new(
        "asgard.awsl.wasm",
        CompilerSourceGroup::single_source("asgard.generated", "asgard.generated", "[export] micro hydrate() -> bool { return false }")
            .as_entry()
            .with_direct_dependencies(vec!["missing.package".into()]),
    )
    .expect("producer 与入口身份成立");
    let error = CompilerSourceInput::with_generated(groups, vec![generated]).expect_err("未提供的依赖必须 fail-closed");
    assert!(error.to_string().contains("未在此之前提供"), "{error}");
}
