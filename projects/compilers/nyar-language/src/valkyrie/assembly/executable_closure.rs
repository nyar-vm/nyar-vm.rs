//! 从完整 MIR 模块中为分区 executable 构建可达 callee 闭包。

use std::collections::BTreeMap;

use miette::{Result, miette};
use nyar::{Identifier, QualifiedName};
use nyar_types::ExecutableFunction;

use crate::{MirFunction, MirModule, MirOperand, MirOperation, mir::AggregateLayoutPlan, mir_function_to_executable};

/// Parse a MIR function `symbol` string into a [`QualifiedName`].
///
/// Free functions use `::` (`core::types::foo`). Instance / imply methods use a
/// single `.` between owner and method (`Option.is_none`). Both must round-trip
/// through the same [`QualifiedName`] identity — never look up MIR by
/// `QualifiedName::to_string()` alone, because that always emits `::`.
pub(crate) fn qualified_name_from_mir_symbol(symbol: &str) -> QualifiedName {
    if symbol.contains("::") {
        return QualifiedName::new(symbol.split("::").map(Identifier::new).collect());
    }
    if let Some((owner, method)) = symbol.rsplit_once('.') {
        return QualifiedName::new(vec![Identifier::new(owner), Identifier::new(method)]);
    }
    QualifiedName::new(symbol.split('.').map(Identifier::new).collect())
}

/// 以 `seed_operations` 为根，沿 MIR `Call` 边从完整 `mir.functions` 收集可达函数。
pub(crate) fn build_reachable_mir_functions(
    seed_operations: &[QualifiedName],
    mir: &MirModule,
) -> Result<BTreeMap<QualifiedName, ExecutableFunction>> {
    let mut mir_by_operation = BTreeMap::new();
    for function in &mir.functions {
        let operation = qualified_name_from_mir_symbol(&function.symbol);
        if mir_by_operation.insert(operation.clone(), function).is_some() {
            return Err(miette!("MIR callable identity collision for `{operation}`"));
        }
    }
    let mut result = BTreeMap::new();
    let mut queue = seed_operations.to_vec();
    let mut index = 0usize;

    while index < queue.len() {
        let operation = queue[index].clone();
        index += 1;
        let mir_fn = mir_by_operation.get(&operation).copied().ok_or_else(|| miette!("executable seed `{operation}` has no MIR definition"))?;
        if result.contains_key(&operation) {
            continue;
        }
        let executable = mir_function_to_executable(mir_fn, &mir.sum_types).map_err(|error| miette!("MIR backend-private conversion failed: {error}"))?;
        result.insert(operation, executable);
        for callee in collect_mir_callee_operations(mir_fn, mir, &mir_by_operation)? {
            if !queue.iter().any(|existing| existing == &callee) {
                queue.push(callee);
            }
        }
    }

    Ok(result)
}

fn collect_mir_callee_operations(
    mir_fn: &MirFunction,
    mir: &MirModule,
    mir_by_operation: &BTreeMap<QualifiedName, &MirFunction>,
) -> Result<Vec<QualifiedName>> {
    let mut callees = Vec::new();
    for block in &mir_fn.blocks {
        for instruction in &block.instructions {
            let MirOperation::Call { callee, arguments, .. } = &instruction.kind
            else {
                continue;
            };
            let MirOperand::Symbol(path) = callee else { continue };
            let operation = QualifiedName::new(path.parts().to_vec());
            if mir_by_operation.contains_key(&operation) {
                callees.push(operation);
            }
            else if !mir.external_calls.iter().any(|external| &external.symbol == path) {
                return Err(miette!("MIR call `{path}` has neither a local definition nor an explicit import contract"));
            }
        }
    }
    Ok(callees)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_method_symbol_round_trips_through_qualified_name() {
        let qn = qualified_name_from_mir_symbol("Option.is_none");
        assert_eq!(qn.parts().len(), 2);
        assert_eq!(qn.parts()[0].as_str(), "Option");
        assert_eq!(qn.parts()[1].as_str(), "is_none");
        // Display uses `::`, which must not be required to find the MIR entry.
        assert_eq!(qn.to_string(), "Option::is_none");
    }

    #[test]
    fn free_function_symbol_keeps_namespace_colons() {
        let qn = qualified_name_from_mir_symbol("std::iterator::for_each");
        assert_eq!(qn.to_string(), "std::iterator::for_each");
        assert_eq!(qn.parts().len(), 3);
    }

    #[test]
    fn bare_callee_reaches_exact_mir_symbol_in_closure() {
        use crate::{
            MirBlock, MirBlockRef, MirFunction, MirInstruction, MirModule, MirOperand, MirOperation, MirTerminator, MirValue, MirValueOrigin,
            MirValueRef, types::hir::ValkyrieType,
        };
        use std::collections::BTreeMap;

        let helper = MirFunction {
            symbol: "wasm_i32_types".to_string(),
            return_type: ValkyrieType::Unit,
            param_types: vec![ValkyrieType::Integer32 { signed: false }],
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
        };
        let arg = MirValueRef(0);
        let out = MirValueRef(1);
        let caller = MirFunction {
            symbol: "nyar::nyar_emitter::wasi::wasi_encode_command_adapt_module_with_mir".to_string(),
            return_type: ValkyrieType::Unit,
            param_types: Vec::new(),
            value_types: BTreeMap::from([(arg, ValkyrieType::Integer32 { signed: false }), (out, ValkyrieType::Unit)]),
            entry: MirBlockRef(0),
            values: vec![MirValue { id: arg, origin: MirValueOrigin::Literal }, MirValue { id: out, origin: MirValueOrigin::CallResult }],
            blocks: vec![MirBlock {
                id: MirBlockRef(0),
                label: "entry".into(),
                parameters: Vec::new(),
                instructions: vec![MirInstruction::from_operation(MirOperation::Call {
                    callee: MirOperand::Symbol(crate::NamePath::new(vec![Identifier::new("wasm_i32_types")])),
                    arguments: vec![MirOperand::Value(arg)],
                })],
                terminator: MirTerminator::Return { value: None },
            }],
        };
        let mir = MirModule {
            name: String::new(),
            functions: vec![caller, helper],
            structs: Vec::new(),
            imports: Vec::new(),
            external_calls: Vec::new(),
            aggregate_layouts: AggregateLayoutPlan::default(),
            sum_types: Vec::new(),
            diagnostics: Vec::new(),
        };
        let seed = qualified_name_from_mir_symbol("nyar::nyar_emitter::wasi::wasi_encode_command_adapt_module_with_mir");
        let reachable = build_reachable_mir_functions(&[seed], &mir).expect("exact MIR seed must resolve");
        assert!(
            reachable.keys().any(|op| op.parts().last().is_some_and(|part| part.as_str() == "wasm_i32_types")),
            "bare Call to wasm_i32_types must enter the reachable closure; keys={:?}",
            reachable.keys().map(|op| op.to_string()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn unqualified_helper_call_fails_without_resolved_identity() {
        use crate::{
            MirBlock, MirBlockRef, MirFunction, MirInstruction, MirModule, MirOperand, MirOperation, MirTerminator, MirValue, MirValueOrigin,
            MirValueRef, types::hir::ValkyrieType,
        };
        use std::collections::BTreeMap;

        let helper = MirFunction {
            symbol: "std::collection::swiss_table_normalize_capacity".to_string(),
            return_type: ValkyrieType::Integer32 { signed: false },
            param_types: vec![ValkyrieType::Integer32 { signed: false }],
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
        };
        let arg = MirValueRef(0);
        let caller = MirFunction {
            symbol: "std.collection.SwissTable.new".to_string(),
            return_type: ValkyrieType::Unit,
            param_types: Vec::new(),
            value_types: BTreeMap::from([(arg, ValkyrieType::Integer32 { signed: false })]),
            entry: MirBlockRef(0),
            values: vec![MirValue { id: arg, origin: MirValueOrigin::Literal }],
            blocks: vec![MirBlock {
                id: MirBlockRef(0),
                label: "entry".into(),
                parameters: Vec::new(),
                instructions: vec![MirInstruction::from_operation(MirOperation::Call {
                    callee: MirOperand::Symbol(crate::NamePath::new(vec![Identifier::new("swiss_table_normalize_capacity")])),
                    arguments: vec![MirOperand::Value(arg)],
                })],
                terminator: MirTerminator::Return { value: None },
            }],
        };
        let mir = MirModule {
            name: String::new(),
            functions: vec![caller, helper],
            structs: Vec::new(),
            imports: Vec::new(),
            external_calls: Vec::new(),
            aggregate_layouts: AggregateLayoutPlan::default(),
            sum_types: Vec::new(),
            diagnostics: Vec::new(),
        };
        let seed = qualified_name_from_mir_symbol("std.collection.SwissTable.new");
        let error = match build_reachable_mir_functions(&[seed], &mir) {
            Ok(_) => panic!("an unqualified helper call must not be linked by its unique short name"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("explicit import contract"), "unexpected failure: {error}");
    }

    #[test]
    fn main_seed_reaches_same_module_answer_helper() {
        use crate::{
            MirBlock, MirBlockRef, MirFunction, MirInstruction, MirModule, MirOperand, MirOperation, MirTerminator, MirValue, MirValueOrigin,
            MirValueRef, types::hir::ValkyrieType,
        };

        let answer = MirFunction {
            symbol: "main::answer".to_string(),
            return_type: ValkyrieType::Integer64 { signed: true },
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
        };
        let out = MirValueRef(0);
        let caller = MirFunction {
            symbol: "main::main".to_string(),
            return_type: ValkyrieType::Integer64 { signed: true },
            param_types: Vec::new(),
            value_types: Default::default(),
            entry: MirBlockRef(0),
            values: vec![MirValue { id: out, origin: MirValueOrigin::CallResult }],
            blocks: vec![MirBlock {
                id: MirBlockRef(0),
                label: "entry".into(),
                parameters: Vec::new(),
                instructions: vec![MirInstruction::from_operation(MirOperation::Call {
                    callee: MirOperand::Symbol(crate::NamePath::new(vec![Identifier::new("main"), Identifier::new("answer")])),
                    arguments: Vec::new(),
                })],
                terminator: MirTerminator::Return { value: None },
            }],
        };
        let mir = MirModule {
            name: String::new(),
            functions: vec![caller, answer],
            structs: Vec::new(),
            imports: Vec::new(),
            external_calls: Vec::new(),
            aggregate_layouts: AggregateLayoutPlan::default(),
            sum_types: Vec::new(),
            diagnostics: Vec::new(),
        };
        let seed = qualified_name_from_mir_symbol("main::main");
        let reachable = build_reachable_mir_functions(&[seed], &mir).expect("qualified call must resolve exactly");
        let answer_op = QualifiedName::new(vec![Identifier::new("main"), Identifier::new("answer")]);
        assert!(
            reachable.contains_key(&answer_op),
            "main seed must pull same-module answer into executable closure, keys={:?}",
            reachable.keys().map(|op| op.to_string()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn bare_new_does_not_widen_closure_to_unrelated_type_new() {
        use crate::{
            MirBlock, MirBlockRef, MirFunction, MirInstruction, MirModule, MirOperand, MirOperation, MirTerminator, MirValue, MirValueOrigin,
            MirValueRef, types::hir::ValkyrieType,
        };

        let tui_new = MirFunction {
            symbol: "TuiRuntime.new".to_string(),
            return_type: ValkyrieType::Named(crate::types::Identifier::new("TuiRuntime")),
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
        };
        let out = MirValueRef(0);
        let caller = MirFunction {
            symbol: "demo.two_sum".to_string(),
            return_type: ValkyrieType::Unit,
            param_types: Vec::new(),
            value_types: Default::default(),
            entry: MirBlockRef(0),
            values: vec![MirValue { id: out, origin: MirValueOrigin::CallResult }],
            blocks: vec![MirBlock {
                id: MirBlockRef(0),
                label: "entry".into(),
                parameters: Vec::new(),
                instructions: vec![MirInstruction::from_operation(MirOperation::Call {
                    callee: MirOperand::Symbol(crate::NamePath::new(vec![Identifier::new("new")])),
                    arguments: Vec::new(),
                })],
                terminator: MirTerminator::Return { value: None },
            }],
        };
        let mir = MirModule {
            name: String::new(),
            functions: vec![caller, tui_new],
            structs: Vec::new(),
            imports: Vec::new(),
            external_calls: Vec::new(),
            aggregate_layouts: AggregateLayoutPlan::default(),
            sum_types: Vec::new(),
            diagnostics: Vec::new(),
        };
        let seed = qualified_name_from_mir_symbol("demo.two_sum");
        let error = match build_reachable_mir_functions(&[seed], &mir) {
            Ok(_) => panic!("an unresolved bare constructor must fail closure construction"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("explicit import contract"), "unexpected failure: {error}");
    }
}
