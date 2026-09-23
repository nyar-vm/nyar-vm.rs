use crate::nyar_backend_native::{NativeBinaryBackend, NativeExecutableKind, classify_native_executable};
use miette::Result;
use nyar::{PartitionBackendRequirement, TargetBackendFamily, backends::TargetCodeGenBackend};

use super::BundledBackendCompiler;
use crate::{DriverBackendInput, DriverCompileReport, DriverCompileRequest, DriverRunContract, bundled_backend_capability_descriptor};

fn native_run_contract(artifact_name: &str, entry_symbol: &str, executable: &[u8]) -> Result<DriverRunContract> {
    match classify_native_executable(executable).map_err(|error| miette::miette!("failed to classify native executable: {error}"))? {
        NativeExecutableKind::Pe => Ok(DriverRunContract {
            logical_entry: entry_symbol.to_string(),
            physical_entry: format!("{artifact_name}.exe"),
            invocation: "windows".to_string(),
            validate: format!("{artifact_name}.exe"),
        }),
        NativeExecutableKind::Elf | NativeExecutableKind::ElfShared => Ok(DriverRunContract {
            logical_entry: entry_symbol.to_string(),
            physical_entry: artifact_name.to_string(),
            invocation: "linux".to_string(),
            validate: artifact_name.to_string(),
        }),
    }
}

pub(super) struct NativeFamilyCompiler;

pub(super) fn supports_requirement(requirement: &PartitionBackendRequirement) -> bool {
    bundled_backend_capability_descriptor(TargetBackendFamily::Native).is_some_and(|descriptor| descriptor.supports_requirement(requirement))
}

impl BundledBackendCompiler for NativeFamilyCompiler {
    fn compile(&self, request: DriverCompileRequest<'_>) -> Result<DriverCompileReport> {
        let DriverBackendInput::Native(input) = request.input
        else {
            return Err(miette::miette!("`native` 家族请求必须携带 `NativeBinaryBackendInput`"));
        };
        let backend = NativeBinaryBackend::new();
        backend.validate(&input)?;
        let artifacts = backend.compile(input.clone(), request.options)?;
        Ok(DriverCompileReport {
            artifacts,
            entry_symbol: Some(input.entry_symbol.clone()),
            run_contracts: vec![native_run_contract(request.artifact_name, &input.entry_symbol, &input.executable)?],
        })
    }
}
