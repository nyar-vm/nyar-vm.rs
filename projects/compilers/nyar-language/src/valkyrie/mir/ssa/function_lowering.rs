//! 单个 HIR 函数体到 MIR 函数的 lowering 入口。

use super::{
    builder::MirBuilder,
    callable_types::{CallableReturnTypes, impl_method_param_type, impl_method_return_type},
    effect_lowering,
    model::*,
    struct_facts::collect_field_declarations,
    value_semantics,
    value_semantics::AggregateLayoutPlan,
};
use crate::{
    symbols::{stable_hir_function_name_path, stable_symbol_key},
    types::hir::{HirFunction, HirModule, ValkyrieType, parse_workload_phase_from_annotations},
};
use std::collections::BTreeMap;

pub(super) fn lower_function_semantic(
    module: &HirModule,
    function: &HirFunction,
    aggregate_layouts: &mut AggregateLayoutPlan,
    singleton_accessor_instances: &BTreeMap<String, nyar_types::ItemInstanceId>,
    callable_returns: &CallableReturnTypes,
    member_dispatch: &[super::generic_call_instantiation::MirDispatchEntry],
    impl_owner_type: Option<ValkyrieType>,
    impl_type_arguments: Vec<ValkyrieType>,
    host_runtime_callables: &BTreeMap<String, nyar_types::ItemInstanceId>,
    diagnostics: &mut Vec<MirDiagnostic>,
) -> MirFunction {
    let effectful_inline_targets = effect_lowering::collect_effectful_inline_targets(module);
    let field_declarations = collect_field_declarations(module, diagnostics);
    let nominal_offset = u32::try_from(field_declarations.len()).expect("aggregate nominal identity overflow");
    let (sum_types, _) = crate::valkyrie::hir::lowering::compute_nominal_declarations(module, nominal_offset, diagnostics);
    value_semantics::ensure_unite_layouts_for_sums(aggregate_layouts, &sum_types);
    let mut builder = MirBuilder::new(
        aggregate_layouts.clone(),
        singleton_accessor_instances.clone(),
        effectful_inline_targets,
        callable_returns.clone(),
        member_dispatch.to_vec(),
        impl_owner_type.clone(),
        host_runtime_callables.clone(),
    );
    // Option/Result 身份只来自 HIR 已冻结的 call.sum_owner / Nominal{ItemId}，禁止按短名缓存。
    builder.option_declaration = None;
    builder.result_declaration = None;
    builder.sum_types = sum_types;
    builder.impl_type_arguments = impl_type_arguments;
    builder.field_declarations = field_declarations;
    // Imply/struct methods declare `: Self`; keep MIR return/params on the owner
    // so SMIR007 compares against `isize`/`i32` (etc.), not unsubstituted `Self`.
    let resolved_return_type = impl_method_return_type(
        &function.return_type,
        &function.generics,
        impl_owner_type.as_ref(),
        &builder.impl_type_arguments,
        &builder.sum_types,
    );
    builder.current_return_type = resolved_return_type.clone();

    // 为函数参数分配 SSA 值，并登记到绑定表与入口块 `parameters`。
    // `self` 参数在 HIR 里常为 `AutoType`/`SelfType`（无显式标注），这里用 impl owner
    // 类型名替换，使后续 `field_type_for_object_operand` 能查到泛型结构体字段类型。
    let mut param_values = Vec::new();
    for (index, param) in function.params.iter().enumerate() {
        let value = builder.next_value(MirValueOrigin::Parameter { index, name: param.name.name.to_string() });
        builder.bindings.insert(param.name.name.to_string(), MirOperand::Value(value));
        let resolved_ty = impl_method_param_type(&param.ty, index, impl_owner_type.as_ref(), &builder.impl_type_arguments, &builder.sum_types);
        builder.value_types.insert(value, resolved_ty);
        param_values.push(value);
    }

    // 入口块参数列表与函数形参一一对应。
    // 对 CLR 等后端，可据此走 `collect_parameter_slots` 并生成 `ldarg`。
    builder.blocks[0].parameters = param_values.clone();

    // `[workload_phase("…")]` → 入口 `begin_phase` + 各 `Return` 前 `end_phase`（普通 Call，无专用 opcode）。
    if let Some(phase) = parse_workload_phase_from_annotations(&function.annotations) {
        builder.emit_phase_host_call("begin_phase", &phase);
        builder.workload_phase = Some(phase);
    }

    // 按顺序 lowering 函数体语句。
    if builder.terminator.is_none() {
        for statement in &function.body.statements {
            builder.lower_statement(statement);
            // 若语句已产生 return 等终结符，则停止继续 lowering，
            // 避免不可达代码进入后续块。
            if builder.terminator.is_some() {
                break;
            }
        }
    }

    // 处理尾表达式。
    if builder.terminator.is_none() {
        if let Some(expr) = &function.body.expr {
            let return_ty = builder.current_return_type.clone();
            // Unit-returning mutators (e.g. ArrayList.push) often end with a call that
            // yields a non-Unit value (`push(self._items, value) -> [T]`). Evaluate the
            // trailing expr for effects, then return Unit — do not promote the call
            // result into Return (SMIR007: actual=Array(T) expected=Unit).
            if matches!(return_ty, ValkyrieType::Unit | ValkyrieType::Void) {
                let _ = builder.lower_expr_to_operand(expr);
                if builder.terminator.is_none() {
                    builder.terminate(MirTerminator::Return { value: None });
                }
            }
            else {
                let operand = builder.lower_expr_to_operand_with_hint(expr, Some(&return_ty));
                if builder.terminator.is_none() {
                    builder.terminate(MirTerminator::Return { value: Some(operand) });
                }
            }
        }
        else {
            builder.terminate(MirTerminator::Return { value: None });
        }
    }

    // 刷新当前块指令到 `blocks`。
    let current_label = builder.current_label.clone();
    builder.flush_block(&current_label);
    // builder 从当前计划克隆并追加布局，直接交还同一身份表；禁止按名称重合并。
    *aggregate_layouts = builder.aggregate_layouts;

    let param_types = function
        .params
        .iter()
        .enumerate()
        .map(|(index, param)| {
            impl_method_param_type(&param.ty, index, impl_owner_type.as_ref(), &builder.impl_type_arguments, &builder.sum_types)
        })
        .collect();
    diagnostics.append(&mut builder.diagnostics);
    let symbol = stable_hir_function_name_path(&module.name, function);
    let mir_function = MirFunction {
        display_symbol: stable_symbol_key(&symbol),
        symbol,
        declaration: function.declaration,
        instance: function.instance,
        return_type: resolved_return_type,
        param_types,
        value_types: builder.value_types,
        entry: builder.entry,
        values: builder.values,
        blocks: builder.blocks,
    };
    // 已删除：suspend_analysis / frame_planning 向 MirFunction 挂 God 元数据。
    mir_function
}
