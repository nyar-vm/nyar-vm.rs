use nyar::{
    BackendInputKind, BinaryArch, BinaryFlavor, HostProjectionBoundary, Identifier, PartitionBackendRequirement, QualifiedName,
    ReferenceManagement, SuspendRuntimeFunctionArtifact, SuspendRuntimePayload, SuspendStateArtifact, TargetFamily, TargetLane,
    backends::CompilationOptions,
};
use nyar_emitter::{LoweredBackendInput, NyarVmBackendInput, testing::compile_lowered_backend_input};
use std_data::binary::nyar_ir::{NyarConstant, NyarFunction, NyarModuleData};
use tempfile::tempdir;

fn vm_options() -> CompilationOptions {
    CompilationOptions {
        target: nyar::BinaryTarget::new(TargetFamily::NyarVm, BinaryArch::Any, BinaryFlavor::ManagedClr),
        artifact_name: "demo".to_string(),
        emit_debug_symbols: false,
        optimize: false,
    }
}

fn vm_requirement(fragment: &str, target: nyar::BinaryTarget) -> PartitionBackendRequirement {
    PartitionBackendRequirement {
        backend_name: "nyar-vm".to_string(),
        interpreter: Identifier::new("nyar.vm"),
        fragment: Identifier::new(fragment),
        lane: TargetLane::Vm,
        input_kind: BackendInputKind::PeImage,
        target,
        host_boundary: HostProjectionBoundary::Vm,
        reference_management: ReferenceManagement::HostGc,
    }
}

#[test]
fn writes_suspend_runtime_sidecar_with_function_symbols() {
    let output_dir = tempdir().expect("temp dir");
    let symbol = QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("async_fn")]);
    let options = vm_options();
    let input = LoweredBackendInput::nyar_vm(NyarVmBackendInput {
        suspend_runtime: Some(SuspendRuntimePayload {
            functions: vec![SuspendRuntimeFunctionArtifact {
                symbol: symbol.clone(),
                entry_block_label: "entry".to_string(),
                frame_fields: Vec::new(),
                states: vec![SuspendStateArtifact {
                    state_id: 0,
                    effect: "Await".to_string(),
                    resume_case_key: 1,
                    frame_carrier: "this".to_string(),
                    spill_fields: Vec::new(),
                    suspend_block_label: "await_0".to_string(),
                    resume_block_label: "resume_0".to_string(),
                    resume_parameter_count: 0,
                    witness_bindings: Vec::new(),
                    continuation_index: None,
                }],
                continuations: Vec::new(),
            }],
        }),
        control_flow: None,
        nyar_module: None,
        output_dir: output_dir.path().to_path_buf(),
    });
    let report = compile_lowered_backend_input("demo", vm_requirement("suspend", options.target.clone()), input, false, &options)
        .expect("nyar-vm compile");

    let sidecar = output_dir.path().join("demo.suspend_runtime.json");
    assert!(sidecar.is_file(), "sidecar should exist");
    let body = std::fs::read_to_string(&sidecar).expect("read sidecar");
    assert!(body.contains("demo::async_fn"), "sidecar body: {body}");
    assert!(report.artifacts.artifacts.iter().any(|artifact| artifact.name.ends_with(".suspend_runtime.json")));
}

#[test]
fn emits_nyar_module_with_run_contract() {
    let output_dir = tempdir().expect("temp dir");
    let options = vm_options();
    let module = NyarModuleData {
        version: 1,
        name: "demo".to_string(),
        constants: vec![NyarConstant::Integer32(0)],
        functions: vec![NyarFunction { name: "main".to_string(), arity: 0, local_count: 0, code_offset: 0, code_length: 5 }],
        imports: Vec::new(),
        exports: Vec::new(),
        witness_entries: Vec::new(),
        code_bytes: vec![0x10, 0x00, 0x00, 0x00, 0x30],
        globals: Vec::new(),
        init_function_indices: Vec::new(),
    };
    let input = LoweredBackendInput::nyar_vm(NyarVmBackendInput {
        suspend_runtime: None,
        control_flow: None,
        nyar_module: Some(module),
        output_dir: output_dir.path().to_path_buf(),
    });
    let report =
        compile_lowered_backend_input("demo", vm_requirement("main", options.target.clone()), input, false, &options).expect("nyar-vm compile");

    assert!(output_dir.path().join("demo.nyar").is_file());
    assert_eq!(report.entry_symbol.as_deref(), Some("main"));
    let contract = report.run_contracts.into_iter().next().expect("run contract");
    assert_eq!(contract.logical_entry, "main");
    assert_eq!(contract.physical_entry, "demo.nyar");
    assert_eq!(contract.invocation, "nyar-vm");
}
