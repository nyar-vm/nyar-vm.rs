use std::collections::{BTreeMap, BTreeSet};

use nyar::{Identifier, QualifiedName, RewriteTheory, TheoryBundle};
use nyar_emitter::{FragmentSubmission, testing::lower_fragment_to_nyar_module};
use std_data::binary::nyar_ir::encode_module;

#[test]
fn emits_nyar_module_with_exports() {
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
        operation_literal_returns: BTreeMap::new(),
        operation_void_returns: BTreeSet::new(),
        witness_tables: Vec::new(),
        witness_calls: Vec::new(),
        control_flow: None,
        suspend_runtime: None,
        ..Default::default()
    };

    let module = lower_fragment_to_nyar_module(&submission);
    assert_eq!(module.exports.len(), 1);
    assert_eq!(module.exports[0].symbol_name, "add_two");
    assert!(!encode_module(&module).is_empty());
}
