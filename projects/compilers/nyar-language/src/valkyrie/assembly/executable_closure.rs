//! 从完整 MIR 模块中为分区 executable 构建可达 callee 闭包。

use std::collections::BTreeMap;

use nyar::{Identifier, NyarType, QualifiedName};
use nyar_types::ExecutableFunction;

use crate::{MirFunction, MirModule, MirOperand, MirOperation, concretize_type_lossy, mir::AggregateLayoutPlan, mir_function_to_executable};

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

/// Resolve the raw MIR `symbol` string for a [`QualifiedName`], preferring the
/// instance-method dotted form when both could exist.
fn mir_symbol_string_for_operation(operation: &QualifiedName, mir_by_symbol: &BTreeMap<&str, &MirFunction>) -> Option<String> {
    let via_colon = operation.to_string();
    if mir_by_symbol.contains_key(via_colon.as_str()) {
        return Some(via_colon);
    }
    let parts = operation.parts();
    if parts.len() >= 2 {
        let owner = parts[..parts.len() - 1].iter().map(|part| part.as_str()).collect::<Vec<_>>().join(".");
        let method = parts[parts.len() - 1].as_str();
        let via_dot = format!("{owner}.{method}");
        if mir_by_symbol.contains_key(via_dot.as_str()) {
            return Some(via_dot);
        }
    }
    if parts.len() == 1 {
        let simple = parts[0].as_str().to_string();
        if mir_by_symbol.contains_key(simple.as_str()) {
            return Some(simple);
        }
    }
    None
}

/// 以 `seed_operations` 为根，沿 MIR `Call` 边从完整 `mir.functions` 收集可达函数。
pub(crate) fn build_reachable_mir_functions(
    seed_operations: &[QualifiedName],
    mir: &MirModule,
    _hir_singleton_names: &[&str],
) -> BTreeMap<QualifiedName, ExecutableFunction> {
    let mir_by_symbol: BTreeMap<&str, &MirFunction> = mir.functions.iter().map(|function| (function.symbol.as_str(), function)).collect();
    let mir_by_operation: BTreeMap<QualifiedName, &MirFunction> =
        mir.functions.iter().map(|function| (qualified_name_from_mir_symbol(function.symbol.as_str()), function)).collect();
    let mut result = BTreeMap::new();
    let mut queue = seed_operations.to_vec();
    let mut index = 0usize;

    while index < queue.len() {
        let operation = queue[index].clone();
        index += 1;
        let Some(mir_fn) = mir_by_operation.get(&operation).copied().or_else(|| {
            mir_symbol_string_for_operation(&operation, &mir_by_symbol).and_then(|symbol| mir_by_symbol.get(symbol.as_str()).copied())
        })
        else {
            continue;
        };
        if result.insert(operation.clone(), mir_function_to_executable(mir_fn)).is_some() {
            continue;
        }
        for callee in collect_mir_callee_operations(mir_fn, &mir_by_symbol, &mir_by_operation) {
            if !queue.iter().any(|existing| existing == &callee) {
                queue.push(callee);
            }
        }
    }

    result
}

/// Node 自举：`build_from_cli_state` 仅由 JS `exports.build()` 直接调用，不在 entry 可达闭包内。
pub(crate) fn find_build_from_cli_seed(mir: &MirModule) -> Option<QualifiedName> {
    mir.functions
        .iter()
        .find(|function| function.symbol.ends_with("::build_from_cli_state"))
        .map(|function| qualified_name_from_mir_symbol(function.symbol.as_str()))
}

/// Node JS-glue CLI 契约入口：由 `.mjs` 启动壳直接调用，未必落在 `[main]` 可达闭包内。
///
/// 包含 `build_from_cli_state` / `version_text` / `print_root_help`，对应 `exports.build` /
/// `exports.version` / `exports.help`。
pub(crate) fn find_node_cli_glue_seeds(mir: &MirModule) -> Vec<QualifiedName> {
    const SEED_SUFFIXES: &[&str] = &["::build_from_cli_state", "::version_text", "::print_root_help"];
    let mut seeds = Vec::new();
    for function in &mir.functions {
        if SEED_SUFFIXES.iter().any(|suffix| function.symbol.ends_with(suffix)) {
            let operation = qualified_name_from_mir_symbol(function.symbol.as_str());
            if !seeds.iter().any(|existing| existing == &operation) {
                seeds.push(operation);
            }
        }
    }
    seeds
}

fn collect_mir_callee_operations(
    mir_fn: &MirFunction,
    mir_by_symbol: &BTreeMap<&str, &MirFunction>,
    mir_by_operation: &BTreeMap<QualifiedName, &MirFunction>,
) -> Vec<QualifiedName> {
    let mut callees = Vec::new();
    for block in &mir_fn.blocks {
        for instruction in &block.instructions {
            let MirOperation::Call { callee, arguments, .. } = &instruction.kind
            else {
                continue;
            };
            let MirOperand::Symbol(path) = callee
            else {
                continue;
            };
            if let Some(operation) = resolve_mir_callee_operation(path, arguments, mir_fn, mir_by_symbol, mir_by_operation) {
                callees.push(operation);
            }
        }
    }
    callees
}

fn resolve_mir_callee_operation(
    path: &crate::NamePath,
    arguments: &[MirOperand],
    mir_fn: &MirFunction,
    mir_by_symbol: &BTreeMap<&str, &MirFunction>,
    mir_by_operation: &BTreeMap<QualifiedName, &MirFunction>,
) -> Option<QualifiedName> {
    if path.parts().len() > 1 {
        let dotted = path.to_string();
        if mir_by_symbol.contains_key(dotted.as_str()) {
            return Some(qualified_name_from_mir_symbol(dotted.as_str()));
        }
        let via_colon = path.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>().join("::");
        if mir_by_symbol.contains_key(via_colon.as_str()) {
            return Some(qualified_name_from_mir_symbol(via_colon.as_str()));
        }
        return None;
    }

    let method_name = path.parts().last()?.as_str();
    if path.parts().len() == 1 {
        // Bare MIR symbols (empty / erased declaring_namespace) are exact keys — not
        // `::name` / `.name` suffixes. Missing this drops helpers like `wasm_i32_types`
        // from the CLR/WASM reachable closure and triggers `unknown_call_signature`.
        if mir_by_symbol.contains_key(method_name) {
            return Some(qualified_name_from_mir_symbol(method_name));
        }
        if let Some(receiver_ty) = arguments.first().and_then(|argument| match argument {
            MirOperand::Value(value) => mir_fn.value_types.get(value).cloned(),
            _ => None,
        }) {
            let receiver_ty = concretize_type_lossy(&receiver_ty);
            if let Some(receiver_name) = receiver_type_name(&receiver_ty) {
                if let Some(symbol) = mir_by_symbol.keys().find(|symbol| {
                    mir_symbol_ends_with_simple(symbol, method_name)
                        && symbol.split(['.', ':']).any(|segment| segment == receiver_name)
                }) {
                    return Some(qualified_name_from_mir_symbol(symbol));
                }
            }
        }
    }

    None
}

/// True when `symbol` is exactly `simple` or ends with `::simple` / `.simple`.
fn mir_symbol_ends_with_simple(symbol: &str, simple: &str) -> bool {
    symbol == simple || symbol.ends_with(&format!("::{simple}")) || symbol.ends_with(&format!(".{simple}"))
}

fn receiver_type_name(ty: &NyarType) -> Option<&str> {
    match ty {
        NyarType::Utf8 => Some("utf8"),
        NyarType::Utf16 => Some("utf16"),
        NyarType::Array(_) => Some("Array"),
        NyarType::Named(name) => Some(name.as_str()),
        NyarType::Apply(base, _) => receiver_type_name(base),
        _ => None,
    }
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
        let reachable = build_reachable_mir_functions(&[seed], &mir, &[]);
        assert!(
            reachable.keys().any(|op| op.parts().last().is_some_and(|part| part.as_str() == "wasm_i32_types")),
            "bare Call to wasm_i32_types must enter the reachable closure; keys={:?}",
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
            symbol: "leetcode.two_sum".to_string(),
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
        let seed = qualified_name_from_mir_symbol("leetcode.two_sum");
        let reachable = build_reachable_mir_functions(&[seed], &mir, &[]);
        assert!(
            !reachable.contains_key(&QualifiedName::new(vec![Identifier::new("TuiRuntime"), Identifier::new("new")])),
            "unresolved bare `new` must not pull unrelated `.new` helpers into the library closure"
        );
    }
}
