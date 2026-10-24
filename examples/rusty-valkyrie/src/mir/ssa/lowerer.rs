//! `MirLowerer` 阶段编排：`HirModule` 到 `MirModule` 的入口。

use super::{
    callable_types::collect_callable_return_types,
    contracts::{
        HostRuntimeCallables, collect_external_call_contracts, collect_host_runtime_callables, collect_semantic_fragments,
        collect_surface_contracts,
    },
    function_lowering::lower_function_semantic,
    generic_call_instantiation,
    method_lowering::{lower_impl_method_functions, lower_singleton_method_functions},
    model::*,
    struct_facts::collect_field_declarations,
    type_analysis::type_identity_table,
    value_semantics,
};
use crate::types::hir::HirModule;
use std::collections::BTreeMap;

/// Semantic MIR lowering 阶段入口：把 `HirModule` 编排为 `MirModule`。
pub struct MirLowerer;

impl MirLowerer {
    /// Lowers HIR to semantic MIR (PerformEffect intact, no state-machine rewrite).
    ///
    /// 单模块路径：仅覆盖本模块的分派表；跨包闭包须用
    /// [`Self::lower_module_semantic_with_host`]。
    pub fn lower_module_semantic(module: &HirModule) -> MirModule {
        let dispatch = generic_call_instantiation::collect_dispatch_entries(&[module]);
        Self::lower_module_semantic_with_host(module, None, true, &dispatch)
    }

    /// 跨包闭包入口：携带程序级 host runtime 身份，供外部调用合同合并使用。
    ///
    /// 分派表必须覆盖完整源码闭包（由调用方预先拼好再传入），不再在 MIR 层做后置泛型物化。
    pub fn lower_module_semantic_with_host(
        module: &HirModule,
        shared_host: Option<&HostRuntimeCallables>,
        attach_host_contracts: bool,
        dispatch: &[generic_call_instantiation::MirDispatchEntry],
    ) -> MirModule {
        let singleton_accessor_instances = crate::mir::singleton_accessor_instance_map(module);
        let mut aggregate_layouts = value_semantics::compute_aggregate_layout_plan(module);
        let mut diagnostics = Vec::new();
        let structs = collect_field_declarations(module, &mut diagnostics);
        let nominal_offset = u32::try_from(structs.len()).expect("aggregate nominal identity overflow");
        let (sum_types, flags_types) = crate::hir::lowering::compute_nominal_declarations(module, nominal_offset, &mut diagnostics);
        value_semantics::ensure_unite_layouts_for_sums(&mut aggregate_layouts, &sum_types);
        let imports: Vec<_> = module.imports.iter().map(|import| import.path.to_string()).collect();
        let mut external_calls = collect_external_call_contracts(module);
        let (host_runtime_callables, host_contracts) = if let Some(shared) = shared_host {
            (shared.by_name.clone(), if attach_host_contracts { shared.contracts.clone() } else { Vec::new() })
        }
        else {
            let host_declaration_base = crate::hir::lowering::max_hir_declaration_index(module)
                .or_else(|| external_calls.iter().filter_map(|contract| contract.declaration).map(|declaration| declaration.index()).max())
                .map(|index| index.saturating_add(1))
                .unwrap_or(0);
            let host_instance_base = crate::hir::lowering::max_hir_callable_instance_index(module)
                .or_else(|| external_calls.iter().filter_map(|contract| contract.instance).map(|instance| instance.index()).max())
                .map(|index| index.saturating_add(1))
                .unwrap_or(0);
            let host_runtime = collect_host_runtime_callables(host_declaration_base, host_instance_base);
            (host_runtime.by_name, host_runtime.contracts)
        };
        external_calls.extend(host_contracts);
        let (exports, entries) = collect_surface_contracts(module);
        let callable_returns = collect_callable_return_types(module);
        let mut functions = Vec::new();
        for function in module.functions.iter().filter(|function| !function.is_abstract) {
            functions.push(lower_function_semantic(
                module,
                function,
                &mut aggregate_layouts,
                &singleton_accessor_instances,
                &callable_returns,
                dispatch,
                None,
                Vec::new(),
                &host_runtime_callables,
                &mut diagnostics,
            ));
        }
        functions.extend(lower_singleton_method_functions(
            module,
            &mut aggregate_layouts,
            &singleton_accessor_instances,
            &callable_returns,
            dispatch,
            &host_runtime_callables,
            &mut diagnostics,
        ));
        functions.extend(lower_impl_method_functions(
            module,
            &mut aggregate_layouts,
            &singleton_accessor_instances,
            &callable_returns,
            dispatch,
            &host_runtime_callables,
            &mut diagnostics,
        ));
        // 编译器内建宿主合同只在函数真正发射 begin/end phase 调用时进入外部调用闭包。
        // 全局注册表存在本身不构成依赖，否则每个模块都会凭空多出未使用的导入。
        let used_callable_instances = functions
            .iter()
            .flat_map(|function| function.blocks.iter())
            .flat_map(|block| block.instructions.iter())
            .filter_map(|instruction| match &instruction.kind {
                MirOperation::Call { callee: MirOperand::Callable(instance), .. } => Some(*instance),
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>();
        external_calls.retain(|contract| match contract.instance {
            Some(instance) if host_runtime_callables.values().any(|host| *host == instance) => used_callable_instances.contains(&instance),
            _ => true,
        });
        // MirFunction 不再携带 per-function diagnostics。
        // TypeId 只消费已冻结的 Nominal{ItemId} / 其他结构化形状；禁止短名 Named 别名收束。
        let type_identities = type_identity_table(&functions, &external_calls, &structs, &sum_types);
        let mut callable_symbols = BTreeMap::new();
        for function in &functions {
            if let Some(instance) = function.instance {
                super::model::register_callable_symbol_entry(&mut callable_symbols, instance, function.display_symbol.clone())
                    .expect("duplicate callable symbol during lowering");
            }
        }
        for contract in &external_calls {
            if let Some(instance) = contract.instance {
                let symbol = crate::symbols::stable_symbol_key(&contract.symbol);
                super::model::register_callable_symbol_entry(&mut callable_symbols, instance, symbol)
                    .expect("duplicate callable symbol during lowering");
            }
        }
        let accessor_instances = crate::mir::collect_singleton_accessor_contracts(module);
        for (instance, symbol) in accessor_instances {
            super::model::register_callable_symbol_entry(&mut callable_symbols, instance, symbol)
                .expect("duplicate singleton accessor symbol during lowering");
        }
        let semantic_fragments = collect_semantic_fragments(module, &exports, &entries);
        let result = MirModule {
            name: module.name.to_string(),
            functions,
            structs,
            imports,
            external_calls,
            exports,
            entries,
            callable_symbols,
            type_identities,
            aggregate_layouts,
            sum_types: sum_types.to_vec(),
            flags_types,
            singleton_instances: crate::mir::collect_singleton_instance_plans(module),
            semantic_fragments,
            diagnostics,
        };
        result
    }
}
