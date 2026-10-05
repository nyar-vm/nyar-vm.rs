use std::collections::BTreeSet;

use super::canonical_program_from_semantic_mir;
use crate::{ValkyrieCompiler, mir::MirLowerer};

#[test]
fn fragment_contract_ignores_colliding_diagnostic_labels() {
    let hir = ValkyrieCompiler::default()
        .compile_source(
            "micro leaf(value: i32) -> i32 { return value } \
         [export(name: \"answer\")] micro answer(value: i32) -> i32 { return leaf(value) }",
        )
        .expect("当前源码必须绑定声明及调用");
    let mut mir = MirLowerer::lower_module_semantic(&hir);
    let original = canonical_program_from_semantic_mir(&mir).expect("原始实例合同");
    for function in &mut mir.functions {
        function.symbol = "diagnostic::same".into();
    }
    mir.callable_identities.clear();
    let renamed = canonical_program_from_semantic_mir(&mir).expect("诊断标签不参与绑定");
    assert_eq!(renamed.linked.fragments, original.linked.fragments);
    assert_eq!(renamed.mir, original.mir);
    let fragment = renamed.linked.fragments.values().next().expect("公开片段");
    assert_eq!(fragment.exported_operations.len(), 2);
    assert_eq!(fragment.internal_call_edges.len(), 1);
}

#[test]
fn fragment_contract_closes_recursive_calls_without_unused_bodies() {
    let program = ValkyrieCompiler::default()
        .compile_source_to_program(
            "micro unused(value: i32) -> i32 { return value } \
         micro recursive(value: i32) -> i32 { return recursive(value) } \
         micro middle(value: i32) -> i32 { return recursive(value) } \
         [export(name: \"answer\")] micro answer(value: i32) -> i32 { return middle(value) }",
        )
        .expect("递归调用的编译闭包必须终止，不执行递归体");
    let canonical = program.canonical();
    let fragment = canonical.linked.fragments.values().next().expect("公开片段");
    assert_eq!(fragment.exported_operations.len(), 3);
    assert_eq!(fragment.internal_call_edges.len(), 3);
    assert_eq!(fragment.exported_operations.iter().copied().collect::<BTreeSet<_>>().len(), 3);
    assert_eq!(canonical.mir.functions.len(), 4);
    assert!(
        fragment
            .internal_call_edges
            .iter()
            .all(|edge| { fragment.exported_operations.contains(&edge.caller) && fragment.exported_operations.contains(&edge.callee) })
    );
}

#[test]
fn fragment_contract_binds_distinct_import_indices() {
    let program = ValkyrieCompiler::default()
        .compile_source_to_program(
            "[host_contract] micro first(value: i32) -> i32; \
         [host_contract] micro second(value: i32) -> i32; \
         micro middle(value: i32) -> i32 { return second(value) } \
         [export(name: \"answer\")] micro answer(value: i32) -> i32 { return middle(first(value)) }",
        )
        .expect("两个显式导入沿完整源码编译链闭合");
    let canonical = program.canonical();
    let fragment = canonical.linked.fragments.values().next().expect("公开片段");
    assert_eq!(fragment.external_imports.len(), 2);
    assert_eq!(fragment.external_call_edges.len(), 2);
    assert_eq!(fragment.external_call_edges.iter().map(|edge| edge.import).collect::<BTreeSet<_>>().len(), 2);
    for edge in &fragment.external_call_edges {
        let import = canonical.linked.imports.get(&edge.import).expect("真实 ImportIndex");
        assert_eq!(fragment.external_imports.get(&import.callee), Some(&import.link));
        assert!(canonical.mir.functions.contains_key(&edge.caller));
        assert!(!canonical.mir.functions.contains_key(&import.callee));
    }
}
