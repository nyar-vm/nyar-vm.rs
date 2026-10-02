use std::collections::{BTreeMap, BTreeSet};

use nyar::{Identifier, QualifiedName, RewriteTheory, TheoryBundle};
use nyar_emitter::{FragmentSubmission, testing::lower_fragment_to_nyar_module};
#[test]
fn rejects_edge_only_submission_without_compiler_owned_executable() {
    let operation = QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("add_two")]);
    let submission = FragmentSubmission {
        module_name: "demo".to_string(),
        fragment_id: Identifier::new("functions"),
        exported_operations: vec![operation.clone()],
        required_capabilities: Vec::new(),
        theory_bundle: TheoryBundle { shared: RewriteTheory::default(), fragment: RewriteTheory::default() },
        entry_operation: Some(operation),
        external_import_links: BTreeMap::new(),
        external_call_edges: Vec::new(),
        internal_call_edges: Vec::new(),
        operation_void_returns: BTreeSet::new(),
        witness_tables: Vec::new(),
        witness_calls: Vec::new(),
        control_flow: None,
        suspend_runtime: None,
        ..Default::default()
    };

    let error = lower_fragment_to_nyar_module(&submission).expect_err("edge-only submission must not be replayed");
    assert!(error.to_string().contains("Compiler-owned executable functions"));
}

#[test]
fn rejects_edge_only_export_alias_submission() {
    let operation = QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("pair_sum")]);
    let submission = FragmentSubmission {
        module_name: "demo".to_string(),
        fragment_id: Identifier::new("functions"),
        exported_operations: vec![operation.clone()],
        required_capabilities: Vec::new(),
        theory_bundle: TheoryBundle { shared: RewriteTheory::default(), fragment: RewriteTheory::default() },
        entry_operation: Some(operation.clone()),
        wasm_export_names: BTreeMap::from([(operation.clone(), "pairSum".to_string())]),
        external_import_links: BTreeMap::new(),
        external_call_edges: Vec::new(),
        internal_call_edges: Vec::new(),
        operation_void_returns: BTreeSet::new(),
        witness_tables: Vec::new(),
        witness_calls: Vec::new(),
        control_flow: None,
        suspend_runtime: None,
        ..Default::default()
    };

    let error = lower_fragment_to_nyar_module(&submission).expect_err("edge-only export must not be replayed");
    assert!(error.to_string().contains("Compiler-owned executable functions"));
}

#[test]
fn rejects_edge_only_noncanonical_export_key() {
    let canonical = QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("container"), Identifier::new("max_area")]);
    let operation = QualifiedName::new(vec![Identifier::new("demo.container"), Identifier::new("max_area")]);
    let submission = FragmentSubmission {
        module_name: "demo".to_string(),
        fragment_id: Identifier::new("functions"),
        exported_operations: vec![operation.clone()],
        required_capabilities: Vec::new(),
        theory_bundle: TheoryBundle { shared: RewriteTheory::default(), fragment: RewriteTheory::default() },
        entry_operation: Some(operation.clone()),
        wasm_export_names: BTreeMap::from([(canonical, "maxArea".to_string())]),
        external_import_links: BTreeMap::new(),
        external_call_edges: Vec::new(),
        internal_call_edges: Vec::new(),
        operation_void_returns: BTreeSet::new(),
        witness_tables: Vec::new(),
        witness_calls: Vec::new(),
        control_flow: None,
        suspend_runtime: None,
        ..Default::default()
    };

    let error = lower_fragment_to_nyar_module(&submission).expect_err("edge-only export must not be replayed");
    assert!(error.to_string().contains("Compiler-owned executable functions"));
}
