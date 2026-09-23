//! Shared JSON sidecars for suspend payloads (control_flow / suspend_runtime).

use std::{fs, path::Path};

use miette::{IntoDiagnostic, Result, WrapErr};
use nyar::{
    ArtifactDescriptor, ArtifactFormat, ArtifactKind, ArtifactSet, BinaryTarget, ControlFlowPayload, SuspendRuntimePayload, TargetLane,
};
use serde::Serialize;

#[derive(Serialize)]
struct ControlFlowPayloadWire {
    functions: Vec<ControlFlowFunctionWire>,
}

#[derive(Serialize)]
struct ControlFlowFunctionWire {
    symbol: String,
    state_machine_type: String,
    state_count: usize,
    witness_bindings: Vec<WitnessBindingWire>,
}

#[derive(Serialize)]
struct WitnessBindingWire {
    r#trait: String,
    method: String,
    index: u32,
}

#[derive(Serialize)]
struct SuspendRuntimePayloadWire {
    functions: Vec<SuspendRuntimeFunctionWire>,
}

#[derive(Serialize)]
struct SuspendRuntimeFunctionWire {
    symbol: String,
    entry_block_label: String,
    states: Vec<SuspendRuntimeStateWire>,
    continuations: Vec<SuspendContinuationWire>,
}

#[derive(Serialize)]
struct SuspendRuntimeStateWire {
    state_id: u32,
    effect: String,
    continuation_index: Option<usize>,
}

#[derive(Serialize)]
struct SuspendContinuationWire {
    index: usize,
    carrier: String,
    dispatch_block_label: String,
    resume_block_label: String,
    handler_exit_block_label: String,
}

/// Serialize a state-machine `ControlFlowPayload` for sidecar / custom-section wire.
pub(crate) fn serialize_control_flow_payload(payload: &ControlFlowPayload) -> String {
    let wire = ControlFlowPayloadWire {
        functions: payload
            .functions
            .iter()
            .map(|function| ControlFlowFunctionWire {
                symbol: function.symbol.to_string(),
                state_machine_type: function.state_machine_type.clone(),
                state_count: function.states.len(),
                witness_bindings: function
                    .states
                    .iter()
                    .flat_map(|state| &state.witness_bindings)
                    .map(|binding| WitnessBindingWire {
                        r#trait: binding.trait_name.clone(),
                        method: binding.method_name.clone(),
                        index: binding.method_index,
                    })
                    .collect(),
            })
            .collect(),
    };
    serde_json::to_string(&wire).expect("control_flow sidecar serialization should not fail")
}

/// Serialize a first-class `SuspendRuntimePayload` for nyar-vm sidecar.
pub(crate) fn serialize_suspend_runtime_payload(payload: &SuspendRuntimePayload) -> String {
    let wire = SuspendRuntimePayloadWire {
        functions: payload
            .functions
            .iter()
            .map(|function| SuspendRuntimeFunctionWire {
                symbol: function.symbol.to_string(),
                entry_block_label: function.entry_block_label.clone(),
                states: function
                    .states
                    .iter()
                    .map(|state| SuspendRuntimeStateWire {
                        state_id: state.state_id,
                        effect: state.effect.clone(),
                        continuation_index: state.continuation_index,
                    })
                    .collect(),
                continuations: function
                    .continuations
                    .iter()
                    .map(|continuation| SuspendContinuationWire {
                        index: continuation.index,
                        carrier: continuation.carrier.clone(),
                        dispatch_block_label: continuation.dispatch_block_label.clone(),
                        resume_block_label: continuation.resume_block_label.clone(),
                        handler_exit_block_label: continuation.handler_exit_block_label.clone(),
                    })
                    .collect(),
            })
            .collect(),
    };
    serde_json::to_string(&wire).expect("suspend_runtime sidecar serialization should not fail")
}

/// Write `{artifact}.control_flow.json` when payload is present; push into `artifacts`.
pub(crate) fn write_control_flow_sidecar(
    output_dir: &Path,
    artifact_name: &str,
    payload: &ControlFlowPayload,
    target: &BinaryTarget,
    lane: TargetLane,
    artifacts: &mut ArtifactSet,
) -> Result<()> {
    let sidecar_name = format!("{artifact_name}.control_flow.json");
    let sidecar_path = output_dir.join(&sidecar_name);
    fs::create_dir_all(output_dir).into_diagnostic().wrap_err_with(|| format!("创建输出目录失败：{}", output_dir.display()))?;
    let body = serialize_control_flow_payload(payload);
    fs::write(&sidecar_path, body).into_diagnostic().wrap_err_with(|| format!("写入 control_flow sidecar 失败：{}", sidecar_path.display()))?;
    artifacts.push(ArtifactDescriptor {
        name: sidecar_name,
        kind: ArtifactKind::AssemblyListing,
        format: ArtifactFormat::RawBinary,
        target: target.clone(),
        lane,
    });
    Ok(())
}
