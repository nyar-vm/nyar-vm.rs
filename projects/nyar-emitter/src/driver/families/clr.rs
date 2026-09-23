use crate::nyar_backend_clr::{ClrBinaryBackend, write_dotnet_runtime_config};
use miette::Result;
use nyar::{
    PartitionBackendRequirement, TargetBackendFamily,
    backends::{TargetCodeGenBackend, clr::ClrImageKind},
};

use super::BundledBackendCompiler;
use crate::{DriverBackendInput, DriverCompileReport, DriverCompileRequest, DriverRunContract, bundled_backend_capability_descriptor};

pub(super) struct ClrFamilyCompiler;

pub(super) fn supports_requirement(requirement: &PartitionBackendRequirement) -> bool {
    bundled_backend_capability_descriptor(TargetBackendFamily::Clr).is_some_and(|descriptor| descriptor.supports_requirement(requirement))
}

impl BundledBackendCompiler for ClrFamilyCompiler {
    fn compile(&self, request: DriverCompileRequest<'_>) -> Result<DriverCompileReport> {
        let DriverBackendInput::Clr(input) = request.input
        else {
            return Err(miette::miette!("`CLR` 家族请求必须携带 `ClrBinaryBackendInput`"));
        };
        let has_entry = input.module.global_methods.iter().any(|method| method.is_entry_point)
            || input.module.types.iter().flat_map(|ty| ty.methods.iter()).any(|method| method.is_entry_point);
        let image_kind = input.image_kind.unwrap_or_else(|| ClrImageKind::infer(has_entry));
        let output_dir = input.output_dir.clone();
        let backend = ClrBinaryBackend::new();
        backend.validate(&input)?;
        let artifacts = backend.compile(input, request.options)?;
        if request.generate_runtime_config {
            write_dotnet_runtime_config(&output_dir, request.artifact_name)?;
        }

        Ok(DriverCompileReport {
            artifacts,
            entry_symbol: if image_kind == ClrImageKind::Executable { Some("Main".to_string()) } else { None },
            run_contracts: vec![DriverRunContract {
                logical_entry: "Main".to_string(),
                physical_entry: format!("{}.{}", request.artifact_name, image_kind.file_extension()),
                invocation: "dotnet".to_string(),
                validate: format!("dotnet exec {}.{}", request.artifact_name, image_kind.file_extension()),
            }],
        })
    }
}
