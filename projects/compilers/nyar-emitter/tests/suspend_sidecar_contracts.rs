use nyar::{
    ControlFlowPayload, Identifier, QualifiedName, SuspendContinuationArtifact, SuspendFunctionArtifact, SuspendRuntimeFunctionArtifact,
    SuspendRuntimePayload, SuspendStateArtifact, SuspendWitnessBinding,
};
use nyar_emitter::testing::{serialize_control_flow_sidecar, serialize_suspend_runtime_sidecar};
use serde_json::Value;

#[test]
fn control_flow_payload_serializes_flattened_witness_bindings() {
    let payload = ControlFlowPayload {
        functions: vec![SuspendFunctionArtifact {
            symbol: QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("async_fn")]),
            state_machine_type: "GenStateMachine".to_string(),
            state_field: "__state".to_string(),
            frame_fields: vec!["spill0".to_string()],
            dispatch_cases: Vec::new(),
            states: vec![
                SuspendStateArtifact {
                    state_id: 0,
                    effect: "Await".to_string(),
                    resume_case_key: 1,
                    frame_carrier: "this".to_string(),
                    spill_fields: Vec::new(),
                    suspend_block_label: "await_0".to_string(),
                    resume_block_label: "resume_0".to_string(),
                    resume_parameter_count: 0,
                    witness_bindings: vec![SuspendWitnessBinding {
                        trait_name: "Future".to_string(),
                        method_name: "poll".to_string(),
                        method_index: 0,
                        type_name: None,
                        impl_symbol: None,
                    }],
                    continuation_index: None,
                },
                SuspendStateArtifact {
                    state_id: 1,
                    effect: "Yield".to_string(),
                    resume_case_key: 2,
                    frame_carrier: "this".to_string(),
                    spill_fields: Vec::new(),
                    suspend_block_label: "yield_1".to_string(),
                    resume_block_label: "resume_1".to_string(),
                    resume_parameter_count: 0,
                    witness_bindings: vec![SuspendWitnessBinding {
                        trait_name: "Iterator".to_string(),
                        method_name: "next".to_string(),
                        method_index: 1,
                        type_name: None,
                        impl_symbol: None,
                    }],
                    continuation_index: Some(3),
                },
            ],
            continuations: Vec::new(),
        }],
    };

    let actual: Value = serde_json::from_str(&serialize_control_flow_sidecar(&payload)).expect("valid control_flow json");

    assert_eq!(actual["functions"][0]["symbol"], "demo::async_fn");
    assert_eq!(actual["functions"][0]["state_machine_type"], "GenStateMachine");
    assert_eq!(actual["functions"][0]["state_count"], 2);
    assert_eq!(actual["functions"][0]["witness_bindings"][0]["trait"], "Future");
    assert_eq!(actual["functions"][0]["witness_bindings"][1]["method"], "next");
    assert_eq!(actual["functions"][0]["witness_bindings"][1]["index"], 1);
}

#[test]
fn suspend_runtime_payload_serializes_valid_json_with_null_optionals() {
    let payload = SuspendRuntimePayload {
        functions: vec![SuspendRuntimeFunctionArtifact {
            symbol: QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("async_fn")]),
            entry_block_label: "entry\"line".to_string(),
            frame_fields: Vec::new(),
            states: vec![SuspendStateArtifact {
                state_id: 7,
                effect: "Await\nReady".to_string(),
                resume_case_key: 1,
                frame_carrier: "this".to_string(),
                spill_fields: Vec::new(),
                suspend_block_label: "await_0".to_string(),
                resume_block_label: "resume_0".to_string(),
                resume_parameter_count: 0,
                witness_bindings: Vec::new(),
                continuation_index: None,
            }],
            continuations: vec![SuspendContinuationArtifact {
                index: 2,
                carrier: "Result<String>".to_string(),
                dispatch_block_label: "dispatch".to_string(),
                resume_block_label: "resume".to_string(),
                handler_exit_block_label: "exit".to_string(),
                resume_parameter_count: 1,
            }],
        }],
    };

    let actual: Value = serde_json::from_str(&serialize_suspend_runtime_sidecar(&payload)).expect("valid suspend_runtime json");

    assert_eq!(actual["functions"][0]["symbol"], "demo::async_fn");
    assert_eq!(actual["functions"][0]["entry_block_label"], "entry\"line");
    assert_eq!(actual["functions"][0]["states"][0]["effect"], "Await\nReady");
    assert_eq!(actual["functions"][0]["states"][0]["continuation_index"], Value::Null);
    assert_eq!(actual["functions"][0]["continuations"][0]["dispatch_block_label"], "dispatch");
    assert_eq!(actual["functions"][0]["continuations"][0]["handler_exit_block_label"], "exit");
}
