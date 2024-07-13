//! `MirLowerer` 阶段编排：`HirModule` 到 `MirModule` 的入口。

use super::{
    callable_types::collect_callable_return_types,
    contracts::{collect_external_call_contracts, collect_semantic_fragments, collect_surface_contracts},
    function_lowering::lower_function_semantic,
    generic_call_instantiation,
    hir_identity::collect_effectful_resume_map,
    method_lowering::{lower_impl_method_functions, lower_singleton_method_functions},
    model::*,
    struct_facts::{
        collect_field_declarations, collect_struct_field_layouts, collect_struct_is_value_type, collect_struct_parent_index,
        merge_imported_struct_field_layouts, merge_imported_struct_is_value_type, merge_imported_struct_parent_index,
    },
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
    /// 单模块路径：monomorph `ItemInstanceId` 从本模块已有实例之后分配。
    /// 跨包闭包须用 [`Self::lower_module_semantic_with_closure_context`]。
    pub fn lower_module_semantic(module: &HirModule) -> MirModule {
        let mut next_mono = None;
        let dispatch = generic_call_instantiation::collect_dispatch_entries(&[module]);
        Self::lower_module_semantic_with_closure_context(module, &mut next_mono, &[], &dispatch)
    }

    /// 跨包闭包：`mono_counter` 为全局实例游标；`None` 时回退为本模块 `max+1`。
    ///
    /// 单模块入口：种子池与分派表仅覆盖本模块；依赖模块的泛型种子须用
    /// [`Self::lower_module_semantic_with_closure_context`]。
    pub fn lower_module_semantic_with_mono_counter(module: &HirModule, mono_counter: &mut Option<u32>) -> MirModule {
        let dispatch = generic_call_instantiation::collect_dispatch_entries(&[module]);
        Self::lower_module_semantic_with_closure_context(module, mono_counter, &[], &dispatch)
    }

    /// 跨包闭包上下文：`seed_pool` 提供已 lowering 的依赖模块函数，
    /// `dispatch` 提供完整源码闭包冻结的成员分派表。
    pub fn lower_module_semantic_with_closure_context(
        module: &HirModule,
        mono_counter: &mut Option<u32>,
        seed_pool: &[MirFunction],
        dispatch: &[generic_call_instantiation::MirDispatchEntry],
    ) -> MirModule {
        let mut struct_field_layouts = collect_struct_field_layouts(&module.structs);
        crate::valkyrie::mir::merge_singleton_field_layouts(module, &mut struct_field_layouts);
        merge_imported_struct_field_layouts(module, &mut struct_field_layouts);
        let singleton_accessors = crate::valkyrie::mir::singleton_accessor_map(module);
        let mut struct_parent_index = collect_struct_parent_index(&module.structs);
        merge_imported_struct_parent_index(module, &mut struct_parent_index);
        let mut struct_is_value_type = collect_struct_is_value_type(&module.structs);
        merge_imported_struct_is_value_type(module, &mut struct_is_value_type);
        let mut aggregate_layouts = value_semantics::compute_aggregate_layout_plan(module);
        let mut diagnostics = Vec::new();
        let structs = collect_field_declarations(module);
        let nominal_offset = u32::try_from(structs.len()).expect("aggregate nominal identity overflow");
        let (mut sum_types, flags_types) = crate::valkyrie::hir::lowering::compute_nominal_declarations(module, nominal_offset);
        value_semantics::ensure_unite_layouts_for_sums(&mut aggregate_layouts, &sum_types);
        let effectful_resume_map = collect_effectful_resume_map(module);
        let mut next_field = structs.iter().map(|declaration| declaration.fields.len()).sum::<usize>();
        for sum in &mut sum_types {
            for variant in &mut sum.variants {
                for field in &mut variant.fields {
                    field.id = nyar_types::FieldId::from_index(next_field as u32).expect("field identity overflow");
                    next_field += 1;
                }
            }
        }
        let imports: Vec<_> = module.imports.iter().map(|import| import.path.to_string()).collect();
        let external_calls = collect_external_call_contracts(module);
        let (exports, entries) = collect_surface_contracts(module);
        let callable_returns = collect_callable_return_types(module);
        let mut functions = Vec::new();
        for function in module.functions.iter().filter(|function| !function.is_abstract) {
            functions.push(lower_function_semantic(
                module,
                function,
                &struct_field_layouts,
                &struct_parent_index,
                &struct_is_value_type,
                &mut aggregate_layouts,
                &singleton_accessors,
                &effectful_resume_map,
                &callable_returns,
                None,
                Vec::new(),
                &mut diagnostics,
            ));
        }
        functions.extend(lower_singleton_method_functions(
            module,
            &struct_field_layouts,
            &struct_parent_index,
            &struct_is_value_type,
            &mut aggregate_layouts,
            &singleton_accessors,
            &effectful_resume_map,
            &callable_returns,
            &mut diagnostics,
        ));
        functions.extend(lower_impl_method_functions(
            module,
            &struct_field_layouts,
            &struct_parent_index,
            &struct_is_value_type,
            &mut aggregate_layouts,
            &singleton_accessors,
            &effectful_resume_map,
            &callable_returns,
            &mut diagnostics,
        ));
        // 泛型种子调用点：为具体代入分配 ItemInstanceId，供 Canonical Invoke 精确匹配返回 TypeId。
        let local_floor = functions
            .iter()
            .filter_map(|function| function.instance)
            .chain(external_calls.iter().filter_map(|contract| contract.instance))
            .map(|instance| instance.index())
            .max()
            .map(|index| index.saturating_add(1))
            .unwrap_or(0);
        let next_mono = mono_counter.get_or_insert(local_floor);
        if *next_mono < local_floor {
            *next_mono = local_floor;
        }
        generic_call_instantiation::materialize_generic_call_instances(&mut functions, next_mono, seed_pool, dispatch, &mut diagnostics);
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
                let symbol = crate::valkyrie::symbols::stable_symbol_key(&contract.symbol);
                super::model::register_callable_symbol_entry(&mut callable_symbols, instance, symbol)
                    .expect("duplicate callable symbol during lowering");
            }
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
            sum_types,
            flags_types,
            singleton_instances: crate::valkyrie::mir::collect_singleton_instance_plans(module),
            semantic_fragments,
            diagnostics,
        };
        result
    }
}
