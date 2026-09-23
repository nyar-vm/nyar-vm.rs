use std::fs;

use crate::nyar_backend_vm::emit_nyar_module;
use miette::{IntoDiagnostic, Result, WrapErr, miette};
use nyar::{ArtifactDescriptor, ArtifactFormat, ArtifactKind, ArtifactSet, PartitionBackendRequirement, TargetBackendFamily, TargetLane};
use std_data::binary::nyar_ir::{NyarExportKind, NyarModuleData};

use super::BundledBackendCompiler;
use crate::{
    DriverBackendInput, DriverCompileReport, DriverCompileRequest, DriverRunContract,
    artifacts::suspend_sidecar::{serialize_control_flow_payload, serialize_suspend_runtime_payload},
    bundled_backend_capability_descriptor,
};

pub(super) struct NyarVmFamilyCompiler;

pub(super) fn supports_requirement(requirement: &PartitionBackendRequirement) -> bool {
    bundled_backend_capability_descriptor(TargetBackendFamily::NyarVm).is_some_and(|descriptor| descriptor.supports_requirement(requirement))
}

impl BundledBackendCompiler for NyarVmFamilyCompiler {
    fn compile(&self, request: DriverCompileRequest<'_>) -> Result<DriverCompileReport> {
        let DriverBackendInput::NyarVm(input) = request.input
        else {
            return Err(miette!("`nyar-vm` 家族请求必须携带 `NyarVmBackendInput`"));
        };

        let mut artifacts = ArtifactSet::default();
        let mut entry_symbol = None;
        let mut run_contracts = Vec::new();

        fs::create_dir_all(&input.output_dir)
            .into_diagnostic()
            .wrap_err_with(|| format!("创建输出目录失败：{}", input.output_dir.display()))?;

        if let Some(payload) = input.suspend_runtime.as_ref() {
            let sidecar_name = format!("{}.suspend_runtime.json", request.artifact_name);
            let sidecar_path = input.output_dir.join(&sidecar_name);
            let body = serialize_suspend_runtime_payload(payload);
            fs::write(&sidecar_path, body)
                .into_diagnostic()
                .wrap_err_with(|| format!("写入 suspend_runtime sidecar 失败：{}", sidecar_path.display()))?;
            artifacts.push(ArtifactDescriptor {
                name: sidecar_name,
                kind: ArtifactKind::AssemblyListing,
                format: ArtifactFormat::RawBinary,
                target: request.options.target.clone(),
                lane: TargetLane::Vm,
            });
        }

        if let Some(payload) = input.control_flow.as_ref() {
            let sidecar_name = format!("{}.control_flow.json", request.artifact_name);
            let sidecar_path = input.output_dir.join(&sidecar_name);
            let body = serialize_control_flow_payload(payload);
            fs::write(&sidecar_path, body)
                .into_diagnostic()
                .wrap_err_with(|| format!("写入 control_flow sidecar 失败：{}", sidecar_path.display()))?;
            artifacts.push(ArtifactDescriptor {
                name: sidecar_name,
                kind: ArtifactKind::AssemblyListing,
                format: ArtifactFormat::RawBinary,
                target: request.options.target.clone(),
                lane: TargetLane::Vm,
            });
        }

        if let Some(module) = input.nyar_module.as_ref() {
            let nyar_name = format!("{}.nyar", request.artifact_name);
            let nyar_path = input.output_dir.join(&nyar_name);
            emit_nyar_module(module, &nyar_path)?;
            artifacts.push(ArtifactDescriptor {
                name: nyar_name,
                kind: ArtifactKind::Executable,
                format: ArtifactFormat::RawBinary,
                target: request.options.target.clone(),
                lane: TargetLane::Vm,
            });

            let entry = resolve_entry_symbol(module);
            entry_symbol = Some(entry.clone());
            run_contracts.push(nyar_vm_run_contract(request.artifact_name, &entry));
        }

        Ok(DriverCompileReport { artifacts, entry_symbol, run_contracts })
    }
}

fn resolve_entry_symbol(module: &NyarModuleData) -> String {
    module
        .exports
        .iter()
        .find(|export| export.kind == NyarExportKind::Function)
        .map(|export| export.symbol_name.clone())
        .or_else(|| module.functions.first().map(|function| function.name.clone()))
        .unwrap_or_else(|| "main".to_string())
}

fn nyar_vm_run_contract(artifact_name: &str, entry: &str) -> DriverRunContract {
    DriverRunContract {
        logical_entry: entry.to_string(),
        physical_entry: format!("{artifact_name}.nyar"),
        invocation: "nyar-vm".to_string(),
        validate: format!("nyar-vm run {artifact_name}.nyar --entry {entry}"),
    }
}
