//! singleton 与 impl 方法的 MIR lowering。

use super::{
    callable_types::{CallableReturnTypes, impl_type_arguments_from_generics},
    function_lowering::lower_function_semantic,
    hir_identity::impl_owner_type_name,
    model::*,
    value_semantics::AggregateLayoutPlan,
};
use crate::types::hir::{HirModule, ValkyrieType};
use std::collections::BTreeMap;

pub(super) fn lower_singleton_method_functions(
    module: &HirModule,
    aggregate_layouts: &mut AggregateLayoutPlan,
    singleton_accessor_instances: &BTreeMap<String, nyar_types::ItemInstanceId>,
    effectful_resume_map: &BTreeMap<nyar_types::ItemId, ValkyrieType>,
    callable_returns: &CallableReturnTypes,
    diagnostics: &mut Vec<MirDiagnostic>,
) -> Vec<MirFunction> {
    let mut functions = Vec::new();
    for singleton in &module.singletons {
        for method in singleton.methods.iter().filter(|method| !method.is_abstract) {
            let mut mir_function = lower_function_semantic(
                module,
                method,
                aggregate_layouts,
                singleton_accessor_instances,
                effectful_resume_map,
                callable_returns,
                Some(ValkyrieType::Named(singleton.name.clone())),
                impl_type_arguments_from_generics(&singleton.generics),
                diagnostics,
            );
            mir_function.display_symbol = format!("{}.{}", singleton.name, method.name);
            functions.push(mir_function);
        }
        if let Some(constructor) = &singleton.constructor {
            let mut mir_function = lower_function_semantic(
                module,
                constructor,
                aggregate_layouts,
                singleton_accessor_instances,
                effectful_resume_map,
                callable_returns,
                Some(ValkyrieType::Named(singleton.name.clone())),
                impl_type_arguments_from_generics(&singleton.generics),
                diagnostics,
            );
            mir_function.display_symbol = format!("{}.{}", singleton.name, constructor.name);
            functions.push(mir_function);
        }
        if let Some(finalizer) = &singleton.finalizer {
            let mut mir_function = lower_function_semantic(
                module,
                finalizer,
                aggregate_layouts,
                singleton_accessor_instances,
                effectful_resume_map,
                callable_returns,
                Some(ValkyrieType::Named(singleton.name.clone())),
                impl_type_arguments_from_generics(&singleton.generics),
                diagnostics,
            );
            mir_function.display_symbol = format!("{}.{}", singleton.name, finalizer.name);
            functions.push(mir_function);
        }
    }
    functions
}

pub(super) fn lower_impl_method_functions(
    module: &HirModule,
    aggregate_layouts: &mut AggregateLayoutPlan,
    singleton_accessor_instances: &BTreeMap<String, nyar_types::ItemInstanceId>,
    effectful_resume_map: &BTreeMap<nyar_types::ItemId, ValkyrieType>,
    callable_returns: &CallableReturnTypes,
    diagnostics: &mut Vec<MirDiagnostic>,
) -> Vec<MirFunction> {
    let mut functions = Vec::new();
    for impl_block in &module.impls {
        let Some(type_name) = impl_owner_type_name(module, &impl_block.target) else {
            continue;
        };
        for method in impl_block.methods.iter().filter(|method| !method.is_abstract) {
            let mut mir_function = lower_function_semantic(
                module,
                method,
                aggregate_layouts,
                singleton_accessor_instances,
                effectful_resume_map,
                callable_returns,
                Some(impl_block.target.clone()),
                impl_type_arguments_from_generics(&impl_block.generics),
                diagnostics,
            );
            mir_function.display_symbol = format!("{type_name}.{}", method.name);
            functions.push(mir_function);
        }
    }
    for item in &module.structs {
        for method in item.methods.iter().filter(|method| !method.is_abstract) {
            let mut mir_function = lower_function_semantic(
                module,
                method,
                aggregate_layouts,
                singleton_accessor_instances,
                effectful_resume_map,
                callable_returns,
                Some(ValkyrieType::Named(item.name.clone())),
                impl_type_arguments_from_generics(&item.generics),
                diagnostics,
            );
            mir_function.display_symbol = format!("{}.{}", item.name, method.name);
            functions.push(mir_function);
        }
    }
    for trait_def in &module.traits {
        for method in trait_def.default_methods.iter().filter(|method| !method.is_abstract) {
            let mut mir_function = lower_function_semantic(
                module,
                method,
                aggregate_layouts,
                singleton_accessor_instances,
                effectful_resume_map,
                callable_returns,
                Some(ValkyrieType::Named(trait_def.name.clone())),
                impl_type_arguments_from_generics(&trait_def.generics),
                diagnostics,
            );
            mir_function.display_symbol = format!("{}.{}", trait_def.name, method.name);
            functions.push(mir_function);
        }
    }
    functions
}
