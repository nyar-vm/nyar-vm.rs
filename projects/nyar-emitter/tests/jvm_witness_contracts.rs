use nyar::{CapabilityTag, Identifier, RewriteTheory, TheoryBundle, WitnessCallEdge, WitnessMethodSlotSubmission, WitnessSubmission};
use nyar_emitter::{
    FragmentSubmission,
    nyar_backend_jvm::{JvmClassFile, JvmInstruction},
    testing::append_jvm_witness_methods,
};

#[test]
fn emits_witness_main_and_impl_stub() {
    let mut class_file = JvmClassFile::new("demo/functions".to_string());
    let submission = FragmentSubmission {
        module_name: "demo".to_string(),
        fragment_id: Identifier::new("functions"),
        exported_operations: Vec::new(),
        required_capabilities: vec![CapabilityTag::new("trait-witness")],
        theory_bundle: TheoryBundle { shared: RewriteTheory::default(), fragment: RewriteTheory::default() },
        entry_operation: None,
        external_import_links: Default::default(),
        external_call_edges: Vec::new(),
        internal_call_edges: Vec::new(),
        operation_literal_returns: Default::default(),
        operation_void_returns: Default::default(),
        control_flow: None,
        suspend_runtime: None,
        witness_tables: vec![WitnessSubmission {
            type_name: "Dog".to_string(),
            trait_name: "Animal".to_string(),
            table_label: "witness_table_Dog_Animal".to_string(),
            fat_ptr_label: "witness_fat_Dog_Animal".to_string(),
            methods: vec![WitnessMethodSlotSubmission {
                method_name: "make_sound".to_string(),
                impl_symbol: "witness_Dog_Animal_make_sound".to_string(),
                method_index: 0,
            }],
            result_literal: "woof".to_string(),
        }],
        witness_calls: vec![WitnessCallEdge {
            trait_name: "Animal".to_string(),
            type_name: "Dog".to_string(),
            method_index: 0,
            print_result: true,
        }],
        ..Default::default()
    };

    let main_body = append_jvm_witness_methods(&mut class_file, &submission).expect("witness main");
    assert!(class_file.methods.iter().any(|method| method.name == "witness_Dog_Animal_make_sound"));
    assert!(main_body.iter().any(|instruction| matches!(
        instruction,
        JvmInstruction::InvokeStatic(method) if method.name == "witness_Dog_Animal_make_sound"
    )));
    assert!(main_body.iter().any(|instruction| matches!(
        instruction,
        JvmInstruction::InvokeVirtual(method) if method.name == "println"
    )));
}

#[test]
fn future_poll_impl_branches_on_receiver_tick() {
    let mut class_file = JvmClassFile::new("demo/functions".to_string());
    let submission = FragmentSubmission {
        module_name: "demo".to_string(),
        fragment_id: Identifier::new("functions"),
        exported_operations: Vec::new(),
        required_capabilities: vec![CapabilityTag::new("trait-witness")],
        theory_bundle: TheoryBundle::default(),
        entry_operation: None,
        external_import_links: Default::default(),
        external_call_edges: Vec::new(),
        internal_call_edges: Vec::new(),
        operation_literal_returns: Default::default(),
        operation_void_returns: Default::default(),
        control_flow: None,
        suspend_runtime: None,
        witness_tables: vec![WitnessSubmission {
            type_name: "ReadyFuture".to_string(),
            trait_name: "Future".to_string(),
            table_label: "witness_table_ReadyFuture_Future".to_string(),
            fat_ptr_label: "witness_fat_ReadyFuture_Future".to_string(),
            methods: vec![WitnessMethodSlotSubmission {
                method_name: "poll".to_string(),
                impl_symbol: "witness_ReadyFuture_Future_poll".to_string(),
                method_index: 0,
            }],
            result_literal: String::new(),
        }],
        witness_calls: vec![WitnessCallEdge {
            trait_name: "Future".to_string(),
            type_name: "ReadyFuture".to_string(),
            method_index: 0,
            print_result: false,
        }],
        ..Default::default()
    };

    append_jvm_witness_methods(&mut class_file, &submission);
    let poll = class_file
        .methods
        .iter()
        .find(|method| method.name == "witness_ReadyFuture_Future_poll")
        .and_then(|method| method.code.as_ref())
        .expect("poll impl");
    assert!(poll.instructions.iter().any(|instruction| matches!(instruction, JvmInstruction::IALoad)));
}
