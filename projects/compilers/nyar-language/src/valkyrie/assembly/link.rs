//! Link reachable dependency MIR bodies into a consumer package MIR.
//!
//! Semantic-group builds keep only the consumer HIR/MIR; dependency packages
//! contribute SPI signatures (`MirExternalCallContract`) but not bodies.
//! Emitter SMIR003 requires those bodies in the executable registry (or a host
//! import). This module pulls reachable Valkyrie→Valkyrie callees from retained
//! dependency MIR modules — the minimal Stage1 link step toward
//! `LinkedSemanticProgram`. Host FFI stays on `external_import_links`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{
    types::{Identifier, NamePath, hir::ValkyrieType},
    valkyrie::mir::{LayoutId, MirFunction, MirModule, MirOperand, MirOperation, MirValue, MirValueOrigin, MirValueRef, merge_aggregate_layout_plan},
};

/// 只沿完整调用符号链接依赖函数及其支撑元数据，不推断泛型或改写调用语义。
pub fn link_reachable_dependency_mir(consumer: &mut MirModule, dependency_mirs: &[MirModule]) -> Result<(), std_data::text::valkyrie::ParseError> {
    if !dependency_mirs.is_empty() {
        // 完整身份必须唯一；依赖顺序不得决定语义绑定。
        let mut pool: BTreeMap<String, (usize, MirFunction)> = BTreeMap::new();
        for (dep_index, dep) in dependency_mirs.iter().enumerate() {
            for function in &dep.functions {
                if pool.insert(function.symbol.clone(), (dep_index, function.clone())).is_some() {
                    return Err(std_data::text::valkyrie::ParseError::invalid(format!("重复依赖 callable identity：`{}`", function.symbol)));
                }
            }
        }

        if !pool.is_empty() {
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
                let Some((dep_index, mir_fn)) = pool.get(&need).map(|(index, function)| (*index, function))
                else {
                    continue;
                };
                if linked_symbols.contains(&mir_fn.symbol) || local.contains(&mir_fn.symbol) {
                    continue;
                }
                linked_symbols.insert(mir_fn.symbol.clone());
                linked_by_dep.entry(dep_index).or_default().insert(mir_fn.symbol.clone());
                local.insert(mir_fn.symbol.clone());
                let mir_fn = mir_fn.clone();
                for callee in collect_static_call_symbols(&mir_fn) {
                    if !symbol_satisfied(&callee, &local) {
                        queue.push_back(callee);
                    }
                }
                consumer.functions.push(mir_fn);
            }

            if !linked_symbols.is_empty() {
                // 支撑元数据按贡献依赖划分。布局 id 是模块局部的：
                // 重分配冲突后，只改写该依赖的已链接函数体
                // （SMIR010：Option.tag FieldGet 不得解析到消费方 FunctionAnalysis id）。
                // Semantic MIR 操作不再携带 layout_id；remap 仍作用于侧表 plan，
                // 并对已链接函数体保留空操作遍历以备后用。
                for (dep_index, symbols) in &linked_by_dep {
                    let dep = &dependency_mirs[*dep_index];
                    for layout in &dep.aggregate_layouts.layouts {
                        if let Some(existing) = consumer.aggregate_layouts.layouts.iter().find(|existing| {
                            existing.name == layout.name && existing.namespace == layout.namespace
                        }) {
                            let mut normalized = layout.clone();
                            normalized.id = existing.id;
                            if *existing != normalized {
                                return Err(std_data::text::valkyrie::ParseError::invalid(format!(
                                    "依赖布局合同冲突：`{}.{}`", layout.namespace, layout.name
                                )));
                            }
                        }
                    }
                    let remap = merge_aggregate_layout_plan(&mut consumer.aggregate_layouts, &dep.aggregate_layouts);
                    if !remap.is_empty() {
                        for function in &mut consumer.functions {
                            if symbols.contains(&function.symbol) {
                                remap_function_layout_ids(function, &remap);
                            }
                        }
                    }
                    for sum in &dep.sum_types {
                        if let Some(existing) = consumer.sum_types.iter().find(|existing| existing.name == sum.name) {
                            if existing != sum {
                                return Err(std_data::text::valkyrie::ParseError::invalid(format!("依赖 sum 合同冲突：`{}`", sum.name)));
                            }
                        } else {
                            consumer.sum_types.push(sum.clone());
                        }
                    }
                    for function in &mut consumer.functions {
                        if symbols.contains(&function.symbol) {
                            relocate_variant_ids(function, &dep.sum_types, &consumer.sum_types)?;
                        }
                    }
                    for hir_struct in &dep.structs {
                        if let Some(existing) = consumer.structs.iter().find(|existing| {
                            existing.name == hir_struct.name && existing.namespace == hir_struct.namespace
                        }) {
                            if existing != hir_struct {
                                return Err(std_data::text::valkyrie::ParseError::invalid(format!(
                                    "依赖结构合同冲突：`{}.{}`", hir_struct.namespace, hir_struct.name
                                )));
                            }
                        } else {
                            consumer.structs.push(hir_struct.clone());
                        }
                    }
                }

                // 链接后的函数体拥有 callable identity 的实现权。对应的
                // imported SPI 只能作为签名证据参与校验，不能与实现同时
                // 进入 Canonical item registry；已达依赖的外部合同则必须
                // 一并并入，否则依赖函数体中的调用会在下一阶段失去事实。
                let local_functions: BTreeMap<_, _> = consumer
                    .functions
                    .iter()
                    .map(|function| (function.symbol.as_str(), function))
                    .collect();
                let mut imported = BTreeMap::new();
                for dep_index in linked_by_dep.keys() {
                    let dependency = &dependency_mirs[*dep_index];
                    for contract in &dependency.external_calls {
                        if let Some(function) = local_functions.get(contract.symbol.to_string().as_str()) {
                            if function.param_types != contract.parameter_types || function.return_type != contract.return_type {
                                return Err(std_data::text::valkyrie::ParseError::invalid(format!(
                                    "依赖 callable `{}` 的实现与导入签名不一致",
                                    contract.symbol
                                )));
                            }
                            continue;
                        }
                        if let Some(existing) = consumer.external_calls.iter().find(|existing| existing.symbol == contract.symbol) {
                            if existing.parameter_types != contract.parameter_types || existing.return_type != contract.return_type {
                                return Err(std_data::text::valkyrie::ParseError::invalid(format!(
                                    "外部 callable `{}` 的依赖合同冲突",
                                    contract.symbol
                                )));
                            }
                        }
                        else if let Some(previous) = imported.get(&contract.symbol) {
                            if previous != contract {
                                return Err(std_data::text::valkyrie::ParseError::invalid(format!("重复外部 callable 合同：`{}`", contract.symbol)));
                            }
                        }
                        else {
                            imported.insert(contract.symbol.clone(), contract.clone());
                        }
                    }
                }
                consumer.external_calls.retain(|contract| !local_functions.contains_key(contract.symbol.to_string().as_str()));
                consumer.external_calls.extend(imported.into_values());

            }
        }
    }
    Ok(())
}

fn remap_function_layout_ids(_function: &mut MirFunction, _remap: &BTreeMap<LayoutId, LayoutId>) {
    // 聚合指令不再在 Semantic MIR 操作上携带 layout_id。
}

fn relocate_variant_ids(
    function: &mut MirFunction,
    source: &[nyar_types::SumTypeLayout],
    destination: &[nyar_types::SumTypeLayout],
) -> Result<(), std_data::text::valkyrie::ParseError> {
    let mut destination_ids = BTreeMap::new();
    let mut index = 0u32;
    for sum in destination {
        for variant in &sum.variants {
            let id = nyar_types::VariantId::from_index(index)
                .ok_or_else(|| std_data::text::valkyrie::ParseError::invalid("variant identity 溢出"))?;
            if destination_ids.insert((sum.name.as_str(), variant.name.as_str()), id).is_some() {
                return Err(std_data::text::valkyrie::ParseError::invalid("重复 variant 声明 identity"));
            }
            index = index.checked_add(1).ok_or_else(|| std_data::text::valkyrie::ParseError::invalid("variant identity 溢出"))?;
        }
    }
    let source_variants: Vec<_> = source.iter().flat_map(|sum| sum.variants.iter().map(move |variant| (sum.name.as_str(), variant.name.as_str()))).collect();
    for block in &mut function.blocks {
        for instruction in &mut block.instructions {
            let (owner, id) = match &mut instruction.kind {
                MirOperation::SumNew { sum_type, variant, .. }
                | MirOperation::SumPayloadGet { sum_type, variant, .. }
                | MirOperation::SumVariantIs { sum_type, variant, .. } => (sum_type, variant),
                _ => continue,
            };
            let declaration = source_variants.get(id.index() as usize)
                .ok_or_else(|| std_data::text::valkyrie::ParseError::invalid(format!("函数 `{}` 引用未知 variant identity {id}", function.symbol)))?;
            if declaration.0 != owner.as_str() {
                return Err(std_data::text::valkyrie::ParseError::invalid(format!("函数 `{}` 的 variant owner 不一致", function.symbol)));
            }
            *id = *destination_ids.get(declaration)
                .ok_or_else(|| std_data::text::valkyrie::ParseError::invalid("目标 registry 缺少 variant 声明"))?;
        }
    }
    Ok(())
}

fn symbol_satisfied(need: &str, local: &BTreeSet<String>) -> bool {
    local.contains(need)
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
            callees.push(path.to_string());
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
    fn unqualified_callee_does_not_pull_unique_dependency_helper() {
        let mut consumer = bare_module("legion", vec![call_fn("legion::caller", "helper")]);
        let original = consumer.clone();
        let dependency = bare_module("library", vec![empty_fn("library::helper")]);
        link_reachable_dependency_mir(&mut consumer, &[dependency]).expect("link contract");
        assert_eq!(consumer, original);
    }

    #[test]
    fn links_exact_callee_from_dependency_qualified_symbol() {
        let mut consumer = bare_module(
            "legion",
            vec![call_fn("legion::emitter_compile_project", "nyar.language.valkyrie::compile_project_from_source")],
        );
        let dependency = bare_module("nyar.language.valkyrie", vec![empty_fn("nyar.language.valkyrie::compile_project_from_source")]);
        link_reachable_dependency_mir(&mut consumer, &[dependency]).expect("link contract");
        assert!(
            consumer.functions.iter().any(|function| function.symbol.ends_with("compile_project_from_source")),
            "symbols={:?}",
            consumer.functions.iter().map(|function| &function.symbol).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_conflicting_struct_contract_and_preserves_distinct_owners() {
        use crate::valkyrie::mir::MirStruct;
        let structure = MirStruct {
            name: "Item".to_owned(), namespace: "first".to_owned(),
            fields: Vec::new(), is_value_type: true,
        };
        let mut consumer = bare_module("consumer", vec![call_fn("caller", "dependency.helper")]);
        consumer.structs.push(structure.clone());
        let mut dependency = bare_module("dependency", vec![empty_fn("dependency.helper")]);
        let mut other = structure;
        other.is_value_type = false;
        dependency.structs.push(other.clone());
        assert!(link_reachable_dependency_mir(&mut consumer.clone(), &[dependency.clone()]).is_err());
        other.namespace = "second".to_owned();
        dependency.structs[0] = other;
        link_reachable_dependency_mir(&mut consumer, &[dependency]).expect("different owners");
        assert_eq!(consumer.structs.len(), 2);
    }

    #[test]
    fn rejects_same_owner_layout_conflict_but_allows_local_id_remap() {
        use crate::valkyrie::mir::{AggregateLayout, MirStorageKind};
        let layout = AggregateLayout {
            id: 3, name: "Item".to_owned(), namespace: "owner".to_owned(),
            storage: MirStorageKind::Value, size: 8, align: 8, fields: Vec::new(),
        };
        let mut consumer = bare_module("consumer", vec![call_fn("caller", "dependency.helper")]);
        consumer.aggregate_layouts.layouts.push(layout.clone());
        let mut dependency = bare_module("dependency", vec![empty_fn("dependency.helper")]);
        let mut imported = layout;
        imported.id = 7;
        dependency.aggregate_layouts.layouts.push(imported);
        link_reachable_dependency_mir(&mut consumer.clone(), &[dependency.clone()]).expect("module-local layout ids");
        dependency.aggregate_layouts.layouts[0].size = 16;
        assert!(link_reachable_dependency_mir(&mut consumer, &[dependency]).is_err());
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
            .push(MirInstruction::from_operation(MirOperation::FieldGet { object: MirOperand::Value(MirValueRef(0)), field: Identifier::new("tag") }));

        let mut consumer = MirModule {
            name: "legion".into(),
            functions: vec![call_fn("legion::use_option", "core::Option.is_none")],
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
        link_reachable_dependency_mir(&mut consumer, &[dependency]).expect("link contract");

        assert!(consumer.functions.iter().any(|f| f.symbol.contains("is_none")), "linked Option.is_none");
        let option = consumer.aggregate_layouts.layouts.iter().find(|layout| layout.name == "Option").expect("Option layout merged");
        assert_ne!(option.id, 3, "must not keep colliding id 3");
        assert!(
            consumer.aggregate_layouts.layouts.iter().any(|layout| layout.name == "FunctionAnalysis" && layout.id == 3),
            "consumer FunctionAnalysis keeps unique id 3"
        );
    }

    #[test]
    fn preserves_unresolved_unwrap_for_upstream_contract_validation() {
        let mut consumer_fn = empty_fn("leetcode::two_sum::two_sum");
        let option_value = MirValueRef(0);
        consumer_fn.values.push(MirValue {
            id: option_value,
            origin: MirValueOrigin::Temporary,
        });
        consumer_fn.value_types.insert(
            option_value,
            ValkyrieType::Apply(
                Box::new(ValkyrieType::Named(Identifier::new("Option"))),
                vec![ValkyrieType::Integer64 { signed: true }],
            ),
        );
        consumer_fn.blocks[0]
            .instructions
            .push(MirInstruction::from_operation_with_results(
                MirOperation::Call {
                    callee: MirOperand::Symbol(NamePath::new(vec![Identifier::new("unwrap")])),
                    arguments: vec![MirOperand::Value(option_value)],
                },
                vec![],
            ));

        let mut consumer = bare_module("leetcode.two_sum", vec![consumer_fn]);
        link_reachable_dependency_mir(&mut consumer, &[]).expect("link contract");
        let function = &consumer.functions[0];
        let instruction = &function.blocks[0].instructions[0];
        assert!(matches!(instruction.kind, MirOperation::Call { .. }));
        assert!(instruction.results.is_empty());
    }

    #[test]
    fn preserves_untyped_unwrap_without_option_metadata() {
        let mut consumer_fn = empty_fn("leetcode::two_sum::two_sum");
        let option_value = MirValueRef(0);
        let payload_value = MirValueRef(1);
        consumer_fn.values.push(MirValue {
            id: option_value,
            origin: MirValueOrigin::Temporary,
        });
        consumer_fn.values.push(MirValue {
            id: payload_value,
            origin: MirValueOrigin::CallResult,
        });
        consumer_fn.blocks[0]
            .instructions
            .push(MirInstruction::from_operation_with_results(
                MirOperation::Call {
                    callee: MirOperand::Symbol(NamePath::new(vec![Identifier::new("unwrap")])),
                    arguments: vec![MirOperand::Value(option_value)],
                },
                vec![payload_value],
            ));

        let mut consumer = bare_module("leetcode.two_sum", vec![consumer_fn]);
        link_reachable_dependency_mir(&mut consumer, &[]).expect("link contract");
        let rewritten = &consumer.functions[0].blocks[0].instructions[0].kind;
        assert!(matches!(rewritten, MirOperation::Call { .. }));
    }

    #[test]
    fn preserves_typed_unwrap_for_semantic_lowering() {
        use crate::types::hir::ValkyrieType;
        use crate::types::Identifier;

        let mut consumer_fn = empty_fn("leetcode::two_sum::two_sum");
        let option_value = MirValueRef(0);
        let payload_value = MirValueRef(1);
        consumer_fn.values.push(MirValue {
            id: option_value,
            origin: MirValueOrigin::Temporary,
        });
        consumer_fn.values.push(MirValue {
            id: payload_value,
            origin: MirValueOrigin::CallResult,
        });
        consumer_fn.value_types.insert(
            option_value,
            ValkyrieType::Apply(
                Box::new(ValkyrieType::Named(Identifier::new("Option"))),
                vec![ValkyrieType::Integer64 { signed: true }],
            ),
        );
        consumer_fn.value_types.insert(payload_value, ValkyrieType::Integer64 { signed: true });
        consumer_fn.blocks[0]
            .instructions
            .push(MirInstruction::from_operation_with_results(
                MirOperation::Call {
                    callee: MirOperand::Symbol(NamePath::new(vec![Identifier::new("unwrap")])),
                    arguments: vec![MirOperand::Value(option_value)],
                },
                vec![payload_value],
            ));

        let mut consumer = bare_module("leetcode.two_sum", vec![consumer_fn]);
        link_reachable_dependency_mir(&mut consumer, &[]).expect("link contract");
        let rewritten = &consumer.functions[0].blocks[0].instructions[0].kind;
        assert!(matches!(rewritten, MirOperation::Call { .. }));
    }

    #[test]
    fn merges_dependency_sum_types_into_consumer() {
        use nyar_types::{SumTypeLayout, SumVariantLayout};

        let mut consumer = bare_module("legion", vec![call_fn("legion::clr_local_slot_bytes", "nyar.emitter::typed_instr")]);
        consumer.sum_types.push(SumTypeLayout {
            name: "ConsumerSum".into(),
            is_unite: true,
            tag_width: 1,
            variants: vec![SumVariantLayout { name: "Existing".into(), tag: 0, payload_type: None }],
        });
        let mut dep_fn = empty_fn("nyar.emitter::typed_instr");
        let out = MirValue { id: MirValueRef(0), origin: MirValueOrigin::Temporary };
        dep_fn.values.push(out.clone());
        dep_fn.blocks[0].instructions.push(MirInstruction::from_operation(MirOperation::SumNew {
            sum_type: "MsilOpcode".into(),
            type_args: Vec::new(),
            variant: nyar_types::VariantId::from_index(0).expect("variant identity"),
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
        link_reachable_dependency_mir(&mut consumer, &[dependency]).expect("link contract");
        assert!(consumer.sum_types.iter().any(|sum| sum.name == "MsilOpcode"), "linked dependency sum layouts must survive into consumer MIR");
        let MirOperation::SumNew { variant, .. } = &consumer.functions[1].blocks[0].instructions[0].kind else {
            panic!("linked function must retain its sum operation");
        };
        assert_eq!(*variant, nyar_types::VariantId::from_index(1).expect("relocated variant identity"));
    }
}
