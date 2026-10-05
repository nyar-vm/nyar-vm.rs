use std::fs;

use crate::nyar_backend_vm::emit_nyar_module;
use miette::{IntoDiagnostic, Result, WrapErr, miette};
use nyar::{ArtifactDescriptor, ArtifactFormat, ArtifactKind, ArtifactSet, PartitionBackendRequirement, TargetBackendFamily, TargetLane};
use nyar_bytecode::{NyarExportKind, NyarModuleData};

use super::BundledBackendCompiler;
use crate::{DriverBackendInput, DriverCompileReport, DriverCompileRequest, DriverRunContract, bundled_backend_capability_descriptor};

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
                run_contracts.push(nyar_vm_run_contract(request.artifact_name, &entry));
            }
        }

        Ok(DriverCompileReport { artifacts, entry_symbol, run_contracts })
    }
}

fn resolve_entry_symbols(module: &NyarModuleData, library_public_exports: &[String]) -> Result<Vec<String>> {
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

fn nyar_vm_run_contract(artifact_name: &str, entry: &str) -> DriverRunContract {
    DriverRunContract {
        logical_entry: entry.to_string(),
        physical_entry: format!("{artifact_name}.nyar"),
        invocation: "nyar-vm".to_string(),
        validate: format!("nyar-vm run {artifact_name}.nyar --entry {entry}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nyar_bytecode::{NyarExport, NyarFunction};

    fn module() -> NyarModuleData {
        NyarModuleData {
            version: 2,
            name: "test".to_owned(),
            constants: Vec::new(),
            functions: vec![NyarFunction { name: "owner::main".to_owned(), arity: 0, local_count: 0, code_offset: 0, code_length: 1 }],
            imports: Vec::new(),
            exports: vec![NyarExport { kind: NyarExportKind::Function, symbol_name: "owner::main".to_owned(), function_index: 0 }],
            witness_entries: Vec::new(),
            code_bytes: vec![0x05],
            globals: Vec::new(),
            init_function_indices: Vec::new(),
            layouts: Vec::new(),
        }
    }

    #[test]
    fn entry_contract_rejects_suffix_and_unexported_function() {
        let mut module = module();
        assert!(resolve_entry_symbols(&module, &[]).is_err());
        assert_eq!(resolve_entry_symbols(&module, &["owner::main".to_owned()]).unwrap(), vec!["owner::main"]);
        module.exports.clear();
        assert!(resolve_entry_symbols(&module, &["owner::main".to_owned()]).is_err());
    }

    #[test]
    fn entry_contract_rejects_ambiguous_and_invalid_exports() {
        let mut module = module();
        module.exports.push(module.exports[0].clone());
        assert!(resolve_entry_symbols(&module, &["owner::main".to_owned()]).is_err());
        module.exports.pop();
        module.exports[0].function_index = -1;
        assert!(resolve_entry_symbols(&module, &["owner::main".to_owned()]).is_err());
        module.exports[0].function_index = 1;
        assert!(resolve_entry_symbols(&module, &["owner::main".to_owned()]).is_err());
    }

    #[test]
    fn entry_contract_preserves_all_declared_library_exports() {
        let mut module = module();
        module.exports.push(NyarExport { kind: NyarExportKind::Function, symbol_name: "second".to_owned(), function_index: 0 });
        let entries = vec!["second".to_owned(), "owner::main".to_owned()];
        assert_eq!(resolve_entry_symbols(&module, &entries).unwrap(), entries);
    }
}
