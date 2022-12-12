use std::{collections::BTreeMap, path::Path};

use nyar::{
    CapabilityTag, ClrSuspendStrategy, ControlFlowPayload, HostProjectionBoundary, Identifier, QualifiedName, SuspendRuntimePayload,
    TargetBackendFamily, TargetLane, TheoryBundle, VmSuspendStrategy,
};

use nyar_emitter::{FragmentSubmission, LoweredBackendInput};

fn witness_fields() -> (Vec<nyar::WitnessSubmission>, Vec<nyar::WitnessCallEdge>) {
    (Vec::new(), Vec::new())
}

#[test]
fn clr_lane_accepts_resolved_trait_witness_with_suspend_payload() {
    use nyar::{SuspendFunctionArtifact, SuspendStateArtifact, SuspendWitnessBinding};

    let (mut witness_tables, witness_calls) = witness_fields();
    witness_tables.push(nyar::WitnessSubmission {
        type_name: "CounterIterator".to_string(),
        trait_name: "Iterator".to_string(),
        table_label: "witness_table".to_string(),
        fat_ptr_label: "witness_fat".to_string(),
        methods: Vec::new(),
        result_literal: String::new(),
    });
    let submission = FragmentSubmission {
        module_name: "demo".to_string(),
        fragment_id: Identifier::new("suspend"),
        exported_operations: Vec::new(),
        required_capabilities: vec![CapabilityTag::new("trait-witness"), CapabilityTag::new("suspend")],
        theory_bundle: TheoryBundle::default(),
        entry_operation: None,
        external_import_links: BTreeMap::new(),
        external_call_edges: Vec::new(),
        internal_call_edges: Vec::new(),
        operation_literal_returns: Default::default(),
        operation_void_returns: Default::default(),
        witness_tables,
        witness_calls,
        control_flow: Some(ControlFlowPayload {
            functions: vec![SuspendFunctionArtifact {
                symbol: QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("gen")]),
                state_machine_type: "GenStateMachine".to_string(),
                state_field: "__state".to_string(),
                frame_fields: Vec::new(),
                dispatch_cases: Vec::new(),
                states: vec![SuspendStateArtifact {
                    state_id: 0,
                    effect: "DelegateYield".to_string(),
                    resume_case_key: 1,
                    frame_carrier: "this".to_string(),
                    spill_fields: Vec::new(),
                    suspend_block_label: "yield_from".to_string(),
                    resume_block_label: "resume".to_string(),
                    resume_parameter_count: 0,
                    witness_bindings: vec![SuspendWitnessBinding {
                        trait_name: "Iterator".to_string(),
                        method_name: "next".to_string(),
                        method_index: 1,
                        type_name: Some("CounterIterator".to_string()),
                        impl_symbol: Some("witness_CounterIterator_Iterator_next".to_string()),
                    }],
                    continuation_index: None,
                }],
                continuations: Vec::new(),
            }],
        }),
        suspend_runtime: None,
        ..Default::default()
    };

    LoweredBackendInput::from_fragment_submission(
        &submission,
        TargetBackendFamily::Clr,
        HostProjectionBoundary::Clr,
        Path::new("."),
        TargetLane::Clr,
        ClrSuspendStrategy::StateMachine,
        VmSuspendStrategy::default(),
        "win32",
    )
    .expect("CLR lane should accept statically resolved trait witness with suspend payload");
}

#[test]
fn clr_lane_rejects_open_witness_capabilities() {
    let (witness_tables, witness_calls) = witness_fields();
    let submission = FragmentSubmission {
        module_name: "demo".to_string(),
        fragment_id: Identifier::new("demo::functions"),
        exported_operations: Vec::new(),
        required_capabilities: vec![CapabilityTag::new("open-witness")],
        theory_bundle: TheoryBundle::default(),
        entry_operation: None,
        external_import_links: BTreeMap::new(),
        external_call_edges: Vec::new(),
        internal_call_edges: Vec::new(),
        operation_literal_returns: Default::default(),
        operation_void_returns: Default::default(),
        witness_tables,
        witness_calls,
        control_flow: None,
        suspend_runtime: None,
        ..Default::default()
    };

    let error = LoweredBackendInput::from_fragment_submission(
        &submission,
        TargetBackendFamily::Clr,
        HostProjectionBoundary::Clr,
        Path::new("."),
        TargetLane::Clr,
        ClrSuspendStrategy::StateMachine,
        VmSuspendStrategy::default(),
        "win32",
    )
    .expect_err("CLR lane should reject open witness capabilities");
    let msg = error.to_string();
    assert!(msg.contains("witness") || msg.contains("UnsupportedTraitDispatch"), "unexpected error: {msg}");
}

#[test]
fn state_machine_lane_accepts_control_flow_payload() {
    let (witness_tables, witness_calls) = witness_fields();
    let submission = FragmentSubmission {
        module_name: "demo".to_string(),
        fragment_id: Identifier::new("suspend"),
        exported_operations: Vec::new(),
        required_capabilities: vec![CapabilityTag::new("suspend")],
        theory_bundle: TheoryBundle::default(),
        entry_operation: None,
        external_import_links: BTreeMap::new(),
        external_call_edges: Vec::new(),
        internal_call_edges: Vec::new(),
        operation_literal_returns: Default::default(),
        operation_void_returns: Default::default(),
        witness_tables,
        witness_calls,
        control_flow: Some(ControlFlowPayload { functions: Vec::new() }),
        suspend_runtime: None,
        ..Default::default()
    };

    LoweredBackendInput::from_fragment_submission(
        &submission,
        TargetBackendFamily::Clr,
        HostProjectionBoundary::Clr,
        Path::new("."),
        TargetLane::Clr,
        ClrSuspendStrategy::StateMachine,
        VmSuspendStrategy::default(),
        "win32",
    )
    .expect("CLR state-machine lane should accept control_flow payload");
}

#[test]
fn state_machine_lane_rejects_first_class_runtime_payload() {
    let (witness_tables, witness_calls) = witness_fields();
    let submission = FragmentSubmission {
        module_name: "demo".to_string(),
        fragment_id: Identifier::new("suspend"),
        exported_operations: Vec::new(),
        required_capabilities: vec![CapabilityTag::new("suspend")],
        theory_bundle: TheoryBundle::default(),
        entry_operation: None,
        external_import_links: BTreeMap::new(),
        external_call_edges: Vec::new(),
        internal_call_edges: Vec::new(),
        operation_literal_returns: Default::default(),
        operation_void_returns: Default::default(),
        witness_tables,
        witness_calls,
        control_flow: None,
        suspend_runtime: Some(SuspendRuntimePayload { functions: Vec::new() }),
        ..Default::default()
    };

    let error = LoweredBackendInput::from_fragment_submission(
        &submission,
        TargetBackendFamily::Clr,
        HostProjectionBoundary::Clr,
        Path::new("."),
        TargetLane::Clr,
        ClrSuspendStrategy::StateMachine,
        VmSuspendStrategy::default(),
        "win32",
    )
    .expect_err("CLR state-machine lane should reject suspend_runtime");
    assert!(error.to_string().contains("state-machine lane"));
}

#[test]
fn clr_runtime_async_accepts_suspend_runtime_payload() {
    let (witness_tables, witness_calls) = witness_fields();
    let submission = FragmentSubmission {
        module_name: "demo".to_string(),
        fragment_id: Identifier::new("suspend"),
        exported_operations: Vec::new(),
        required_capabilities: vec![CapabilityTag::new("suspend")],
        theory_bundle: TheoryBundle::default(),
        entry_operation: None,
        external_import_links: BTreeMap::new(),
        external_call_edges: Vec::new(),
        internal_call_edges: Vec::new(),
        operation_literal_returns: Default::default(),
        operation_void_returns: Default::default(),
        witness_tables,
        witness_calls,
        control_flow: None,
        suspend_runtime: Some(SuspendRuntimePayload { functions: Vec::new() }),
        ..Default::default()
    };

    LoweredBackendInput::from_fragment_submission(
        &submission,
        TargetBackendFamily::Clr,
        HostProjectionBoundary::Clr,
        Path::new("."),
        TargetLane::Clr,
        ClrSuspendStrategy::RuntimeAsync,
        VmSuspendStrategy::default(),
        "win32",
    )
    .expect("CLR RuntimeAsync should accept suspend_runtime payload");
}

#[test]
fn clr_runtime_async_rejects_control_flow_payload() {
    let (witness_tables, witness_calls) = witness_fields();
    let submission = FragmentSubmission {
        module_name: "demo".to_string(),
        fragment_id: Identifier::new("suspend"),
        exported_operations: Vec::new(),
        required_capabilities: vec![CapabilityTag::new("suspend")],
        theory_bundle: TheoryBundle::default(),
        entry_operation: None,
        external_import_links: BTreeMap::new(),
        external_call_edges: Vec::new(),
        internal_call_edges: Vec::new(),
        operation_literal_returns: Default::default(),
        operation_void_returns: Default::default(),
        witness_tables,
        witness_calls,
        control_flow: Some(ControlFlowPayload { functions: Vec::new() }),
        suspend_runtime: None,
        ..Default::default()
    };

    let error = LoweredBackendInput::from_fragment_submission(
        &submission,
        TargetBackendFamily::Clr,
        HostProjectionBoundary::Clr,
        Path::new("."),
        TargetLane::Clr,
        ClrSuspendStrategy::RuntimeAsync,
        VmSuspendStrategy::default(),
        "win32",
    )
    .expect_err("CLR RuntimeAsync should reject control_flow payload");
    assert!(error.to_string().contains("first-class suspend lane"));
}

#[test]
fn first_class_lane_accepts_suspend_runtime_payload() {
    let (witness_tables, witness_calls) = witness_fields();
    let submission = FragmentSubmission {
        module_name: "demo".to_string(),
        fragment_id: Identifier::new("suspend"),
        exported_operations: Vec::new(),
        required_capabilities: vec![CapabilityTag::new("suspend")],
        theory_bundle: TheoryBundle::default(),
        entry_operation: None,
        external_import_links: BTreeMap::new(),
        external_call_edges: Vec::new(),
        internal_call_edges: Vec::new(),
        operation_literal_returns: Default::default(),
        operation_void_returns: Default::default(),
        witness_tables,
        witness_calls,
        control_flow: None,
        suspend_runtime: Some(SuspendRuntimePayload { functions: Vec::new() }),
        ..Default::default()
    };

    LoweredBackendInput::from_fragment_submission(
        &submission,
        TargetBackendFamily::NyarVm,
        HostProjectionBoundary::Vm,
        Path::new("."),
        TargetLane::Vm,
        ClrSuspendStrategy::default(),
        VmSuspendStrategy::default(),
        "default",
    )
    .expect("nyar-vm lane should accept suspend_runtime payload");
}

#[test]
fn vm_state_machine_lane_accepts_control_flow_payload() {
    use nyar::{ControlFlowPayload, SuspendFunctionArtifact, SuspendStateArtifact, VmSuspendStrategy};

    let (witness_tables, witness_calls) = witness_fields();
    let submission = FragmentSubmission {
        module_name: "demo".to_string(),
        fragment_id: Identifier::new("suspend"),
        exported_operations: Vec::new(),
        required_capabilities: vec![CapabilityTag::new("suspend")],
        theory_bundle: TheoryBundle::default(),
        entry_operation: None,
        external_import_links: BTreeMap::new(),
        external_call_edges: Vec::new(),
        internal_call_edges: Vec::new(),
        operation_literal_returns: Default::default(),
        operation_void_returns: Default::default(),
        witness_tables,
        witness_calls,
        control_flow: Some(ControlFlowPayload {
            functions: vec![SuspendFunctionArtifact {
                symbol: QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("gen")]),
                state_machine_type: "GenStateMachine".to_string(),
                state_field: "__state".to_string(),
                frame_fields: Vec::new(),
                dispatch_cases: Vec::new(),
                states: vec![SuspendStateArtifact {
                    state_id: 0,
                    effect: "Yield".to_string(),
                    resume_case_key: 1,
                    frame_carrier: "this".to_string(),
                    spill_fields: Vec::new(),
                    suspend_block_label: "yield_0".to_string(),
                    resume_block_label: "resume_0".to_string(),
                    resume_parameter_count: 1,
                    witness_bindings: Vec::new(),
                    continuation_index: None,
                }],
                continuations: Vec::new(),
            }],
        }),
        suspend_runtime: None,
        ..Default::default()
    };

    LoweredBackendInput::from_fragment_submission(
        &submission,
        TargetBackendFamily::NyarVm,
        HostProjectionBoundary::Vm,
        Path::new("."),
        TargetLane::Vm,
        ClrSuspendStrategy::default(),
        VmSuspendStrategy::StateMachine,
        "default",
    )
    .expect("nyar-vm state-machine lane should accept control_flow payload");
}

#[test]
fn first_class_lane_rejects_state_machine_control_flow_payload() {
    let (witness_tables, witness_calls) = witness_fields();
    let submission = FragmentSubmission {
        module_name: "demo".to_string(),
        fragment_id: Identifier::new("suspend"),
        exported_operations: Vec::new(),
        required_capabilities: vec![CapabilityTag::new("suspend")],
        theory_bundle: TheoryBundle::default(),
        entry_operation: None,
        external_import_links: BTreeMap::new(),
        external_call_edges: Vec::new(),
        internal_call_edges: Vec::new(),
        operation_literal_returns: Default::default(),
        operation_void_returns: Default::default(),
        witness_tables,
        witness_calls,
        control_flow: Some(ControlFlowPayload { functions: Vec::new() }),
        suspend_runtime: None,
        ..Default::default()
    };

    let error = LoweredBackendInput::from_fragment_submission(
        &submission,
        TargetBackendFamily::NyarVm,
        HostProjectionBoundary::Vm,
        Path::new("."),
        TargetLane::Vm,
        ClrSuspendStrategy::default(),
        VmSuspendStrategy::default(),
        "default",
    )
    .expect_err("nyar-vm lane should reject control_flow payload");
    assert!(error.to_string().contains("first-class suspend lane"));
}

#[test]
fn jvm_lane_accepts_control_flow_payload() {
    let (witness_tables, witness_calls) = witness_fields();
    let submission = FragmentSubmission {
        module_name: "demo".to_string(),
        fragment_id: Identifier::new("suspend"),
        exported_operations: Vec::new(),
        required_capabilities: vec![CapabilityTag::new("suspend")],
        theory_bundle: TheoryBundle::default(),
        entry_operation: None,
        external_import_links: BTreeMap::new(),
        external_call_edges: Vec::new(),
        internal_call_edges: Vec::new(),
        operation_literal_returns: Default::default(),
        operation_void_returns: Default::default(),
        witness_tables,
        witness_calls,
        control_flow: Some(ControlFlowPayload { functions: Vec::new() }),
        suspend_runtime: None,
        ..Default::default()
    };

    LoweredBackendInput::from_fragment_submission(
        &submission,
        TargetBackendFamily::Jvm,
        HostProjectionBoundary::Jvm,
        Path::new("."),
        TargetLane::Jvm,
        ClrSuspendStrategy::default(),
        VmSuspendStrategy::default(),
        "default",
    )
    .expect("JVM lane should accept control_flow payload");
}

#[test]
fn jvm_lane_rejects_suspend_runtime_payload() {
    let (witness_tables, witness_calls) = witness_fields();
    let submission = FragmentSubmission {
        module_name: "demo".to_string(),
        fragment_id: Identifier::new("suspend"),
        exported_operations: Vec::new(),
        required_capabilities: vec![CapabilityTag::new("suspend")],
        theory_bundle: TheoryBundle::default(),
        entry_operation: None,
        external_import_links: BTreeMap::new(),
        external_call_edges: Vec::new(),
        internal_call_edges: Vec::new(),
        operation_literal_returns: Default::default(),
        operation_void_returns: Default::default(),
        witness_tables,
        witness_calls,
        control_flow: None,
        suspend_runtime: Some(SuspendRuntimePayload { functions: Vec::new() }),
        ..Default::default()
    };

    let error = LoweredBackendInput::from_fragment_submission(
        &submission,
        TargetBackendFamily::Jvm,
        HostProjectionBoundary::Jvm,
        Path::new("."),
        TargetLane::Jvm,
        ClrSuspendStrategy::default(),
        VmSuspendStrategy::default(),
        "default",
    )
    .expect_err("JVM lane should reject suspend_runtime");
    assert!(error.to_string().contains("state-machine lane"));
}

#[test]
fn wasm_lane_accepts_control_flow_payload() {
    let (witness_tables, witness_calls) = witness_fields();
    let submission = FragmentSubmission {
        module_name: "demo".to_string(),
        fragment_id: Identifier::new("suspend"),
        exported_operations: Vec::new(),
        required_capabilities: vec![CapabilityTag::new("suspend")],
        theory_bundle: TheoryBundle::default(),
        entry_operation: None,
        external_import_links: BTreeMap::new(),
        external_call_edges: Vec::new(),
        internal_call_edges: Vec::new(),
        operation_literal_returns: Default::default(),
        operation_void_returns: Default::default(),
        witness_tables,
        witness_calls,
        control_flow: Some(ControlFlowPayload { functions: Vec::new() }),
        suspend_runtime: None,
        ..Default::default()
    };

    LoweredBackendInput::from_fragment_submission(
        &submission,
        TargetBackendFamily::Wasm,
        HostProjectionBoundary::WasmJsGlue,
        Path::new("."),
        TargetLane::Wasm,
        ClrSuspendStrategy::default(),
        VmSuspendStrategy::default(),
        "default",
    )
    .expect("Wasm lane should accept control_flow payload");
}

#[test]
fn native_lane_accepts_resolved_trait_witness_with_suspend_payload() {
    use nyar::{SuspendFunctionArtifact, SuspendStateArtifact, SuspendWitnessBinding};

    let (mut witness_tables, witness_calls) = witness_fields();
    witness_tables.push(nyar::WitnessSubmission {
        type_name: "ReadyFuture".to_string(),
        trait_name: "Future".to_string(),
        table_label: "witness_table_ReadyFuture_Future".to_string(),
        fat_ptr_label: "witness_fat_ReadyFuture_Future".to_string(),
        methods: vec![nyar::WitnessMethodSlotSubmission {
            method_name: "poll".to_string(),
            impl_symbol: "witness_ReadyFuture_Future_poll".to_string(),
            method_index: 0,
        }],
        result_literal: String::new(),
    });
    let submission = FragmentSubmission {
        module_name: "demo".to_string(),
        fragment_id: Identifier::new("suspend"),
        exported_operations: Vec::new(),
        required_capabilities: vec![CapabilityTag::new("trait-witness"), CapabilityTag::new("suspend")],
        theory_bundle: TheoryBundle::default(),
        entry_operation: None,
        external_import_links: BTreeMap::new(),
        external_call_edges: Vec::new(),
        internal_call_edges: Vec::new(),
        operation_literal_returns: Default::default(),
        operation_void_returns: Default::default(),
        witness_tables,
        witness_calls,
        control_flow: Some(ControlFlowPayload {
            functions: vec![SuspendFunctionArtifact {
                symbol: QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("async_fn")]),
                state_machine_type: "AsyncFnStateMachine".to_string(),
                state_field: "__state".to_string(),
                frame_fields: vec!["__witness_payload_0".to_string()],
                dispatch_cases: Vec::new(),
                states: vec![SuspendStateArtifact {
                    state_id: 0,
                    effect: "Await".to_string(),
                    resume_case_key: 1,
                    frame_carrier: "this".to_string(),
                    spill_fields: vec!["__witness_payload_0".to_string()],
                    suspend_block_label: "await".to_string(),
                    resume_block_label: "resume".to_string(),
                    resume_parameter_count: 0,
                    witness_bindings: vec![SuspendWitnessBinding {
                        trait_name: "Future".to_string(),
                        method_name: "poll".to_string(),
                        method_index: 0,
                        type_name: Some("ReadyFuture".to_string()),
                        impl_symbol: Some("witness_ReadyFuture_Future_poll".to_string()),
                    }],
                    continuation_index: None,
                }],
                continuations: Vec::new(),
            }],
        }),
        suspend_runtime: None,
        ..Default::default()
    };

    LoweredBackendInput::from_fragment_submission(
        &submission,
        TargetBackendFamily::Native,
        HostProjectionBoundary::Native,
        Path::new("."),
        TargetLane::Native,
        ClrSuspendStrategy::StateMachine,
        VmSuspendStrategy::default(),
        "linux-gnu",
    )
    .expect("Native lane should accept resolved trait witness with suspend payload");
}

#[test]
fn wasm_lane_rejects_suspend_runtime_payload() {
    let (witness_tables, witness_calls) = witness_fields();
    let submission = FragmentSubmission {
        module_name: "demo".to_string(),
        fragment_id: Identifier::new("suspend"),
        exported_operations: Vec::new(),
        required_capabilities: vec![CapabilityTag::new("suspend")],
        theory_bundle: TheoryBundle::default(),
        entry_operation: None,
        external_import_links: BTreeMap::new(),
        external_call_edges: Vec::new(),
        internal_call_edges: Vec::new(),
        operation_literal_returns: Default::default(),
        operation_void_returns: Default::default(),
        witness_tables,
        witness_calls,
        control_flow: None,
        suspend_runtime: Some(SuspendRuntimePayload { functions: Vec::new() }),
        ..Default::default()
    };

    let error = LoweredBackendInput::from_fragment_submission(
        &submission,
        TargetBackendFamily::Wasm,
        HostProjectionBoundary::WasiComponent,
        Path::new("."),
        TargetLane::Wasm,
        ClrSuspendStrategy::default(),
        VmSuspendStrategy::default(),
        "default",
    )
    .expect_err("Wasm lane should reject suspend_runtime");
    assert!(error.to_string().contains("state-machine lane"));
}
