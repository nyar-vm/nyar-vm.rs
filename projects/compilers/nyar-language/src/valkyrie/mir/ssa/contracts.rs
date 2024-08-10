//! 模块级外部调用、导出与入口合同收集，以及语义片段构造。

use super::model::*;
use crate::types::{
    Identifier, NamePath,
    hir::{HirModule, ValkyrieType, parse_export_spec_from_annotations, resolve_attribute_id},
};
use nyar::{CapabilityTag, RewriteTheory, SemanticFragment};
use nyar_types::{ExternalImportLink, ItemInstanceId, builtin_attribute};
use std::collections::BTreeMap;

/// 编译器内建的 workload phase 宿主调用；实例身份在 lowering 前冻结。
pub(crate) struct HostRuntimeCallables {
    pub contracts: Vec<MirExternalCallContract>,
    pub by_name: BTreeMap<String, ItemInstanceId>,
}

/// 跨源码组程序闭包：在全部 HIR 实例池之后分配唯一 host runtime 身份。
pub(crate) fn allocate_program_host_runtime(modules: &[&HirModule]) -> HostRuntimeCallables {
    let declaration_base = modules
        .iter()
        .filter_map(|module| crate::valkyrie::hir::lowering::max_hir_declaration_index(module))
        .max()
        .map(|index| index.saturating_add(1))
        .unwrap_or(0);
    let instance_base = modules
        .iter()
        .filter_map(|module| crate::valkyrie::hir::lowering::max_hir_callable_instance_index(module))
        .max()
        .map(|index| index.saturating_add(1))
        .unwrap_or(0);
    collect_host_runtime_callables(declaration_base, instance_base)
}

pub(crate) fn collect_host_runtime_callables(declaration_base: u32, instance_base: u32) -> HostRuntimeCallables {
    let mut contracts = Vec::new();
    let mut by_name = BTreeMap::new();
    for (offset, name) in [(0u32, "begin_phase"), (1u32, "end_phase")] {
        let declaration = nyar_types::ItemId::from_index(declaration_base + offset).expect("host runtime declaration");
        let instance = ItemInstanceId::from_index(instance_base + offset).expect("host runtime instance");
        by_name.insert(name.to_string(), instance);
        contracts.push(MirExternalCallContract {
            declaration: Some(declaration),
            instance: Some(instance),
            symbol: NamePath::new(vec![Identifier::new(name)]),
            link: ExternalImportLink::host(None, vec![name.to_string()]),
            parameter_types: vec![ValkyrieType::Utf8],
            return_type: ValkyrieType::Unit,
        });
    }
    HostRuntimeCallables { contracts, by_name }
}

pub(super) fn collect_external_call_contracts(module: &HirModule) -> Vec<MirExternalCallContract> {
    fn collect_abstract_functions(module: &HirModule, out: &mut Vec<MirExternalCallContract>) {
        out.extend(
            module
                .functions
                .iter()
                .filter(|function| {
                    function.is_abstract && crate::valkyrie::backend_contract::interop::function_interop_contract(function).is_some()
                })
                .filter_map(|function| {
                    let link = crate::valkyrie::backend_contract::interop::function_interop_contract(function)?;
                    Some(MirExternalCallContract {
                        declaration: function.declaration,
                        instance: function.instance,
                        symbol: crate::valkyrie::symbols::stable_hir_function_name_path(&module.name, function),
                        link,
                        parameter_types: function.params.iter().map(|parameter| parameter.ty.clone()).collect(),
                        return_type: function.return_type.clone(),
                    })
                }),
        );
        for submodule in &module.submodules {
            collect_abstract_functions(submodule, out);
        }
    }
    let mut contracts = Vec::new();
    collect_abstract_functions(module, &mut contracts);
    contracts.extend(
        module
            .imported_semantic_exports
            .iter()
            .flat_map(|export| {
                export.functions.iter().filter_map(move |function| {
                    let link = crate::valkyrie::backend_contract::interop::function_interop_contract(function)?;
                    let symbol = crate::valkyrie::symbols::stable_hir_function_name_path(&export.module, function);
                    Some(MirExternalCallContract {
                        declaration: function.declaration,
                        instance: function.instance,
                        symbol,
                        link,
                        parameter_types: function.params.iter().map(|parameter| parameter.ty.clone()).collect(),
                        return_type: function.return_type.clone(),
                    })
                })
            })
            .collect::<Vec<_>>(),
    );
    contracts
}

pub(super) fn collect_surface_contracts(module: &HirModule) -> (Vec<MirExportContract>, Vec<MirEntryContract>) {
    fn visit(module: &HirModule, exports: &mut Vec<MirExportContract>, entries: &mut Vec<MirEntryContract>) {
        for function in &module.functions {
            let symbol = crate::valkyrie::symbols::stable_hir_function_name_path(&module.name, function);
            if let Some(spec) = parse_export_spec_from_annotations(&function.annotations) {
                exports.push(MirExportContract {
                    instance: function.instance,
                    symbol: symbol.clone(),
                    exported_name: spec.resolve_exported_name(&function.name),
                    partition: spec.primary_partition(),
                });
            }
            if function.annotations.iter().any(|attribute| resolve_attribute_id(attribute) == Some(builtin_attribute::main())) {
                entries.push(MirEntryContract { instance: function.instance, symbol });
            }
        }
        for submodule in &module.submodules {
            visit(submodule, exports, entries);
        }
    }

    let mut exports = Vec::new();
    let mut entries = Vec::new();
    visit(module, &mut exports, &mut entries);
    (exports, entries)
}

pub(super) fn collect_semantic_fragments(
    module: &HirModule,
    exports: &[MirExportContract],
    entries: &[MirEntryContract],
) -> Vec<SemanticFragment> {
    let mut fragments = BTreeMap::<Identifier, SemanticFragment>::new();
    for function in &module.functions {
        let Some(instance) = function.instance
        else {
            continue;
        };
        let export = exports.iter().find(|export| export.instance == Some(instance));
        let is_entry = entries.iter().any(|entry| entry.instance == Some(instance));
        if export.is_none() && !is_entry {
            continue;
        }
        let id = if let Some(export) = export {
            Identifier::new(&format!("export__{}", export.partition.replace('.', "_")))
        }
        else if entries.len() > 1 {
            let name =
                function.name.as_str().chars().map(|ch| if ch.is_ascii_alphanumeric() || ch == '_' { ch } else { '_' }).collect::<String>();
            Identifier::new(&format!("main_{name}"))
        }
        else {
            Identifier::new("functions")
        };
        let fragment = fragments.entry(id.clone()).or_insert_with(|| SemanticFragment {
            id,
            exported_operations: Vec::new(),
            required_capabilities: Vec::new(),
            reference_management_hint: None,
            entry_operation: None,
            rewrite_theory: RewriteTheory::default(),
            wasm_export_names: BTreeMap::new(),
        });
        if !fragment.exported_operations.contains(&instance) {
            fragment.exported_operations.push(instance);
        }
        if is_entry {
            fragment.entry_operation = Some(instance);
        }
        if let Some(export) = export {
            fragment.wasm_export_names.insert(instance, export.exported_name.clone());
        }
        if crate::valkyrie::hir::control_flow_validation::function_needs_suspend_fragment(&function.body)
            && !fragment.required_capabilities.iter().any(|capability| capability.as_str() == "suspend")
        {
            fragment.required_capabilities.push(CapabilityTag::new("suspend"));
        }
    }
    if fragments.is_empty() {
        fragments.insert(
            Identifier::new("functions"),
            SemanticFragment {
                id: Identifier::new("functions"),
                exported_operations: Vec::new(),
                required_capabilities: Vec::new(),
                reference_management_hint: None,
                entry_operation: None,
                rewrite_theory: RewriteTheory::default(),
                wasm_export_names: BTreeMap::new(),
            },
        );
    }
    fragments.into_values().collect()
}
