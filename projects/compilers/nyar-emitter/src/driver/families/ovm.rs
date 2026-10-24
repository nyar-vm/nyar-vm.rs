use std::fs;

use crate::nyar_backend_vm::emit_nyar_module;
use miette::{IntoDiagnostic, Result, WrapErr, miette};
use nyar::{ArtifactDescriptor, ArtifactFormat, ArtifactKind, ArtifactSet, PartitionBackendRequirement, TargetBackendFamily, TargetLane};
use odin_bytecode::{NyarExportKind, NyarModuleData};

use super::BundledBackendCompiler;
use crate::{DriverBackendInput, DriverCompileReport, DriverCompileRequest, DriverRunContract, bundled_backend_capability_descriptor};

pub(super) struct OvmFamilyCompiler;

pub(super) fn supports_requirement(requirement: &PartitionBackendRequirement) -> bool {
    bundled_backend_capability_descriptor(TargetBackendFamily::Ovm).is_some_and(|descriptor| descriptor.supports_requirement(requirement))
}

impl BundledBackendCompiler for OvmFamilyCompiler {
    fn compile(&self, request: DriverCompileRequest<'_>) -> Result<DriverCompileReport> {
        let DriverBackendInput::Ovm(input) = request.input
        else {
            return Err(miette!("`ovm` 家族请求必须携带 `OvmBackendInput`"));
        };

        let mut artifacts = ArtifactSet::default();
        let mut entry_symbol = None;
        let mut run_contracts = Vec::new();

        fs::create_dir_all(&input.output_dir)
            .into_diagnostic()
            .wrap_err_with(|| format!("创建输出目录失败：{}", input.output_dir.display()))?;

        if let Some(module) = input.nyar_module.as_ref() {
            let entries = resolve_entry_symbols(module, &input.library_public_exports)?;
            let nyar_name = format!("{}.nyar", request.artifact_name);
            let nyar_path = input.output_dir.join(&nyar_name);
            emit_nyar_module(module, &nyar_path)?;
            artifacts.push(ArtifactDescriptor {
                name: nyar_name,
                path: format!("{}.nyar", request.artifact_name),
                kind: ArtifactKind::Executable,
                format: ArtifactFormat::RawBinary,
                target: request.options.target.clone(),
                lane: TargetLane::Vm,
            });

            entry_symbol = entries.first().cloned();
            for entry in entries {
                run_contracts.push(ovm_run_contract(request.artifact_name, &entry));
            }
        }

        Ok(DriverCompileReport { artifacts, entry_symbol, run_contracts })
    }
}

#[doc(hidden)]
pub fn resolve_entry_symbols(module: &NyarModuleData, library_public_exports: &[String]) -> Result<Vec<String>> {
    let entries = if library_public_exports.is_empty() { vec!["main".to_owned()] } else { library_public_exports.to_vec() };
    for entry in &entries {
        let matches: Vec<_> =
            module.exports.iter().filter(|export| export.kind == NyarExportKind::Function && export.symbol_name == *entry).collect();
        if matches.len() != 1 {
            return Err(miette!("Nyar 入口 `{entry}` 必须对应唯一明确函数导出"));
        }
        let index = usize::try_from(matches[0].function_index).map_err(|_| miette!("Nyar 入口 `{entry}` 的函数索引无效"))?;
        if module.functions.get(index).is_none() {
            return Err(miette!("Nyar 入口 `{entry}` 的函数索引越界"));
        }
    }
    Ok(entries)
}

fn ovm_run_contract(artifact_name: &str, entry: &str) -> DriverRunContract {
    DriverRunContract {
        logical_entry: entry.to_string(),
        physical_entry: format!("{artifact_name}.nyar"),
        invocation: "ovm".to_string(),
        validate: format!("ovm run {artifact_name}.nyar --entry {entry}"),
    }
}
