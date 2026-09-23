use std::collections::BTreeMap;

use nyar::{
    CapabilityTag, ControlFlowPayload, ExternalCallArgument, ExternalCallEdge, ExternalImportLink, Identifier, QualifiedName, RewriteTheory,
    SuspendFunctionArtifact, SuspendStateArtifact, TheoryBundle,
};
use nyar_emitter::{
    FragmentSubmission,
    nyar_backend_jvm::{JvmInstruction, JvmTypeDescriptor},
    testing::lower_fragment_to_jvm_class,
};

#[test]
fn lowers_external_println_call_into_jvm_bytecode() {
    let main = QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("main")]);
    let console_write_line = QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("console_write_line")]);
    let class_file = lower_fragment_to_jvm_class(&FragmentSubmission {
        module_name: "demo".to_string(),
        fragment_id: Identifier::new("main"),
        exported_operations: vec![main.clone()],
        required_capabilities: vec![CapabilityTag::new("host-interop")],
        theory_bundle: TheoryBundle { shared: RewriteTheory::default(), fragment: RewriteTheory::default() },
        entry_operation: Some(main.clone()),
        external_import_links: BTreeMap::from([(
            console_write_line.clone(),
            ExternalImportLink::host(
                Some(Identifier::new("jvm")),
                vec!["java/lang/System".to_string(), "out".to_string(), "java/io/PrintStream".to_string(), "println".to_string()],
            ),
        )]),
        external_call_edges: vec![ExternalCallEdge::new(
            main,
            console_write_line,
            vec![ExternalCallArgument::StringLiteral("hello from jvm".to_string())],
        )],
        internal_call_edges: Vec::new(),
        operation_literal_returns: Default::default(),
        operation_void_returns: Default::default(),
        control_flow: None,
        suspend_runtime: None,
        witness_tables: Vec::new(),
        witness_calls: Vec::new(),
        ..Default::default()
    })
    .expect("lower jvm class");

    let main_method =
        class_file.methods.iter().find(|method| method.name == "demo__main").and_then(|method| method.code.as_ref()).expect("main method code");

    assert!(main_method.instructions.iter().any(|instruction| matches!(
        instruction,
        JvmInstruction::GetStatic(field)
            if field.owner == "java/lang/System"
                && field.name == "out"
                && field.descriptor == JvmTypeDescriptor::Object("java/io/PrintStream".to_string())
    )));
    assert!(main_method.instructions.iter().any(|instruction| matches!(
        instruction,
        JvmInstruction::LdcString(value) if value == "hello from jvm"
    )));
    assert!(main_method.instructions.iter().any(|instruction| matches!(
        instruction,
        JvmInstruction::InvokeVirtual(method)
            if method.owner == "java/io/PrintStream" && method.name == "println"
    )));
}

#[test]
fn emits_state_count_methods_for_control_flow_payload() {
    let symbol = QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("gen")]);
    let class_file = lower_fragment_to_jvm_class(&FragmentSubmission {
        module_name: "demo".to_string(),
        fragment_id: Identifier::new("suspend"),
        exported_operations: Vec::new(),
        required_capabilities: vec![CapabilityTag::new("suspend")],
        theory_bundle: TheoryBundle { shared: RewriteTheory::default(), fragment: RewriteTheory::default() },
        entry_operation: None,
        external_import_links: BTreeMap::new(),
        external_call_edges: Vec::new(),
        internal_call_edges: Vec::new(),
        operation_literal_returns: Default::default(),
        operation_void_returns: Default::default(),
        control_flow: Some(ControlFlowPayload {
            functions: vec![SuspendFunctionArtifact {
                symbol: symbol.clone(),
                state_machine_type: "GenStateMachine".to_string(),
                state_field: "__state".to_string(),
                frame_fields: Vec::new(),
                dispatch_cases: Vec::new(),
                states: vec![
                    SuspendStateArtifact {
                        state_id: 0,
                        effect: "Yield".to_string(),
                        resume_case_key: 1,
                        frame_carrier: "this".to_string(),
                        spill_fields: Vec::new(),
                        suspend_block_label: "yield_0".to_string(),
                        resume_block_label: "resume_0".to_string(),
                        resume_parameter_count: 0,
                        witness_bindings: Vec::new(),
                        continuation_index: None,
                    },
                    SuspendStateArtifact {
                        state_id: 1,
                        effect: "Complete".to_string(),
                        resume_case_key: 2,
                        frame_carrier: "this".to_string(),
                        spill_fields: Vec::new(),
                        suspend_block_label: "done".to_string(),
                        resume_block_label: "done".to_string(),
                        resume_parameter_count: 0,
                        witness_bindings: Vec::new(),
                        continuation_index: None,
                    },
                ],
                continuations: Vec::new(),
            }],
        }),
        suspend_runtime: None,
        witness_tables: Vec::new(),
        witness_calls: Vec::new(),
        ..Default::default()
    })
    .expect("lower jvm class");

    let method = class_file
        .methods
        .iter()
        .find(|method| method.name == "sm_demo__gen_move_next")
        .and_then(|method| method.code.as_ref())
        .expect("move_next method code");
    assert!(method.instructions.iter().any(|instruction| matches!(instruction, JvmInstruction::IConst(2))));
    assert!(class_file.methods.iter().any(|method| method.name == "main"));
}
