use crate::nyar_backend_jvm::{JvmBinaryBackend, internal_name_to_binary_name};
use miette::Result;
use nyar::{PartitionBackendRequirement, TargetBackendFamily, backends::TargetCodeGenBackend};

use super::BundledBackendCompiler;
use crate::{DriverBackendInput, DriverCompileReport, DriverCompileRequest, DriverRunContract, bundled_backend_capability_descriptor};

/// `JVM` 后端家族编译器。
pub(super) struct JvmFamilyCompiler;

pub(super) fn supports_requirement(requirement: &PartitionBackendRequirement) -> bool {
    bundled_backend_capability_descriptor(TargetBackendFamily::Jvm).is_some_and(|descriptor| descriptor.supports_requirement(requirement))
}

impl BundledBackendCompiler for JvmFamilyCompiler {
    fn compile(&self, request: DriverCompileRequest<'_>) -> Result<DriverCompileReport> {
        let DriverBackendInput::Jvm(input) = request.input
        else {
            return Err(miette::miette!("`JVM` 家族请求必须携带 `JvmBinaryBackendInput`"));
        };
        let entry_class = internal_name_to_binary_name(&input.class_file.internal_name);
        let backend = JvmBinaryBackend::new();
        backend.validate(&input)?;
        let artifacts = backend.compile(input, request.options)?;
        Ok(DriverCompileReport {
            artifacts,
            entry_symbol: Some(entry_class.clone()),
            run_contracts: vec![jvm_run_contract(request.artifact_name, &entry_class)],
        })
    }
}

/// 构造 `java -jar` 运行契约。
fn jvm_run_contract(artifact_name: &str, entry_class: &str) -> DriverRunContract {
    DriverRunContract {
        logical_entry: entry_class.to_string(),
        physical_entry: format!("{}.jar", artifact_name),
        invocation: "java".to_string(),
        validate: format!("java -jar {}.jar", artifact_name),
    }
}
