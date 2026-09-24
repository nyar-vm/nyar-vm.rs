//! Link reachable dependency MIR bodies into a consumer package MIR.
//!
//! Semantic-group builds keep only the consumer HIR/MIR; dependency packages
//! contribute SPI signatures (`MirExternalCallContract`) but not bodies.
//! Emitter SMIR003 requires those bodies in the executable registry (or a host
//! import). This module pulls reachable Valkyrie→Valkyrie callees from retained
//! dependency MIR modules — the minimal Stage1 link step toward
//! `LinkedSemanticProgram`. Host FFI stays on `external_import_links`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::valkyrie::mir::{LayoutId, MirFunction, MirModule, MirOperand, MirOperation, merge_aggregate_layout_plan};

/// Merge reachable dependency MIR functions (and supporting layouts/sums) into `consumer`.
///
/// Seeds are static `Call` callees in `consumer` that are not already local.
/// Resolution prefers exact symbol match, then unique simple-name match against
/// the dependency pool. Already-local symbols are never replaced.
pub fn link_reachable_dependency_mir(consumer: &mut MirModule, dependency_mirs: &[MirModule]) {
    if dependency_mirs.is_empty() {
        return;
    }

    // symbol → (dependency index, body). First dep wins on duplicate symbols.
    let mut pool: BTreeMap<String, (usize, MirFunction)> = BTreeMap::new();
    for (dep_index, dep) in dependency_mirs.iter().enumerate() {
        for function in &dep.functions {
            pool.entry(function.symbol.clone()).or_insert_with(|| (dep_index, function.clone()));
        }
    }
    if pool.is_empty() {
        return;
    }

    // Simple name → exact symbol when unique; empty string marks ambiguity.
    let mut by_simple: BTreeMap<String, String> = BTreeMap::new();
    for symbol in pool.keys() {
        let simple = simple_symbol_name(symbol).to_string();
        by_simple
            .entry(simple)
            .and_modify(|existing| {
                if !existing.is_empty() && existing != symbol {
                    existing.clear();
                }
            })
            .or_insert_with(|| symbol.clone());
    }

    let mut local: BTreeSet<String> = consumer.functions.iter().map(|function| function.symbol.clone()).collect();
    let mut queue = VecDeque::new();
    for function in &consumer.functions {
        for callee in collect_static_call_symbols(function) {
            if !symbol_satisfied(&callee, &local) {
                queue.push_back(callee);
            }
        }
    }

    let mut linked_symbols = BTreeSet::new();
    let mut linked_by_dep: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
    while let Some(need) = queue.pop_front() {
        let Some((dep_index, mir_fn)) = resolve_from_pool(&need, &pool, &by_simple)
        else {
            continue;
        };
        if linked_symbols.contains(&mir_fn.symbol) || local.contains(&mir_fn.symbol) {
            continue;
        }
        linked_symbols.insert(mir_fn.symbol.clone());
        linked_by_dep.entry(dep_index).or_default().insert(mir_fn.symbol.clone());
        local.insert(mir_fn.symbol.clone());
        for callee in collect_static_call_symbols(mir_fn) {
            if !symbol_satisfied(&callee, &local) {
                queue.push_back(callee);
            }
        }
        consumer.functions.push(mir_fn.clone());
    }

    if linked_symbols.is_empty() {
        return;
    }

    // Supporting metadata per contributing dependency. Layout ids are module-local:
    // reassign collisions and rewrite only that dep's linked function bodies
    // (SMIR010: Option.tag FieldGet must not resolve to consumer FunctionAnalysis id).
    // ADR 0011: Semantic MIR ops no longer carry layout_id; remap is still applied to
    // the side-table plan and kept as a no-op pass over linked bodies for future use.
    for (dep_index, symbols) in &linked_by_dep {
        let dep = &dependency_mirs[*dep_index];
        let remap = merge_aggregate_layout_plan(&mut consumer.aggregate_layouts, &dep.aggregate_layouts);
        if !remap.is_empty() {
            for function in &mut consumer.functions {
                if symbols.contains(&function.symbol) {
                    remap_function_layout_ids(function, &remap);
                }
            }
        }
        for sum in &dep.sum_types {
            if !consumer.sum_types.iter().any(|existing| existing.name == sum.name) {
                consumer.sum_types.push(sum.clone());
            }
        }
        for hir_struct in &dep.structs {
            if !consumer.structs.iter().any(|existing| existing.name == hir_struct.name) {
                consumer.structs.push(hir_struct.clone());
            }
        }
    }

    eprintln!("[seed-debug] dependency-mir-link linked={} consumer_functions={}", linked_symbols.len(), consumer.functions.len());
}

fn remap_function_layout_ids(_function: &mut MirFunction, _remap: &BTreeMap<LayoutId, LayoutId>) {
    // ADR 0011: aggregate instructions no longer carry layout_id on Semantic MIR ops.
}

fn simple_symbol_name(symbol: &str) -> &str {
    symbol.rsplit([':', '.']).next().unwrap_or(symbol)
}

fn symbol_satisfied(need: &str, local: &BTreeSet<String>) -> bool {
    local.contains(need) || local.iter().any(|symbol| mir_symbol_ends_with_simple(symbol, need) || mir_symbol_ends_with_simple(need, symbol))
}

fn mir_symbol_ends_with_simple(symbol: &str, simple: &str) -> bool {
    symbol == simple || symbol.ends_with(&format!("::{simple}")) || symbol.ends_with(&format!(".{simple}"))
}

fn resolve_from_pool<'a>(
    need: &str,
    pool: &'a BTreeMap<String, (usize, MirFunction)>,
    by_simple: &BTreeMap<String, String>,
) -> Option<(usize, &'a MirFunction)> {
    if let Some((dep_index, function)) = pool.get(need) {
        return Some((*dep_index, function));
    }
    let simple = simple_symbol_name(need);
    if let Some(exact) = by_simple.get(simple) {
        if !exact.is_empty() {
            return pool.get(exact).map(|(dep_index, function)| (*dep_index, function));
        }
    }
    None
}

fn collect_static_call_symbols(mir_fn: &MirFunction) -> Vec<String> {
    let mut callees = Vec::new();
    for block in &mir_fn.blocks {
        for instruction in &block.instructions {
            let MirOperation::Call { callee, .. } = &instruction.kind
            else {
                continue;
            };
            let MirOperand::Symbol(path) = callee
            else {
                continue;
            };
            // NamePath Display uses `.`; MIR symbols often use `::`. Keep both.
            let dotted = path.to_string();
            callees.push(dotted.clone());
            if dotted.contains('.') && !dotted.contains("::") {
                callees.push(path.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>().join("::"));
            }
            if let Some(simple) = path.parts().last() {
                callees.push(simple.as_str().to_string());
            }
        }
    }
    callees
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        types::{Identifier, NamePath, hir::ValkyrieType},
        valkyrie::mir::{AggregateLayoutPlan, MirBlock, MirBlockRef, MirInstruction, MirTerminator, MirValue, MirValueOrigin, MirValueRef},
    };

    fn empty_fn(symbol: &str) -> MirFunction {
        MirFunction {
            symbol: symbol.to_string(),
            return_type: ValkyrieType::Unit,
            param_types: Vec::new(),
            value_types: Default::default(),
            entry: MirBlockRef(0),
            values: Vec::new(),
            blocks: vec![MirBlock {
                id: MirBlockRef(0),
                label: "entry".into(),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: MirTerminator::Return { value: None },
            }],
        }
    }

    #[allow(deprecated)]
    fn call_fn(symbol: &str, callee: &str) -> MirFunction {
        let mut function = empty_fn(symbol);
        let out = MirValue { id: MirValueRef(0), origin: MirValueOrigin::Temporary };
        function.values.push(out.clone());
        function.blocks[0].instructions.push(MirInstruction::from_operation(MirOperation::Call {
            callee: MirOperand::Symbol(NamePath::new(vec![Identifier::new(callee)])),
            arguments: Vec::new(),
        }));
        function
    }

    fn bare_module(name: &str, functions: Vec<MirFunction>) -> MirModule {
        MirModule {
            name: name.into(),
            functions,
            structs: Vec::new(),
            imports: Vec::new(),
            external_calls: Vec::new(),
            aggregate_layouts: AggregateLayoutPlan::default(),
            sum_types: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn links_bare_callee_from_dependency_qualified_symbol() {
        let mut consumer = bare_module("legion", vec![call_fn("legion::emitter_compile_project", "compile_project_from_source")]);
        let dependency = bare_module("nyar.language.valkyrie", vec![empty_fn("nyar.language.valkyrie::compile_project_from_source")]);
        link_reachable_dependency_mir(&mut consumer, &[dependency]);
        assert!(
            consumer.functions.iter().any(|function| function.symbol.ends_with("compile_project_from_source")),
            "symbols={:?}",
            consumer.functions.iter().map(|function| &function.symbol).collect::<Vec<_>>()
        );
    }

    #[test]
    fn merges_colliding_aggregate_layouts_into_consumer_plan() {
        use crate::valkyrie::mir::{AggregateLayout, FieldLayout, MirStorageKind};
        use nyar_types::NyarType;

        let consumer_layout = AggregateLayout {
            id: 3,
            name: "FunctionAnalysis".into(),
            namespace: String::new(),
            storage: MirStorageKind::Value,
            size: 8,
            align: 8,
            fields: vec![FieldLayout { name: "symbol".into(), ty: NyarType::Utf8, offset: 0, size: 8, align: 8 }],
        };
        let mut consumer_plan = AggregateLayoutPlan::default();
        consumer_plan.layouts.push(consumer_layout.clone());
        consumer_plan.type_name_to_layout.insert("FunctionAnalysis".into(), 3);

        let option_layout = AggregateLayout {
            id: 3, // collide with consumer FunctionAnalysis
            name: "Option".into(),
            namespace: String::new(),
            storage: MirStorageKind::Reference,
            size: 16,
            align: 8,
            fields: vec![
                FieldLayout { name: "tag".into(), ty: NyarType::Integer32 { signed: true }, offset: 0, size: 4, align: 4 },
                FieldLayout { name: "payload".into(), ty: NyarType::Utf8, offset: 8, size: 8, align: 8 },
            ],
        };
        let mut dep_plan = AggregateLayoutPlan::default();
        dep_plan.layouts.push(option_layout);
        dep_plan.type_name_to_layout.insert("Option".into(), 3);

        let mut dep_fn = empty_fn("core::Option.is_none");
        let tag = MirValue { id: MirValueRef(0), origin: MirValueOrigin::Temporary };
        dep_fn.values.push(tag.clone());
        dep_fn.blocks[0]
            .instructions
            .push(MirInstruction::from_operation(MirOperation::FieldGet { object: MirOperand::Value(MirValueRef(0)), field: "tag".into() }));

        let mut consumer = MirModule {
            name: "legion".into(),
            functions: vec![call_fn("legion::use_option", "Option.is_none")],
            structs: Vec::new(),
            imports: Vec::new(),
            external_calls: Vec::new(),
            aggregate_layouts: consumer_plan,
            sum_types: Vec::new(),
            diagnostics: Vec::new(),
        };
        let dependency = MirModule {
            name: "core".into(),
            functions: vec![dep_fn],
            structs: Vec::new(),
            imports: Vec::new(),
            external_calls: Vec::new(),
            aggregate_layouts: dep_plan,
            sum_types: Vec::new(),
            diagnostics: Vec::new(),
        };
        link_reachable_dependency_mir(&mut consumer, &[dependency]);

        assert!(consumer.functions.iter().any(|f| f.symbol.contains("is_none")), "linked Option.is_none");
        let option = consumer.aggregate_layouts.layouts.iter().find(|layout| layout.name == "Option").expect("Option layout merged");
        assert_ne!(option.id, 3, "must not keep colliding id 3");
        assert!(
            consumer.aggregate_layouts.layouts.iter().any(|layout| layout.name == "FunctionAnalysis" && layout.id == 3),
            "consumer FunctionAnalysis keeps unique id 3"
        );
    }

    #[test]
    fn merges_dependency_sum_types_into_consumer() {
        use nyar_types::{SumTypeLayout, SumVariantLayout};

        let mut consumer = bare_module("legion", vec![call_fn("legion::clr_local_slot_bytes", "typed_instr")]);
        let mut dep_fn = empty_fn("nyar.emitter::typed_instr");
        let out = MirValue { id: MirValueRef(0), origin: MirValueOrigin::Temporary };
        dep_fn.values.push(out.clone());
        dep_fn.blocks[0].instructions.push(MirInstruction::from_operation(MirOperation::SumNew {
            sum_type: "MsilOpcode".into(),
            type_args: Vec::new(),
            variant: "Stloc0".into(),
            payload_type: None,
            payload: None,
        }));
        let dependency = MirModule {
            name: "nyar.emitter".into(),
            functions: vec![dep_fn],
            structs: Vec::new(),
            imports: Vec::new(),
            external_calls: Vec::new(),
            aggregate_layouts: AggregateLayoutPlan::default(),
            sum_types: vec![SumTypeLayout {
                name: "MsilOpcode".into(),
                is_unite: false,
                tag_width: 4,
                variants: vec![SumVariantLayout { name: "Stloc0".into(), tag: 0, payload_type: None }],
            }],
            diagnostics: Vec::new(),
        };
        link_reachable_dependency_mir(&mut consumer, &[dependency]);
        assert!(consumer.sum_types.iter().any(|sum| sum.name == "MsilOpcode"), "linked dependency sum layouts must survive into consumer MIR");
    }
}
