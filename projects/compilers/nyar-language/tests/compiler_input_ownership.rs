//! 验证输入事实、语义选择和证据的所有权；不依赖题目或标准库名称。

use nyar_language::{CompilerSourceFile, CompilerSourceGroup, ValkyrieCompiler, semantic_closure_hash_for_groups};

fn group(key: &str, source: &str, dependencies: &[&str]) -> CompilerSourceGroup {
    CompilerSourceGroup::from_source_files(
        key,
        key,
        vec![CompilerSourceFile { path: "source.v".into(), source: source.into() }],
        dependencies.iter().map(|key| (*key).into()).collect(),
    )
}

#[test]
fn unused_inventory_never_materializes_and_names_do_not_affect_selection() {
    for key in ["unused_package", "other_inventory", "third_dependency"] {
        let groups = [group(key, "micro broken() { value = }", &[]), group("app", "[export] micro run() -> i32 { return 23 }", &[key])];
        let modules = ValkyrieCompiler::default().resolve_source_groups(&groups, "wasm32").expect("未被需求的组是合法空模块");
        assert!(modules[0].functions.is_empty());
        assert_eq!(modules[1].functions.len(), 1);
    }
}

#[test]
fn compiler_discovers_reachable_declarations_without_caller_plan() {
    let mut library = group("library", "micro answer() -> i32 { return 23 }", &[]);
    library.sources.push(CompilerSourceFile { path: "broken.v".into(), source: "micro broken() { value = }".into() });
    let groups = [library, group("app", "[export] micro run() -> i32 { return answer() }", &["library"])];
    let modules = ValkyrieCompiler::default().resolve_source_groups(&groups, "wasm32").expect("Compiler 选择并绑定可达调用");
    assert_eq!(modules[0].functions.len(), 1);
    assert!(modules[0].functions[0].declaration.is_some());
    assert_eq!(modules[1].imported_semantic_exports[0].functions[0].declaration, modules[0].functions[0].declaration);
}

#[test]
fn malformed_identity_and_dependency_order_fail_before_parsing() {
    let bad = group("duplicate", "micro broken(", &[]);
    let error = ValkyrieCompiler::default().resolve_source_groups(&[bad.clone(), bad], "wasm32").expect_err("身份不能被 map 覆盖");
    assert!(error.to_string().contains("重复"));
    let error = ValkyrieCompiler::default()
        .resolve_source_groups(&[group("app", "micro broken(", &["missing"])], "wasm32")
        .expect_err("工程依赖错误不应成为语法错误");
    assert!(error.to_string().contains("missing"));
}

#[test]
fn evidence_is_computed_from_source_and_dependency_edges() {
    let baseline = [
        group("library", "micro answer() -> i32 { return 23 }", &[]),
        group("app", "[export] micro run() -> i32 { return answer() }", &["library"]),
    ];
    let first = semantic_closure_hash_for_groups(&baseline, "wasm32");
    let mut changed = baseline.clone();
    changed[0].sources[0].source = "micro answer() -> i32 { return 24 }".into();
    assert_ne!(first, semantic_closure_hash_for_groups(&changed, "wasm32"));
    let mut changed = baseline.clone();
    changed[1].direct_dependencies.clear();
    assert_ne!(first, semantic_closure_hash_for_groups(&changed, "wasm32"));
    let mut changed = baseline;
    changed[0].sources.push(CompilerSourceFile { path: "unused.v".into(), source: "micro ignored() -> i32 { return 42 }".into() });
    assert_eq!(first, semantic_closure_hash_for_groups(&changed, "wasm32"));
}

#[test]
fn empty_brace_namespace_and_pruned_namespace_cannot_change_following_scope() {
    let groups = [group("app", "namespace hidden {} [export] micro run() -> i32 { return 23 }", &[])];
    let modules = ValkyrieCompiler::default().resolve_source_groups(&groups, "wasm32").expect("空 namespace 保持作用域边界");
    assert_eq!(modules[0].functions.len(), 1);
    assert!(modules[0].functions[0].declaring_namespace.parts().is_empty());
}
